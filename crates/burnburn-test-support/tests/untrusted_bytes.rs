//! Stable counterpart of the cargo-fuzz targets in `fuzz/` (which need nightly): every parser of
//! untrusted bytes returns a typed result for random and for mutated real inputs, and never
//! panics. `ruida::decode` on random bytes is already covered by
//! `burnburn-ruida/tests/properties.rs::decode_never_panics_on_arbitrary_bytes`.
//!
//! These are invariants over existing code (no red phase); a panic found here gets a minimal
//! regression test in the owner's crate.
#![allow(clippy::unwrap_used)]

use std::sync::OnceLock;

use burnburn_io::decode_container;
use burnburn_io::dto::v1::{Document, Manifest};
use burnburn_ruida::udp::{frame, parse_frame};
use burnburn_ruida::{Magic, decode_rd_file};
use burnburn_test_support::fixtures::read_fixture;
use proptest::prelude::*;

/// A damage pattern applied to a valid input: what a truncated download, a flipped bit or a
/// hand-edited file looks like to the parser.
#[derive(Debug, Clone)]
enum Mutation {
    Flip { at: usize, mask: u8 },
    Set { at: usize, value: u8 },
    Truncate { at: usize },
    Insert { at: usize, bytes: Vec<u8> },
    Remove { at: usize, len: usize },
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (any::<usize>(), 1u8..).prop_map(|(at, mask)| Mutation::Flip { at, mask }),
        (any::<usize>(), any::<u8>()).prop_map(|(at, value)| Mutation::Set { at, value }),
        any::<usize>().prop_map(|at| Mutation::Truncate { at }),
        (any::<usize>(), prop::collection::vec(any::<u8>(), 1..16))
            .prop_map(|(at, bytes)| Mutation::Insert { at, bytes }),
        (any::<usize>(), 1usize..64).prop_map(|(at, len)| Mutation::Remove { at, len }),
    ]
}

fn mutate(mut bytes: Vec<u8>, mutations: &[Mutation]) -> Vec<u8> {
    for m in mutations {
        let n = bytes.len().max(1);
        match m {
            Mutation::Flip { at, mask } => {
                if let Some(b) = bytes.get_mut(at % n) {
                    *b ^= mask;
                }
            }
            Mutation::Set { at, value } => {
                if let Some(b) = bytes.get_mut(at % n) {
                    *b = *value;
                }
            }
            Mutation::Truncate { at } => bytes.truncate(at % n),
            Mutation::Insert { at, bytes: extra } => {
                let at = at % (bytes.len() + 1);
                bytes.splice(at..at, extra.iter().copied());
            }
            Mutation::Remove { at, len } => {
                let at = at % n;
                let end = (at + len).min(bytes.len());
                if at < end {
                    bytes.drain(at..end);
                }
            }
        }
    }
    bytes
}

fn rd_fixtures() -> &'static [Vec<u8>] {
    static RD: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    RD.get_or_init(|| {
        ["rectangle-default.rd", "rectangle-fill.rd", "gray-ramp-grayscale.rd"]
            .iter()
            .map(|f| read_fixture("ruida", f).unwrap())
            .collect()
    })
}

fn project_fixture() -> &'static [u8] {
    static PROJECT: OnceLock<Vec<u8>> = OnceLock::new();
    PROJECT.get_or_init(|| read_fixture("projects", "v1/rectangle-120x80.burnburn").unwrap())
}

/// Bytes that look like JSON: the parser gets past the first character more often than with
/// uniformly random bytes.
fn json_like() -> impl Strategy<Value = String> {
    proptest::string::string_regex(
        r#"[\[\]{}",: 0-9eE.+\-a-z_]{0,256}|\{"next_id":[0-9]{1,20},"objects":\[.{0,64}\],"operations":\[\]\}"#,
    )
    .unwrap()
}

#[test]
fn unmodified_fixtures_still_decode() {
    // Guard for the mutation tests below: they start from inputs the parsers accept.
    for rd in rd_fixtures() {
        decode_rd_file(rd, Magic::FIXTURE_0X88).unwrap();
    }
    decode_container(project_fixture()).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn rd_file_decode_never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..4096), magic in any::<u8>()) {
        let _ = decode_rd_file(&bytes, Magic(magic));
        let _ = decode_rd_file(&bytes, Magic::FIXTURE_0X88);
    }

    #[test]
    fn rd_file_decode_never_panics_on_damaged_fixtures(which in 0usize..3, damage in prop::collection::vec(mutation(), 1..6)) {
        let bytes = mutate(rd_fixtures()[which].clone(), &damage);
        let _ = decode_rd_file(&bytes, Magic::FIXTURE_0X88);
    }

    /// Contract: a datagram is accepted iff its first two bytes are the 16-bit sum of the rest,
    /// and the payload is then exactly the remainder.
    #[test]
    fn udp_parse_frame_accepts_only_its_checksummed_remainder(bytes in prop::collection::vec(any::<u8>(), 0..1600)) {
        match parse_frame(&bytes) {
            Ok(payload) => {
                prop_assert_eq!(payload, &bytes[2..]);
                let sum = bytes[2..].iter().map(|&b| u32::from(b)).sum::<u32>() & 0xffff;
                prop_assert_eq!(u32::from(u16::from_be_bytes([bytes[0], bytes[1]])), sum);
            }
            Err(_) => {
                let well_formed = bytes.len() >= 2
                    && u32::from(u16::from_be_bytes([bytes[0], bytes[1]]))
                        == bytes[2..].iter().map(|&b| u32::from(b)).sum::<u32>() & 0xffff;
                prop_assert!(!well_formed, "a well-formed datagram was rejected");
            }
        }
    }

    #[test]
    fn udp_framed_payload_parses_back(payload in prop::collection::vec(any::<u8>(), 1..1470)) {
        let datagram = frame(&payload).unwrap();
        prop_assert_eq!(parse_frame(&datagram).unwrap(), &payload[..]);
    }

    #[test]
    fn container_decode_never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = decode_container(&bytes);
    }

    #[test]
    fn container_decode_never_panics_on_damaged_fixture(damage in prop::collection::vec(mutation(), 1..6)) {
        let bytes = mutate(project_fixture().to_vec(), &damage);
        let _ = decode_container(&bytes);
    }

    #[test]
    fn dto_json_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1024), text in json_like()) {
        let _ = serde_json::from_slice::<Document>(&bytes);
        let _ = serde_json::from_slice::<Manifest>(&bytes);
        let _ = serde_json::from_str::<Document>(&text);
        let _ = serde_json::from_str::<Manifest>(&text);
    }
}
