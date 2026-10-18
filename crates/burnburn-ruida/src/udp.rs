//! OSS-described UDP datagram framing (pure functions, no socket).
//!
//! `checksum_hi, checksum_lo, scrambled_payload...` with `checksum = sum(scrambled_payload) & 0xffff`.
//! Source: public implementations (`03-transport.md`); not confirmed by a LightBurn capture.
//! The checksum bytes are plain big-endian bytes, never scrambled and not 7-bit fields.

use crate::command::ReplyByte;
use crate::scramble::{Magic, unscramble_byte};

/// Conservative payload limit: 1470 + 2 checksum bytes fits a 1500-byte MTU without fragmentation.
pub const MAX_PAYLOAD: usize = 1470;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UdpError {
    EmptyPayload,
    PayloadTooLarge { len: usize, max: usize },
    TooShort { len: usize },
    ChecksumMismatch { declared: u16, computed: u16 },
}

/// Datagram checksum over already-scrambled bytes.
pub fn checksum(scrambled_payload: &[u8]) -> u16 {
    scrambled_payload.iter().fold(0u16, |acc, &b| acc.wrapping_add(u16::from(b)))
}

/// Prefix an already-scrambled payload with its checksum. Never scrambles again.
pub fn frame(scrambled_payload: &[u8]) -> Result<Vec<u8>, UdpError> {
    if scrambled_payload.is_empty() {
        return Err(UdpError::EmptyPayload);
    }
    if scrambled_payload.len() > MAX_PAYLOAD {
        return Err(UdpError::PayloadTooLarge { len: scrambled_payload.len(), max: MAX_PAYLOAD });
    }
    let mut out = Vec::with_capacity(scrambled_payload.len() + 2);
    out.extend_from_slice(&checksum(scrambled_payload).to_be_bytes());
    out.extend_from_slice(scrambled_payload);
    Ok(out)
}

/// Validate a datagram and return its scrambled payload.
pub fn parse_frame(datagram: &[u8]) -> Result<&[u8], UdpError> {
    let [hi, lo, payload @ ..] = datagram else {
        return Err(UdpError::TooShort { len: datagram.len() });
    };
    let declared = u16::from_be_bytes([*hi, *lo]);
    let computed = checksum(payload);
    if declared != computed {
        return Err(UdpError::ChecksumMismatch { declared, computed });
    }
    Ok(payload)
}

/// Normalise a single wire reply byte in exactly one place (wire C6 is ACK, not a power opcode).
pub fn reply_from_wire(wire: u8, magic: Magic) -> Option<ReplyByte> {
    match unscramble_byte(wire, magic) {
        0xcc => Some(ReplyByte::Ack),
        0xcd => Some(ReplyByte::Err),
        0xce => Some(ReplyByte::Enq),
        0xcf => Some(ReplyByte::Nak),
        _ => None,
    }
}
