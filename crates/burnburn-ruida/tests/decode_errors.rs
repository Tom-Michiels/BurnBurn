//! Failure behaviour is part of the decoder contract: truncated, malformed and unknown are distinct.

use burnburn_ruida::{Command, DecodeError, FieldError, MAX_DECODE_INPUT, Malformation, Power, RelCoord, decode};

#[test]
fn truncated_fixed_length_record_at_end_is_truncated_error() {
    // `C9 02` speed needs five payload bytes; only three present.
    assert_eq!(
        decode(&[0xc9, 0x02, 0x00, 0x00, 0x06]),
        Err(DecodeError::Truncated { offset: 0, needed: 7, available: 5 })
    );
    // Input ending inside a two-byte prefix.
    assert_eq!(decode(&[0xeb, 0xc6]), Err(DecodeError::Truncated { offset: 1, needed: 2, available: 1 }));
}

#[test]
fn opcode_inside_payload_is_malformed_not_truncated() {
    // `8B` relative Y needs two payload bytes, but the next opcode arrives after one.
    assert_eq!(
        decode(&[0x8b, 0x7f, 0xd7]),
        Err(DecodeError::Malformed { offset: 0, reason: Malformation::ShortPayload { expected: 2, actual: 1 } })
    );
    // Extra 7-bit bytes after a fixed-length record.
    assert_eq!(
        decode(&[0xc6, 0x01, 0x19, 0x4d, 0x00]),
        Err(DecodeError::Malformed { offset: 0, reason: Malformation::TrailingPayload { extra: 1 } })
    );
}

#[test]
fn payload_before_first_opcode_is_rejected() {
    assert_eq!(decode(&[0x01, 0xd7]), Err(DecodeError::PayloadWithoutOpcode { offset: 0 }));
}

#[test]
fn unknown_opcode_is_preserved_with_its_payload() {
    let recs = decode(&[0xfe, 0x01, 0x02, 0xd7]).unwrap();
    assert_eq!(recs[0].command, Command::Unknown { opcode: 0xfe, bytes: vec![0xfe, 0x01, 0x02] });
    assert_eq!((recs[0].offset, recs[0].len), (0, 3));
    assert_eq!(recs[1].command, Command::EndOfFile);
}

#[test]
fn registry_label_without_typed_grammar_is_kept_verbatim() {
    // `F2 02` ELEMENT_NAME carries a variable payload; it is labelled, not interpreted.
    let recs = decode(&[0xf2, 0x02, 0x05, 0x2a, 0x39]).unwrap();
    assert_eq!(
        recs[0].command,
        Command::Registered { label: "ELEMENT_NAME", bytes: vec![0xf2, 0x02, 0x05, 0x2a, 0x39] }
    );
}

#[test]
fn longest_prefix_wins_over_shorter_registry_entry() {
    // `CA 01 13` is AIR_ASSIST_ON, not an unspecific `CA` record.
    let recs = decode(&[0xca, 0x01, 0x13]).unwrap();
    assert_eq!(recs[0].command, Command::Mode(burnburn_ruida::ModeFlag::AirAssistOn));
}

#[test]
fn signed_relative_extremes_decode() {
    let recs = decode(&[0x8a, 0x40, 0x00, 0xab, 0x3f, 0x7f]).unwrap();
    assert_eq!(recs[0].command, Command::MoveRelX(RelCoord::from_um(-8192).unwrap()));
    assert_eq!(recs[1].command, Command::CutRelY(RelCoord::from_um(8191).unwrap()));
}

#[test]
fn power_field_max_is_accepted() {
    let recs = decode(&[0xc6, 0x02, 0x7f, 0x7f]).unwrap();
    assert!(matches!(recs[0].command, Command::LaserPower { value, .. } if value == Power::from_raw(16383).unwrap()));
}

#[test]
fn oversized_input_is_rejected_before_parsing() {
    let big = vec![0xd7; MAX_DECODE_INPUT + 1];
    assert_eq!(decode(&big), Err(DecodeError::InputTooLarge { len: MAX_DECODE_INPUT + 1, max: MAX_DECODE_INPUT }));
}

#[test]
fn part_byte_above_127_is_out_of_range() {
    let e = burnburn_ruida::Byte7::new(128).unwrap_err();
    assert!(matches!(e, FieldError::OutOfRange { value: 128, max: 127, .. }));
}
