//! Typed commands → unscrambled bytes.

use std::fmt;

use crate::command::{Command, Kind, spec_for};
use crate::field::{AbsCoord, FieldError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    Field(FieldError),
    /// A verbatim (`Registered`/`Unknown`) record that is not a valid opcode + 7-bit payload.
    InvalidVerbatimRecord,
    /// `FileSum`/`EndOfFile` inside a body passed to `encode_rd_file`: the footer is computed.
    ReservedCommandInBody,
    /// File sum does not fit its five-byte field.
    FileSumOverflow {
        sum: u64,
    },
}

impl From<FieldError> for EncodeError {
    fn from(e: FieldError) -> Self {
        EncodeError::Field(e)
    }
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EncodeError {}

/// Append one command's unscrambled bytes to `out`. On error `out` is left unchanged.
pub fn encode_command(command: &Command, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let (kind, payload) = match command {
        Command::Registered { bytes, .. } | Command::Unknown { bytes, .. } => {
            let declared = match command {
                Command::Unknown { opcode, .. } => Some(*opcode),
                _ => None,
            };
            let valid = matches!(bytes.split_first(), Some((op, rest))
                if op & 0x80 != 0 && declared.is_none_or(|d| d == *op) && rest.iter().all(|b| b & 0x80 == 0));
            if !valid {
                return Err(EncodeError::InvalidVerbatimRecord);
            }
            out.extend_from_slice(bytes);
            return Ok(());
        }
        typed => kind_and_payload(typed)?,
    };
    // Every `Kind` has a spec; a missing one would be a table bug, reported rather than panicking.
    let spec = spec_for(kind).ok_or(EncodeError::InvalidVerbatimRecord)?;
    out.extend_from_slice(spec.prefix);
    out.extend_from_slice(&payload);
    Ok(())
}

fn xy(x: AbsCoord, y: AbsCoord) -> Result<Vec<u8>, FieldError> {
    let mut v = x.to_payload()?;
    v.extend(y.to_payload()?);
    Ok(v)
}

fn cat(parts: [Vec<u8>; 2]) -> Vec<u8> {
    parts.concat()
}

fn kind_and_payload(c: &Command) -> Result<(Kind, Vec<u8>), FieldError> {
    Ok(match *c {
        Command::MoveAbs { x, y } => (Kind::MoveAbs, xy(x, y)?),
        Command::CutAbs { x, y } => (Kind::CutAbs, xy(x, y)?),
        Command::MoveRel { dx, dy } => (Kind::MoveRel, cat([dx.to_payload()?, dy.to_payload()?])),
        Command::CutRel { dx, dy } => (Kind::CutRel, cat([dx.to_payload()?, dy.to_payload()?])),
        Command::MoveRelX(d) => (Kind::MoveRelX, d.to_payload()?),
        Command::MoveRelY(d) => (Kind::MoveRelY, d.to_payload()?),
        Command::CutRelX(d) => (Kind::CutRelX, d.to_payload()?),
        Command::CutRelY(d) => (Kind::CutRelY, d.to_payload()?),
        Command::ImmediatePower { slot, value } => (Kind::ImmediatePower(slot), value.to_payload()?),
        Command::LaserPower { laser, bound, value } => (Kind::LaserPower(laser, bound), value.to_payload()?),
        Command::PartPower { laser, bound, part, value } => {
            (Kind::PartPower(laser, bound), cat([part.to_payload(), value.to_payload()?]))
        }
        Command::ThroughPower { laser, value } => (Kind::ThroughPower(laser), value.to_payload()?),
        Command::LaserDelay { edge, raw } => (Kind::Delay(edge), raw.to_payload()?),
        Command::Speed(s) => (Kind::Speed, s.to_payload()?),
        Command::PartSpeed { part, speed } => (Kind::PartSpeed, cat([part.to_payload(), speed.to_payload()?])),
        Command::Mode(flag) => (Kind::Mode(flag), Vec::new()),
        Command::LayerNumberPart(p) => (Kind::LayerNumberPart, p.to_payload()),
        Command::LaserSelect(m) => (Kind::LaserSelect, m.to_payload()),
        Command::LayerColorPart { part, color } => {
            (Kind::LayerColorPart, cat([part.to_payload(), color.to_payload()?]))
        }
        Command::WorkModePart { part, mode } => (Kind::WorkModePart, cat([part.to_payload(), mode.to_payload()])),
        Command::MaxLayerPart(p) => (Kind::MaxLayerPart, p.to_payload()),
        Command::Bounds { kind, x, y } => (Kind::Bounds(kind), xy(x, y)?),
        Command::PartBounds { kind, part, x, y } => (Kind::PartBounds(kind), cat([part.to_payload(), xy(x, y)?])),
        Command::BlockEnd => (Kind::BlockEnd, Vec::new()),
        Command::ArrayStart(i) => (Kind::ArrayStart, i.to_payload()),
        Command::ArrayEnd => (Kind::ArrayEnd, Vec::new()),
        Command::SetAbsolute => (Kind::SetAbsolute, Vec::new()),
        Command::Process(a) => (Kind::Process(a), Vec::new()),
        Command::Reference(r) => (Kind::Reference(r), Vec::new()),
        Command::FileSum(s) => (Kind::FileSum, s.to_payload()?),
        Command::EndOfFile => (Kind::EndOfFile, Vec::new()),
        Command::Reply(r) => (Kind::Reply(r), Vec::new()),
        // Handled verbatim by the caller.
        Command::Registered { .. } | Command::Unknown { .. } => (Kind::EndOfFile, Vec::new()),
    })
}

pub fn encode(commands: &[Command]) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    for c in commands {
        encode_command(c, &mut out)?;
    }
    Ok(out)
}
