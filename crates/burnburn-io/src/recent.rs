//! Recent projects and recent templates (File ▸ Recent Projects / Recent Templates, M017/M018).
//!
//! The lists live in the user's data directory as small JSON files written through the
//! [`FileSystem`] port (atomic replace); the "opened" time is a parameter — this crate has no
//! clock. Loading never fails the application: a missing list is empty, a damaged or newer one
//! starts empty and reports why ([`RecentIssue`]); only an explicit save replaces a damaged file,
//! and a file from a newer version is never overwritten. Entries whose files vanished stay until
//! [`RecentList::prune_missing`] is called (a network volume may just be offline).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::SaveError;
use crate::fs::{FileSystem, FsError, FsErrorKind};
use crate::store::write_atomic;

pub const DEFAULT_MAX_RECENT: usize = 10;
/// Upper bound of a list, whatever the caller asks for.
pub const MAX_RECENT_LIMIT: usize = 100;
const SCHEMA: u32 = 1;
const MAX_FILE_BYTES: u64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentKind {
    Projects,
    Templates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEntry {
    pub path: PathBuf,
    /// Display name, supplied by the caller (usually the file stem).
    pub name: String,
    /// Seconds since the Unix epoch, supplied by the caller.
    pub opened: u64,
}

/// Most recent first, unique by path, at most `max` entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentList {
    max: usize,
    entries: Vec<RecentEntry>,
}

impl RecentList {
    pub fn new(max: usize) -> Self {
        RecentList { max: max.clamp(1, MAX_RECENT_LIMIT), entries: Vec::new() }
    }

    pub fn entries(&self) -> &[RecentEntry] {
        &self.entries
    }

    /// Records that `path` was opened or saved: moves it to the front (updating name and time)
    /// and drops the oldest entries beyond the maximum.
    pub fn touch(&mut self, path: &Path, name: &str, opened: u64) {
        self.entries.retain(|e| e.path != path);
        self.entries.insert(0, RecentEntry { path: path.to_path_buf(), name: name.to_owned(), opened });
        self.entries.truncate(self.max);
    }

    /// Removes the entry for `path`; whether it was present.
    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.path != path);
        self.entries.len() != before
    }

    /// Drops entries whose file no longer exists and returns them. On a filesystem error
    /// nothing is removed.
    pub fn prune_missing(&mut self, fs: &dyn FileSystem) -> Result<Vec<RecentEntry>, FsError> {
        let mut gone = Vec::new();
        for e in &self.entries {
            if !fs.exists(&e.path)? {
                gone.push(e.clone());
            }
        }
        self.entries.retain(|e| !gone.contains(e));
        Ok(gone)
    }
}

/// Why a list could not be read; the list is empty in all cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecentIssue {
    Corrupt(String),
    NewerSchema { found: u32, supported: u32 },
    Fs(FsError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentLoad {
    pub list: RecentList,
    pub issue: Option<RecentIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecentError {
    Fs(FsError),
    /// Saved, but the directory sync failed.
    Unconfirmed(FsError),
    /// The file on disk was written by a newer version and is left alone.
    NewerSchema {
        found: u32,
        supported: u32,
    },
    Encode(String),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    schema: u32,
    entries: Vec<EntryDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDto {
    path: String,
    name: String,
    opened: u64,
}

pub struct RecentStore<'a> {
    fs: &'a dyn FileSystem,
    dir: PathBuf,
    max: usize,
}

impl<'a> RecentStore<'a> {
    /// `dir` is the user's data directory (the host decides where; tests use an in-memory one).
    pub fn new(fs: &'a dyn FileSystem, dir: &Path) -> Self {
        RecentStore { fs, dir: dir.to_path_buf(), max: DEFAULT_MAX_RECENT }
    }

    pub fn with_max(mut self, max: usize) -> Self {
        self.max = max;
        self
    }

    pub fn file_path(&self, kind: RecentKind) -> PathBuf {
        self.dir.join(match kind {
            RecentKind::Projects => "recent-projects.json",
            RecentKind::Templates => "recent-templates.json",
        })
    }

    pub fn load(&self, kind: RecentKind) -> RecentLoad {
        let empty = |issue| RecentLoad { list: RecentList::new(self.max), issue };
        let path = self.file_path(kind);
        let bytes = match self.fs.read(&path, MAX_FILE_BYTES) {
            Ok(b) => b,
            Err(e) if e.kind == FsErrorKind::NotFound => return empty(None),
            Err(e) => return empty(Some(RecentIssue::Fs(e))),
        };
        match decode(&bytes) {
            Ok(entries) => {
                let mut list = RecentList::new(self.max);
                list.entries = entries;
                list.entries.truncate(list.max);
                RecentLoad { list, issue: None }
            }
            Err(issue) => empty(Some(issue)),
        }
    }

    /// Writes the list (atomic replace). A file from a newer version is refused, a damaged
    /// one is replaced.
    pub fn save(&self, kind: RecentKind, list: &RecentList) -> Result<(), RecentError> {
        let path = self.file_path(kind);
        match self.fs.read(&path, MAX_FILE_BYTES) {
            Ok(existing) => {
                if let Err(RecentIssue::NewerSchema { found, supported }) = decode(&existing) {
                    return Err(RecentError::NewerSchema { found, supported });
                }
            }
            Err(e) if e.kind == FsErrorKind::NotFound => {}
            Err(e) => return Err(RecentError::Fs(e)),
        }
        let dto = FileDto {
            schema: SCHEMA,
            // A path that is not valid UTF-8 cannot be written to JSON and is skipped.
            entries: list
                .entries
                .iter()
                .filter_map(|e| {
                    Some(EntryDto { path: e.path.to_str()?.to_owned(), name: e.name.clone(), opened: e.opened })
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&dto).map_err(|e| RecentError::Encode(e.to_string()))?;
        self.fs.create_dir_all(&self.dir).map_err(RecentError::Fs)?;
        write_atomic(self.fs, &path, &bytes).map_err(|e| match e {
            SaveError::Fs(e) => RecentError::Fs(e),
            SaveError::Unconfirmed(e) => RecentError::Unconfirmed(e),
            other => RecentError::Encode(other.to_string()),
        })
    }
}

fn decode(bytes: &[u8]) -> Result<Vec<RecentEntry>, RecentIssue> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| RecentIssue::Corrupt(e.to_string()))?;
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64);
    if let Some(found) = schema
        && found > u64::from(SCHEMA)
    {
        return Err(RecentIssue::NewerSchema { found: u32::try_from(found).unwrap_or(u32::MAX), supported: SCHEMA });
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| RecentIssue::Corrupt(e.to_string()))?;
    if dto.schema != SCHEMA {
        return Err(RecentIssue::Corrupt(format!("unknown schema {}", dto.schema)));
    }
    Ok(dto
        .entries
        .into_iter()
        .map(|e| RecentEntry { path: PathBuf::from(e.path), name: e.name, opened: e.opened })
        .collect())
}
