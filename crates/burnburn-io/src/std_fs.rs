//! The real filesystem adapter over `std::fs`.
//!
//! Durability notes (verified only on macOS arm64 by the integration test, not by power cuts):
//! - `sync_file` uses `File::sync_all`; on Apple platforms the Rust standard library issues
//!   `fcntl(F_FULLFSYNC)` for it.
//! - `replace` is `std::fs::rename`: atomic on one POSIX volume; on Windows it maps to
//!   `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` (not verified here, no Windows runner).
//! - `sync_dir` opens the directory and syncs it on Unix; it is a no-op on Windows, where a
//!   directory cannot be opened as a file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::fs::{FileSystem, FsError, FsErrorKind, FsOp};

#[derive(Debug, Default, Clone, Copy)]
pub struct StdFileSystem;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn err(op: FsOp, path: &Path, e: &io::Error) -> FsError {
    let kind = match e.kind() {
        io::ErrorKind::NotFound => FsErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => FsErrorKind::PermissionDenied,
        io::ErrorKind::AlreadyExists => FsErrorKind::AlreadyExists,
        io::ErrorKind::StorageFull => FsErrorKind::NoSpace,
        io::ErrorKind::Interrupted => FsErrorKind::Interrupted,
        io::ErrorKind::FileTooLarge => FsErrorKind::TooLarge,
        _ => FsErrorKind::Other,
    };
    FsError::new(op, path, kind)
}

impl FileSystem for StdFileSystem {
    fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, FsError> {
        let file = File::open(path).map_err(|e| err(FsOp::Read, path, &e))?;
        let mut out = Vec::new();
        // Read one byte past the limit to detect oversize files without trusting metadata.
        file.take(limit.saturating_add(1)).read_to_end(&mut out).map_err(|e| err(FsOp::Read, path, &e))?;
        if out.len() as u64 > limit {
            return Err(FsError::new(FsOp::Read, path, FsErrorKind::TooLarge));
        }
        Ok(out)
    }

    fn exists(&self, path: &Path) -> Result<bool, FsError> {
        path.try_exists().map_err(|e| err(FsOp::Exists, path, &e))
    }

    fn create_dir_all(&self, dir: &Path) -> Result<(), FsError> {
        fs::create_dir_all(dir).map_err(|e| err(FsOp::CreateDir, dir, &e))
    }

    fn create_temp_in(&self, dir: &Path, stem: &str) -> Result<PathBuf, FsError> {
        let pid = std::process::id();
        loop {
            let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!(".{stem}.{pid}.{n}.tmp"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(err(FsOp::CreateTemp, dir, &e)),
            }
        }
    }

    fn write_all(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| err(FsOp::Write, path, &e))?;
        f.write_all(bytes).map_err(|e| err(FsOp::Write, path, &e))
    }

    fn sync_file(&self, path: &Path) -> Result<(), FsError> {
        let f = OpenOptions::new().write(true).open(path).map_err(|e| err(FsOp::SyncFile, path, &e))?;
        f.sync_all().map_err(|e| err(FsOp::SyncFile, path, &e))
    }

    fn replace(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        fs::rename(from, to).map_err(|e| err(FsOp::Replace, to, &e))
    }

    #[cfg(unix)]
    fn sync_dir(&self, dir: &Path) -> Result<(), FsError> {
        let d = File::open(dir).map_err(|e| err(FsOp::SyncDir, dir, &e))?;
        d.sync_all().map_err(|e| err(FsOp::SyncDir, dir, &e))
    }

    #[cfg(not(unix))]
    fn sync_dir(&self, _dir: &Path) -> Result<(), FsError> {
        Ok(())
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<u64, FsError> {
        let mut f = OpenOptions::new().append(true).create(true).open(path).map_err(|e| err(FsOp::Append, path, &e))?;
        f.write_all(bytes).map_err(|e| err(FsOp::Append, path, &e))?;
        f.metadata().map(|m| m.len()).map_err(|e| err(FsOp::Append, path, &e))
    }

    fn list_dir(&self, dir: &Path) -> Result<Vec<PathBuf>, FsError> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).map_err(|e| err(FsOp::List, dir, &e))? {
            let entry = entry.map_err(|e| err(FsOp::List, dir, &e))?;
            if entry.file_type().map_err(|e| err(FsOp::List, dir, &e))?.is_file() {
                out.push(entry.path());
            }
        }
        out.sort();
        Ok(out)
    }

    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        fs::remove_file(path).map_err(|e| err(FsOp::Remove, path, &e))
    }
}
