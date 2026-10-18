//! Journal crash safety and recovery (ADR 0008 §4, contracts-phase-c §IM17 journal format).
#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use burnburn_history::{History, HistoryError, HistoryRecord, JournalLimits, segment_path};
use burnburn_io::fs::FileSystem;
use common::*;

/// Fills a history with a few records: one job, a queue item, a note-free quantity change.
fn some_records(fs: &Fs) -> History {
    let (mut h, _) = open(fs);
    let c = compile_session(&plate(), &profile(1));
    let job = record(&mut h, fs, &c, &profile(1), t(0));
    let item = h.enqueue(fs, job, 3, t(1)).unwrap();
    h.change_quantity(fs, item, 4, t(2)).unwrap();
    h
}

fn seg(n: u32) -> std::path::PathBuf {
    segment_path(&dir(), n)
}

/// Builds one record by hand from the documented byte format.
fn raw_record(json: &str) -> Vec<u8> {
    let payload = json.as_bytes();
    let mut out = Vec::new();
    out.extend_from_slice(&0xB0B0_4A31u32.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(payload).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn read(fs: &Fs, p: &Path) -> Vec<u8> {
    fs.read(p, u64::MAX).unwrap()
}

#[test]
fn segment_bytes_follow_the_documented_format() {
    let fs = Fs::new();
    some_records(&fs);
    let bytes = read(&fs, &seg(1));
    assert_eq!(&bytes[..8], b"BBJ\0\x01\x00\x00\x00");
    assert_eq!(&bytes[8..12], &0xB0B0_4A31u32.to_le_bytes());
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let payload = &bytes[20..20 + len];
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), crc32fast::hash(payload));
    let text = std::str::from_utf8(payload).unwrap();
    assert!(text.starts_with(r#"{"schema":1,"seq":1,"wall_time":"2026-10-14T"#), "{text}");
    assert!(text.contains(r#""kind":"job-compiled","body":{"#), "{text}");
}

#[test]
fn torn_tail_record_is_ignored_and_reported() {
    let fs = Fs::new();
    let mut h = some_records(&fs);
    let job = h.jobs().next().unwrap().id;
    let before = read(&fs, &seg(1)).len() as u64;
    fs.tear_next_append(17);
    assert!(matches!(h.enqueue(&fs, job, 1, t(3)), Err(HistoryError::Fs(_))));
    drop(h);

    let (mut h, report) = open(&fs);
    assert_eq!(report.corrupt, None);
    assert_eq!(report.records, 3, "complete records survive, the torn one is not one of them");
    assert_eq!(report.torn_tails.len(), 1);
    assert_eq!((report.torn_tails[0].offset, report.torn_tails[0].bytes), (before, 17));
    assert_eq!(read(&fs, &seg(1)).len() as u64, before + 17, "torn bytes are left in place, never truncated");

    // The next record goes to a new segment, and the history stays readable afterwards.
    h.enqueue(&fs, job, 2, t(4)).unwrap();
    assert!(fs.exists(&seg(2)).unwrap());
    drop(h);
    let (h, report) = open(&fs);
    assert_eq!((report.records, report.torn_tails.len(), report.corrupt), (4, 1, None));
    assert!(!h.read_only());
    assert_eq!(h.queue().count(), 2);
}

#[test]
fn every_prefix_of_a_journal_opens_without_losing_complete_records() {
    let fs = Fs::new();
    some_records(&fs);
    let full = read(&fs, &seg(1));
    // Offsets where each complete record ends.
    let mut ends = Vec::new();
    let mut at = 8;
    while at < full.len() {
        let len = u32::from_le_bytes(full[at + 4..at + 8].try_into().unwrap()) as usize;
        at += 12 + len;
        ends.push(at);
    }
    for cut in 8..=full.len() {
        let crash = Fs::new();
        for p in fs.mem.files().iter().filter(|p| p.starts_with(dir().join("blobs"))) {
            crash.mem.insert(p, read(&fs, p));
        }
        crash.mem.insert(&seg(1), full[..cut].to_vec());
        let (_, report) = History::open(&crash, &dir(), &JournalLimits::default()).unwrap();
        let complete = ends.iter().filter(|&&e| e <= cut).count() as u64;
        assert_eq!(report.records, complete, "cut at {cut}");
        assert_eq!(report.corrupt, None, "cut at {cut}");
        assert_eq!(report.torn_tails.is_empty(), cut == 8 || ends.contains(&cut), "cut at {cut}");
    }
}

#[test]
fn damage_in_the_middle_opens_read_only() {
    let fs = Fs::new();
    some_records(&fs);
    let mut bytes = read(&fs, &seg(1));
    bytes[8 + 12 + 5] ^= 0x01; // inside the first record's payload
    fs.mem.insert(&seg(1), bytes);
    let (mut h, report) = open(&fs);
    let corrupt = report.corrupt.expect("mid-segment damage is corruption");
    assert_eq!(corrupt.offset, 8);
    assert!(h.read_only());
    assert_eq!(h.jobs().count(), 0);
    let job = burnburn_history::compiled_job_id(
        &burnburn_types::ContentHash::of(burnburn_types::HashDomain::Plan, b"x"),
        &burnburn_types::ContentHash::of(burnburn_types::HashDomain::Rd, b"x"),
        profile(1).revision(),
    );
    assert_eq!(h.enqueue(&fs, job, 1, t(5)), Err(HistoryError::ReadOnly));
}

#[test]
fn unknown_kind_is_kept_and_non_increasing_seq_is_corruption() {
    let fs = Fs::new();
    some_records(&fs);
    let mut bytes = read(&fs, &seg(1));
    bytes.extend(raw_record(
        r#"{"schema":1,"seq":9,"wall_time":"2026-10-18T10:00:00.000Z","kind":"order-placed","body":{"x":1}}"#,
    ));
    fs.mem.insert(&seg(1), bytes.clone());
    let (h, report) = open(&fs);
    assert_eq!((report.records, report.unrecognized, report.corrupt), (4, 1, None));
    assert!(matches!(&h.records()[3].record, HistoryRecord::Unrecognized { kind, .. } if kind == "order-placed"));

    bytes.extend(raw_record(
        r#"{"schema":1,"seq":9,"wall_time":"2026-10-18T10:00:00.000Z","kind":"order-placed","body":{}}"#,
    ));
    fs.mem.insert(&seg(1), bytes.clone());
    let (h, report) = open(&fs);
    assert!(h.read_only(), "a repeated seq is not explainable by a crash");
    assert_eq!(report.corrupt.map(|c| c.reason), Some("non-increasing record seq".to_string()));

    let mut future = read(&fs, &seg(1))[..8].to_vec();
    future
        .extend(raw_record(r#"{"schema":2,"seq":1,"wall_time":"2026-10-18T10:00:00.000Z","kind":"queued","body":{}}"#));
    fs.mem.insert(&seg(1), future);
    let (h, _) = open(&fs);
    assert!(h.read_only(), "an unknown schema is never guessed");
}

#[test]
fn segments_rotate_and_reopen_in_order() {
    let fs = Fs::new();
    let limits = JournalLimits { max_segment: 600, max_record: 1 << 20 };
    let (mut h, _) = History::open(&fs, &dir(), &limits).unwrap();
    let c = compile_session(&plate(), &profile(1));
    let job = record(&mut h, &fs, &c, &profile(1), t(0));
    for q in 1..=10 {
        h.enqueue(&fs, job, q, t(i64::from(q))).unwrap();
    }
    drop(h);
    let (h, report) = History::open(&fs, &dir(), &limits).unwrap();
    assert!(report.segments >= 3, "{report:?}");
    assert_eq!(report.records, 11);
    let quantities: Vec<u32> = h.queue().map(|q| q.quantity).collect();
    assert_eq!(quantities, (1..=10).collect::<Vec<_>>());
    let seqs: Vec<u64> = h.records().iter().map(|r| r.seq.raw()).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn tampered_blob_is_refused() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let c = compile_session(&plate(), &profile(1));
    let job = record(&mut h, &fs, &c, &profile(1), t(0));
    let rd_key = h.job(job).unwrap().blobs.rd;
    let path = dir().join("blobs").join(rd_key.file_name());
    let mut bytes = read(&fs, &path);
    bytes[10] ^= 0xff;
    fs.mem.insert(&path, bytes);
    assert_eq!(h.load_rd(&fs, job), Err(HistoryError::BlobMismatch));
    assert!(matches!(h.load_artifact(&fs, job, 1470), Err(HistoryError::BlobMismatch)));
}

#[test]
fn failed_sync_never_reports_a_record_as_written() {
    let fs = Fs::new();
    let mut h = some_records(&fs);
    let job = h.jobs().next().unwrap().id;
    fs.mem.fail_on(burnburn_io::fs::FsOp::SyncFile, 0);
    assert!(h.enqueue(&fs, job, 9, t(9)).is_err());
    assert_eq!(h.queue().count(), 1, "an unsynced record is not indexed");
    fs.mem.simulate_power_loss();
    let (h, report) = open(&fs);
    assert_eq!((report.records, report.corrupt), (3, None));
    assert_eq!(h.queue().count(), 1);
}
