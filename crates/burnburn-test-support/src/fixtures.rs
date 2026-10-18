//! Checked access to the repository fixtures under `test-data/`.
//!
//! Every read goes through the manifest of its set (`test-data/<set>/manifest.json`): a fixture
//! whose size or SHA-256 differs is an error, never silently used. A file in a set that the
//! manifest does not list is reported by [`unlisted_files`].

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Why a fixture could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixtureError {
    Io { path: PathBuf, message: String },
    Manifest(String),
    NotInManifest(String),
    SizeMismatch { path: PathBuf, expected: u64, actual: u64 },
    HashMismatch { path: PathBuf, expected: String, actual: String },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixtureError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            FixtureError::Manifest(m) => write!(f, "manifest: {m}"),
            FixtureError::NotInManifest(p) => write!(f, "{p}: not listed in the manifest"),
            FixtureError::SizeMismatch { path, expected, actual } => {
                write!(f, "{}: {actual} bytes, manifest says {expected}", path.display())
            }
            FixtureError::HashMismatch { path, expected, actual } => {
                write!(f, "{}: sha256 {actual}, manifest says {expected}", path.display())
            }
        }
    }
}

impl std::error::Error for FixtureError {}

/// One manifest entry whose bytes were checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFile {
    pub path: String,
    pub role: String,
    pub sha256: String,
}

/// The repository root (two levels above this crate's manifest).
pub fn repo_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.ancestors().nth(2).map(Path::to_path_buf).unwrap_or_else(|| crate_dir.to_path_buf())
}

/// `test-data/`.
pub fn test_data_dir() -> PathBuf {
    repo_root().join("test-data")
}

/// `test-data/<set>/`, e.g. `fixture_dir("projects")`.
pub fn fixture_dir(set: &str) -> PathBuf {
    test_data_dir().join(set)
}

/// `test-data/ruida/`.
pub fn ruida_dir() -> PathBuf {
    fixture_dir("ruida")
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

struct Entry {
    path: String,
    role: String,
    bytes: u64,
    sha256: String,
}

fn io_error(path: &Path, e: std::io::Error) -> FixtureError {
    FixtureError::Io { path: path.to_path_buf(), message: e.to_string() }
}

/// Two manifest shapes are accepted: `files[] { path, role, bytes, sha256 }` (ruida, trace) and,
/// when there is no `files` array, `fixtures[] { file, bytes, sha256 }` (projects, owned by Kern).
fn load_manifest(dir: &Path) -> Result<Vec<Entry>, FixtureError> {
    let path = dir.join("manifest.json");
    let raw = std::fs::read(&path).map_err(|e| io_error(&path, e))?;
    let manifest: Value = serde_json::from_slice(&raw).map_err(|e| FixtureError::Manifest(e.to_string()))?;
    let (files, path_key) = match (manifest["files"].as_array(), manifest["fixtures"].as_array()) {
        (Some(files), _) => (files, "path"),
        (None, Some(fixtures)) => (fixtures, "file"),
        (None, None) => return Err(FixtureError::Manifest("no `files` or `fixtures` array".into())),
    };
    files
        .iter()
        .map(|f| {
            let field = |name: &str| {
                f[name]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| FixtureError::Manifest(format!("entry without `{name}`")))
            };
            let bytes = f["bytes"].as_u64().ok_or_else(|| FixtureError::Manifest("entry without `bytes`".into()))?;
            let role = if path_key == "path" { field("role")? } else { "fixture".to_owned() };
            Ok(Entry { path: field(path_key)?, role, bytes, sha256: field("sha256")? })
        })
        .collect()
}

fn read_checked(dir: &Path, entry: &Entry) -> Result<Vec<u8>, FixtureError> {
    let path = dir.join(&entry.path);
    let bytes = std::fs::read(&path).map_err(|e| io_error(&path, e))?;
    let actual = bytes.len() as u64;
    if actual != entry.bytes {
        return Err(FixtureError::SizeMismatch { path, expected: entry.bytes, actual });
    }
    let actual = sha256_hex(&bytes);
    if actual != entry.sha256 {
        return Err(FixtureError::HashMismatch { path, expected: entry.sha256.clone(), actual });
    }
    Ok(bytes)
}

/// Checks size and SHA-256 of every file listed in `<dir>/manifest.json`.
pub fn verify_manifest(dir: &Path) -> Result<Vec<VerifiedFile>, FixtureError> {
    load_manifest(dir)?
        .into_iter()
        .map(|entry| {
            read_checked(dir, &entry)?;
            Ok(VerifiedFile { path: entry.path, role: entry.role, sha256: entry.sha256 })
        })
        .collect()
}

/// Reads `relative` (as listed in the manifest, e.g. `rectangle-default.rd`) from
/// `test-data/ruida/` after checking it against the manifest.
pub fn read_ruida_fixture(relative: &str) -> Result<Vec<u8>, FixtureError> {
    read_fixture("ruida", relative)
}

/// Reads `relative` from `test-data/<set>/` after checking it against that set's manifest.
pub fn read_fixture(set: &str, relative: &str) -> Result<Vec<u8>, FixtureError> {
    let dir = fixture_dir(set);
    let entries = load_manifest(&dir)?;
    let entry = entries
        .iter()
        .find(|e| e.path == relative)
        .ok_or_else(|| FixtureError::NotInManifest(format!("{set}/{relative}")))?;
    read_checked(&dir, entry)
}

/// Files under `dir` (recursively, `/`-separated, sorted) that the manifest does not list.
/// `manifest.json` and `README.md` at the top level are the set's own description.
pub fn unlisted_files(dir: &Path) -> Result<Vec<String>, FixtureError> {
    let listed: std::collections::BTreeSet<String> = load_manifest(dir)?.into_iter().map(|e| e.path).collect();
    let mut found = Vec::new();
    collect_files(dir, "", &mut found)?;
    found.retain(|p| p != "manifest.json" && p != "README.md" && !listed.contains(p));
    found.sort();
    Ok(found)
}

fn collect_files(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), FixtureError> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_error(dir, e))? {
        let entry = entry.map_err(|e| io_error(dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".DS_Store" {
            continue;
        }
        let relative = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let kind = entry.file_type().map_err(|e| io_error(&entry.path(), e))?;
        if kind.is_dir() {
            collect_files(&entry.path(), &relative, out)?;
        } else {
            out.push(relative);
        }
    }
    Ok(())
}
