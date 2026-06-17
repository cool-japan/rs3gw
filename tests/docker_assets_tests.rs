//! Static validation of the repository's Docker assets — **without** running
//! Docker.
//!
//! These tests parse `Dockerfile`, `docker-compose.yml` and
//! `docker-compose.dev.yml` and assert a handful of structural invariants, the
//! most important being that every *host* path the assets reference actually
//! exists on disk. They are pure `std` (no YAML / regex crates) and never shell
//! out to Docker, so they run anywhere `cargo test` runs and catch the classic
//! "someone renamed/deleted a mounted config file" regression at CI time.
//!
//! The parsers are intentionally targeted at the structural conventions used by
//! these specific files rather than being a full Dockerfile/YAML grammar.

use std::fs;
use std::path::PathBuf;

/// Absolute path to the repository root (the crate manifest directory).
///
/// Derived from `CARGO_MANIFEST_DIR` so the tests never hardcode an absolute
/// path (COOLJAPAN security policy) yet still resolve assets reliably.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read a repo-relative file to a `String`, panicking with a clear message.
fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Assert that a repo-relative path exists on disk, naming the referrer.
fn assert_repo_path_exists(rel: &str, context: &str) {
    let path = repo_root().join(rel);
    assert!(
        path.exists(),
        "expected path `{rel}` to exist (resolved: {}) — referenced by {context}",
        path.display()
    );
}

/// Number of leading ASCII spaces on a line (its YAML indentation depth).
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Strip a single pair of surrounding single/double quotes, if present.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// Collect the raw list items found under any *service-level* `volumes:` block
/// (a `volumes:` key that is itself indented). The top-level volume
/// *declaration* block at column 0 is deliberately excluded — it declares names,
/// it does not mount paths.
///
/// Each returned entry keeps its `HOST:CONTAINER[:flag]` shape (quotes removed)
/// so callers can interpret it as either a bind mount or a named volume.
fn collect_service_volume_mounts(content: &str) -> Vec<String> {
    let mut mounts = Vec::new();
    let mut block_indent: Option<usize> = None;

    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = indent_of(raw);

        if let Some(open_indent) = block_indent {
            if indent > open_indent {
                // Still inside the active volumes block: collect list items.
                if let Some(rest) = trimmed.strip_prefix("- ") {
                    mounts.push(unquote(rest).to_string());
                }
                continue;
            }
            // Dedent to a sibling/parent key ends the block; re-examine below.
            block_indent = None;
        }

        // A service-level `volumes:` header is indented (> 0). The top-level
        // declaration block lives at column 0 and must not open a mount block.
        if indent > 0 && trimmed == "volumes:" {
            block_indent = Some(indent);
        }
    }

    mounts
}

/// Collect the names declared under the top-level `volumes:` block (column 0).
///
/// Nested per-volume configuration (e.g. `driver: local`) is ignored: only the
/// `name:` keys sitting at the block's base indent are returned.
fn collect_declared_volumes(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_block = false;
    let mut base_indent: Option<usize> = None;

    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = indent_of(raw);

        if in_block {
            if indent == 0 {
                // Back to a top-level key — the declaration block has ended.
                in_block = false;
                base_indent = None;
                // fall through to re-examine this line as a potential header
            } else {
                // The first child fixes the indent of the volume-name keys;
                // anything deeper is that volume's own configuration.
                let base = *base_indent.get_or_insert(indent);
                if indent == base {
                    if let Some(name) = trimmed.strip_suffix(':') {
                        // A declaration key is a single bare token ending in ':'.
                        if !name.is_empty() && !name.contains(char::is_whitespace) {
                            names.push(name.to_string());
                        }
                    }
                }
                continue;
            }
        }

        if indent == 0 && trimmed == "volumes:" {
            in_block = true;
            base_indent = None;
        }
    }

    names
}

/// Every concrete host path a `COPY`/`ADD` instruction reads from must exist.
#[test]
fn test_dockerfile_copy_paths_exist() {
    let content = read_repo_file("Dockerfile");

    for (idx, raw) in content.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw.trim();
        let upper = line.to_ascii_uppercase();
        if !(upper.starts_with("COPY ") || upper.starts_with("ADD ")) {
            continue;
        }

        // `--from=<stage>` copies from a previous build stage, not the repo.
        if line.contains("--from=") {
            continue;
        }

        // Drop the instruction keyword and any `--flag` args (e.g. `--chown=`),
        // leaving `src... dest`.
        let tokens: Vec<&str> = line
            .split_whitespace()
            .skip(1)
            .filter(|t| !t.starts_with("--"))
            .collect();

        // Need at least one source plus the destination.
        if tokens.len() < 2 {
            continue;
        }

        // The last token is the in-image destination; the rest are host sources.
        let sources = &tokens[..tokens.len() - 1];
        for &src in sources {
            let src = unquote(src);

            // `ADD` may take a URL source — that is not a repo path.
            if src.starts_with("http://") || src.starts_with("https://") {
                continue;
            }

            if src.contains('*') || src.contains('?') {
                // Glob source (e.g. `Cargo.lock*`): the exact match may be
                // absent, but its parent directory must exist. The parent is the
                // segment before the last '/', or '.' when the glob has none.
                let parent = match src.rsplit_once('/') {
                    Some((dir, _)) => dir,
                    None => ".",
                };
                assert_repo_path_exists(
                    parent,
                    &format!("Dockerfile:{lineno} glob source `{src}`"),
                );
            } else {
                // Concrete source: it must exist on disk.
                assert_repo_path_exists(src, &format!("Dockerfile:{lineno} `{line}`"));
            }
        }
    }
}

/// The Dockerfile must expose valid ports, install the runtime binary at the
/// documented path, and configure a health check.
#[test]
fn test_dockerfile_exposes_valid_ports_and_targets_binary() {
    let content = read_repo_file("Dockerfile");

    // (a) At least one EXPOSE, and every port token parses as a u16. Ports may
    //     carry a protocol suffix such as `9000/tcp`.
    let mut expose_count = 0usize;
    for raw in content.lines() {
        let line = raw.trim();
        if !line.to_ascii_uppercase().starts_with("EXPOSE ") {
            continue;
        }
        expose_count += 1;
        for tok in line.split_whitespace().skip(1) {
            let port = tok.split('/').next().unwrap_or(tok);
            assert!(
                port.parse::<u16>().is_ok(),
                "EXPOSE port `{tok}` is not a valid u16 (line: `{line}`)"
            );
        }
    }
    assert!(
        expose_count >= 1,
        "Dockerfile must contain at least one EXPOSE instruction"
    );

    // (b) The runtime stage installs the binary at this exact path.
    assert!(
        content.contains("/usr/local/bin/rs3gw"),
        "Dockerfile must reference the runtime binary path /usr/local/bin/rs3gw"
    );

    // (c) A health check must be configured: a HEALTHCHECK instruction, or an
    //     explicit curl against the /health endpoint.
    let has_healthcheck = content
        .lines()
        .any(|l| l.trim().to_ascii_uppercase().starts_with("HEALTHCHECK"))
        || (content.contains("curl") && content.contains("/health"));
    assert!(
        has_healthcheck,
        "Dockerfile must define a HEALTHCHECK (or a curl .../health check)"
    );
}

/// Both compose files must declare `services:` and every `./` bind-mount source
/// they reference must exist on disk (catches renamed/deleted mounted configs).
#[test]
fn test_compose_files_reference_existing_assets() {
    for file in ["docker-compose.yml", "docker-compose.dev.yml"] {
        let content = read_repo_file(file);

        // Top-level `services:` key (column 0) must be present.
        assert!(
            content.lines().any(|l| l.starts_with("services:")),
            "{file} must declare a top-level `services:` key"
        );

        for mount in collect_service_volume_mounts(&content) {
            // The host source is everything before the first ':'; any trailing
            // `:/container/path` and `:ro`/`:rw` flag is thereby dropped.
            let host = mount.split(':').next().unwrap_or_default();
            if let Some(rel) = host.strip_prefix("./") {
                assert_repo_path_exists(rel, &format!("{file} bind mount `{mount}`"));
            }
        }
    }
}

/// Every named volume a service mounts must be declared under the top-level
/// `volumes:` block of the same compose file.
#[test]
fn test_compose_named_volumes_declared() {
    for file in ["docker-compose.yml", "docker-compose.dev.yml"] {
        let content = read_repo_file(file);
        let declared = collect_declared_volumes(&content);

        for mount in collect_service_volume_mounts(&content) {
            let host = mount.split(':').next().unwrap_or_default();

            // A *named* volume is a host token that is not a path: it does not
            // start with '.' or '/'. (Bind mounts and anonymous volumes are
            // covered by the asset-existence test instead.)
            if host.is_empty() || host.starts_with('.') || host.starts_with('/') {
                continue;
            }

            assert!(
                declared.iter().any(|d| d == host),
                "{file}: service references named volume `{host}` (mount `{mount}`) \
                 that is not declared under the top-level `volumes:` block \
                 (declared: {declared:?})"
            );
        }
    }
}

/// The top-level `version:` key is obsolete under Compose v2 — `docker compose
/// config` emits a deprecation warning when it is present. Guard against the key
/// ever creeping back into either compose file.
#[test]
fn test_compose_files_have_no_obsolete_version_key() {
    for file in ["docker-compose.yml", "docker-compose.dev.yml"] {
        let content = read_repo_file(file);

        for (idx, raw) in content.lines().enumerate() {
            // `trim_end` strips only trailing whitespace, so this `starts_with`
            // check still requires zero leading indentation: a `version:` key
            // nested/indented under a service is correctly ignored.
            let line = raw.trim_end();
            assert!(
                !line.starts_with("version:"),
                "{file}:{} declares an obsolete top-level `version:` key (`{line}`) — \
                 Compose v2 ignores it and `docker compose config` warns; remove it",
                idx + 1
            );
        }
    }
}

/// Extract every single/double-quoted token from `s`, preserving order.
///
/// Used to pull the path entries out of a TOML array fragment without depending
/// on a TOML parser: only the contents *between* matching quotes are returned,
/// so commas, brackets and whitespace between entries are ignored.
fn collect_quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != c {
                j += 1;
            }
            if j >= bytes.len() {
                break; // Unterminated quote — stop scanning.
            }
            out.push(s[start..j].to_string());
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Parse the `[workspace]` `members = [ ... ]` array from a root `Cargo.toml`.
///
/// A tiny `std`-only TOML-subset reader: it tracks the active `[table]` header,
/// and once inside `[workspace]` collects the quoted entries of the (possibly
/// multi-line) `members` array. `#` line comments, trailing commas and quotes
/// are all handled. Only the exact `[workspace]` table is read — sibling tables
/// such as `[workspace.package]` are ignored.
fn parse_workspace_members(content: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut in_workspace = false;
    let mut in_members = false;

    for raw in content.lines() {
        // Drop any trailing `#` comment (no member path contains '#').
        let line = match raw.split_once('#') {
            Some((before, _)) => before,
            None => raw,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if in_members {
            // Inside a multi-line array: collect entries until the closing ']'.
            members.extend(collect_quoted(trimmed));
            if trimmed.contains(']') {
                in_members = false;
            }
            continue;
        }

        // A `[table]` header switches which table subsequent keys belong to.
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_workspace = trimmed == "[workspace]";
            continue;
        }
        if !in_workspace {
            continue;
        }

        // The `members` array key (entries may be inline or on following lines).
        if let Some(after_key) = trimmed.strip_prefix("members") {
            if let Some(array) = after_key.trim_start().strip_prefix('=') {
                members.extend(collect_quoted(array));
                // The array stays open until its closing ']' is seen.
                if !array.contains(']') {
                    in_members = true;
                }
            }
        }
    }

    members
}

/// Collect the *host* source tokens of every builder-stage `COPY`/`ADD`
/// instruction, using the same selection logic as
/// [`test_dockerfile_copy_paths_exist`]: `--from=<stage>` copies and `--flag`
/// arguments are skipped, and each instruction's final destination token is
/// dropped, leaving only the host sources.
fn dockerfile_copy_sources(content: &str) -> Vec<String> {
    let mut sources = Vec::new();

    for raw in content.lines() {
        let line = raw.trim();
        let upper = line.to_ascii_uppercase();
        if !(upper.starts_with("COPY ") || upper.starts_with("ADD ")) {
            continue;
        }
        // `--from=<stage>` copies from a previous build stage, not the repo.
        if line.contains("--from=") {
            continue;
        }

        let tokens: Vec<&str> = line
            .split_whitespace()
            .skip(1)
            .filter(|t| !t.starts_with("--"))
            .collect();
        if tokens.len() < 2 {
            continue;
        }

        // The last token is the in-image destination; the rest are host sources.
        for &src in &tokens[..tokens.len() - 1] {
            sources.push(unquote(src).to_string());
        }
    }

    sources
}

/// Every Cargo **workspace member** must be COPYed into the Docker builder stage.
///
/// If a member directory is missing from the builder context, `cargo build
/// --release` cannot load the workspace and the image build aborts. This guards
/// the regression where the `examples/wasm-plugins/*` members were added to the
/// workspace but never copied into the builder stage.
#[test]
fn test_dockerfile_copies_all_workspace_members() {
    let members = parse_workspace_members(&read_repo_file("Cargo.toml"));
    assert!(
        !members.is_empty(),
        "failed to parse any `[workspace]` members from root Cargo.toml"
    );

    let sources = dockerfile_copy_sources(&read_repo_file("Dockerfile"));

    for member in &members {
        // The root member is satisfied by `Cargo.toml` + `src` already copied.
        if member == "." {
            continue;
        }

        // A COPY source `S` covers member `M` when it is the member itself, an
        // ancestor directory of it, or the whole build context (`.`).
        let covered = sources
            .iter()
            .any(|s| s == member || s == "." || member.starts_with(&format!("{s}/")));

        assert!(
            covered,
            "workspace member `{member}` is not COPYed into the Docker builder \
             stage — add a `COPY` covering it (builder COPY sources: {sources:?})"
        );
    }
}

/// A Cargo build target declared via a `[[bench]]`/`[[bin]]`/`[[example]]`/
/// `[[test]]` array-of-tables entry in `Cargo.toml`.
struct DeclaredTarget {
    /// The target kind: `bench`, `bin`, `example` or `test`.
    kind: String,
    /// The `name = "..."` value, if the table sets one.
    name: Option<String>,
    /// The explicit `path = "..."` value, if the table overrides the default.
    path: Option<String>,
}

/// Parse every `[[bench]]`, `[[bin]]`, `[[example]]` and `[[test]]` table from a
/// root `Cargo.toml`, returning each target's kind plus its `name`/`path` keys.
///
/// A `std`-only TOML-subset reader: it tracks the active array-of-tables header
/// and, while inside one of the four tracked kinds, records the `name` and
/// `path` string values. Any subsequent `[table]` *or* `[[table]]` header closes
/// the current target. Trailing `#` comments are stripped. Keys that merely
/// share a prefix with `name`/`path` (e.g. `harness`, `required-features`) are
/// ignored because the `=` separator must follow the bare key.
fn parse_declared_targets(content: &str) -> Vec<DeclaredTarget> {
    const KINDS: [&str; 4] = ["bench", "bin", "example", "test"];
    let mut targets: Vec<DeclaredTarget> = Vec::new();
    let mut active: Option<usize> = None;

    for raw in content.lines() {
        // Drop any trailing `#` comment (no target name/path contains '#').
        let line = match raw.split_once('#') {
            Some((before, _)) => before,
            None => raw,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // An array-of-tables header `[[kind]]` opens a new target when `kind` is
        // tracked; any other array-of-tables header closes the current target.
        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            let inner = trimmed[2..trimmed.len() - 2].trim();
            if KINDS.contains(&inner) {
                targets.push(DeclaredTarget {
                    kind: inner.to_string(),
                    name: None,
                    path: None,
                });
                active = Some(targets.len() - 1);
            } else {
                active = None;
            }
            continue;
        }
        // A single `[table]` header also closes any active target.
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            active = None;
            continue;
        }

        // Inside a tracked target: capture the `name`/`path` string values. The
        // `=` separator must immediately follow the bare key, so prefixed keys
        // such as `required-features` are not mistaken for `name`/`path`.
        if let Some(idx) = active {
            if let Some(val) = trimmed
                .strip_prefix("name")
                .and_then(|rest| rest.trim_start().strip_prefix('='))
            {
                if let Some(s) = collect_quoted(val).into_iter().next() {
                    targets[idx].name = Some(s);
                }
            } else if let Some(val) = trimmed
                .strip_prefix("path")
                .and_then(|rest| rest.trim_start().strip_prefix('='))
            {
                if let Some(s) = collect_quoted(val).into_iter().next() {
                    targets[idx].path = Some(s);
                }
            }
        }
    }

    targets
}

/// Resolve a declared target's source path relative to the crate root, applying
/// Cargo's default-path conventions when the table sets no explicit `path`.
///
/// Defaults by kind: bench → `benches/{name}.rs`, example → `examples/{name}.rs`,
/// test → `tests/{name}.rs`, bin → `src/bin/{name}.rs`. Returns `None` only when
/// a target declares neither a `path` nor a `name` (and so cannot be resolved).
fn resolve_target_path(target: &DeclaredTarget) -> Option<String> {
    if let Some(path) = &target.path {
        return Some(path.clone());
    }
    let name = target.name.as_ref()?;
    let dir = match target.kind.as_str() {
        "bench" => "benches",
        "example" => "examples",
        "test" => "tests",
        "bin" => "src/bin",
        _ => return None,
    };
    Some(format!("{dir}/{name}.rs"))
}

/// Every Cargo build **target** declared in the root `Cargo.toml` via a
/// `[[bench]]`/`[[bin]]`/`[[example]]`/`[[test]]` table must have its source
/// directory COPYed into the Docker builder stage.
///
/// Cargo validates *all* declared targets when it parses the manifest — even a
/// plain `cargo build --release` that only compiles the default binary. If the
/// directory holding a declared target is absent from the builder context (e.g.
/// `benches/` for a `[[bench]]`), manifest parsing fails and the image build
/// aborts before anything is compiled. This generalises the
/// `examples/wasm-plugins` workspace-member regression to the whole class of
/// "declared target whose directory was never copied".
#[test]
fn test_dockerfile_copies_all_declared_target_dirs() {
    let targets = parse_declared_targets(&read_repo_file("Cargo.toml"));
    assert!(
        !targets.is_empty(),
        "failed to parse any `[[bench]]`/`[[bin]]`/`[[example]]`/`[[test]]` \
         targets from root Cargo.toml"
    );

    let sources = dockerfile_copy_sources(&read_repo_file("Dockerfile"));

    for target in &targets {
        let resolved = resolve_target_path(target).unwrap_or_else(|| {
            panic!(
                "declared [[{}]] target sets neither `name` nor `path` in Cargo.toml",
                target.kind
            )
        });

        // The top directory is the path's first '/'-separated segment, e.g.
        // `benches`, `examples` or `src` — that is what a `COPY` must cover.
        let top = resolved.split('/').next().unwrap_or(resolved.as_str());

        // A COPY source `S` covers top directory `T` when it is the directory
        // itself, an ancestor of it, or the whole build context (`.`).
        let covered = sources
            .iter()
            .any(|s| s == top || s == "." || top.starts_with(&format!("{s}/")));

        assert!(
            covered,
            "declared [[{}]] target `{}` (source `{resolved}`, top dir `{top}`) is \
             not COPYed into the Docker builder stage — add a `COPY` covering \
             `{top}` (builder COPY sources: {sources:?})",
            target.kind,
            target.name.as_deref().unwrap_or("<unnamed>"),
        );
    }
}

/// The root crate's `build.rs` (tonic/prost proto codegen) and the `proto/`
/// directory it compiles must both be COPYed into the Docker builder stage.
///
/// `build.rs` runs during `cargo build --release`; if it is absent the build
/// script never runs and `tonic::include_proto!` finds no `$OUT_DIR` output,
/// cascading into a flood of unresolved-symbol errors. If `proto/` is absent
/// the build script's `compile_protos` call fails outright. This guards the
/// regression where the builder copied `src`/`tests`/`examples`/`benches` but
/// never the build script or its proto inputs.
#[test]
fn test_dockerfile_copies_build_script_inputs() {
    let sources = dockerfile_copy_sources(&read_repo_file("Dockerfile"));

    // (a) `build.rs` is COPYed when it exists at the repo root. A COPY source
    //     covers it when it is the file itself or the whole build context.
    if repo_root().join("build.rs").exists() {
        let covered = sources.iter().any(|s| s == "build.rs" || s == ".");
        assert!(
            covered,
            "repo has a root `build.rs` but the Docker builder stage never \
             COPYs it — the proto build script will not run and \
             `tonic::include_proto!` will have no `$OUT_DIR` output \
             (builder COPY sources: {sources:?})"
        );
    }

    // (b) `proto/` is COPYed when it exists at the repo root. A COPY source
    //     covers it when it is `proto`, an ancestor, or the whole context.
    if repo_root().join("proto").exists() {
        let covered = sources
            .iter()
            .any(|s| s == "proto" || s == "." || "proto".starts_with(&format!("{s}/")));
        assert!(
            covered,
            "repo has a root `proto/` directory (the build script's \
             `compile_protos` input) but the Docker builder stage never COPYs \
             it — the proto codegen will fail (builder COPY sources: {sources:?})"
        );
    }
}
