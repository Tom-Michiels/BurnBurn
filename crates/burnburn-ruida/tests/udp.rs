//! Datagram framing per `03-transport.md` (OSS-described; no socket involved).

use burnburn_ruida::udp::{MAX_PAYLOAD, UdpError, checksum, frame, parse_frame, reply_from_wire};
use burnburn_ruida::{Magic, ReplyByte};

#[test]
fn frame_prefixes_big_endian_sum_of_scrambled_bytes() {
    assert_eq!(frame(&[0xc6, 0x46]).unwrap(), [0x01, 0x0c, 0xc6, 0x46]);
    // 300 × 0xff = 76500 → modulo 65536 = 10964 = 0x2ad4.
    assert_eq!(checksum(&[0xff; 300]), 0x2ad4);
}

#[test]
fn payload_limits_are_enforced() {
    assert_eq!(frame(&[]), Err(UdpError::EmptyPayload));
    assert!(frame(&vec![0u8; MAX_PAYLOAD]).is_ok());
    assert_eq!(frame(&vec![0u8; MAX_PAYLOAD + 1]), Err(UdpError::PayloadTooLarge { len: 1471, max: 1470 }));
}

#[test]
fn parse_frame_checks_checksum_and_length() {
    assert_eq!(parse_frame(&[0x01, 0x0c, 0xc6, 0x46]).unwrap(), &[0xc6, 0x46]);
    assert_eq!(parse_frame(&[0x01]), Err(UdpError::TooShort { len: 1 }));
    assert_eq!(
        parse_frame(&[0x01, 0x0d, 0xc6, 0x46]),
        Err(UdpError::ChecksumMismatch { declared: 0x010d, computed: 0x010c })
    );
}

#[test]
fn wire_reply_bytes_normalise_to_logical_replies() {
    let m = Magic::FIXTURE_0X88;
    assert_eq!(reply_from_wire(0xc6, m), Some(ReplyByte::Ack));
    assert_eq!(reply_from_wire(0x46, m), Some(ReplyByte::Err));
    assert_eq!(reply_from_wire(0x48, m), Some(ReplyByte::Nak));
    assert_eq!(reply_from_wire(0xc8, m), Some(ReplyByte::Enq));
    // Raw CC sent unscrambled is not an ACK on the wire.
    assert_eq!(reply_from_wire(0xcc, m), None);
}
