//! Unscrambled record stream → typed records.

use std::fmt;

use crate::command::{Command, Kind, SPECS, Spec};
use crate::field::{AbsCoord, Byte7, FieldError, Power, RelCoord, Speed, U35};
use crate::registry::REGISTRY;

/// Largest input `decode` accepts (bounded work and memory for untrusted bytes).
pub const MAX_DECODE_INPUT: usize = 16 * 1024 * 1024;

/// One decoded record with its provenance in the unscrambled stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub offset: usize,
    pub len: usize,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InputTooLarge {
        len: usize,
        max: usize,
    },
    /// Payload bytes (high bit clear) where an opcode was expected.
    PayloadWithoutOpcode {
        offset: usize,
    },
    /// Input ended inside a known record.
    Truncated {
        offset: usize,
        needed: usize,
        available: usize,
    },
    /// A known record with the wrong payload length or an invalid field.
    Malformed {
        offset: usize,
        reason: Malformation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformation {
    /// An opcode byte appeared before the payload was complete.
    ShortPayload {
        expected: usize,
        actual: usize,
    },
    /// Extra 7-bit bytes after a fixed-length record.
    TrailingPayload {
        extra: usize,
    },
    Field(FieldError),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DecodeError {}

/// Decode an unscrambled RD byte stream. Never panics; unknown records become
/// [`Command::Unknown`].
pub fn decode(raw: &[u8]) -> Result<Vec<Record>, DecodeError> {
    if raw.len() > MAX_DECODE_INPUT {
        return Err(DecodeError::InputTooLarge { len: raw.len(), max: MAX_DECODE_INPUT });
    }
    let mut records = Vec::new();
    let mut offset = 0;
    while let Some(rest) = raw.get(offset..).filter(|r| !r.is_empty()) {
        let (command, len) = decode_one(rest, offset)?;
        records.push(Record { offset, len, command });
        offset += len;
    }
    Ok(records)
}

/// Decode the record at the start of `rest` (which is non-empty). Returns the command and its length.
fn decode_one(rest: &[u8], offset: usize) -> Result<(Command, usize), DecodeError> {
    let opcode = rest.first().copied().unwrap_or(0);
    if opcode & 0x80 == 0 {
        return Err(DecodeError::PayloadWithoutOpcode { offset });
    }
    // The high-bit split: the record runs until the next opcode byte or the end of input.
    let split_len = 1 + rest.iter().skip(1).take_while(|b| **b & 0x80 == 0).count();
    let record = rest.get(..split_len).unwrap_or(rest);
    let at_end = split_len == rest.len();

    if let Some(spec) = longest(SPECS.iter().map(|s| (s.prefix, s)), record) {
        return decode_typed(spec, record, offset, at_end).map(|c| (c, split_len));
    }
    if let Some(label) = longest(REGISTRY.iter().map(|(p, l)| (*p, *l)), record) {
        return Ok((Command::Registered { label, bytes: record.to_vec() }, split_len));
    }
    if at_end {
        // Input ended inside the prefix of a known command (e.g. a lone `C6`).
        let partial = SPECS.iter().map(|s| s.prefix).chain(REGISTRY.iter().map(|(p, _)| *p));
        if let Some(prefix) =
            partial.filter(|p| p.len() > record.len() && p.starts_with(record)).min_by_key(|p| p.len())
        {
            return Err(DecodeError::Truncated { offset, needed: prefix.len(), available: record.len() });
        }
    }
    Ok((Command::Unknown { opcode, bytes: record.to_vec() }, split_len))
}

/// The entry with the longest prefix that `record` starts with.
fn longest<'a, T>(entries: impl Iterator<Item = (&'a [u8], T)>, record: &[u8]) -> Option<T> {
    entries.filter(|(p, _)| record.starts_with(p)).max_by_key(|(p, _)| p.len()).map(|(_, t)| t)
}

fn decode_typed(spec: &Spec, record: &[u8], offset: usize, at_end: bool) -> Result<Command, DecodeError> {
    let needed = spec.prefix.len() + spec.kind.payload_len();
    if record.len() < needed {
        return Err(if at_end {
            DecodeError::Truncated { offset, needed, available: record.len() }
        } else {
            DecodeError::Malformed {
                offset,
                reason: Malformation::ShortPayload {
                    expected: spec.kind.payload_len(),
                    actual: record.len() - spec.prefix.len(),
                },
            }
        });
    }
    if record.len() > needed {
        return Err(DecodeError::Malformed {
            offset,
            reason: Malformation::TrailingPayload { extra: record.len() - needed },
        });
    }
    let payload = record.get(spec.prefix.len()..).unwrap_or(&[]);
    build(spec.kind, payload).map_err(|e| DecodeError::Malformed { offset, reason: Malformation::Field(e) })
}

/// Payload slice `a..b`; the caller has already checked the exact length.
fn at(payload: &[u8], a: usize, b: usize) -> Result<&[u8], FieldError> {
    payload.get(a..b).ok_or(FieldError::WrongWidth { expected: b, actual: payload.len() })
}

fn xy(p: &[u8], from: usize) -> Result<(AbsCoord, AbsCoord), FieldError> {
    Ok((AbsCoord::from_payload(at(p, from, from + 5)?)?, AbsCoord::from_payload(at(p, from + 5, from + 10)?)?))
}

fn build(kind: Kind, p: &[u8]) -> Result<Command, FieldError> {
    let rel = |a| RelCoord::from_payload(at(p, a, a + 2)?);
    let part = || Byte7::from_payload(at(p, 0, 1)?);
    Ok(match kind {
        Kind::MoveAbs => {
            let (x, y) = xy(p, 0)?;
            Command::MoveAbs { x, y }
        }
        Kind::CutAbs => {
            let (x, y) = xy(p, 0)?;
            Command::CutAbs { x, y }
        }
        Kind::MoveRel => Command::MoveRel { dx: rel(0)?, dy: rel(2)? },
        Kind::CutRel => Command::CutRel { dx: rel(0)?, dy: rel(2)? },
        Kind::MoveRelX => Command::MoveRelX(rel(0)?),
        Kind::MoveRelY => Command::MoveRelY(rel(0)?),
        Kind::CutRelX => Command::CutRelX(rel(0)?),
        Kind::CutRelY => Command::CutRelY(rel(0)?),
        Kind::ImmediatePower(slot) => Command::ImmediatePower { slot, value: Power::from_payload(p)? },
        Kind::LaserPower(laser, bound) => Command::LaserPower { laser, bound, value: Power::from_payload(p)? },
        Kind::PartPower(laser, bound) => {
            Command::PartPower { laser, bound, part: part()?, value: Power::from_payload(at(p, 1, 3)?)? }
        }
        Kind::ThroughPower(laser) => Command::ThroughPower { laser, value: Power::from_payload(p)? },
        Kind::Delay(edge) => Command::LaserDelay { edge, raw: U35::from_payload(p)? },
        Kind::Speed => Command::Speed(Speed::from_payload(p)?),
        Kind::PartSpeed => Command::PartSpeed { part: part()?, speed: Speed::from_payload(at(p, 1, 6)?)? },
        Kind::Mode(flag) => Command::Mode(flag),
        Kind::LayerNumberPart => Command::LayerNumberPart(part()?),
        Kind::LaserSelect => Command::LaserSelect(part()?),
        Kind::LayerColorPart => Command::LayerColorPart { part: part()?, color: U35::from_payload(at(p, 1, 6)?)? },
        Kind::WorkModePart => Command::WorkModePart { part: part()?, mode: Byte7::from_payload(at(p, 1, 2)?)? },
        Kind::MaxLayerPart => Command::MaxLayerPart(part()?),
        Kind::Bounds(kind) => {
            let (x, y) = xy(p, 0)?;
            Command::Bounds { kind, x, y }
        }
        Kind::PartBounds(kind) => {
            let (x, y) = xy(p, 1)?;
            Command::PartBounds { kind, part: part()?, x, y }
        }
        Kind::BlockEnd => Command::BlockEnd,
        Kind::ArrayStart => Command::ArrayStart(part()?),
        Kind::ArrayEnd => Command::ArrayEnd,
        Kind::SetAbsolute => Command::SetAbsolute,
        Kind::Process(a) => Command::Process(a),
        Kind::Reference(r) => Command::Reference(r),
        Kind::FileSum => Command::FileSum(U35::from_payload(p)?),
        Kind::EndOfFile => Command::EndOfFile,
        Kind::Reply(r) => Command::Reply(r),
    })
}
