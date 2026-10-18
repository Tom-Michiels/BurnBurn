//! 7-bit payload fields.
//!
//! Every payload byte carries 7 bits (high bit zero); multi-byte values are big-endian groups of 7
//! bits. Values are range-checked before packing so masking can never silently wrap a value.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldError {
    /// The value does not fit the field's confirmed range.
    OutOfRange { field: &'static str, value: i128, min: i128, max: i128 },
    /// A payload byte had its high bit set (that byte would be read as an opcode).
    HighBitInPayload { byte: u8 },
    /// Wrong number of payload bytes for the field.
    WrongWidth { expected: usize, actual: usize },
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldError::OutOfRange { field, value, min, max } => {
                write!(f, "{field} value {value} outside {min}..={max}")
            }
            FieldError::HighBitInPayload { byte } => write!(f, "payload byte {byte:#04x} has its high bit set"),
            FieldError::WrongWidth { expected, actual } => {
                write!(f, "field needs {expected} payload bytes, got {actual}")
            }
        }
    }
}

impl std::error::Error for FieldError {}

/// Widest supported field (9 × 7 = 63 bits fits `u64`).
const MAX_WIDTH: usize = 9;

/// Unsigned big-endian 7-bit groups → value.
pub fn unpack7(bytes: &[u8]) -> Result<u64, FieldError> {
    if bytes.len() > MAX_WIDTH {
        return Err(FieldError::WrongWidth { expected: MAX_WIDTH, actual: bytes.len() });
    }
    bytes.iter().try_fold(0u64, |acc, &b| {
        if b & 0x80 != 0 { Err(FieldError::HighBitInPayload { byte: b }) } else { Ok((acc << 7) | u64::from(b)) }
    })
}

/// Signed (two's complement over `7 * len` bits) 7-bit groups → value.
pub fn unpack7_signed(bytes: &[u8]) -> Result<i64, FieldError> {
    let value = unpack7(bytes)?;
    let bits = 7 * bytes.len();
    if bits == 0 {
        return Ok(0);
    }
    // bits <= 63, so both the value and the two's-complement span fit i128 without overflow.
    let value = i128::from(value);
    let signed = if value & (1i128 << (bits - 1)) != 0 { value - (1i128 << bits) } else { value };
    i64::try_from(signed).map_err(|_| FieldError::OutOfRange {
        field: "signed 7-bit field",
        value: signed,
        min: i64::MIN.into(),
        max: i64::MAX.into(),
    })
}

/// Value → `width` big-endian 7-bit groups. Rejects values that do not fit.
pub fn pack7(value: u64, width: usize) -> Result<Vec<u8>, FieldError> {
    if width == 0 || width > MAX_WIDTH {
        return Err(FieldError::WrongWidth { expected: MAX_WIDTH, actual: width });
    }
    let max = (1u64 << (7 * width)) - 1;
    check_range("7-bit field", value.into(), 0, max.into())?;
    Ok((0..width).rev().map(|i| ((value >> (7 * i)) & 0x7f) as u8).collect())
}

fn check_range(field: &'static str, value: i128, min: i128, max: i128) -> Result<(), FieldError> {
    if value < min || value > max { Err(FieldError::OutOfRange { field, value, min, max }) } else { Ok(()) }
}

fn exact_width(bytes: &[u8], expected: usize) -> Result<(), FieldError> {
    if bytes.len() == expected { Ok(()) } else { Err(FieldError::WrongWidth { expected, actual: bytes.len() }) }
}

const U35_MAX: u64 = (1 << 35) - 1;

/// Power command value on the 14-bit scale (0..=16383). The percent mapping is deliberately not
/// part of this type: LightBurn wrote 3277 for 20 %, which no single formula is proven for yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Power(u16);

impl Power {
    pub const MAX_RAW: u16 = 16383;

    pub fn from_raw(raw: u16) -> Result<Self, FieldError> {
        check_range("power raw14", raw.into(), 0, Self::MAX_RAW.into())?;
        Ok(Power(raw))
    }

    pub fn raw(self) -> u16 {
        self.0
    }

    pub(crate) fn from_payload(bytes: &[u8]) -> Result<Self, FieldError> {
        exact_width(bytes, 2)?;
        let v = unpack7(bytes)?;
        Power::from_raw(u16::try_from(v).unwrap_or(u16::MAX))
    }

    pub(crate) fn to_payload(self) -> Result<Vec<u8>, FieldError> {
        pack7(self.0.into(), 2)
    }
}

/// Relative coordinate in µm: signed14, −8192..=8191.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelCoord(i16);

impl RelCoord {
    pub const MIN_UM: i32 = -8192;
    pub const MAX_UM: i32 = 8191;

    pub fn from_um(um: i32) -> Result<Self, FieldError> {
        check_range("relative coordinate µm", um.into(), Self::MIN_UM.into(), Self::MAX_UM.into())?;
        Ok(RelCoord(i16::try_from(um).unwrap_or(0)))
    }

    pub fn um(self) -> i32 {
        self.0.into()
    }

    pub(crate) fn from_payload(bytes: &[u8]) -> Result<Self, FieldError> {
        exact_width(bytes, 2)?;
        let v = unpack7_signed(bytes)?;
        RelCoord::from_um(i32::try_from(v).unwrap_or(i32::MAX))
    }

    pub(crate) fn to_payload(self) -> Result<Vec<u8>, FieldError> {
        let bits = i64::from(self.0) & 0x3fff;
        pack7(bits.unsigned_abs(), 2)
    }
}

/// Absolute coordinate in µm, five 7-bit bytes read unsigned (0..=2^35−1).
///
/// Only positive values are confirmed by the corpus. Whether a set 35th bit means a negative
/// physical coordinate is OPEN; this type never interprets a sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AbsCoord(u64);

impl AbsCoord {
    pub const MAX_UM: u64 = U35_MAX;

    pub fn from_um(um: u64) -> Result<Self, FieldError> {
        check_range("absolute coordinate µm", um.into(), 0, Self::MAX_UM.into())?;
        Ok(AbsCoord(um))
    }

    pub fn um(self) -> u64 {
        self.0
    }

    pub(crate) fn from_payload(bytes: &[u8]) -> Result<Self, FieldError> {
        exact_width(bytes, 5)?;
        AbsCoord::from_um(unpack7(bytes)?)
    }

    pub(crate) fn to_payload(self) -> Result<Vec<u8>, FieldError> {
        pack7(self.0, 5)
    }
}

/// Speed in µm/s (UI mm/s × 1000), five 7-bit bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Speed(u64);

impl Speed {
    pub const MAX_UM_PER_S: u64 = U35_MAX;

    pub fn from_um_per_s(v: u64) -> Result<Self, FieldError> {
        check_range("speed µm/s", v.into(), 0, Self::MAX_UM_PER_S.into())?;
        Ok(Speed(v))
    }

    pub fn um_per_s(self) -> u64 {
        self.0
    }

    pub(crate) fn from_payload(bytes: &[u8]) -> Result<Self, FieldError> {
        exact_width(bytes, 5)?;
        Speed::from_um_per_s(unpack7(bytes)?)
    }

    pub(crate) fn to_payload(self) -> Result<Vec<u8>, FieldError> {
        pack7(self.0, 5)
    }
}

/// Raw five-byte unsigned value whose unit is not (yet) confirmed: delays, colours, file sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct U35(u64);

impl U35 {
    pub const MAX: u64 = U35_MAX;

    pub fn new(v: u64) -> Result<Self, FieldError> {
        check_range("u35", v.into(), 0, Self::MAX.into())?;
        Ok(U35(v))
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub(crate) fn from_payload(bytes: &[u8]) -> Result<Self, FieldError> {
        exact_width(bytes, 5)?;
        U35::new(unpack7(bytes)?)
    }

    pub(crate) fn to_payload(self) -> Result<Vec<u8>, FieldError> {
        pack7(self.0, 5)
    }
}

/// One 7-bit payload byte (part index, mask, mode, array index): 0..=127.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Byte7(u8);

impl Byte7 {
    pub fn new(v: u8) -> Result<Self, FieldError> {
        check_range("7-bit byte", v.into(), 0, 127)?;
        Ok(Byte7(v))
    }

    pub fn get(self) -> u8 {
        self.0
    }

    pub(crate) fn from_payload(bytes: &[u8]) -> Result<Self, FieldError> {
        exact_width(bytes, 1)?;
        let v = unpack7(bytes)?;
        Byte7::new(u8::try_from(v).unwrap_or(u8::MAX))
    }

    pub(crate) fn to_payload(self) -> Vec<u8> {
        vec![self.0]
    }
}
