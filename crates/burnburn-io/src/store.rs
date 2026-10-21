//! Save, load and recovery routes over the [`FileSystem`] port.

use std::path::{Path, PathBuf};

use crate::container::{MAX_CONTAINER_BYTES, SCHEMA_VERSION, container_schema, decode_container, encode_container};
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
    write_atomic(fs, path, &bytes)
}

/// The atomic write route shared by projects and small user files: temp file in the target
/// directory → write → sync → replace → sync the directory. A failure removes the temp file and
/// leaves the previous file at `path` untouched.
pub(crate) fn write_atomic(fs: &dyn FileSystem, path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let stem = path.file_name().and_then(|n| n.to_str()).unwrap_or("project");
    let tmp = fs.create_temp_in(dir, stem).map_err(SaveError::Fs)?;
    let staged = fs.write_all(&tmp, bytes).and_then(|()| fs.sync_file(&tmp)).and_then(|()| fs.replace(&tmp, path));
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

/// What [`save_project_keeping_previous`] did besides saving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    /// Where the previous file was copied to, when there was one worth keeping.
    pub backup: Option<PathBuf>,
}

/// Save that never destroys a file the current build cannot reproduce.
///
/// When a file already exists at `path`:
/// - written by an **older** schema, or **unreadable**: it is first copied byte for byte to
///   `<name>.schema<N>.bak` / `<name>.unreadable.bak` (same atomic route; an existing backup is never
///   replaced, a numbered name is used instead) and only then replaced. If the copy fails the
///   original stays and nothing is saved.
/// - written by a **newer** schema: refused with [`SaveError::NewerFileOnDisk`]; nothing changes.
/// - of the current schema: replaced as by [`save_project`], no backup.
pub fn save_project_keeping_previous(
    fs: &dyn FileSystem,
    path: &Path,
    project: &Project,
) -> Result<SaveOutcome, SaveError> {
    // Validate the new content first, so a failing project never costs a backup.
    let bytes = encode_container(project)?;
    decode_container(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;

    let mut backup = None;
    if fs.exists(path).map_err(SaveError::Fs)? {
        let old = fs.read(path, MAX_CONTAINER_BYTES).map_err(SaveError::Fs)?;
        let tag = match container_schema(&old) {
            Ok(found) if found > SCHEMA_VERSION => {
                return Err(SaveError::NewerFileOnDisk { found, supported: SCHEMA_VERSION });
            }
            Ok(found) if found == SCHEMA_VERSION => None,
            Ok(found) => Some(format!("schema{found}")),
            Err(_) => Some("unreadable".to_owned()),
        };
        if let Some(tag) = tag {
            let target = free_backup_path(fs, path, &tag)?;
            write_atomic(fs, &target, &old)?;
            backup = Some(target);
        }
    }
    write_atomic(fs, path, &bytes)?;
    Ok(SaveOutcome { backup })
}

/// `<name>.<tag>.bak`, or `<name>.<tag>.<n>.bak` when that exists already.
fn free_backup_path(fs: &dyn FileSystem, path: &Path, tag: &str) -> Result<PathBuf, SaveError> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("project");
    for n in 1..=999u32 {
        let candidate = if n == 1 { format!("{name}.{tag}.bak") } else { format!("{name}.{tag}.{n}.bak") };
        let candidate = path.with_file_name(candidate);
        if !fs.exists(&candidate).map_err(SaveError::Fs)? {
            return Ok(candidate);
        }
    }
    Err(SaveError::Invalid(format!("too many backups of {name} next to it; remove old .bak files")))
}
