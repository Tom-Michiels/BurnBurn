//! Whole `.rd` files: scrambling, file sum and end record.

use std::fmt;

use crate::command::Command;
use crate::decode::{DecodeError, Record, decode};
use crate::encode::{EncodeError, encode};
use crate::field::U35;
use crate::scramble::{Magic, scramble, unscramble};

/// A decoded, checksum-verified RD file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdFile {
    /// Unscrambled bytes.
    pub raw: Vec<u8>,
    pub records: Vec<Record>,
    /// The verified file-sum value.
    pub file_sum: u64,
}

impl RdFile {
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.records.iter().map(|r| &r.command)
    }

    /// Body commands: everything except the `FileSum` + `EndOfFile` footer.
    pub fn body(&self) -> &[Record] {
        let n = self.records.len().saturating_sub(2);
        self.records.get(..n).unwrap_or(&[])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    Decode(DecodeError),
    MissingEndOfFile,
    DataAfterEndOfFile { offset: usize },
    MissingFileSum,
    ChecksumMismatch { declared: u64, computed: u64 },
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FileError {}

/// `sum(raw bytes before E5 05) + 0xD7` (RD-confirmed on all three fixtures).
pub fn file_sum(raw_before_sum_record: &[u8]) -> u64 {
    raw_before_sum_record.iter().map(|&b| u64::from(b)).sum::<u64>() + 0xd7
}

pub fn decode_rd_file(scrambled: &[u8], magic: Magic) -> Result<RdFile, FileError> {
    let raw = unscramble(scrambled, magic);
    let records = decode(&raw).map_err(FileError::Decode)?;
    if let Some(eof) = records.iter().position(|r| r.command == Command::EndOfFile) {
        if let Some(after) = records.get(eof + 1) {
            return Err(FileError::DataAfterEndOfFile { offset: after.offset });
        }
    } else {
        return Err(FileError::MissingEndOfFile);
    }
    let sum_record = records.len().checked_sub(2).and_then(|i| records.get(i)).ok_or(FileError::MissingFileSum)?;
    let Command::FileSum(declared) = sum_record.command else {
        return Err(FileError::MissingFileSum);
    };
    let computed = file_sum(raw.get(..sum_record.offset).unwrap_or(&[]));
    if declared.get() != computed {
        return Err(FileError::ChecksumMismatch { declared: declared.get(), computed });
    }
    Ok(RdFile { raw, records, file_sum: computed })
}

/// Encode `body`, append `E5 05 sum` + `D7`, and scramble. The body must not contain the footer.
pub fn encode_rd_file(body: &[Command], magic: Magic) -> Result<Vec<u8>, EncodeError> {
    if body.iter().any(|c| matches!(c, Command::FileSum(_) | Command::EndOfFile)) {
        return Err(EncodeError::ReservedCommandInBody);
    }
    let mut raw = encode(body)?;
    let sum = file_sum(&raw);
    let sum = U35::new(sum).map_err(|_| EncodeError::FileSumOverflow { sum })?;
    raw.extend(encode(&[Command::FileSum(sum), Command::EndOfFile])?);
    Ok(scramble(&raw, magic))
}
