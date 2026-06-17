#![cfg(feature = "server")]
//! Robustness ("fuzz") harness for the hand-rolled XML request parsers.
//!
//! The S3 request-body parsers in `rs3gw::api::utils` are hand-written and do
//! byte-offset slicing after `str::find` (e.g. `&rest[start + 12..]`). That
//! style is fast but panic-prone on malformed input, and these parsers sit
//! directly on the network edge — a client controls every byte. A panic here
//! would abort the worker (the release profile uses `panic = "abort"`), so the
//! parsers MUST treat every possible input as untrusted and never panic.
//!
//! This is a portable, deterministic fuzz target (no nightly / `cargo-fuzz`
//! needed): a seeded xorshift PRNG drives three input strategies — (1) random
//! "XML-ish" soup built from tag fragments, entities, multibyte code points,
//! control chars and quotes; (2) byte-level mutations of otherwise-valid
//! request templates; (3) a curated corpus of known offset/boundary "panic
//! bait". Every generated input is fed to every public parser; the only assertion is
//! "it returned" (i.e. did not panic). A failure prints the exact offending
//! input and parser so the case can be promoted into the curated corpus.
//!
//! Determinism (fixed seeds) means a regression reproduces identically on every
//! run — there is no flakiness and no dependency on an external RNG crate.

use std::panic::{catch_unwind, AssertUnwindSafe};

use rs3gw::api::utils::{
    parse_complete_multipart_parts, parse_cors_xml, parse_delete_objects_xml, parse_encryption_xml,
    parse_http_date, parse_lifecycle_xml, parse_tagging_xml, parse_versioning_xml,
    parse_website_xml,
};

/// Minimal deterministic PRNG (xorshift64*). Not cryptographic — we only need a
/// reproducible stream of bytes to drive input generation, so no `rand` /
/// SciRS2 RNG dependency is pulled into the test target.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // Avoid the zero state, which is a fixed point for xorshift.
        Self {
            state: seed | 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish integer in `[0, n)`. `n == 0` yields 0.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    fn pick<'a, T>(&mut self, choices: &'a [T]) -> &'a T {
        &choices[self.below(choices.len())]
    }
}

/// Building blocks deliberately chosen to exercise the parsers' tag-finding and
/// offset-slicing logic, including the exact tags each parser keys on.
const FRAGMENTS: &[&str] = &[
    "<Part>",
    "</Part>",
    "<PartNumber>",
    "</PartNumber>",
    "<ETag>",
    "</ETag>",
    "<Tag>",
    "</Tag>",
    "<Key>",
    "</Key>",
    "<Value>",
    "</Value>",
    "<Object>",
    "</Object>",
    "<Delete>",
    "</Delete>",
    "<Quiet>",
    "</Quiet>",
    "<Status>",
    "</Status>",
    "<CORSRule>",
    "</CORSRule>",
    "<AllowedMethod>",
    "<AllowedOrigin>",
    "<Rule>",
    "</Rule>",
    "<ID>",
    "<Prefix>",
    "<Expiration>",
    "<Days>",
    "<RoutingRules>",
    "<IndexDocument>",
    "<Suffix>",
    "<ServerSideEncryptionConfiguration>",
    "<SSEAlgorithm>",
    "aws:kms",
    "AES256",
    "&quot;",
    "&amp;",
    "&lt;",
    "&gt;",
    "&",
    "<",
    ">",
    "\"",
    "'",
    "1",
    "0",
    "-1",
    "999999999999999999999999",
    "abc",
    " ",
    "\t",
    "\n",
    "\u{0}",
    "é",
    "🦀",
    "\u{FFFD}",
    "２３",
    "Enabled",
    "Disabled",
];

/// Valid-ish request templates that the mutation strategy chews on.
const TEMPLATES: &[&str] = &[
    "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>\"abc\"</ETag></Part></CompleteMultipartUpload>",
    "<Tagging><TagSet><Tag><Key>k</Key><Value>v</Value></Tag></TagSet></Tagging>",
    "<Delete><Object><Key>a</Key></Object><Object><Key>b</Key></Object><Quiet>true</Quiet></Delete>",
    "<ServerSideEncryptionConfiguration><Rule><ApplyServerSideEncryptionByDefault><SSEAlgorithm>AES256</SSEAlgorithm></ApplyServerSideEncryptionByDefault></Rule></ServerSideEncryptionConfiguration>",
    "<CORSConfiguration><CORSRule><AllowedMethod>GET</AllowedMethod><AllowedOrigin>*</AllowedOrigin></CORSRule></CORSConfiguration>",
    "<VersioningConfiguration><Status>Enabled</Status></VersioningConfiguration>",
    "<LifecycleConfiguration><Rule><ID>r</ID><Status>Enabled</Status><Expiration><Days>30</Days></Expiration></Rule></LifecycleConfiguration>",
    "<WebsiteConfiguration><IndexDocument><Suffix>index.html</Suffix></IndexDocument></WebsiteConfiguration>",
    "Tue, 15 Nov 1994 08:12:31 GMT",
];

/// Strategy 1 — assemble random XML-ish soup from `FRAGMENTS`.
fn gen_soup(rng: &mut XorShift64) -> String {
    let parts = 1 + rng.below(40);
    let mut s = String::new();
    for _ in 0..parts {
        // `pick` yields `&&str` (FRAGMENTS is `&[&str]`); deref once to a `&str`.
        let fragment = *rng.pick(FRAGMENTS);
        s.push_str(fragment);
    }
    // Random truncation can leave a tag dangling at EOF — the classic
    // "found the tag, sliced past the end" trap.
    let keep = rng.below(s.len() + 1);
    truncate_on_char_boundary(&mut s, keep);
    s
}

/// Strategy 2 — byte-mutate a valid template (delete / duplicate / insert /
/// truncate), then hand the corrupted bytes back as a (lossy) UTF-8 string so
/// the `&str` parsers still receive well-formed UTF-8 but broken structure.
fn gen_mutation(rng: &mut XorShift64) -> String {
    let template = *rng.pick(TEMPLATES);
    let mut bytes = template.as_bytes().to_vec();
    let mutations = 1 + rng.below(6);
    for _ in 0..mutations {
        if bytes.is_empty() {
            break;
        }
        match rng.below(5) {
            0 => {
                // Delete a byte.
                let at = rng.below(bytes.len());
                bytes.remove(at);
            }
            1 => {
                // Duplicate a byte run.
                let at = rng.below(bytes.len());
                let b = bytes[at];
                bytes.insert(at, b);
            }
            2 => {
                // Insert an arbitrary byte (incl. `<`, `>`, `&`, NUL, high bytes).
                let at = rng.below(bytes.len() + 1);
                bytes.insert(at, (rng.next_u64() & 0xFF) as u8);
            }
            3 => {
                // Truncate.
                let at = rng.below(bytes.len() + 1);
                bytes.truncate(at);
            }
            _ => {
                // Overwrite with a structural metacharacter.
                let at = rng.below(bytes.len());
                bytes[at] = *rng.pick(&[b'<', b'>', b'&', b'"', b'/', 0u8]);
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Truncate `s` to at most `len` bytes without splitting a UTF-8 code point.
fn truncate_on_char_boundary(s: &mut String, mut len: usize) {
    if len >= s.len() {
        return;
    }
    while len > 0 && !s.is_char_boundary(len) {
        len -= 1;
    }
    s.truncate(len);
}

/// Curated regression corpus: inputs that specifically target the offset
/// arithmetic (tag at EOF, multibyte right after a tag, integer overflow,
/// dangling entities). These run on every invocation regardless of seed.
const CURATED: &[&str] = &[
    "",
    " ",
    "<",
    ">",
    "&",
    "\u{0}",
    "<Part>",
    "</Part>",
    "<Part><PartNumber>",
    "<Part><PartNumber>1",
    "<Part><ETag>",
    "<Part><ETag>\"",
    "<Part><PartNumber>1</PartNumber><ETag>",
    "<Tag><Key>",
    "<Tag><Key>k</Key><Value>",
    "<Object><Key>",
    "<Object><Key>k",
    // Multibyte code point immediately after a tag the parser slices past.
    "<Key>é",
    "<PartNumber>１２３",
    "<ETag>🦀</ETag>",
    "<Value>🦀",
    // u32 overflow in PartNumber (parse::<u32> must fail gracefully).
    "<Part><PartNumber>999999999999999999999999</PartNumber><ETag>\"x\"</ETag></Part>",
    // Dangling / partial entities in an ETag.
    "<Part><PartNumber>1</PartNumber><ETag>&quot;</ETag></Part>",
    "<Part><PartNumber>1</PartNumber><ETag>&amp</ETag></Part>",
    "<Part><PartNumber>1</PartNumber><ETag>&</ETag></Part>",
    // Closing tags with no opener / inverted nesting.
    "</PartNumber></Part>",
    "<ETag></PartNumber>",
    // Deeply repeated opener (no closers) — bounded.
    "<Part><Part><Part><Part><Part><Part><Part><Part>",
];

/// Run every parser on one input inside `catch_unwind`; a panic fails the test
/// with the offending parser name and input so it can be promoted to `CURATED`.
fn assert_all_parsers_survive(input: &str) {
    // (name, call) pairs — each closure ignores the result; we only care that
    // the parser returns rather than panics.
    macro_rules! check {
        ($name:literal, $call:expr) => {{
            let result = catch_unwind(AssertUnwindSafe(|| {
                let _ = $call;
            }));
            assert!(
                result.is_ok(),
                "parser `{}` panicked on input {:?}",
                $name,
                input
            );
        }};
    }

    check!(
        "parse_complete_multipart_parts",
        parse_complete_multipart_parts(input)
    );
    check!("parse_tagging_xml", parse_tagging_xml(input));
    check!("parse_delete_objects_xml", parse_delete_objects_xml(input));
    check!("parse_encryption_xml", parse_encryption_xml(input));
    check!("parse_cors_xml", parse_cors_xml(input));
    check!("parse_versioning_xml", parse_versioning_xml(input));
    check!("parse_lifecycle_xml", parse_lifecycle_xml(input));
    check!("parse_website_xml", parse_website_xml(input));
    check!("parse_http_date", parse_http_date(input));
}

#[test]
fn curated_corpus_never_panics() {
    for input in CURATED {
        assert_all_parsers_survive(input);
    }
}

#[test]
fn random_soup_never_panics() {
    // Several fixed seeds for broader coverage; all deterministic.
    for seed in [0x1234_5678u64, 0xDEAD_BEEF, 0x0BAD_F00D, 1, u64::MAX] {
        let mut rng = XorShift64::new(seed);
        for _ in 0..5_000 {
            let input = gen_soup(&mut rng);
            assert_all_parsers_survive(&input);
        }
    }
}

#[test]
fn template_mutations_never_panic() {
    for seed in [0xCAFE_BABEu64, 0x5EED_1234, 42, 0xFFFF_0000_FFFF_0000] {
        let mut rng = XorShift64::new(seed);
        for _ in 0..5_000 {
            let input = gen_mutation(&mut rng);
            assert_all_parsers_survive(&input);
        }
    }
}
