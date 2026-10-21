//! Profile library (W4-UI-gaten, P07): the machine profiles a bundle brought in, kept outside the
//! session in one file so they are still there at the next start.
//!
//! JSON (`format`, `schema`, `profiles`), written atomically through the [`FileSystem`] port. Each
//! profile is stored exactly like the frozen profiles of a project (same DTO, content hash
//! recomputed on load). A newer schema is refused with its version; two profiles with one id are
//! refused when saving (one machine twice).

use std::collections::BTreeSet;
use std::path::Path;

use burnburn_job::MachineProfile;
use serde::{Deserialize, Serialize};


use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::profile::{ProfileDto, profile_from_dto, profile_to_dto};
use crate::store::write_atomic;

pub const PROFILE_LIBRARY_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-profile-library";
const MAX_PROFILES: usize = 1_000;
const MAX_BYTES: u64 = 16 << 20;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    profiles: Vec<ProfileDto>,
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "profile library", detail }
    } else {
        LoadError::Json { entry: "profile library", detail }
    }
}

/// Bytes → profiles, fully validated.
pub fn decode_profile_library(bytes: &[u8]) -> Result<Vec<MachineProfile>, LoadError> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(PROFILE_LIBRARY_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: PROFILE_LIBRARY_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(LoadError::InvalidValue { path: "schema".into(), reason: format!("unknown schema {schema}") });
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;
    if dto.profiles.len() > MAX_PROFILES {
        return Err(LoadError::Limit("library profiles"));
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(dto.profiles.len());
    for (i, p) in dto.profiles.iter().enumerate() {
        let profile = profile_from_dto(i, p)?;
        if !seen.insert(profile.revision().id.clone()) {
            return Err(LoadError::InvalidValue {
                path: format!("profiles[{i}].id"),
                reason: format!("duplicate machine {:?}", profile.revision().id),
            });
        }
        out.push(profile);
    }
    Ok(out)
}

/// Profiles → bytes (compact JSON).
pub fn encode_profile_library(profiles: &[MachineProfile]) -> Result<Vec<u8>, SaveError> {
    let dto = FileDto {
        format: FORMAT.to_owned(),
        schema: PROFILE_LIBRARY_SCHEMA,
        profiles: profiles.iter().map(profile_to_dto).collect(),
    };
    serde_json::to_vec(&dto).map_err(|e| SaveError::Encode(e.to_string()))
}

/// Atomic save through the filesystem port. What is written is decoded again first, so a file
/// that would not load (duplicate machine, too many profiles) is never written.
pub fn save_profile_library(fs: &dyn FileSystem, path: &Path, profiles: &[MachineProfile]) -> Result<(), SaveError> {
    let bytes = encode_profile_library(profiles)?;
    decode_profile_library(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_profile_library(fs: &dyn FileSystem, path: &Path) -> Result<Vec<MachineProfile>, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_profile_library(&bytes)
}
