//! Reversible RD byte scrambling (not encryption).

/// Scrambling key. 0x88 is confirmed for the studied LightBurn exports only; it may be
/// firmware/device specific, so callers configure it explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Magic(pub u8);

impl Magic {
    /// The magic confirmed by the three LightBurn Core 2.1.04 fixtures.
    pub const FIXTURE_0X88: Magic = Magic(0x88);
}

/// Logical (raw) byte → wire byte.
pub fn scramble_byte(raw: u8, magic: Magic) -> u8 {
    (swap07(raw) ^ magic.0).wrapping_add(1)
}

/// Wire byte → logical (raw) byte.
pub fn unscramble_byte(wire: u8, magic: Magic) -> u8 {
    swap07(wire.wrapping_sub(1) ^ magic.0)
}

/// Swap bit 0 and bit 7 (an involution).
fn swap07(b: u8) -> u8 {
    (b & 0x7e) | ((b & 1) << 7) | ((b >> 7) & 1)
}

pub fn scramble(raw: &[u8], magic: Magic) -> Vec<u8> {
    raw.iter().map(|&b| scramble_byte(b, magic)).collect()
}

pub fn unscramble(wire: &[u8], magic: Magic) -> Vec<u8> {
    wire.iter().map(|&b| unscramble_byte(b, magic)).collect()
}
