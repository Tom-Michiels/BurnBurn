//! The stored list under Archief ▸ Recente projecten: a JSON array of path strings in
//! `~/Library/Application Support/BurnBurn/recent-projects.json`. The path is a parameter, so
//! tests never touch the user's folder. A damaged file is an empty list, never a crash.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Larger files are not read: the list holds ten paths.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// File name of the list inside the application support folder (`.../BurnBurn/`).
pub const RECENT_FILE: &str = "recent-projects.json";

/// `<support>/BurnBurn/recent-projects.json` for the user's Application Support folder `support`.
pub fn recent_projects_file(support: &Path) -> PathBuf {
    support.join("BurnBurn").join(RECENT_FILE)
}

/// Title of a recent entry: the file name (the full path when it has none).
pub fn recent_title(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => {
            let full = path.to_string_lossy();
            if full.is_empty() { "?".to_owned() } else { full.into_owned() }
        }
    }
}

/// Read the stored list; a missing, unreadable, oversized or corrupted file is an empty list.
pub fn read_recent_projects(file: &Path) -> Vec<PathBuf> {
    let Ok(meta) = std::fs::metadata(file) else { return Vec::new() };
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(file) else { return Vec::new() };
    match serde_json::from_slice::<Vec<String>>(&bytes) {
        Ok(list) => list.into_iter().filter(|p| !p.is_empty()).map(PathBuf::from).collect(),
        Err(_) => Vec::new(),
    }
}

/// Write the list atomically (temporary file in the same folder, then rename), creating the
/// folder when missing. Paths that are not valid UTF-8 cannot be stored and are skipped.
pub fn write_recent_projects(file: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    let strings: Vec<&str> = paths.iter().filter_map(|p| p.to_str()).collect();
    let json = serde_json::to_vec_pretty(&strings).map_err(std::io::Error::other)?;
    let dir = file.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = file.file_name().map_or_else(|| RECENT_FILE.to_owned(), |n| n.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(&json)?;
        f.sync_all()?;
        std::fs::rename(&temp, file)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
