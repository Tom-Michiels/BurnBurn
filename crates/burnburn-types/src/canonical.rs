//! Canonical byte projection for content hashes (ADR 0004).
//!
//! Hashes are never computed over `serde_json` output, `Debug` or map iteration order. Every
//! encoded value is explicitly tagged and length-prefixed, so different structures cannot
//! produce the same bytes. Hashing itself (SHA-256) lives in the crates that own the content.
//!
//! Encoding (all integers little-endian):
//! - `tag(t)`: `u32` byte length, then the UTF-8 bytes.
//! - `u64(v)`: 8 bytes.
//! - `f64(v)`: IEEE-754 bits as 8 bytes; `-0.0` is written as `0.0`; non-finite values are an error.
//! - `bytes(b)`: `u64` length, then the bytes.
//! - `seq(items)`: `u64` item count, then each item's encoding.

use std::fmt;

/// Types with a canonical byte projection.
pub trait CanonicalEncode {
    fn encode(&self, out: &mut CanonicalWriter);
}

impl<T: CanonicalEncode + ?Sized> CanonicalEncode for &T {
    fn encode(&self, out: &mut CanonicalWriter) {
        (**self).encode(out);
    }
}

/// A NaN or infinity reached the canonical encoder. Domain types make this unreachable;
/// it is reported instead of hashed so a corrupt value can never get a stable hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonFiniteValue;

impl fmt::Display for NonFiniteValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("non-finite value in canonical encoding")
    }
}

impl std::error::Error for NonFiniteValue {}

#[derive(Debug, Default)]
pub struct CanonicalWriter {
    buf: Vec<u8>,
    non_finite: bool,
}

impl CanonicalWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tag(&mut self, t: &str) {
        // Tags are short identifiers; saturate rather than wrap for absurd lengths.
        let len = u32::try_from(t.len()).unwrap_or(u32::MAX);
        self.buf.extend_from_slice(&len.to_le_bytes());
        self.buf.extend_from_slice(t.as_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        if !v.is_finite() {
            self.non_finite = true;
            return;
        }
        let v = if v == 0.0 { 0.0 } else { v };
        self.buf.extend_from_slice(&v.to_bits().to_le_bytes());
    }

    pub fn bool(&mut self, v: bool) {
        self.u64(u64::from(v));
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.u64(b.len() as u64);
        self.buf.extend_from_slice(b);
    }

    pub fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }

    pub fn seq<T: CanonicalEncode>(&mut self, items: impl ExactSizeIterator<Item = T>) {
        self.u64(items.len() as u64);
        for item in items {
            item.encode(self);
        }
    }

    /// The canonical bytes, or an error if any non-finite float was written.
    pub fn finish(self) -> Result<Vec<u8>, NonFiniteValue> {
        if self.non_finite { Err(NonFiniteValue) } else { Ok(self.buf) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Pair(u64, f64);
    impl CanonicalEncode for Pair {
        fn encode(&self, out: &mut CanonicalWriter) {
            out.u64(self.0);
            out.f64(self.1);
        }
    }

    #[test]
    fn canonical_writer_bytes_match_hand_encoding() {
        let mut w = CanonicalWriter::new();
        w.tag("ab");
        w.u64(1);
        w.f64(-0.0);
        w.bytes(&[7]);
        w.seq([Pair(2, 1.0)].iter());
        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            2, 0, 0, 0, b'a', b'b',                 // tag: u32 len 2 + "ab"
            1, 0, 0, 0, 0, 0, 0, 0,                 // u64 1
            0, 0, 0, 0, 0, 0, 0, 0,                 // -0.0 normalised to +0.0 bits
            1, 0, 0, 0, 0, 0, 0, 0, 7,              // bytes: u64 len 1 + 0x07
            1, 0, 0, 0, 0, 0, 0, 0,                 // seq: 1 item
            2, 0, 0, 0, 0, 0, 0, 0,                 //   u64 2
            0, 0, 0, 0, 0, 0, 0xf0, 0x3f,           //   1.0 = 0x3ff0000000000000
        ];
        assert_eq!(w.finish().unwrap(), expected);

        let mut bad = CanonicalWriter::new();
        bad.f64(f64::NAN);
        assert_eq!(bad.finish(), Err(NonFiniteValue));
    }
}
