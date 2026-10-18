//! The exact RD bytes of one compiled job revision, split for transport (DR-10).

use std::fmt;

use burnburn_job::JobPlanId;
use burnburn_types::{ContentHash, HashDomain};

/// One immutable RD file for one plan. The hash is computed here from the bytes, so the
/// recorded `rd_hash` can never name other bytes than the ones that are uploaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdArtifact {
    plan: JobPlanId,
    rd_hash: ContentHash,
    chunks: Vec<Vec<u8>>,
    len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactError {
    Empty,
    /// The chunk size must be at least one byte.
    ZeroChunkSize,
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactError::Empty => f.write_str("empty RD file"),
            ArtifactError::ZeroChunkSize => f.write_str("chunk size must be positive"),
        }
    }
}

impl std::error::Error for ArtifactError {}

impl RdArtifact {
    /// `rd_file` is the scrambled `.rd` file exactly as exported; it is split at `max_chunk`
    /// bytes without re-encoding (`03-transport.md`).
    pub fn from_rd_file(plan: JobPlanId, rd_file: &[u8], max_chunk: usize) -> Result<Self, ArtifactError> {
        if max_chunk == 0 {
            return Err(ArtifactError::ZeroChunkSize);
        }
        if rd_file.is_empty() {
            return Err(ArtifactError::Empty);
        }
        let chunks = rd_file.chunks(max_chunk).map(<[u8]>::to_vec).collect();
        Ok(RdArtifact { plan, rd_hash: rd_hash(rd_file), chunks, len: rd_file.len() })
    }

    pub fn plan(&self) -> JobPlanId {
        self.plan
    }

    /// `ContentHash::of(HashDomain::Rd, file bytes)`.
    pub fn rd_hash(&self) -> ContentHash {
        self.rd_hash
    }

    pub fn chunks(&self) -> &[Vec<u8>] {
        &self.chunks
    }

    /// Total file size in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// The RD hash used everywhere (export, history, upload evidence).
pub fn rd_hash(rd_file: &[u8]) -> ContentHash {
    ContentHash::of(HashDomain::Rd, rd_file)
}
