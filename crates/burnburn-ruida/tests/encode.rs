//! Encoder contract: known vectors, byte identity with real exports, range errors.

mod common;

use burnburn_ruida::{AbsCoord, Command, EncodeError, Magic, RelCoord, Speed, decode_rd_file, encode, encode_rd_file};
use common::fixture;

#[test]
fn speed_and_relative_move_encode_to_specified_bytes() {
    // 02-binaire-specificatie §2: 100 mm/s → 00 00 06 0d 20; −100 µm → 7f 1c.
    let bytes = encode(&[
        Command::Speed(Speed::from_um_per_s(100_000).unwrap()),
        Command::MoveRelY(RelCoord::from_um(-100).unwrap()),
    ])
    .unwrap();
    assert_eq!(bytes, [0xc9, 0x02, 0x00, 0x00, 0x06, 0x0d, 0x20, 0x8b, 0x7f, 0x1c]);
}

#[test]
fn absolute_travel_encodes_line_fixture_start_point() {
    // Line fixture @0x1bd: `88 00 00 30 77 17 00 00 1b 2c 18` → (801.687, 448.024) mm.
    let bytes =
        encode(&[Command::MoveAbs { x: AbsCoord::from_um(801_687).unwrap(), y: AbsCoord::from_um(448_024).unwrap() }])
            .unwrap();
    assert_eq!(bytes, [0x88, 0x00, 0x00, 0x30, 0x77, 0x17, 0x00, 0x00, 0x1b, 0x2c, 0x18]);
}

#[test]
fn fixture_bodies_reencode_byte_identical_with_computed_footer() {
    // The footer (file sum + D7) and scrambling are computed by the encoder, then compared with
    // the real LightBurn bytes. Unknown/registered records are carried verbatim.
    for name in ["rectangle-default.rd", "rectangle-fill.rd", "gray-ramp-grayscale.rd"] {
        let original = fixture(name);
        let file = decode_rd_file(&original, Magic::FIXTURE_0X88).unwrap();
        let body: Vec<Command> = file.body().iter().map(|r| r.command.clone()).collect();
        let encoded = encode_rd_file(&body, Magic::FIXTURE_0X88).unwrap();
        assert_eq!(encoded.len(), original.len(), "{name}");
        assert!(
            encoded == original,
            "{name}: first difference at {:?}",
            encoded.iter().zip(&original).position(|(a, b)| a != b)
        );
    }
}

#[test]
fn footer_commands_in_body_are_rejected() {
    assert_eq!(encode_rd_file(&[Command::EndOfFile], Magic::FIXTURE_0X88), Err(EncodeError::ReservedCommandInBody));
}

#[test]
fn verbatim_record_must_be_opcode_plus_seven_bit_payload() {
    let bad = [
        Command::Unknown { opcode: 0xfe, bytes: vec![0xfe, 0x80] },
        Command::Unknown { opcode: 0xfe, bytes: vec![0x01] },
        Command::Unknown { opcode: 0xfe, bytes: vec![0xfd] },
        Command::Unknown { opcode: 0xfe, bytes: vec![] },
        Command::Registered { label: "ELEMENT_NAME", bytes: vec![0xf2, 0x02, 0xff] },
    ];
    for c in bad {
        assert_eq!(encode(std::slice::from_ref(&c)), Err(EncodeError::InvalidVerbatimRecord), "{c:?}");
    }
}
