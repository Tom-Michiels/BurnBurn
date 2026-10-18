//! The production journal storage on the real filesystem, inside a temporary directory only.
#![allow(clippy::unwrap_used)]

use burnburn_history::{History, JournalLimits};
use burnburn_io::fs::FileSystem;
use burnburn_runtime::StdJournalFs;

#[test]
fn std_journal_fs_appends_and_an_empty_history_opens_without_creating_files() {
    let tmp = tempfile::tempdir().unwrap();
    let fs = StdJournalFs::new();
    let file = tmp.path().join("seg.bbj");
    assert_eq!(fs.append(&file, b"abc").unwrap(), 3);
    assert_eq!(fs.append(&file, b"de").unwrap(), 5);
    fs.sync_file(&file).unwrap();
    assert_eq!(fs.read(&file, 100).unwrap(), b"abcde");

    let dir = tmp.path().join("history");
    let (h, report) = History::open(&fs, &dir, &JournalLimits::default()).unwrap();
    assert_eq!((report.segments, report.records), (0, 0));
    assert!(!h.read_only());
    assert!(!dir.exists(), "opening never creates anything");
}
