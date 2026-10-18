//! Save, load and recovery routes over the [`FileSystem`] port.

use std::path::{Path, PathBuf};

use crate::container::{MAX_CONTAINER_BYTES, decode_container, encode_container};
use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::project::{Project, ProjectUuid};

/// Explicit Save: temp file in the target directory → write → sync → atomic replace → sync dir.
///
/// When any step fails the temp file is removed (best effort) and the previous file at `path`
/// is untouched and readable. This is crash consistency at the API level; durability across a
/// power cut depends on the OS and is not claimed beyond the adapter notes in `std_fs`.
pub fn save_project(fs: &dyn FileSystem, path: &Path, project: &Project) -> Result<(), SaveError> {
    let bytes = encode_container(project)?;
    // Never write what we could not read back: the same validation as opening.
    decode_container(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let stem = path.file_name().and_then(|n| n.to_str()).unwrap_or("project");
    let tmp = fs.create_temp_in(dir, stem).map_err(SaveError::Fs)?;
    let staged = fs.write_all(&tmp, &bytes).and_then(|()| fs.sync_file(&tmp)).and_then(|()| fs.replace(&tmp, path));
    if let Err(e) = staged {
        // The original error matters; a failing cleanup leaves only a hidden temp file.
        let _ = fs.remove_file(&tmp);
        return Err(SaveError::Fs(e));
    }
    fs.sync_dir(dir).map_err(SaveError::Unconfirmed)
}

/// Opens a project; nothing on disk is modified.
pub fn load_project(fs: &dyn FileSystem, path: &Path) -> Result<Project, LoadError> {
    let bytes = fs.read(path, MAX_CONTAINER_BYTES).map_err(LoadError::Fs)?;
    decode_container(&bytes)
}

/// Where the recovery copy of `uuid` lives inside the application's recovery directory.
pub fn recovery_path(recovery_dir: &Path, uuid: ProjectUuid) -> PathBuf {
    recovery_dir.join(format!("{uuid}.burnburn"))
}

/// Writes a recovery copy (same format, same atomic route). Never touches the saved file.
pub fn write_recovery(fs: &dyn FileSystem, recovery_dir: &Path, project: &Project) -> Result<PathBuf, SaveError> {
    let path = recovery_path(recovery_dir, project.uuid);
    save_project(fs, &path, project)?;
    Ok(path)
}

/// Reads a recovery copy as a fresh process would (no in-memory state is involved).
pub fn read_recovery(
    fs: &dyn FileSystem,
    recovery_dir: &Path,
    uuid: ProjectUuid,
) -> Result<Option<Project>, LoadError> {
    let path = recovery_path(recovery_dir, uuid);
    if !fs.exists(&path).map_err(LoadError::Fs)? {
        return Ok(None);
    }
    load_project(fs, &path).map(Some)
}

/// Removes the recovery copy (after the user saved or discarded). Missing is not an error.
pub fn discard_recovery(fs: &dyn FileSystem, recovery_dir: &Path, uuid: ProjectUuid) -> Result<(), SaveError> {
    let path = recovery_path(recovery_dir, uuid);
    if fs.exists(&path).map_err(SaveError::Fs)? {
        fs.remove_file(&path).map_err(SaveError::Fs)?;
    }
    Ok(())
}
