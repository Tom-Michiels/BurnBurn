//! In-memory [`FileSystem`] with fault injection, for deterministic storage tests.
//!
//! Model: a file has current bytes and the bytes last made durable by `sync_file`.
//! [`MemoryFileSystem::simulate_power_loss`] reverts every file to its durable bytes (files
//! never synced disappear). A failing `write_all` leaves a torn file (first half written).
//! Directory durability is not modelled: a completed `replace` is treated as durable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::fs::{FileSystem, FsError, FsErrorKind, FsOp};

#[derive(Debug, Clone, Default)]
struct File {
    bytes: Vec<u8>,
    durable: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
struct State {
    files: BTreeMap<PathBuf, File>,
    dirs: BTreeSet<PathBuf>,
    /// Pending faults: (operation, number of matching calls to let through first).
    faults: Vec<(FsOp, usize)>,
    temp_counter: u64,
    /// The next append keeps only this many bytes and fails.
    tear_append: Option<usize>,
    log: Vec<(FsOp, PathBuf)>,
    modified: BTreeMap<PathBuf, u64>,
}

#[derive(Debug, Default)]
pub struct MemoryFileSystem {
    state: Mutex<State>,
}

impl MemoryFileSystem {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panicking test thread must not hide the state from the assertion that follows.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The next call of `op` fails (after `skip` successful calls of that op).
    pub fn fail_on(&self, op: FsOp, skip: usize) {
        self.lock().faults.push((op, skip));
    }

    /// Reverts all files to their last synced bytes; never-synced files vanish.
    pub fn simulate_power_loss(&self) {
        let mut st = self.lock();
        let files = std::mem::take(&mut st.files);
        st.files = files
            .into_iter()
            .filter_map(|(p, f)| f.durable.map(|d| (p, File { bytes: d.clone(), durable: Some(d) })))
            .collect();
    }

    /// Paths of all files, sorted.
    pub fn files(&self) -> Vec<PathBuf> {
        self.lock().files.keys().cloned().collect()
    }

    /// Operations performed so far, in order.
    pub fn log(&self) -> Vec<(FsOp, PathBuf)> {
        self.lock().log.clone()
    }

    /// The next `append` writes only the first `keep` bytes and then fails (torn append).
    pub fn tear_next_append(&self, keep: usize) {
        self.lock().tear_append = Some(keep);
    }

    /// Test setup helper: says when `path` was last written (seconds since the Unix epoch).
    pub fn set_modified_for_test(&self, path: &Path, seconds: u64) {
        self.lock().modified.insert(path.to_path_buf(), seconds);
    }

    /// Test setup helper: makes `dir` exist without logging or fault checks.
    pub fn create_dir_all_for_test(&self, dir: &Path) {
        let mut st = self.lock();
        for a in dir.ancestors() {
            st.dirs.insert(a.to_path_buf());
        }
    }

    /// Test setup helper: puts a durable file in place.
    pub fn insert(&self, path: &Path, bytes: Vec<u8>) {
        let mut st = self.lock();
        if let Some(dir) = path.parent() {
            st.dirs.insert(dir.to_path_buf());
        }
        st.files.insert(path.to_path_buf(), File { bytes: bytes.clone(), durable: Some(bytes) });
    }
}

impl State {
    fn enter(&mut self, op: FsOp, path: &Path) -> Result<(), FsError> {
        self.log.push((op, path.to_path_buf()));
        if let Some(i) = self.faults.iter().position(|(o, _)| *o == op) {
            if self.faults[i].1 == 0 {
                self.faults.remove(i);
                return Err(FsError::new(op, path, FsErrorKind::Other));
            }
            self.faults[i].1 -= 1;
        }
        Ok(())
    }

    fn file_mut(&mut self, op: FsOp, path: &Path) -> Result<&mut File, FsError> {
        self.files.get_mut(path).ok_or_else(|| FsError::new(op, path, FsErrorKind::NotFound))
    }
}

impl FileSystem for MemoryFileSystem {
    fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, FsError> {
        let mut st = self.lock();
        st.enter(FsOp::Read, path)?;
        let f = st.file_mut(FsOp::Read, path)?;
        if f.bytes.len() as u64 > limit {
            return Err(FsError::new(FsOp::Read, path, FsErrorKind::TooLarge));
        }
        Ok(f.bytes.clone())
    }

    fn exists(&self, path: &Path) -> Result<bool, FsError> {
        let mut st = self.lock();
        st.enter(FsOp::Exists, path)?;
        Ok(st.files.contains_key(path) || st.dirs.contains(path))
    }

    fn create_dir_all(&self, dir: &Path) -> Result<(), FsError> {
        let mut st = self.lock();
        st.enter(FsOp::CreateDir, dir)?;
        for a in dir.ancestors() {
            st.dirs.insert(a.to_path_buf());
        }
        Ok(())
    }

    fn create_temp_in(&self, dir: &Path, stem: &str) -> Result<PathBuf, FsError> {
        let mut st = self.lock();
        st.enter(FsOp::CreateTemp, dir)?;
        loop {
            st.temp_counter += 1;
            let p = dir.join(format!("{stem}.{}.tmp", st.temp_counter));
            if !st.files.contains_key(&p) {
                st.files.insert(p.clone(), File::default());
                return Ok(p);
            }
        }
    }

    fn write_all(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let mut st = self.lock();
        let entered = st.enter(FsOp::Write, path);
        let f = st.files.entry(path.to_path_buf()).or_default();
        match entered {
            Ok(()) => {
                f.bytes = bytes.to_vec();
                Ok(())
            }
            Err(e) => {
                // Torn write: the first half reached the file before the failure.
                f.bytes = bytes[..bytes.len() / 2].to_vec();
                Err(e)
            }
        }
    }

    fn sync_file(&self, path: &Path) -> Result<(), FsError> {
        let mut st = self.lock();
        st.enter(FsOp::SyncFile, path)?;
        let f = st.file_mut(FsOp::SyncFile, path)?;
        f.durable = Some(f.bytes.clone());
        Ok(())
    }

    fn replace(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let mut st = self.lock();
        st.enter(FsOp::Replace, to)?;
        let f = st.files.remove(from).ok_or_else(|| FsError::new(FsOp::Replace, from, FsErrorKind::NotFound))?;
        st.files.insert(to.to_path_buf(), f);
        Ok(())
    }

    fn sync_dir(&self, dir: &Path) -> Result<(), FsError> {
        self.lock().enter(FsOp::SyncDir, dir)
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<u64, FsError> {
        let mut st = self.lock();
        let tear = st.tear_append.take();
        st.enter(FsOp::Append, path)?;
        if path.parent().is_some_and(|d| !st.dirs.contains(d)) {
            return Err(FsError::new(FsOp::Append, path, FsErrorKind::NotFound));
        }
        let f = st.files.entry(path.to_path_buf()).or_default();
        match tear {
            Some(keep) => {
                f.bytes.extend_from_slice(&bytes[..keep.min(bytes.len())]);
                Err(FsError::new(FsOp::Append, path, FsErrorKind::Other))
            }
            None => {
                f.bytes.extend_from_slice(bytes);
                Ok(f.bytes.len() as u64)
            }
        }
    }

    fn list_dir(&self, dir: &Path) -> Result<Vec<PathBuf>, FsError> {
        let mut st = self.lock();
        st.enter(FsOp::List, dir)?;
        if !st.dirs.contains(dir) {
            return Err(FsError::new(FsOp::List, dir, FsErrorKind::NotFound));
        }
        Ok(st.files.keys().filter(|p| p.parent() == Some(dir)).cloned().collect())
    }

    fn modified(&self, path: &Path) -> Result<Option<u64>, FsError> {
        Ok(self.lock().modified.get(path).copied())
    }

    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        let mut st = self.lock();
        st.enter(FsOp::Remove, path)?;
        st.files.remove(path).map(|_| ()).ok_or_else(|| FsError::new(FsOp::Remove, path, FsErrorKind::NotFound))
    }
}
