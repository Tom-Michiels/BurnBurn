//! Known vectors from `02-binaire-specificatie.md` §1–2 and `03-transport.md` (reply table).

use burnburn_ruida::field::{FieldError, pack7, unpack7, unpack7_signed};
use burnburn_ruida::{AbsCoord, Magic, Power, RelCoord, Speed, scramble_byte, unscramble_byte};

const M: Magic = Magic::FIXTURE_0X88;

#[test]
fn scramble_matches_specified_byte_examples() {
    // raw → wire at magic 0x88.
    for (raw, wire) in [(0xcc, 0xc6), (0xcd, 0x46), (0xcf, 0x48), (0xce, 0xc8), (0x00, 0x89)] {
        assert_eq!(scramble_byte(raw, M), wire, "raw {raw:#04x}");
        assert_eq!(unscramble_byte(wire, M), raw, "wire {wire:#04x}");
    }
}

#[test]
fn scramble_is_a_bijection_for_every_magic() {
    for magic in 0..=255u8 {
        let mut seen = [false; 256];
        for raw in 0..=255u8 {
            let wire = scramble_byte(raw, Magic(magic));
            assert!(!seen[usize::from(wire)], "magic {magic:#04x} maps two bytes to {wire:#04x}");
            seen[usize::from(wire)] = true;
            assert_eq!(unscramble_byte(wire, Magic(magic)), raw);
        }
    }
}

#[test]
fn seven_bit_fields_match_specified_examples() {
    assert_eq!(pack7(100_000, 5).unwrap(), [0x00, 0x00, 0x06, 0x0d, 0x20]); // 100 mm/s
    assert_eq!(pack7(30_000, 5).unwrap(), [0x00, 0x00, 0x01, 0x6a, 0x30]); // 30 mm
    assert_eq!(pack7(3277, 2).unwrap(), [0x19, 0x4d]); // LightBurn 20 %
    assert_eq!(unpack7(&[0x00, 0x00, 0x01, 0x43, 0x27]).unwrap(), 24_999); // Line file sum
    assert_eq!(unpack7_signed(&[0x7f, 0x1c]).unwrap(), -100);
    assert_eq!(unpack7_signed(&[0x3f, 0x7f]).unwrap(), 8191);
    assert_eq!(unpack7_signed(&[0x40, 0x00]).unwrap(), -8192);
    assert_eq!(unpack7_signed(&[0x7e, 0x38]).unwrap(), -200); // Grayscale X-delta
}

#[test]
fn field_boundaries_are_inclusive_and_overflow_is_a_typed_error() {
    assert_eq!(RelCoord::from_um(-8192).unwrap().um(), -8192);
    assert_eq!(RelCoord::from_um(8191).unwrap().um(), 8191);
    assert!(matches!(RelCoord::from_um(8192), Err(FieldError::OutOfRange { min: -8192, max: 8191, .. })));
    assert!(matches!(RelCoord::from_um(-8193), Err(FieldError::OutOfRange { .. })));

    assert_eq!(Power::from_raw(0).unwrap().raw(), 0);
    assert_eq!(Power::from_raw(16383).unwrap().raw(), 16383);
    assert!(matches!(Power::from_raw(16384), Err(FieldError::OutOfRange { value: 16384, .. })));

    let max35 = (1u64 << 35) - 1;
    assert_eq!(AbsCoord::from_um(max35).unwrap().um(), max35);
    assert!(matches!(AbsCoord::from_um(1 << 35), Err(FieldError::OutOfRange { .. })));
    assert!(matches!(Speed::from_um_per_s(1 << 35), Err(FieldError::OutOfRange { .. })));

    // Packing never masks a too-large value into range.
    assert!(matches!(pack7(1 << 14, 2), Err(FieldError::OutOfRange { .. })));
    assert_eq!(pack7((1 << 14) - 1, 2).unwrap(), [0x7f, 0x7f]);
}

#[test]
fn payload_byte_with_high_bit_is_rejected() {
    assert_eq!(unpack7(&[0x00, 0x80]), Err(FieldError::HighBitInPayload { byte: 0x80 }));
    assert_eq!(unpack7_signed(&[0xff, 0x00]), Err(FieldError::HighBitInPayload { byte: 0xff }));
}
