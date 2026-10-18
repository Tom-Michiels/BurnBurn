//! Production journal storage: io's `StdFileSystem` with a working `append`.
//!
//! io's port has `append` since C-7, but `StdFileSystem::append` is not implemented yet (Kern,
//! in progress). Until it is, this adapter supplies it with `OpenOptions::append`; afterwards
//! it can be replaced by `StdFileSystem` itself.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use burnburn_io::fs::{FileSystem, FsError, FsErrorKind, FsOp};
use burnburn_io::std_fs::StdFileSystem;

#[derive(Debug, Default, Clone, Copy)]
pub struct StdJournalFs {
    inner: StdFileSystem,
}

impl StdJournalFs {
    pub fn new() -> Self {
        StdJournalFs::default()
    }
}

fn kind(e: &std::io::Error) -> FsErrorKind {
    match e.kind() {
        std::io::ErrorKind::NotFound => FsErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => FsErrorKind::PermissionDenied,
        std::io::ErrorKind::StorageFull => FsErrorKind::NoSpace,
        std::io::ErrorKind::Interrupted => FsErrorKind::Interrupted,
        _ => FsErrorKind::Other,
    }
}

impl FileSystem for StdJournalFs {
    fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, FsError> {
        self.inner.read(path, limit)
    }
    fn exists(&self, path: &Path) -> Result<bool, FsError> {
        self.inner.exists(path)
    }
    fn create_dir_all(&self, dir: &Path) -> Result<(), FsError> {
        self.inner.create_dir_all(dir)
    }
    fn create_temp_in(&self, dir: &Path, stem: &str) -> Result<PathBuf, FsError> {
        self.inner.create_temp_in(dir, stem)
    }
    fn write_all(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        self.inner.write_all(path, bytes)
    }
    fn sync_file(&self, path: &Path) -> Result<(), FsError> {
        self.inner.sync_file(path)
    }
    fn replace(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.inner.replace(from, to)
    }
    fn sync_dir(&self, dir: &Path) -> Result<(), FsError> {
        self.inner.sync_dir(dir)
    }
    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        self.inner.remove_file(path)
    }

    fn list_dir(&self, dir: &Path) -> Result<Vec<PathBuf>, FsError> {
        let err = |e: std::io::Error| FsError::new(FsOp::List, dir, kind(&e));
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(err)? {
            let entry = entry.map_err(err)?;
            if entry.file_type().map_err(err)?.is_file() {
                out.push(entry.path());
            }
        }
        out.sort();
        Ok(out)
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<u64, FsError> {
        let err = |e: std::io::Error| FsError::new(FsOp::Write, path, kind(&e));
        let mut f = OpenOptions::new().create(true).append(true).open(path).map_err(err)?;
        f.write_all(bytes).map_err(err)?;
        let len = f.metadata().map_err(err)?.len();
        Ok(len)
    }
}
