//! File-level contract: file sum, end record and data after it.

mod common;

use burnburn_ruida::{FileError, Magic, decode_rd_file, scramble, unscramble};
use common::fixture;

const M: Magic = Magic::FIXTURE_0X88;

fn line_raw() -> Vec<u8> {
    unscramble(&fixture("rectangle-default.rd"), M)
}

#[test]
fn changed_body_byte_is_a_checksum_mismatch() {
    let mut raw = line_raw();
    // Speed payload @0x18d+6 (`... 0d 20`): 0x20 → 0x21 stays a valid 7-bit record.
    assert_eq!(raw[0x18d + 6], 0x20);
    raw[0x18d + 6] = 0x21;
    assert_eq!(
        decode_rd_file(&scramble(&raw, M), M),
        Err(FileError::ChecksumMismatch { declared: 24_999, computed: 25_000 })
    );
}

#[test]
fn missing_end_record_is_reported() {
    let mut raw = line_raw();
    raw.pop();
    assert_eq!(decode_rd_file(&scramble(&raw, M), M), Err(FileError::MissingEndOfFile));
}

#[test]
fn data_after_end_record_is_reported() {
    let mut raw = line_raw();
    raw.extend([0xe7, 0x00]);
    assert_eq!(decode_rd_file(&scramble(&raw, M), M), Err(FileError::DataAfterEndOfFile { offset: 525 }));
}

#[test]
fn end_record_without_file_sum_is_reported() {
    assert_eq!(decode_rd_file(&scramble(&[0xe7, 0x00, 0xd7], M), M), Err(FileError::MissingFileSum));
}

#[test]
fn wrong_magic_does_not_yield_a_valid_file() {
    assert!(decode_rd_file(&scramble(&line_raw(), M), Magic(0x11)).is_err());
}
