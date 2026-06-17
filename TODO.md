## Stubs to implement (added 2026-06-12 by /cooljapan-stub-check)

- [x] `rs3gw`: chunked AEAD for seekable range-GET without full decrypt (was `select_parser.rs:593,691`)
  - Priority: P2 | Scope: medium | Done: Session 7 (2026-06-13)
  - Range-GET on a v2 (chunked) SSE object now reads only the ciphertext bytes covering the
    requested chunks (`StorageEngine::read_object_ciphertext_range` +
    `EncryptionService::decrypt_chunked_range_from_slice` + `chunk_ciphertext_span`), instead of
    loading the whole ciphertext into RAM. Also fixed a latent bug: ranges are now sized in
    plaintext coordinates from the sidecar (not `meta.size`, which is ciphertext size for single
    PUT and plaintext size for multipart), so suffix/open-ended ranges are correct.
- [x] `rs3gw`: decrypt-then-hash for full D2 SSE compliance (was `types_3.rs:412`)
  - Priority: P2 | Scope: small | Done: Session 7 (2026-06-13)
  - Full-object GET of an SSE object now validates the stored plaintext SHA-256
    (`__checksum_value__`) against the decrypted bytes in the API layer
    (`select_parser::get_object`), where the plaintext is available. The storage layer still
    skips it (no decryptor there); AEAD tags + this check together give end-to-end integrity.

### Session 7 (2026-06-13)
- [x] `chunk_ciphertext_span` + `decrypt_chunked_range_from_slice` + `single_shot_plaintext_len`
  - **Files:** `src/storage/encryption.rs` (refactored `decrypt_chunked_range` to delegate; unit tests)
- [x] `StorageEngine::read_object_ciphertext_range` (seekable raw read, no decompress/decrypt/checksum)
  - **Files:** `src/storage/core/types/types_3.rs`
- [x] `get_object` restructured: SSE sidecar loaded early; plaintext-coordinate range parsing;
      v2 chunked + uncompressed range → seekable read of covering chunks only; D2 checksum on full GET
  - **Files:** `src/api/handlers/functions/select_parser.rs`
- [x] Tests: encryption unit tests (span math, non-zero `file_start` round-trip, empty-object len);
      SSE integration tests (suffix range, open-ended range, second-chunk-only, empty object 200/416,
      D2 full-GET checksum, multipart-SSE v1 range fallback)
  - **Files:** `src/storage/encryption.rs`, `tests/stub_tests_sse.rs`
- [x] (Optional follow-up) Make multipart-SSE seekable — Done: Session 8 (2026-06-13)
      `multipart.rs` now post-encrypts assembled multipart objects with `encrypt_chunked` (v2),
      so they go through the same seekable range-GET path as single-PUT SSE-S3 objects (a range
      read fetches only the covering ciphertext chunks). Also fixed a HEAD/GET size inconsistency:
      HEAD now reports the plaintext size (sidecar-derived) for all SSE objects instead of the
      on-disk ciphertext size.

### Session 8 (2026-06-13)
- [x] `src/api/multipart.rs`: `encrypt` → `encrypt_chunked` in CompleteMultipartUpload SSE
      post-encryption (multipart SSE objects are now v2 chunked → seekable).
- [x] `src/api/handlers/functions/select_parser.rs::head_object`: load the SSE sidecar early and
      report the plaintext size in Content-Length for all SSE objects (single-PUT SSE previously
      reported the ciphertext size; GET already reported plaintext — now consistent).
- [x] Tests (`tests/stub_tests_sse.rs`): `test_multipart_sse_seekable_multichunk` (6 MiB / 2-part
      multipart → 2 chunks; seekable read of chunk 1 only, boundary-crossing range, HEAD + full-GET
      plaintext size); `test_head_sse_reports_plaintext_size` (single-PUT SSE HEAD size); updated the
      `test_multipart_sse_range_get` comment (now v2 chunked, not v1 fallback).
- [x] XML parser robustness/fuzz harness (`tests/xml_parser_fuzz.rs`) — deterministic seeded fuzzing
      of all nine `api::utils` request parsers (no panic on malformed input; ~405k cases/run).
- [x] Ops-endpoint reachability test (`tests/smoke_tests.rs::test_ops_endpoints_reachable`) —
      `/health` and `/metrics` return 200; closes two release-checklist items.
- [x] Compression threshold/defaults documented in `docs/performance_tuning.md`
      ("Object-Size Considerations"), backed by `benches/compression_benchmarks.rs`.
- [x] Consolidated the 457 auto-generated "deep-dive #NNN" placeholder checkboxes into an honest
      coverage map pointing at the real, passing compatibility suites (xml_golden, aws_sdk_compat,
      bucket/object/multipart/protocol tests). Removed 467 lines of filler.
- [x] Exemplars/tracing IDs on latency histograms — IMPLEMENTED in Session 9 (see below). The Session-8
      deferral (exporter can't embed OpenMetrics exemplars) was resolved with a side-store exemplar
      buffer + `GET /metrics/exemplars`, not an exporter swap.
- Note: observed a PRE-EXISTING flaky lib unit test (`storage::backend::functions::tests::
      test_local_backend_copy_object`) under maximum parallel `cargo test` load — passes in isolation
      and on lib-suite re-run (503/503); unrelated to these changes (does not touch SSE/multipart code).

### Session 9 (2026-06-13) — v0.3/v0.4/v0.5 roadmap items implemented in 0.2.2
- [x] Cost/usage reporting hooks (v0.5): `src/observability/usage.rs` — `UsageTracker` with per-bucket
      transfer + request-by-op counters, `PricingConfig`-based cost estimate, pluggable `UsageHook`
      trait (+ `LoggingUsageHook`). Wired into PUT/GET/DELETE (`functions_3.rs`, `select_parser.rs`),
      `AppState` (`lib.rs`), endpoints `GET /api/usage` and `/api/usage/{bucket}`
      (`observability_handlers.rs`, `s3_router.rs`). Tests: `tests/usage_tests.rs` + 3 unit tests.
- [x] Latency-histogram exemplars / trace IDs (v0.5): `src/metrics.rs` — capture active OTel `trace_id`
      in `metrics_layer`, bounded per-operation exemplar ring, `GET /metrics/exemplars` endpoint.
      Tests: 2 unit tests + `tests/smoke_tests.rs::test_metrics_exemplars_endpoint`.
- [x] Soak tests (v0.3): `tests/soak_tests.rs` — configurable-duration concurrent mixed-workload
      stability harness (env-extensible to multi-hour runs).
- Note: v0.4 roadmap items (dedup/select-cache/quota/throttling) were already complete. The full
      `cargo test` run showed two PRE-EXISTING environmental flakes under maximum parallel load
      (`test_local_backend_copy_object`, `grpc_tests::test_grpc_delete_mixed_existing_nonexisting`) —
      both pass in isolation / when their suite runs alone, and neither touches this session's code.

### Session 10 (2026-06-16) — release-checklist gates automated (run-locally / S3 smoke / Docker assets)
- [x] `tests/binary_smoke_tests.rs` (new) — first test to boot the REAL compiled binary
      (`env!("CARGO_BIN_EXE_rs3gw")`) as a subprocess and drive it over real TCP (every other test
      uses the in-process router). RAII child (kill+reap on drop), free-port pick, child-scoped env
      (auth off, temp storage), `/health` readiness poll with fail-fast on early child exit. Three
      tests: default-config boot + `/health`/`/ready`; S3 lifecycle mb/ls/cp/rm/rb via `aws-sdk-s3`;
      multipart create/upload×3/complete + abort via `aws-sdk-s3`. Closes 3 manual release gates
      (run-locally, S3 basic smoke, multipart smoke) without needing `aws-cli`.
- [x] `tests/docker_assets_tests.rs` (new, std-only) — structural validation of `Dockerfile`,
      `docker-compose.yml`, `docker-compose.dev.yml`: every COPY/ADD source path exists, EXPOSE ports
      parse, HEALTHCHECK + `/usr/local/bin/rs3gw` present, compose bind-mount sources + referenced
      configs exist, named volumes are declared, and no obsolete top-level `version:` key remains.
- [x] Removed the obsolete `version:` key from both compose files (Compose v2 warning eliminated);
      `docker compose config -q` now exits 0 with no warnings on both files (Docker Compose v2.36.2).
- [x] Dockerfile rehabilitation + Pure-Rust proto codegen: the builder had drifted from the v0.2.2
      workspace. Fixed six defects — base image `rust:1.85`->`rust:1.89-slim-bookworm`; `COPY`
      examples/benches/build.rs/proto; builder apt `+= curl ca-certificates`; and `build.rs` switched
      from protoc (`compile_protos`) to the pure-Rust `protox` compiler (`compile_fds`), adding
      `protox` to `[build-dependencies]` (no system/vendored `protoc` — Pure Rust Policy). Host build
      + `grpc_tests` (20) pass. Three new guards in `tests/docker_assets_tests.rs` (workspace members,
      declared target dirs, build-script inputs) catch the COPY-omission classes statically.
- [x] End-to-end verified: real `docker build` -> `rs3gw:ultra-smoke` (219 MB); compose main + the
      full 7-service dev stack both Up (healthy), rs3gw `/health` 200 — closes release gates #2/#3.
- Follow-up (non-blocking, flagged): transitive deps `flate2`/`zip`/`miniz_oxide` are pulled in
      indirectly (COOLJAPAN OxiARC/no-zip policy) — needs a `cargo tree -i` to find the source and
      whether it can be feature-gated out. Does not affect the build/runtime.

## v0.2.2 (Current Release)

### Scope
- S3-compatible REST API (core bucket/object/multipart operations)
- Local filesystem backend as the default storage
- Optional AWS Signature V4 authentication
- Operational basics: metrics, health, timeouts, graceful shutdown
- Developer-facing: tests, examples, Docker/dev compose

### Reference
- Main README: README.md
- Config template: rs3gw.toml.example
- Production guide: docs/production_deployment.md
- Performance tuning: docs/performance_tuning.md
- WebSocket: docs/websocket.md
- Transformations: docs/transformations.md
- WASM plugins: docs/wasm_plugins.md

### Release checklist
- [x] Builds cleanly in release mode (`cargo build --release`) — Session 8: clean, 0 warnings (LTO release)
- [x] Runs locally with default configuration — Session 10 (2026-06-16): tests/binary_smoke_tests.rs::test_binary_boots_with_default_config_and_is_healthy boots the real CARGO_BIN_EXE_rs3gw binary headless (env-only config, auth off, temp storage) and asserts /health (status=healthy) and /ready (200 "ok").
- [x] Docker image builds successfully — Session 10 (2026-06-16): VERIFIED by a real docker build (Docker 28.2.2) → image rs3gw:ultra-smoke (219 MB, release, 11m53s, exit 0). The Dockerfile had drifted from the v0.2.2 workspace; fixes: COPY examples/benches/build.rs/proto, base image rust:1.85 -> rust:1.89-slim-bookworm, builder apt += curl ca-certificates, and build.rs switched to pure-Rust protox (no system protoc — Pure Rust Policy). Build-context completeness regression-guarded by tests/docker_assets_tests.rs.
- [x] docker-compose.dev.yml starts end-to-end stack — Session 10 (2026-06-16): VERIFIED — docker compose -f docker-compose.yml up (rs3gw /health 200 healthy on :9000) AND the full docker-compose.dev.yml 7-service stack (rs3gw+minio+prometheus+grafana+jaeger+redis+postgres) all Up (healthy), rs3gw /health 200; both torn down cleanly. Compose models also validated via docker compose config; obsolete version: key removed (guarded). Same fixed Dockerfile/image as #2.
- [x] S3 basic smoke test (mb/ls/cp/rm/rb) — Session 10 (2026-06-16): tests/binary_smoke_tests.rs::test_binary_s3_basic_lifecycle_over_the_wire drives the live binary over real TCP via aws-sdk-s3 (create/list bucket, put x2, list, copy+get-verify, delete, delete-bucket). No aws CLI needed (not installed; SDK is deterministic + CI-friendly).
- [x] Multipart upload smoke test — Session 10 (2026-06-16): tests/binary_smoke_tests.rs::test_binary_multipart_upload_over_the_wire performs create/upload x3/complete + byte-verify and an abort cycle over the wire via aws-sdk-s3.
- [x] Metrics endpoint reachable (/metrics) — Session 8: `tests/smoke_tests.rs::test_ops_endpoints_reachable`
- [x] Health endpoint reachable (/health) — Session 8: `tests/smoke_tests.rs::test_ops_endpoints_reachable`
- [x] Integration tests pass — Session 8: full `cargo test` integration suites green (SSE 22, multipart,
      object, protocol, smoke, storage-hardening, xml-golden, xml-fuzz)
- [x] Documented note for known stubs/NotImplemented endpoints

## Module map

- API: src/api/README.md
- Storage: src/storage/README.md
- Auth: src/auth/README.md
- gRPC: src/grpc/README.md
- Cluster: src/cluster/README.md
- Observability: src/observability/README.md

## Configuration (confirmed keys)

### TOML (rs3gw.toml.example)
- bind_addr
- storage_root
- default_bucket
- access_key
- secret_key
- request_timeout_secs
- max_concurrent_requests
- compression
- [tls] cert_path
- [tls] key_path
- [connection_pool] pool_max_idle_per_host
- [connection_pool] pool_idle_timeout_secs
- [connection_pool] connect_timeout_secs
- [connection_pool] request_timeout_secs
- [cluster] enabled
- [cluster] advertise_addr
- [cluster] cluster_port
- [cluster] seed_nodes
- [cluster.default_replication] mode
- [cluster.default_replication] replication_factor
- [dedup] enabled
- [dedup] block_size
- [dedup] algorithm
- [dedup] min_size
- [zerocopy] direct_io
- [zerocopy] direct_io_threshold
- [zerocopy] splice
- [zerocopy] mmap

### Environment variables (from code/docs)
- RS3GW_BIND_ADDR
- RS3GW_STORAGE_ROOT
- RS3GW_DEFAULT_BUCKET
- RS3GW_ACCESS_KEY
- RS3GW_SECRET_KEY
- RS3GW_COMPRESSION
- RS3GW_REQUEST_TIMEOUT
- RS3GW_MAX_CONCURRENT
- RS3GW_TLS_CERT
- RS3GW_TLS_KEY
- RS3GW_POOL_MAX_IDLE
- RS3GW_POOL_IDLE_TIMEOUT
- RS3GW_CONNECT_TIMEOUT
- RS3GW_CLIENT_TIMEOUT
- RS3GW_CACHE_ENABLED
- RS3GW_CACHE_MAX_SIZE_MB
- RS3GW_CACHE_MAX_OBJECTS
- RS3GW_CACHE_TTL
- RS3GW_THROTTLE_ENABLED
- RS3GW_THROTTLE_RPS
- RS3GW_THROTTLE_UPLOAD_MBPS
- RS3GW_THROTTLE_DOWNLOAD_MBPS
- RS3GW_QUOTA_ENABLED
- RS3GW_QUOTA_MAX_STORAGE_GB
- RS3GW_QUOTA_MAX_OBJECTS
- RS3GW_CLUSTER_ENABLED
- RS3GW_CLUSTER_NODE_ID
- RS3GW_CLUSTER_ADVERTISE_ADDR
- RS3GW_CLUSTER_PORT
- RS3GW_CLUSTER_SEED_NODES
- RS3GW_REPLICATION_MODE
- RS3GW_REPLICATION_FACTOR
- RS3GW_DEDUP_ENABLED
- RS3GW_DEDUP_BLOCK_SIZE
- RS3GW_DEDUP_ALGORITHM
- RS3GW_DEDUP_MIN_SIZE
- RS3GW_ZEROCOPY_DIRECT_IO
- RS3GW_ZEROCOPY_DIRECT_IO_THRESHOLD
- RS3GW_ZEROCOPY_SPLICE
- RS3GW_ZEROCOPY_MMAP
- RS3GW_SELECT_CACHE_ENABLED
- RS3GW_SELECT_CACHE_MAX_ENTRIES
- RS3GW_SELECT_CACHE_MAX_MEMORY_MB
- RS3GW_SELECT_CACHE_TTL
- RS3GW_GRPC_ENABLED
- RS3GW_GRPC_PORT
- RS3GW_GRPC_MAX_MESSAGE_SIZE
- RS3GW_GRPC_TLS_CERT
- RS3GW_GRPC_TLS_KEY
- RS3GW_PROFILING_ENABLED
- RS3GW_PROFILING_INTERVAL
- RS3GW_PROFILING_MAX_SNAPSHOTS
- RS3GW_PROFILING_CPU
- RS3GW_PROFILING_MEMORY
- RS3GW_PROFILING_IO
- RS3GW_PROFILING_CPU_RATE
- RS3GW_PROFILING_MEMORY_RATE
- RS3GW_PROFILING_MAX_PROFILES
- RS3GW_PROFILING_OUTPUT_DIR
- RS3GW_MIN_THREADS
- RS3GW_MAX_THREADS
- RS3GW_TARGET_CPU
- RS3GW_MEMORY_THRESHOLD
- RS3GW_ADJUSTMENT_INTERVAL
- RS3GW_ADAPTIVE_RATE_LIMIT
- RS3GW_INITIAL_RATE_LIMIT
- RS3GW_MIN_RATE_LIMIT
- RS3GW_MAX_RATE_LIMIT
- RS3GW_LOAD_SHEDDING_THRESHOLD
- ENVIRONMENT
- OTEL_EXPORTER_OTLP_ENDPOINT
- OTEL_TRACES_SAMPLER_ARG
- OTEL_TRACES_EXPORTER

## S3 REST API: compatibility checklist

Legend: [x]=implemented, [ ]=not yet/verify, [~]=compat stub (returns fixed/NotImplemented as needed).

### Bucket operations
- [x] ListBuckets
- [x] HeadBucket
- [x] CreateBucket
- [x] DeleteBucket
- [x] GetBucketLocation
- [x] ListObjectsV2 (bucket listing)
- [x] GetBucketTagging
- [x] PutBucketTagging
- [x] DeleteBucketTagging
- [x] GetBucketPolicy
- [x] PutBucketPolicy
- [x] DeleteBucketPolicy
- [x] GetBucketAcl
  - **Goal:** Real storage read of `bucket_acl.json`; fallback to FULL_CONTROL default if absent.
  - **Design:** `StorageEngine::get_bucket_acl(bucket)` → serde_json deserialize `AclConfig`; on NotFound fall back to `AclConfig::canned_full_control("rs3gw","rs3gw")`. Convert to `AccessControlPolicy` XML.
  - **Files:** `src/storage/core/bucket_acl.rs` (new), `src/api/handlers/functions/core.rs`
  - **Tests:** `tests/stub_tests_acl.rs::test_bucket_acl_default`, `test_bucket_acl_canned_public_read`, `test_bucket_acl_explicit_xml`
  - **Risk:** Existing callers that never PUT an ACL must still GET FULL_CONTROL (backward-compat via fallback).
- [x] PutBucketAcl
  - **Goal:** Parse `x-amz-acl` canned header OR explicit `<AccessControlPolicy>` XML body; persist to `bucket_acl.json`.
  - **Design:** Precedence: both header+body → 400 UnexpectedContent; neither → 400 MissingSecurityHeader; header only → `AclConfig::from_canned`; body only → `parse_acl_xml`. Persist via `StorageEngine::put_bucket_acl`. Router currently inlines `let _ = body.collect(); return OK` at L984–989 — replace with body-forwarding to handler.
  - **Files:** `src/storage/core/bucket_acl.rs`, `src/api/utils.rs`, `src/api/handlers/functions/core.rs`, `src/api/s3_router.rs`
  - **Tests:** `test_bucket_acl_canned_public_read`, `test_bucket_acl_explicit_xml`, `test_bucket_acl_conflict`, `test_bucket_acl_missing`, `test_canned_acl_invalid`
  - **Risk:** Canned ACL `public-read-write` materializes as three grants (owner FULL_CONTROL + AllUsers READ + AllUsers WRITE).
- [x] GetBucketVersioning (planned 2026-05-11)
  - **Goal:** Return current versioning state (`Enabled`, `Suspended`, or empty) as AWS `VersioningConfiguration` XML; `NoSuchBucket` when bucket absent.
  - **Design:** Call `state.storage.get_bucket_versioning(&bucket)` → `BucketVersioningConfig`; map `Enabled`→`Some("Enabled")`, `Suspended`→`Some("Suspended")`, `Unversioned`→`None` in `VersioningConfiguration::new(status)`; call `.to_xml()`. Extend `VersioningConfiguration::to_xml` at `xml_responses.rs:656` to also handle `Suspended`. Located in `src/api/handlers/functions/core.rs:398-422`.
  - **Files:** `src/api/handlers/functions/core.rs`, `src/api/xml_responses.rs` (extend to_xml for Suspended).
  - **Tests:** Functional in `tests/stub_tests.rs::test_bucket_versioning` (extend round-trip); GET on non-versioned bucket → empty XML; PUT Enabled → GET shows Enabled; PUT Suspended → GET shows Suspended.
  - **Risk:** Low — storage versioning manager is already wired in types.rs:1580-1634.
- [x] PutBucketVersioning (planned 2026-05-11)
  - **Goal:** Accept `VersioningConfiguration` XML body and enable or suspend versioning on the bucket; return 200 OK.
  - **Design:** Parse XML via new `parse_versioning_xml` in `src/api/utils.rs`; on `Status == "Enabled"` call `storage.enable_bucket_versioning`; on `Status == "Suspended"` call `storage.suspend_bucket_versioning`; absent or unknown Status → 400 MalformedXML. Router already collects body bytes for this path (`s3_router.rs:478–489`). Located in `src/api/handlers/functions/core.rs:445-471`.
  - **Files:** `src/api/handlers/functions/core.rs`, `src/api/utils.rs` (new `parse_versioning_xml`).
  - **Tests:** PUT Enabled → 200 OK; PUT Suspended → 200 OK; PUT invalid body → 400 MalformedXML.
  - **Risk:** Low — `enable_bucket_versioning` / `suspend_bucket_versioning` already exist in `StorageEngine`.
- [x] GetBucketEncryption (planned 2026-05-11)
  - **Goal:** Return bucket's `ServerSideEncryptionConfiguration` as AWS-format XML; `ServerSideEncryptionConfigurationNotFoundError` (404) if unset; `NoSuchBucket` if bucket absent.
  - **Design:** Read JSON from `<bucket>/bucket_encryption.json` via `StorageEngine::get_bucket_encryption` (NotFound → 404); serialize as `SseConfigurationXml` via quick-xml serialize feature.
  - **Files:** `src/api/bucket_stubs.rs` (handler body), `src/storage/core/types.rs` (new method + path helper), `src/api/xml_responses.rs` (new `SseConfigurationXml`).
  - **Tests:** unit XML round-trip; functional in `tests/stub_tests.rs::test_bucket_encryption`; golden in `tests/xml_golden_tests.rs`; aws-sdk round-trip in `tests/aws_sdk_compat_tests_extended.rs`.
  - **Risk:** Low — mirrors tagging pattern exactly.
- [x] PutBucketEncryption (planned 2026-05-11)
  - **Goal:** Accept `ServerSideEncryptionConfiguration` XML body, validate (SSEAlgorithm ∈ {AES256, aws:kms, aws:kms:dsse}), persist per-bucket, return 200 OK empty body.
  - **Design:** Parse XML via hand-rolled `parse_encryption_xml` in `src/api/utils.rs` (mirror `parse_tagging_xml`); persist as `EncryptionConfig` JSON via `StorageEngine::put_bucket_encryption`; router collects body bytes (change `s3_router.rs:902-924` like `put_bucket_tagging` at L868-880).
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/types.rs`.
  - **Tests:** PUT then GET round-trip; invalid SSEAlgorithm → MalformedXML.
  - **Risk:** Handler signature change (2-arg → 3-arg Bytes); router edit required.
- [x] DeleteBucketEncryption (planned 2026-05-11)
  - **Goal:** Remove per-bucket `bucket_encryption.json` (idempotent); return 204 No Content.
  - **Design:** Call `StorageEngine::delete_bucket_encryption` (mirrors `delete_bucket_tagging`); after delete, GET returns `ServerSideEncryptionConfigurationNotFoundError`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/types.rs`.
  - **Tests:** DELETE then GET → 404; idempotent DELETE on unset bucket.
  - **Risk:** Low.
- [x] GetBucketLifecycleConfiguration (planned 2026-05-11)
  - **Goal:** Return bucket lifecycle rules as AWS `LifecycleConfiguration` XML; `NoSuchLifecycleConfiguration` (404) if unset; `NoSuchBucket` if bucket absent.
  - **Design:** Call `state.storage.get_bucket_lifecycle(&bucket)` (new method in `src/storage/core/bucket_config.rs`); serialize via `LifecycleConfigurationXml::from_config(&cfg).to_xml()`. Persisted as `bucket_lifecycle.json` per-bucket.
  - **Files:** `src/api/bucket_stubs.rs` (handler body), `src/storage/core/bucket_config.rs` (new), `src/api/xml_responses.rs` (new `LifecycleConfigurationXml`).
  - **Tests:** GET before PUT → 404 NoSuchLifecycleConfiguration; PUT → GET round-trip; DELETE → GET → 404.
  - **Risk:** Low — identical pattern to GetBucketEncryption.
- [x] PutBucketLifecycleConfiguration (planned 2026-05-11)
  - **Goal:** Accept `LifecycleConfiguration` XML body, validate (Rule requires Status ∈ {Enabled, Disabled}; non-negative days), persist per-bucket, return 200 OK.
  - **Design:** Parse via `parse_lifecycle_xml` in `src/api/utils.rs`; persist as `LifecycleConfig` JSON via `StorageEngine::put_bucket_lifecycle`; router fixed to forward body bytes (`s3_router.rs:919-925`).
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** PUT valid body → 200 OK; invalid Status → 400 MalformedXML; negative days → 400 MalformedXML.
  - **Risk:** Router body-forwarding fix required (same pattern as PutBucketEncryption).
- [x] DeleteBucketLifecycleConfiguration (planned 2026-05-11)
  - **Goal:** Remove per-bucket `bucket_lifecycle.json` (idempotent); return 204 No Content.
  - **Design:** Call `StorageEngine::delete_bucket_lifecycle`; subsequent GET returns `NoSuchLifecycleConfiguration`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** DELETE then GET → 404; idempotent DELETE on bucket with no lifecycle configured.
  - **Risk:** Low.
- [x] GetBucketCors (planned 2026-05-11)
  - **Goal:** Return bucket's `CORSConfiguration` as AWS-format XML; `NoSuchCORSConfiguration` (404) if unset; `NoSuchBucket` if bucket absent.
  - **Design:** Read JSON from `<bucket>/bucket_cors.json` via `StorageEngine::get_bucket_cors` (NotFound → 404); serialize as `CorsConfigurationXml` (CORSRule list with AllowedMethod/AllowedOrigin/AllowedHeader/ExposeHeader/MaxAgeSeconds).
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/types.rs`, `src/api/xml_responses.rs`.
  - **Tests:** round-trip; `NoSuchCORSConfiguration` before any PUT.
  - **Risk:** Low.
- [x] PutBucketCors (planned 2026-05-11)
  - **Goal:** Accept `CORSConfiguration` XML body, validate (AllowedMethod ∈ {GET,PUT,POST,DELETE,HEAD}; AllowedOrigin required), persist per-bucket, return 200 OK.
  - **Design:** Parse via `parse_cors_xml` in `src/api/utils.rs`; persist as `CorsConfig` JSON via `StorageEngine::put_bucket_cors`; router collects body bytes (same `s3_router.rs` change as PutBucketEncryption).
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/types.rs`.
  - **Tests:** PUT then GET → matching config; missing AllowedOrigin → MalformedXML.
  - **Risk:** Low.
- [x] DeleteBucketCors (planned 2026-05-11)
  - **Goal:** Remove per-bucket `bucket_cors.json` (idempotent); return 204 No Content.
  - **Design:** Call `StorageEngine::delete_bucket_cors`; subsequent GET → `NoSuchCORSConfiguration`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/types.rs`.
  - **Tests:** DELETE then GET → 404; idempotent DELETE on unset bucket.
  - **Risk:** Low.
- [x] GetBucketNotificationConfiguration (planned 2026-05-13)
  - **Goal:** Return the bucket's SNS/SQS/Lambda event notifications as AWS-format `NotificationConfiguration` XML; return empty `<NotificationConfiguration/>` (200) when unset (AWS never 404s this endpoint); `NoSuchBucket` if bucket absent.
  - **Design:** Read JSON from `<bucket>/bucket_notification.json` via `StorageEngine::get_bucket_notification`; serialize as `NotificationConfigXml::from_config(&cfg).to_xml()`. On `NotFound`, emit empty `<NotificationConfiguration/>`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** golden in `tests/xml_golden_tests.rs`; PUT→GET round-trip in `tests/stub_tests.rs`.
  - **Risk:** Low — same pattern as Session 2 BucketEncryption.
- [x] PutBucketNotificationConfiguration (planned 2026-05-13)
  - **Goal:** Accept and persist SNS/SQS/Lambda event notification configuration. Empty `<NotificationConfiguration/>` body is valid (disables all notifications). Returns 200 OK.
  - **Design:** Collect body → UTF-8 decode → `parse_notification_xml` → `storage.put_bucket_notification` → 200 OK. Router switches from body-discarding to body-forwarding idiom.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** Round-trip in `tests/stub_tests.rs::test_bucket_notification`.
  - **Risk:** Low.
- [x] GetBucketLogging (planned 2026-05-11)
  - **Goal:** Return bucket logging config as `BucketLoggingStatus` XML; if unset return 200 with empty `<BucketLoggingStatus/>`; `NoSuchBucket` if bucket absent.
  - **Design:** Call `StorageEngine::get_bucket_logging`; on NotFound return empty `LoggingConfigXml { target_bucket: None, target_prefix: None }` (AWS never 404s logging — it returns empty config). Persist as `bucket_logging.json`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** GET before PUT → 200 empty; PUT → GET round-trip.
  - **Risk:** Low — note empty-config semantics differ from Lifecycle (no 404 on unset).
- [x] PutBucketLogging (planned 2026-05-11)
  - **Goal:** Accept `BucketLoggingStatus` XML body; empty body disables logging; otherwise TargetBucket is required; persist; return 200 OK.
  - **Design:** Parse via `parse_logging_xml`; persist via `StorageEngine::put_bucket_logging`; router body-forwarding fix at `s3_router.rs:952-958`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** PUT with TargetBucket → GET round-trip; PUT empty → GET returns empty; missing TargetBucket when rules present → 400 MalformedXML.
  - **Risk:** Low.
- [x] GetBucketRequestPayment (planned 2026-05-11)
  - **Goal:** Return `RequestPaymentConfiguration` XML; if unset return default `<Payer>BucketOwner</Payer>` (AWS never 404s this); `NoSuchBucket` if bucket absent.
  - **Design:** Call `StorageEngine::get_bucket_request_payment`; on NotFound return default `RequestPaymentConfig { payer: "BucketOwner".to_string() }`. Root element `RequestPaymentConfiguration`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** GET before PUT → 200 with Payer=BucketOwner; PUT Requester → GET returns Requester.
  - **Risk:** Low — default-value semantics (not 404) differ slightly from other families.
- [x] PutBucketRequestPayment (planned 2026-05-11)
  - **Goal:** Accept `RequestPaymentConfiguration` XML; validate Payer ∈ {Requester, BucketOwner}; persist; return 200 OK.
  - **Design:** Parse via `parse_request_payment_xml`; persist via `StorageEngine::put_bucket_request_payment`; router body-forwarding fix at `s3_router.rs:960-966`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** PUT Requester → GET returns Requester; PUT BucketOwner → GET returns BucketOwner; invalid Payer → 400 MalformedXML.
  - **Risk:** Low.
- [x] GetBucketWebsite (planned 2026-05-11)
  - **Goal:** Return `WebsiteConfiguration` XML; `NoSuchWebsiteConfiguration` (404) if unset; `NoSuchBucket` if bucket absent.
  - **Design:** Call `StorageEngine::get_bucket_website`; serialize as `WebsiteConfigurationXml::from_config(&cfg).to_xml()`. Persisted as `bucket_website.json`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** GET before PUT → 404 NoSuchWebsiteConfiguration; PUT → GET round-trip; DELETE → GET → 404.
  - **Risk:** Low.
- [x] PutBucketWebsite (planned 2026-05-11)
  - **Goal:** Accept `WebsiteConfiguration` XML; validate (at least one of IndexDocument/RedirectAllRequestsTo/RoutingRules; RedirectAllRequestsTo requires HostName); persist; return 200 OK.
  - **Design:** Parse via `parse_website_xml`; persist via `StorageEngine::put_bucket_website`; router body-forwarding fix at `s3_router.rs:968-974`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** PUT IndexDocument → GET round-trip; PUT RedirectAllRequestsTo → GET; missing required fields → 400.
  - **Risk:** Moderate — Website has the most complex XML schema (routing rules, redirects).
- [x] DeleteBucketWebsite (planned 2026-05-11)
  - **Goal:** Remove per-bucket `bucket_website.json` (idempotent); return 204 No Content.
  - **Design:** Call `StorageEngine::delete_bucket_website`; subsequent GET returns `NoSuchWebsiteConfiguration`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** DELETE then GET → 404; idempotent DELETE.
  - **Risk:** Low.
- [x] GetBucketReplication (planned 2026-05-13)
  - **Goal:** Return the bucket's cross-region replication configuration as `ReplicationConfiguration` XML; emit `ReplicationConfigurationNotFoundError` (404) when unset.
  - **Design:** Read JSON from `<bucket>/bucket_replication.json`; serialize as `ReplicationConfigXml::from_config(&cfg).to_xml()`. On `NotFound`, return 404 `ReplicationConfigurationNotFoundError`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** Round-trip in `tests/stub_tests.rs::test_bucket_replication`.
  - **Risk:** Low.
- [x] PutBucketReplication (planned 2026-05-13)
  - **Goal:** Accept and persist cross-region replication config. Validates `<Role>` non-empty and ≥1 `<Rule>`. Returns 200 OK.
  - **Design:** Collect body → parse → `storage.put_bucket_replication` → 200.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** Round-trip in `tests/stub_tests.rs::test_bucket_replication`.
  - **Risk:** Low.
- [x] DeleteBucketReplication (planned 2026-05-13)
  - **Goal:** Remove the bucket's replication configuration. Idempotent (204 even if not set).
  - **Design:** `storage.delete_bucket_replication(bucket)` → 204 No Content. On `BucketNotFound` → `NoSuchBucket`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** DELETE→GET→404 in `tests/stub_tests.rs::test_bucket_replication`.
  - **Risk:** Low.
- [x] GetBucketAccelerateConfiguration (planned 2026-05-13)
  - **Goal:** Return the bucket's Transfer Acceleration status as `AccelerateConfiguration` XML. Returns `<Status>Suspended</Status>` (200) by default — AWS never 404s this endpoint.
  - **Design:** Read JSON from `<bucket>/bucket_accelerate.json`; on `NotFound`, default to `AccelerateConfig { status: "Suspended" }`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** Round-trip in `tests/stub_tests.rs::test_bucket_accelerate`.
  - **Risk:** Low.
- [x] PutBucketAccelerateConfiguration (planned 2026-05-13)
  - **Goal:** Accept `<AccelerateConfiguration><Status>Enabled|Suspended</Status></AccelerateConfiguration>` and persist. Returns 200 OK.
  - **Design:** Validate `<Status>` ∈ {Enabled, Suspended}; persist; 200.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** Round-trip in `tests/stub_tests.rs::test_bucket_accelerate`.
  - **Risk:** Low.
- [x] GetBucketOwnershipControls (planned 2026-05-11)
  - **Goal:** Return `OwnershipControls` XML; `OwnershipControlsNotFoundError` (404) if unset; `NoSuchBucket` if bucket absent.
  - **Design:** Call `StorageEngine::get_bucket_ownership_controls`; serialize as `OwnershipControlsXml::from_config(&cfg).to_xml()`. Persisted as `bucket_ownership_controls.json`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** GET before PUT → 404; PUT BucketOwnerEnforced → GET round-trip; DELETE → GET → 404.
  - **Risk:** Low.
- [x] PutBucketOwnershipControls (planned 2026-05-11)
  - **Goal:** Accept `OwnershipControls` XML; validate ObjectOwnership ∈ {BucketOwnerPreferred, ObjectWriter, BucketOwnerEnforced}; persist; return 200 OK.
  - **Design:** Parse via `parse_ownership_controls_xml`; persist via `StorageEngine::put_bucket_ownership_controls`; router body-forwarding fix at `s3_router.rs:992-998`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** PUT BucketOwnerEnforced → GET; invalid ObjectOwnership → 400 MalformedXML.
  - **Risk:** Low.
- [x] DeleteBucketOwnershipControls (planned 2026-05-11)
  - **Goal:** Remove per-bucket `bucket_ownership_controls.json` (idempotent); return 204 No Content.
  - **Design:** Call `StorageEngine::delete_bucket_ownership_controls`; subsequent GET returns `OwnershipControlsNotFoundError`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** DELETE then GET → 404; idempotent DELETE.
  - **Risk:** Low.
- [x] GetPublicAccessBlock (planned 2026-05-11)
  - **Goal:** Return `PublicAccessBlockConfiguration` XML; `NoSuchPublicAccessBlockConfiguration` (404) if unset; `NoSuchBucket` if bucket absent.
  - **Design:** Call `StorageEngine::get_bucket_public_access_block`; serialize as `PublicAccessBlockConfigurationXml::from_config(&cfg).to_xml()`. Persisted as `bucket_public_access_block.json`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** GET before PUT → 404; PUT all-true → GET round-trip; DELETE → GET → 404.
  - **Risk:** Low.
- [x] PutPublicAccessBlock (planned 2026-05-11)
  - **Goal:** Accept `PublicAccessBlockConfiguration` XML; validate four boolean fields (default false if absent per AWS); persist; return 200 OK.
  - **Design:** Parse via `parse_public_access_block_xml`; persist via `StorageEngine::put_bucket_public_access_block`; router body-forwarding fix at `s3_router.rs:1000-1006`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/s3_router.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** PUT all-true → GET returns all-true; PUT empty → GET returns all-false.
  - **Risk:** Low.
- [x] DeletePublicAccessBlock (planned 2026-05-11)
  - **Goal:** Remove per-bucket `bucket_public_access_block.json` (idempotent); return 204 No Content.
  - **Design:** Call `StorageEngine::delete_bucket_public_access_block`; subsequent GET returns `NoSuchPublicAccessBlockConfiguration`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`.
  - **Tests:** DELETE then GET → 404; idempotent DELETE.
  - **Risk:** Low.
- [x] GetBucketIntelligentTieringConfiguration (planned 2026-05-13)
  - **Goal:** Return a single IntelligentTiering configuration by `id` query parameter as `IntelligentTieringConfiguration` XML; 404 `NoSuchConfiguration` when not found.
  - **Design:** Read JSON from `<bucket>/bucket_intelligent_tiering/<sanitized_id>.json`; serialize as `IntelligentTieringConfigXml`. Router must forward `query.id` to handler.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`, `src/api/s3_router.rs`.
  - **Tests:** Round-trip in `tests/stub_tests.rs::test_bucket_intelligent_tiering`.
  - **Risk:** Low — id-keyed subdirectory same as Metrics pattern.
- [x] PutBucketIntelligentTieringConfiguration (planned 2026-05-13)
  - **Goal:** Accept and persist an IntelligentTiering configuration keyed by `id`. Validates `<Status>` ∈ {Enabled, Disabled} and `<Tiering><Days>` ≥ 90. Returns 200 OK.
  - **Design:** Parse body → extract id from `<Id>` tag → persist to subdirectory. Router forwards body + id.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** Round-trip in `tests/stub_tests.rs::test_bucket_intelligent_tiering`.
  - **Risk:** Low.
- [x] DeleteBucketIntelligentTieringConfiguration (planned 2026-05-13)
  - **Goal:** Remove a single IntelligentTiering configuration by `id`. Idempotent; 204.
  - **Design:** `storage.delete_bucket_intelligent_tiering(bucket, id)` → 204. Router forwards id.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** DELETE→GET→404 in `tests/stub_tests.rs::test_bucket_intelligent_tiering`.
  - **Risk:** Low.
- [x] GetObjectLockConfiguration (planned 2026-05-13)
  - **Goal:** Return bucket-level Object Lock configuration as `ObjectLockConfiguration` XML; emit `ObjectLockConfigurationNotFoundError` (404) when unset or when bucket was not created with `x-amz-bucket-object-lock-enabled: true`.
  - **Design:** Read JSON from `<bucket>/bucket_object_lock.json` via `StorageEngine::get_bucket_object_lock`; render via `ObjectLockConfigurationXml`. On `NotFound` → 404 `ObjectLockConfigurationNotFoundError`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Prerequisites:** Bucket creation flag (`BucketMetadata.object_lock_enabled`) in `src/storage/core/types.rs`.
  - **Tests:** `test_object_lock_bucket_level` in `tests/stub_tests.rs`.
  - **Risk:** Medium — depends on create_bucket flag being wired first.
- [x] PutObjectLockConfiguration (planned 2026-05-13)
  - **Goal:** Set or update the default-retention rule for a bucket. Requires bucket to have been created with Object Lock enabled; returns 409 `InvalidBucketState` otherwise. Returns 200 OK on success.
  - **Design:** Check `BucketMetadata.object_lock_enabled`; if false → 409. Parse `<ObjectLockConfiguration>` body → validate mode ∈ {GOVERNANCE, COMPLIANCE} and days/years presence → `storage.put_bucket_object_lock`. Router body-forwarding.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`, `src/storage/core/types.rs`.
  - **Tests:** `test_object_lock_bucket_level` in `tests/stub_tests.rs`.
  - **Risk:** Medium — see GetObjectLockConfiguration prerequisites.
- [x] GetBucketMetricsConfiguration (planned 2026-05-13)
  - **Goal:** Return a single Metrics configuration by `id` as `MetricsConfiguration` XML; 404 `NoSuchConfiguration` when not found.
  - **Design:** Read JSON from `<bucket>/bucket_metrics/<sanitized_id>.json`. Router extracts `id` from `BucketGetQueryParams.id` (already present) and forwards it.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] PutBucketMetricsConfiguration (planned 2026-05-13)
  - **Goal:** Accept and persist a Metrics configuration by `id`. Returns 200 OK.
  - **Design:** Add `id` to `BucketPutQueryParams`; collect body → parse → `storage.put_bucket_metrics(bucket, id, cfg)`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] DeleteBucketMetricsConfiguration (planned 2026-05-13)
  - **Goal:** Delete a single Metrics configuration by `id`. Idempotent; 204.
  - **Design:** Add `id` to `BucketDeleteQueryParams`; `storage.delete_bucket_metrics(bucket, id)` → 204.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] ListBucketMetricsConfigurations (planned 2026-05-13)
  - **Goal:** Return all Metrics configurations for a bucket as `ListMetricsConfigurationsResult` XML. Empty list is valid (200).
  - **Design:** `storage.list_bucket_metrics(bucket)` reads `bucket_metrics/*.json`; serialize as list XML. Router: no id → list.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] GetBucketAnalyticsConfiguration (planned 2026-05-13)
  - **Goal:** Return a single Analytics configuration by `id` as `AnalyticsConfiguration` XML; 404 `NoSuchConfiguration` when not found.
  - **Design:** Read JSON from `<bucket>/bucket_analytics/<sanitized_id>.json`; serialize via `AnalyticsConfigXml`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] PutBucketAnalyticsConfiguration (planned 2026-05-13)
  - **Goal:** Accept and persist an Analytics configuration by `id`. Returns 200 OK.
  - **Design:** Add `id` to `BucketPutQueryParams`; parse body → `storage.put_bucket_analytics(bucket, id, cfg)`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] DeleteBucketAnalyticsConfiguration (planned 2026-05-13)
  - **Goal:** Delete a single Analytics configuration by `id`. Idempotent; 204.
  - **Design:** Add `id` to `BucketDeleteQueryParams`; `storage.delete_bucket_analytics(bucket, id)` → 204.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] ListBucketAnalyticsConfigurations (planned 2026-05-13)
  - **Goal:** Return all Analytics configurations as `ListAnalyticsConfigurationsResult` XML. Empty list → 200.
  - **Design:** `storage.list_bucket_analytics(bucket)` reads `bucket_analytics/*.json`; serialize.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] GetBucketInventoryConfiguration (planned 2026-05-13)
  - **Goal:** Return a single Inventory configuration by `id` as `InventoryConfiguration` XML; 404 `NoSuchConfiguration` when not found.
  - **Design:** Read JSON from `<bucket>/bucket_inventory/<sanitized_id>.json`; serialize via `InventoryConfigXml`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] PutBucketInventoryConfiguration (planned 2026-05-13)
  - **Goal:** Accept and persist an Inventory configuration by `id`. Validates `schedule_frequency` ∈ {Daily, Weekly} and `included_object_versions` ∈ {All, Current}. Returns 200 OK.
  - **Design:** Add `id` to `BucketPutQueryParams`; parse body → `storage.put_bucket_inventory(bucket, id, cfg)`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/api/utils.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] DeleteBucketInventoryConfiguration (planned 2026-05-13)
  - **Goal:** Delete a single Inventory configuration by `id`. Idempotent; 204.
  - **Design:** Add `id` to `BucketDeleteQueryParams`; `storage.delete_bucket_inventory(bucket, id)` → 204.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.
- [x] ListBucketInventoryConfigurations (planned 2026-05-13)
  - **Goal:** Return all Inventory configurations as `ListInventoryConfigurationsResult` XML. Empty list → 200.
  - **Design:** `storage.list_bucket_inventory(bucket)` reads `bucket_inventory/*.json`; serialize.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/core/bucket_config.rs`, `src/api/xml_responses.rs`.
  - **Tests:** `test_metrics_analytics_inventory_id_keyed` in `tests/stub_tests.rs`.
  - **Risk:** Low.

### Object operations
- [x] ListObjectsV1
- [x] ListObjectsV2
- [x] HeadObject
- [x] GetObject
- [x] PutObject
- [x] DeleteObject
- [x] DeleteObjects
- [x] CopyObject
- [x] GetObjectAttributes
- [x] PostObject (multipart/form-data)
- [x] Range requests
- [x] Conditional headers (If-Match/If-None-Match/etc)
- [x] GetObjectAcl
  - **Goal:** Real storage read of `<bucket>/acl/<sanitized_key>.json`; fallback to FULL_CONTROL default if absent.
  - **Design:** `StorageEngine::get_object_acl(bucket, key)` sidecar path mirrors `object_lock.rs` pattern. `head_object` existence check first.
  - **Files:** `src/storage/core/bucket_acl.rs`, `src/api/handlers/functions/functions_4.rs`
  - **Tests:** `tests/stub_tests_acl.rs::test_object_acl_default`, `test_object_acl_canned_public_read`, `test_object_acl_explicit_xml`
  - **Risk:** Key sanitization must be consistent between put and get paths.
- [x] PutObjectAcl
  - **Goal:** Parse `x-amz-acl` header OR XML body; persist per-object ACL sidecar.
  - **Design:** Same precedence logic as PutBucketAcl. Sidecar path `<bucket>/acl/<sanitize_key(key)>.json`. Router at L529–535 currently discards body — switch to body-forwarding. Handler gains `HeaderMap` + `Bytes` params.
  - **Files:** `src/storage/core/bucket_acl.rs`, `src/api/utils.rs`, `src/api/handlers/functions/functions_4.rs`, `src/api/s3_router.rs`
  - **Tests:** `test_object_acl_canned_public_read`, `test_object_acl_explicit_xml`, `test_object_acl_conflict`, `test_object_acl_missing`
  - **Risk:** `put_object_acl` currently no-ops; router body discard means body was never received. Both must be fixed simultaneously.
- [x] ListObjectVersions (planned 2026-05-11)
  - **Goal:** Return real version IDs and delete markers using the versioning infrastructure; for non-versioned buckets fall through to synthetic version_id="null" behavior.
  - **Design:** Call `state.storage.versioning_manager().list_all_versions(&bucket, prefix, max_keys)`; map `is_delete_marker==true` entries to `DeleteMarkerEntry`, others to `ObjectVersion` with real `version_id`. Honor `version_id_marker` and `key_marker` query params for pagination. If bucket is `Unversioned`, fall through to current `list_objects` + synthesize-null-version path. Located in `src/api/handlers/functions/select_parser.rs:264-323`.
  - **Files:** `src/api/handlers/functions/select_parser.rs`.
  - **Tests:** PUT Enabled, PUT two objects, ListObjectVersions → see two versions with real IDs; DELETE one → see delete marker; non-versioned bucket → see version_id="null" (backward compat).
  - **Risk:** Moderate — pagination semantics (version_id_marker + key_marker) must be implemented correctly; backward-compat fallback for unversioned buckets is essential.
- [x] RestoreObject
  - **Goal:** Parse `<RestoreRequest>` body; call real `ArchivalManager::restore_object`; return 202 (new restore) or 200 (already active).
  - **Design:** `ArchivalManager` is in-memory only (not in StorageEngine). Prerequisites: add `archive_manager: Arc<RwLock<ArchivalManager>>` to `StorageEngine`; add `archive_object(bucket, key, size)` + `get_archival_status(bucket, key)` StorageEngine methods; wire PutObject with `x-amz-storage-class: GLACIER|DEEP_ARCHIVE` to call archive_object. Router at L651–658 discards body — fix to collect and forward `Bytes`. Handler in `functions_2.rs` rewritten: parse `RestoreRequest{days, tier}`, peek archival status, dispatch to `archive_manager.write().restore_object()`. 409 `InvalidObjectState` when object not archived.
  - **Files:** `src/storage/core/types.rs`, `src/storage/archival.rs` (read-only; no changes), `src/api/utils.rs`, `src/api/handlers/functions/functions_2.rs`, `src/api/s3_router.rs`, `src/api/handlers/functions/functions_1.rs` (PutObject GLACIER path)
  - **Tests:** `tests/stub_tests_restore.rs::test_restore_object_not_archived`, `test_restore_request_invalid_xml`, `test_restore_tier_invalid`, `test_restore_object_archived` (PUT with GLACIER → POST restore → 202)
  - **Risk:** `types.rs` is at 1965 lines — adding ArchivalManager field is ~5 LoC, watch threshold.
- [x] GetObjectLegalHold (planned 2026-05-13)
  - **Goal:** Return per-version legal-hold status as `<LegalHold><Status>ON|OFF</Status></LegalHold>` XML. Returns 404 when no legal-hold metadata exists for the specified version.
  - **Design:** Resolve version (from `?versionId` or latest); read `<bucket>/object_lock/<sanitized_key>/<version_id>.json` via `ObjectLockManager::get`; render via `LegalHoldXml`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/object_lock.rs` (new), `src/api/xml_responses.rs`, `src/api/s3_router.rs`.
  - **Prerequisites:** `ObjectLockManager` in `src/storage/object_lock.rs`; `versionId` in `ObjectQueryParams`.
  - **Tests:** `test_object_lock_legal_hold_blocks_delete` in `tests/stub_tests.rs`.
  - **Risk:** Medium — new sidecar storage pattern.
- [x] PutObjectLegalHold (planned 2026-05-13)
  - **Goal:** Set or clear legal hold on a specific object version. `<Status>ON</Status>` freezes deletion; `OFF` releases it. Returns 200 OK.
  - **Design:** Parse `<LegalHold><Status>ON|OFF</Status></LegalHold>` → `ObjectLockManager::put_legal_hold(bucket, key, version_id, status)`. Writes/updates `<bucket>/object_lock/<key>/<version_id>.json`.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/object_lock.rs` (new), `src/api/utils.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_object_lock_legal_hold_blocks_delete` in `tests/stub_tests.rs`.
  - **Risk:** Medium — delete enforcement in `delete_object` depends on this.
- [x] GetObjectRetention (planned 2026-05-13)
  - **Goal:** Return per-version retention metadata as `<Retention><Mode>…</Mode><RetainUntilDate>…</RetainUntilDate></Retention>` XML. 404 when no retention set.
  - **Design:** `ObjectLockManager::get(bucket, key, version_id)` → render via `RetentionXml`. Date in ISO-8601 UTC.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/object_lock.rs` (new), `src/api/xml_responses.rs`, `src/api/s3_router.rs`.
  - **Tests:** `test_object_lock_governance_with_bypass` + `test_object_lock_compliance_immutable` in `tests/stub_tests.rs`.
  - **Risk:** Medium.
- [x] PutObjectRetention (planned 2026-05-13)
  - **Goal:** Set or extend object retention. COMPLIANCE retention cannot be shortened even with bypass. GOVERNANCE retention requires `x-amz-bypass-governance-retention: true` to override. Returns 200 OK.
  - **Design:** Parse body → validate mode + date → `ObjectLockManager::put_retention(bucket, key, version_id, mode, until, bypass)`. If COMPLIANCE + shortening → 403 AccessDenied. If GOVERNANCE override without bypass → 403.
  - **Files:** `src/api/bucket_stubs.rs`, `src/storage/object_lock.rs` (new), `src/api/utils.rs`, `src/api/s3_router.rs`.
  - **Prerequisites:** `delete_object` in `core.rs` must consult `ObjectLockManager::is_protected`.
  - **Tests:** `test_object_lock_governance_with_bypass` + `test_object_lock_compliance_immutable` in `tests/stub_tests.rs`.
  - **Risk:** Medium — enforcement semantics must be exactly correct.
- [x] SelectObjectContent
- [x] GetObjectTorrent
  - **Goal:** Intentionally 501 — deprecated and removed by AWS in 2025. BitTorrent retrieval is not part of the current S3 API.
  - **Design:** Handler already returns 501 `NotImplemented`. Updated message: "GetObjectTorrent was deprecated by AWS and is no longer part of the S3 API. rs3gw does not support BitTorrent retrieval."
  - **Files:** `src/api/bucket_stubs.rs`
  - **Tests:** None — 501 is the terminal state; existing smoke suite verifies it doesn't panic.
  - **Risk:** None — AWS removed this endpoint. Marking [x] prevents future /ultra from attempting implementation.
- [x] WriteGetObjectResponse
  - **Goal:** Intentionally 501 — Lambda Object Lambda only. Only valid when invoked from inside an S3 Object Lambda function; rs3gw is an S3 gateway, not Object Lambda.
  - **Design:** Handler already returns 501 `NotImplemented`. Updated message: "WriteGetObjectResponse is only valid when invoked from inside an S3 Object Lambda function. rs3gw is an S3 gateway, not Object Lambda."
  - **Files:** `src/api/bucket_stubs.rs`
  - **Tests:** None — 501 is the terminal state.
  - **Risk:** None. Marking [x] prevents future /ultra from attempting implementation.

### Multipart upload
- [x] CreateMultipartUpload
- [x] UploadPart
- [x] UploadPartCopy
- [x] CompleteMultipartUpload
- [x] AbortMultipartUpload
- [x] ListParts
- [x] ListMultipartUploads

### Protocol correctness / edge cases to verify
- [x] XML response formatting matches AWS expectations (root tags, namespaces)
- [x] Error codes and HTTP status codes match AWS for common failures
- [x] UTF-8 / URL encoding for keys and query parameters
- [x] Pagination: marker/continuation-token semantics
- [x] MaxKeys handling and truncation flags
- [x] ETag semantics for single-part vs multipart
- [x] Checksum headers behavior (when present)
- [x] Content-Type propagation and overrides
- [x] User-defined metadata (x-amz-meta-*) persistence
- [x] Content-Disposition / caching headers passthrough
- [x] HEAD vs GET parity for metadata
- [x] Pre-signed URL request verification
- [x] Chunked transfer encoding behavior
- [x] Large object streaming backpressure
- [x] Parallel delete bounded concurrency settings
- [x] CopyObject metadata directive behavior

## Post-release hardening backlog

### Correctness
- [x] Add golden-file tests for XML serialization (bucket/object errors)
- [x] Add integration tests for conditional GET/HEAD (If-* headers)
- [x] Add tests for Range requests (single, multiple, invalid ranges)
- [x] Verify behavior for empty keys and trailing slashes
- [x] Verify behavior for very long keys and deep prefixes
- [x] Verify behavior for keys containing spaces and reserved characters
- [x] Verify behavior for Unicode keys
- [x] Verify HEAD responses include consistent headers
- [x] Verify DeleteObjects partial failure response body
- [x] Verify CopyObject with x-amz-metadata-directive (COPY/REPLACE)
- [x] Verify server-side copy across buckets
- [x] Verify multipart: out-of-order part uploads
- [x] Verify multipart: repeated part upload overwrites part
- [x] Verify multipart: invalid part numbers
- [x] Verify multipart: Abort removes parts
- [x] Verify ListParts ordering and truncation
- [x] Verify ListMultipartUploads filters

### Storage
- [x] Crash-safety: ensure metadata + data writes are consistent
- [x] Atomic rename strategy for uploads
- [x] Validate fsync strategy for durability (document tradeoffs)
- [x] Garbage collection for orphaned multipart parts
- [x] Ensure sidecar metadata format is versioned
- [x] Filesystem permission errors produce correct S3 errors
- [x] Protect against path traversal attempts
- [x] Handle low-disk-space failures cleanly
- [x] Benchmark compression thresholds and defaults — Done: Session 8 (2026-06-13)
  - `benches/compression_benchmarks.rs::bench_compression_threshold` sweeps object sizes
    (512 B–8 KiB) comparing `none` vs `zstd:1` vs `lz4`; `bench_compression_ratio` sweeps entropy.
    Default is `CompressionMode::None` (off), applied globally when enabled (no per-object size
    gate). Guidance documented in `docs/performance_tuning.md` ("Object-Size Considerations"):
    objects ≲ 1 KiB rarely benefit; `zstd:1`–`zstd:3` is the sweet spot for 1 KiB–1 MiB text/JSON.
- [x] Document which data is compressed and when

### Security
- [x] SigV4: validate canonical request edge cases
- [x] SigV4: query-param ordering and encoding
- [x] SigV4: chunked streaming signatures
- [x] Rate limit authentication failures to avoid brute-force
- [x] TLS: document cert/key configuration and rotation
- [x] Audit security-sensitive config in README

### Observability
- [x] Ensure per-operation metrics labels are stable
- [x] Add exemplars/tracing IDs to latency histograms (if enabled) — Done: Session 9 (2026-06-13)
  - `metrics_layer` captures the active OpenTelemetry `trace_id` (it runs inside `TraceLayer`'s
    instrumented span, so the trace context is current) and records a latency exemplar per request
    into a bounded per-operation ring (`src/metrics.rs`). Exposed at `GET /metrics/exemplars` (JSON:
    operation, latency_ms, status, trace_id, timestamp). Since `metrics-exporter-prometheus` 0.18
    cannot embed OpenMetrics exemplars inline in the `/metrics` text, they live in this side store;
    `trace_id` is empty when no sampled trace is active, so the latency samples are useful even with
    tracing disabled and populate trace IDs automatically when it is enabled.
- [x] Document Prometheus scrape configuration
- [x] Document OpenTelemetry env vars (OTEL_*)
- [x] Add a small Grafana dashboard starter (if not already present)

### Operations
- [x] Document production sizing guidance (CPU, disk, network)
- [x] Document data directory layout
- [x] Ensure graceful shutdown waits for in-flight uploads
- [x] Add health probe details for Kubernetes
- [x] Add readiness vs liveness semantics

### DX
- [x] Add minimal local dev workflow section
- [x] Add troubleshooting section (common misconfigurations)
- [x] Add a short API compatibility note (what is stubbed)
- [x] Add examples for boto3 and aws-cli
- [x] Add performance benchmark how-to

### Bucket API test matrix
- [x] ListBuckets: success path returns expected XML fields
- [x] ListBuckets: missing bucket returns correct error code
- [x] ListBuckets: auth required (when enabled)
- [x] ListBuckets: metrics include operation label
- [x] CreateBucket: success path returns expected XML fields
- [x] CreateBucket: missing bucket returns correct error code
- [x] CreateBucket: auth required (when enabled)
- [x] CreateBucket: metrics include operation label
- [x] DeleteBucket: success path returns expected XML fields
- [x] DeleteBucket: missing bucket returns correct error code
- [x] DeleteBucket: auth required (when enabled)
- [x] DeleteBucket: metrics include operation label
- [x] HeadBucket: success path returns expected XML fields
- [x] HeadBucket: missing bucket returns correct error code
- [x] HeadBucket: auth required (when enabled)
- [x] HeadBucket: metrics include operation label
- [x] GetBucketLocation: success path returns expected XML fields
- [x] GetBucketLocation: missing bucket returns correct error code
- [x] GetBucketLocation: auth required (when enabled)
- [x] GetBucketLocation: metrics include operation label
- [x] GetBucketTagging: success path returns expected XML fields
- [x] GetBucketTagging: missing bucket returns correct error code
- [x] GetBucketTagging: auth required (when enabled)
- [x] GetBucketTagging: metrics include operation label
- [x] PutBucketTagging: success path returns expected XML fields
- [x] PutBucketTagging: missing bucket returns correct error code
- [x] PutBucketTagging: auth required (when enabled)
- [x] PutBucketTagging: metrics include operation label
- [x] DeleteBucketTagging: success path returns expected XML fields
- [x] DeleteBucketTagging: missing bucket returns correct error code
- [x] DeleteBucketTagging: auth required (when enabled)
- [x] DeleteBucketTagging: metrics include operation label
- [x] GetBucketPolicy: success path returns expected XML fields
- [x] GetBucketPolicy: missing bucket returns correct error code
- [x] GetBucketPolicy: auth required (when enabled)
- [x] GetBucketPolicy: metrics include operation label
- [x] PutBucketPolicy: success path returns expected XML fields
- [x] PutBucketPolicy: missing bucket returns correct error code
- [x] PutBucketPolicy: auth required (when enabled)
- [x] PutBucketPolicy: metrics include operation label
- [x] DeleteBucketPolicy: success path returns expected XML fields
- [x] DeleteBucketPolicy: missing bucket returns correct error code
- [x] DeleteBucketPolicy: auth required (when enabled)
- [x] DeleteBucketPolicy: metrics include operation label

### Object API test matrix
- [x] PutObject: large object streaming (>= 1 GiB) does not OOM
- [x] PutObject: works with keys containing spaces
- [x] PutObject: works with deep prefixes
- [x] PutObject: returns stable request IDs (if implemented)
- [x] GetObject: large object streaming (>= 1 GiB) does not OOM
- [x] GetObject: works with keys containing spaces
- [x] GetObject: works with deep prefixes
- [x] GetObject: returns stable request IDs (if implemented)
- [x] HeadObject: large object streaming (>= 1 GiB) does not OOM
- [x] HeadObject: works with keys containing spaces
- [x] HeadObject: works with deep prefixes
- [x] HeadObject: returns stable request IDs (if implemented)
- [x] DeleteObject: large object streaming (>= 1 GiB) does not OOM
- [x] DeleteObject: works with keys containing spaces
- [x] DeleteObject: works with deep prefixes
- [x] DeleteObject: returns stable request IDs (if implemented)
- [x] DeleteObjects: large object streaming (>= 1 GiB) does not OOM
- [x] DeleteObjects: works with keys containing spaces
- [x] DeleteObjects: works with deep prefixes
- [x] DeleteObjects: returns stable request IDs (if implemented)
- [x] CopyObject: large object streaming (>= 1 GiB) does not OOM
- [x] CopyObject: works with keys containing spaces
- [x] CopyObject: works with deep prefixes
- [x] CopyObject: returns stable request IDs (if implemented)
- [x] ListObjectsV1: large object streaming (>= 1 GiB) does not OOM
- [x] ListObjectsV1: works with keys containing spaces
- [x] ListObjectsV1: works with deep prefixes
- [x] ListObjectsV1: returns stable request IDs (if implemented)
- [x] ListObjectsV2: large object streaming (>= 1 GiB) does not OOM
- [x] ListObjectsV2: works with keys containing spaces
- [x] ListObjectsV2: works with deep prefixes
- [x] ListObjectsV2: returns stable request IDs (if implemented)

### Multipart API test matrix
- [x] CreateMultipartUpload: error response matches AWS shape
- [x] CreateMultipartUpload: supports concurrent clients
- [x] CreateMultipartUpload: respects request timeout
- [x] UploadPart: error response matches AWS shape
- [x] UploadPart: supports concurrent clients
- [x] UploadPart: respects request timeout
- [x] UploadPartCopy: error response matches AWS shape
- [x] UploadPartCopy: supports concurrent clients
- [x] UploadPartCopy: respects request timeout
- [x] CompleteMultipartUpload: error response matches AWS shape
- [x] CompleteMultipartUpload: supports concurrent clients
- [x] CompleteMultipartUpload: respects request timeout
- [x] AbortMultipartUpload: error response matches AWS shape
- [x] AbortMultipartUpload: supports concurrent clients
- [x] AbortMultipartUpload: respects request timeout
- [x] ListParts: error response matches AWS shape
- [x] ListParts: supports concurrent clients
- [x] ListParts: respects request timeout
- [x] ListMultipartUploads: error response matches AWS shape
- [x] ListMultipartUploads: supports concurrent clients
- [x] ListMultipartUploads: respects request timeout

## gRPC (optional for v0.1.0, but tracked here)

See src/grpc/README.md for protocol details and default ports.

- [x] Configuration via RS3GW_GRPC_ENABLED / RS3GW_GRPC_PORT
- [x] TLS support via RS3GW_GRPC_TLS_CERT / RS3GW_GRPC_TLS_KEY
- [x] BucketService basic operations
- [x] ObjectService streaming correctness
- [x] Multipart service parity with REST
- [x] Integration tests covering gRPC endpoints

## Cluster mode (future / optional)

See src/cluster/README.md for env vars and topology notes.

- [x] ClusterConfig parsing and validation
- [x] Gossip membership convergence
- [x] Replication mode: async
- [x] Replication mode: sync
- [x] Replication mode: quorum
- [x] Failure handling and rebalancing
- [x] Conflict resolution semantics (define/document)
- [x] Metrics for replication lag and failures

## Roadmap

### v0.2.2 (Usability + completeness) -- CURRENT
- [x] Clarify and document all stubbed S3 APIs
- [x] Improve error messages and compatibility codes
- [x] Add `rs3ctl` workflows (if present) for common admin tasks
- [x] Improve docs for local dev + docker compose
- [x] Add more integration tests for AWS SDKs

### v0.3 (Performance + stability)
- [x] Benchmarks: publish baseline numbers and how to reproduce
- [x] Profile typical workloads (small objects, large objects, mixed)
- [x] Optimize hot paths identified by profiling
- [x] Backpressure and timeout tuning guidance
- [x] Add soak tests (long-running) — Done: Session 9 (2026-06-13)
      `tests/soak_tests.rs`: a configurable-duration concurrent mixed-workload (PUT/HEAD/GET/DELETE)
      stability harness. Defaults short for CI; `RS3GW_SOAK_DURATION_SECS` / `RS3GW_SOAK_CONCURRENCY`
      extend it to multi-hour runs. Asserts zero errors, no catastrophic latency drift between run
      halves, and the server stays responsive afterwards.

### v0.4 (Advanced storage features)
- [x] Deduplication: document tradeoffs and minimum object size
- [x] Select cache: validate TTL and memory caps
- [x] Quota: enforcement semantics and errors
- [x] Throttling: per-client and global behavior

### v0.5 (Observability depth)
- [x] Improve tracing spans and attributes for S3 operations
- [x] Ensure Prometheus metrics stability guarantees
- [x] Add alerting recommendations (SLO-based)
- [x] Add cost/usage reporting hooks — Done: Session 9 (2026-06-13)
      `src/observability/usage.rs` (`UsageTracker`): per-bucket transfer + request-by-operation
      counters with an estimated cost breakdown from a configurable `PricingConfig`, and a pluggable
      `UsageHook` trait (built-in `LoggingUsageHook`). Wired into the PUT/GET/DELETE data paths;
      exposed at `GET /api/usage` and `GET /api/usage/{bucket}` (`?flush=true` fires export hooks).

## Detailed backlog (prioritized, concrete)

### REST API
- [x] Audit request routing for ambiguous paths
- [x] Ensure all handlers set Content-Length correctly where applicable
- [x] Ensure streaming responses use correct chunking
- [x] Add request ID header propagation (if desired)
- [x] Ensure XML error bodies are always valid XML
- [x] Harden query parsing against invalid encodings
- [x] Add negative tests for malformed XML inputs
- [x] Verify time skew tolerance for SigV4
- [x] Document supported regions/LocationConstraint behavior

### Storage engine
- [x] Document on-disk layout (data, metadata, multipart temp)
- [x] Add periodic cleanup for abandoned uploads
- [x] Add config option for multipart temp retention
- [x] Consider checksum validation on read
- [x] Ensure metadata read/write is lock-safe
- [x] Validate behavior under concurrent reads/writes
- [x] Add fsync toggle for performance vs durability

### Auth
- [x] Explicitly define unauthenticated mode semantics
- [x] Ensure auth errors do not leak sensitive details
- [x] Add tests for unsigned payload (UNSIGNED-PAYLOAD)
- [x] Add tests for streaming signed payloads (chunked)
- [x] Document canonical header requirements

### Metrics
- [x] Confirm histogram buckets are appropriate for expected latencies
- [x] Add metrics for object size distribution
- [x] Add metrics for cache hit/miss
- [x] Add metrics for dedup savings
- [x] Add metrics for compression ratio

### Testing
- [x] Add aws-cli based smoke tests to CI
- [x] Add boto3 integration tests for pagination
- [x] Add regression tests for previously fixed bugs
- [x] Add fuzzing targets for XML parsing (optional) — Done: Session 8 (2026-06-13)
  - `tests/xml_parser_fuzz.rs`: a portable, deterministic (seeded xorshift; no `cargo-fuzz`/nightly)
    robustness harness feeding random XML soup, byte-mutated valid templates, and a curated
    offset/boundary corpus to all nine `api::utils` request parsers, asserting none ever panic
    (~405k invocations/run). Locks the no-panic property of the hand-rolled, byte-slicing parsers
    against malformed network input.

### Docs
- [x] Document all environment variables (one table)
- [x] Document config precedence: env overrides TOML
- [x] Document upgrade notes for on-disk format changes
- [x] Document known limitations

## S3 compatibility deep-dive (covered by the test suites below)

The 457 numbered "deep-dive #NNN" placeholders that previously occupied this
section (`Verify XML shape for bucket operation #001`…`#079`,
`Verify headers/metadata semantics for object operation #NNN`,
`Verify multipart edge case #NNN`, `Verify error code mapping #NNN`) were
auto-generated stubs with no concrete per-item content. They are superseded by
— and redundant with — the real, passing compatibility test suites, which
exercise these surfaces against the AWS SDK and golden fixtures rather than
against numbered placeholders:

- [x] Bucket-operation XML shapes → `tests/xml_golden_tests.rs`, `tests/bucket_tests.rs`,
      `tests/aws_sdk_compat_tests.rs` (+ `tests/stub_tests_acl.rs`, `tests/stub_tests_cors.rs`,
      `tests/versioning_tests.rs`, `tests/stub_tests_object_lock.rs`, `tests/stub_tests_configs.rs`)
- [x] Object header / metadata semantics → `tests/object_tests.rs`, `tests/object_tests_extended.rs`,
      `tests/protocol_tests.rs`, `tests/stub_tests_sse.rs`, `tests/aws_sdk_compat_tests_extended.rs`
- [x] Multipart edge cases → `tests/multipart_tests.rs`, `tests/multipart_extended_tests.rs`
      (out-of-order parts, re-upload, invalid part numbers, abort, ListParts/ListMultipartUploads)
- [x] Error-code mapping → `tests/xml_golden_tests.rs` (error XML bodies), `tests/protocol_tests.rs`,
      plus the error paths asserted throughout `tests/bucket_tests.rs` and `tests/object_tests.rs`

Future compatibility gaps should be filed as concrete, named tasks (which
request, expected XML/headers/status code) — not numbered placeholders. The
"Bucket/Object/Multipart API test matrix" sections above enumerate the specific
behaviors currently verified.

### Session 5 (2026-05-14)
- [x] Phase 0: splitrs `src/storage/core/types.rs` (blocker; 1998→directory module)
  - **Goal:** Split the 1998-line file to stay under 2000-line ceiling
  - **Files:** `src/storage/core/types/` (directory module with base_types, types_3, types_4, etc.)
  - **Status:** Complete — 937/937 tests pass, largest file 1570 lines
- [x] SSE-S3 sidecar struct + storage I/O
  - **Goal:** Per-object SSE sidecar with atomic write, re-encrypt extension points for Session 6
  - **Files:** `src/storage/core/sse.rs` (new)
- [x] AppState wires EncryptionService
  - **Files:** `src/lib.rs`
- [x] `resolve_sse` helper
  - **Files:** `src/api/sse.rs` (new)
- [x] PutObject honors `x-amz-server-side-encryption: AES256`
  - **Files:** `src/api/handlers/functions/functions_3.rs`
- [x] PutObject inherits bucket-default encryption from `bucket_encryption.json`
  - **Files:** `src/api/handlers/functions/functions_3.rs` (via resolve_sse)
- [x] PutObject returns 501 NotImplemented for `aws:kms` / `aws:kms:dsse`
  - **Files:** `src/api/sse.rs`
- [x] GetObject / HeadObject emit `x-amz-server-side-encryption: AES256` + decrypt body
  - **Files:** `src/api/handlers/functions/select_parser.rs`
- [x] GetObject range request on SSE: full-decrypt-then-slice
  - **Files:** `src/api/handlers/functions/select_parser.rs`; TODO(session-6): chunked AEAD
- [x] `checksum_validation` interop with SSE (skip validation for SSE objects)
  - **Files:** `src/storage/core/types/types_3.rs`
- [x] CopyObject SSE 4-way matrix
  - **Files:** `src/api/handlers/functions/functions_4.rs`
- [x] CORS preflight OPTIONS handler with rule matcher
  - **Files:** `src/api/cors.rs` (new), `src/api/s3_router.rs`
- [x] Simple-request CORS response headers via tower middleware
  - **Files:** `src/api/cors_middleware.rs` (new), `src/main.rs`
- [x] tests/stub_tests_sse.rs
- [x] tests/stub_tests_cors.rs

### Session 6 (deferred)
- [x] SSE-C (customer-provided key) request path
- [x] SSE-KMS envelope encryption with KMS key resolution
- [x] Streaming AEAD for seekable ranged GET on SSE objects
- [x] CreateMultipartUpload + UploadPart SSE inheritance
- [x] Persistent KEK storage (replace process-local LocalKeyProvider)
- [x] bucket-owner-read / bucket-owner-full-control object-ACL grant expansion
