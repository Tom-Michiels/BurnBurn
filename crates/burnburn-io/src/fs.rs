//! The filesystem port. Production: [`crate::std_fs::StdFileSystem`]; tests:
//! [`crate::memory::MemoryFileSystem`] with per-step fault injection.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FsOp {
    Read,
    Exists,
    CreateTemp,
    CreateDir,
    Write,
    SyncFile,
    Replace,
    SyncDir,
    Remove,
    Append,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FsErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    NoSpace,
    TooLarge,
    Interrupted,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError {
    pub op: FsOp,
    pub path: PathBuf,
    pub kind: FsErrorKind,
}

impl FsError {
    pub fn new(op: FsOp, path: &Path, kind: FsErrorKind) -> Self {
        FsError { op, path: path.to_path_buf(), kind }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} failed for {}: {:?}", self.op, self.path.display(), self.kind)
    }
}

impl std::error::Error for FsError {}

/// Every step is separate so tests can inject a failure on write, flush or replace.
pub trait FileSystem: Send + Sync {
    /// Whole file; `TooLarge` when it exceeds `limit` bytes.
    fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, FsError>;
    fn exists(&self, path: &Path) -> Result<bool, FsError>;
    fn create_dir_all(&self, dir: &Path) -> Result<(), FsError>;
    /// Exclusively creates a new empty file in `dir` whose name starts with `stem`.
    fn create_temp_in(&self, dir: &Path, stem: &str) -> Result<PathBuf, FsError>;
    /// Replaces the file's content with `bytes`.
    fn write_all(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError>;
    /// Flushes file data to stable storage as far as the OS allows.
    fn sync_file(&self, path: &Path) -> Result<(), FsError>;
    /// Atomically renames `from` over `to` (same volume).
    fn replace(&self, from: &Path, to: &Path) -> Result<(), FsError>;
    /// Persists directory entries (the rename). A documented no-op where unsupported.
    fn sync_dir(&self, dir: &Path) -> Result<(), FsError>;
    fn remove_file(&self, path: &Path) -> Result<(), FsError>;
    /// Appends `bytes` (creating the file if needed) and returns the new length. Not durable
    /// until `sync_file`; a failure may leave a prefix of `bytes` behind (C-7, journal).
    ///
    /// The default rewrites the whole file (read + `write_all`); it exists so other port
    /// implementations keep compiling. `StdFileSystem` and `MemoryFileSystem` append for real.
    fn append(&self, path: &Path, bytes: &[u8]) -> Result<u64, FsError> {
        let mut all = match self.read(path, u64::MAX) {
            Ok(b) => b,
            Err(e) if e.kind == FsErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(FsError { op: FsOp::Append, ..e }),
        };
        all.extend_from_slice(bytes);
        self.write_all(path, &all).map_err(|e| FsError { op: FsOp::Append, ..e })?;
        Ok(all.len() as u64)
    }

    /// The files (not directories) directly inside `dir`, sorted by path. The default reports
    /// that the implementation cannot list (an explicit error, never an empty list).
    fn list_dir(&self, dir: &Path) -> Result<Vec<PathBuf>, FsError> {
        Err(FsError::new(FsOp::List, dir, FsErrorKind::Other))
    }
}
