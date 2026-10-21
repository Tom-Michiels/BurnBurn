use std::fmt;

use burnburn_types::ContentHash;

use crate::fs::FsError;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoadError {
    Fs(FsError),
    /// Not a readable zip container.
    Container(String),
    /// A size or count limit of the container contract was exceeded.
    Limit(&'static str),
    MissingEntry(&'static str),
    /// An entry name outside the v1 layout (including `..` or absolute paths).
    UnexpectedEntry(String),
    NotAProject,
    /// The file was written by a newer BurnBurn. Reported, never silently downgraded.
    NewerSchema {
        found: u32,
        supported: u32,
    },
    /// A field this schema version does not define (no silent loss).
    UnknownField {
        entry: &'static str,
        detail: String,
    },
    Json {
        entry: &'static str,
        detail: String,
    },
    /// A value failed domain validation; `path` is the JSON path, e.g. `objects[0].subpaths[0].start[1]`.
    InvalidValue {
        path: String,
        reason: String,
    },
    AssetHashMismatch(ContentHash),
    MissingAsset(ContentHash),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Fs(e) => write!(f, "{e}"),
            LoadError::Container(m) => write!(f, "not a readable project container: {m}"),
            LoadError::Limit(what) => write!(f, "project container exceeds limit: {what}"),
            LoadError::MissingEntry(e) => write!(f, "project container lacks {e}"),
            LoadError::UnexpectedEntry(e) => write!(f, "unexpected entry {e:?} in project container"),
            LoadError::NotAProject => f.write_str("not a BurnBurn project"),
            LoadError::NewerSchema { found, supported } => {
                write!(
                    f,
                    "project schema {found} is newer than supported schema {supported}: the project was saved by a newer BurnBurn. Update BurnBurn to open it"
                )
            }
            LoadError::UnknownField { entry, detail } => write!(f, "{entry}: {detail}"),
            LoadError::Json { entry, detail } => write!(f, "{entry}: {detail}"),
            LoadError::InvalidValue { path, reason } => write!(f, "invalid value at {path}: {reason}"),
            LoadError::AssetHashMismatch(h) => write!(f, "asset {h} does not match its hash"),
            LoadError::MissingAsset(h) => write!(f, "asset {h} is referenced but missing"),
        }
    }
}

impl std::error::Error for LoadError {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SaveError {
    Fs(FsError),
    /// The new file replaced the old one, but the directory sync failed: the content at the
    /// path is the new project, its durability across a crash is unconfirmed.
    Unconfirmed(FsError),
    Encode(String),
    /// The project is inconsistent (for example an operation targets missing geometry).
    Invalid(String),
    /// The file at the target path was written by a newer BurnBurn; this build does not overwrite it.
    NewerFileOnDisk {
        found: u32,
        supported: u32,
    },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Fs(e) => write!(f, "{e}"),
            SaveError::Unconfirmed(e) => write!(f, "saved, but not confirmed durable: {e}"),
            SaveError::Encode(m) => write!(f, "could not encode project: {m}"),
            SaveError::Invalid(m) => write!(f, "project is inconsistent: {m}"),
            SaveError::NewerFileOnDisk { found, supported } => {
                write!(
                    f,
                    "the file on disk has schema {found}, newer than supported schema {supported}: not overwritten"
                )
            }
        }
    }
}

impl std::error::Error for SaveError {}
