//! Storage port of the journal: io's [`FileSystem`] (with `append`, design decision C-7).
//!
//! `JournalFs` is only a name for "a FileSystem used by the journal"; every `FileSystem`
//! is one. (It carried its own `append` until io's port had it.)

use std::path::Path;

use burnburn_io::fs::{FileSystem, FsError};

pub trait JournalFs: FileSystem {}

impl<T: FileSystem + ?Sized> JournalFs for T {}

/// Atomic whole-file write: temp in the same directory → write → sync → replace → sync dir.
/// Until io offers `write_file` (C-7), the journal keeps this copy of the route.
pub(crate) fn write_atomically(fs: &dyn FileSystem, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_name().and_then(|n| n.to_str()).unwrap_or("history");
    let temp = fs.create_temp_in(dir, stem)?;
    let result = (|| {
        fs.write_all(&temp, bytes)?;
        fs.sync_file(&temp)?;
        fs.replace(&temp, path)?;
        fs.sync_dir(dir)
    })();
    if result.is_err() {
        // Best effort: a leftover temp file is harmless (never read as a segment or blob).
        let _ = fs.remove_file(&temp);
    }
    result
}
