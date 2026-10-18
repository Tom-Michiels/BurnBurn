//! Public contract (DR-17): one domain-separated hash for every crate. The oracle is the framing
//! written out byte by byte, hashed with SHA-256 directly.

use burnburn_types::{ContentHash, HashDomain};
use sha2::{Digest, Sha256};

fn framed(tag: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"burnburn-v1\0");
    h.update((tag.len() as u32).to_le_bytes());
    h.update(tag);
    h.update(payload);
    h.finalize().into()
}

#[test]
fn content_hash_of_uses_versioned_length_prefixed_domain_tags() {
    let payload = b"\x89PNG fake bytes";
    assert_eq!(ContentHash::of(HashDomain::Asset, payload).as_bytes(), &framed(b"asset", payload));
    assert_eq!(ContentHash::of(HashDomain::SourcePixels, payload).as_bytes(), &framed(b"source-pixels", payload));
    assert_eq!(ContentHash::of(HashDomain::TraceKey, b"").as_bytes(), &framed(b"trace-key", b""));
}

#[test]
fn every_domain_gives_a_different_digest_for_the_same_payload() {
    let all = [
        HashDomain::Asset,
        HashDomain::Project,
        HashDomain::Plan,
        HashDomain::Rd,
        HashDomain::SourcePixels,
        HashDomain::TraceKey,
        HashDomain::EvalSketch,
        HashDomain::EvalText,
        HashDomain::EvalModifier,
        HashDomain::EvalPart,
        HashDomain::EvalGenerator,
        HashDomain::PlanSource,
        HashDomain::CompiledJob,
        HashDomain::Profile,
        HashDomain::HistoryRecord,
    ];
    let hashes: std::collections::BTreeSet<ContentHash> = all.iter().map(|d| ContentHash::of(*d, b"x")).collect();
    assert_eq!(hashes.len(), all.len());
}

#[test]
fn phase_c_domains_have_their_fixed_tags() {
    // Tags are part of every stored hash (ADR 0004, C-5); changing one is a format change.
    let tags: Vec<&str> = [
        HashDomain::EvalModifier,
        HashDomain::EvalPart,
        HashDomain::EvalGenerator,
        HashDomain::PlanSource,
        HashDomain::CompiledJob,
    ]
    .iter()
    .map(|d| d.tag())
    .collect();
    assert_eq!(tags, ["eval-modifier", "eval-part", "eval-generator", "plan-source", "compiled-job"]);
    assert_eq!(ContentHash::of(HashDomain::PlanSource, b"").as_bytes(), &framed(b"plan-source", b""));
}
