use std::fmt;

/// A SHA-256 digest over a canonical projection. Every BurnBurn hash is computed by
/// [`ContentHash::of`] with a [`HashDomain`]; the crates own only their payload layout.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

/// Why a string is not a content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashParseError {
    /// Not exactly 64 characters.
    Length,
    /// A character other than `0-9` / `a-f` (lowercase only, so the text form is canonical).
    NotLowerHex,
}

impl fmt::Display for HashParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashParseError::Length => f.write_str("content hash must be 64 hex characters"),
            HashParseError::NotLowerHex => f.write_str("content hash must be lowercase hexadecimal"),
        }
    }
}

impl std::error::Error for HashParseError {}

/// The closed set of hash domains (DR-17). Each has a fixed tag string (ADR 0004); two domains
/// can never produce the same digest for the same payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HashDomain {
    Asset,
    Project,
    Plan,
    Rd,
    SourcePixels,
    TraceKey,
    EvalSketch,
    EvalText,
    /// Inputs of a modifier chain (IM21).
    EvalModifier,
    /// Inputs of a part instance (IM21).
    EvalPart,
    /// Inputs of a generator (IM22, C-5).
    EvalGenerator,
    /// CAM compile source (C-5).
    PlanSource,
    /// A compiled job (IM17, C-5).
    CompiledJob,
    Profile,
    HistoryRecord,
}

impl HashDomain {
    /// The tag written (length-prefixed, like `CanonicalWriter::tag`) after the version prefix.
    pub fn tag(self) -> &'static str {
        match self {
            HashDomain::Asset => "asset",
            HashDomain::Project => "project",
            HashDomain::Plan => "plan",
            HashDomain::Rd => "rd",
            HashDomain::SourcePixels => "source-pixels",
            HashDomain::TraceKey => "trace-key",
            HashDomain::EvalSketch => "eval-sketch",
            HashDomain::EvalText => "eval-text",
            HashDomain::EvalModifier => "eval-modifier",
            HashDomain::EvalPart => "eval-part",
            HashDomain::EvalGenerator => "eval-generator",
            HashDomain::PlanSource => "plan-source",
            HashDomain::CompiledJob => "compiled-job",
            HashDomain::Profile => "profile",
            HashDomain::HistoryRecord => "history-record",
        }
    }
}

impl ContentHash {
    /// `SHA-256("burnburn-v1\0" ‖ u32le(len(tag)) ‖ tag ‖ payload)`.
    pub fn of(domain: HashDomain, payload: &[u8]) -> ContentHash {
        use sha2::{Digest, Sha256};
        let tag = domain.tag().as_bytes();
        // Tags are short constants; the length always fits.
        let len = u32::try_from(tag.len()).unwrap_or(u32::MAX);
        let mut h = Sha256::new();
        h.update(b"burnburn-v1\0");
        h.update(len.to_le_bytes());
        h.update(tag);
        h.update(payload);
        ContentHash(h.finalize().into())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ContentHash(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase, 64 characters.
    pub fn to_hex(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(64);
        for b in self.0 {
            out.push(char::from(DIGITS[usize::from(b >> 4)]));
            out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
        }
        out
    }

    pub fn from_hex(s: &str) -> Result<Self, HashParseError> {
        fn nibble(c: u8) -> Result<u8, HashParseError> {
            match c {
                b'0'..=b'9' => Ok(c - b'0'),
                b'a'..=b'f' => Ok(c - b'a' + 10),
                _ => Err(HashParseError::NotLowerHex),
            }
        }
        let bytes = s.as_bytes();
        if bytes.len() != 64 {
            return Err(HashParseError::Length);
        }
        let mut out = [0u8; 32];
        let (pairs, _) = bytes.as_chunks::<2>();
        for (slot, &[hi, lo]) in out.iter_mut().zip(pairs) {
            *slot = (nibble(hi)? << 4) | nibble(lo)?;
        }
        Ok(ContentHash(out))
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_hex_roundtrips_and_rejects_bad_input() {
        // SHA-256("") from FIPS 180-2 test vectors; here only the text form is under test.
        let text = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let h = ContentHash::from_hex(text).unwrap();
        assert_eq!(h.as_bytes()[0], 0xe3);
        assert_eq!(h.as_bytes()[31], 0x55);
        assert_eq!(h.to_hex(), text);
        assert_eq!(ContentHash::from_hex(&text[..62]), Err(HashParseError::Length));
        assert_eq!(ContentHash::from_hex(&text.to_uppercase()), Err(HashParseError::NotLowerHex));
        assert_eq!(ContentHash::from_hex(&format!("{}zz", &text[..62])), Err(HashParseError::NotLowerHex));
    }
}
