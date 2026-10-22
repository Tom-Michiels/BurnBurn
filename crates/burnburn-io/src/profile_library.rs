//! Profile library (W4-UI-gaten, P07): the machine profiles a bundle brought in, kept outside the
//! session in one file so they are still there at the next start.
//!
//! JSON (`format`, `schema`, `profiles`), written atomically through the [`FileSystem`] port. Each
//! profile is stored exactly like the frozen profiles of a project (same DTO, content hash
//! recomputed on load). A newer schema is refused with its version; two profiles with one id are
//! refused when saving (one machine twice).

use std::collections::BTreeSet;
use std::path::Path;

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate};
use burnburn_job::MachineProfile;
use serde::{Deserialize, Serialize};

use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::profile::{ProfileDto, profile_from_dto, profile_to_dto};
use crate::store::write_atomic;

/// The newest schema this version reads and writes. Schema 1 has profiles only; schema 2 adds the
/// `settings` section (GRBL dialect, declared model and firmware). A library without settings is still written as
/// schema 1, byte-identical to earlier versions.
pub const PROFILE_LIBRARY_SCHEMA: u32 = 2;
const SCHEMA_PLAIN: u32 = 1;
const FORMAT: &str = "burnburn-profile-library";
const MAX_PROFILES: usize = 1_000;
const MAX_BYTES: u64 = 16 << 20;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    profiles: Vec<ProfileDto>,
    /// Settings kept next to the profiles, per machine id (schema 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    settings: Vec<SettingsDto>,
}

/// What belongs to a machine but not to its profile: the GRBL dialect and the identity the user
/// declared. Each part is omitted when absent.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsDto {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    grbl: Option<GrblDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    firmware: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrblDto {
    max_spindle: u32,
    laser_mode: bool,
    power_mode: String,
    air_assist: bool,
    work_coordinate: Option<String>,
    decimals: u8,
}

fn grbl_to_dto(g: &GrblProfile) -> GrblDto {
    GrblDto {
        max_spindle: g.max_spindle,
        laser_mode: g.laser_mode,
        power_mode: match g.power_mode {
            PowerMode::Constant => "constant",
            PowerMode::Dynamic => "dynamic",
        }
        .to_owned(),
        air_assist: g.air_assist,
        work_coordinate: g.work_coordinate.map(|w| format!("{w:?}")),
        decimals: g.decimals,
    }
}

fn grbl_from_dto(i: usize, d: &GrblDto) -> Result<GrblProfile, LoadError> {
    let bad = |field: &str, reason: String| LoadError::InvalidValue { path: format!("settings[{i}].grbl.{field}"), reason };
    let power_mode = match d.power_mode.as_str() {
        "constant" => PowerMode::Constant,
        "dynamic" => PowerMode::Dynamic,
        other => return Err(bad("power_mode", format!("unknown power mode {other:?}"))),
    };
    let work_coordinate = match d.work_coordinate.as_deref() {
        None => None,
        Some("G54") => Some(WorkCoordinate::G54),
        Some("G55") => Some(WorkCoordinate::G55),
        Some("G56") => Some(WorkCoordinate::G56),
        Some("G57") => Some(WorkCoordinate::G57),
        Some("G58") => Some(WorkCoordinate::G58),
        Some("G59") => Some(WorkCoordinate::G59),
        Some(other) => return Err(bad("work_coordinate", format!("unknown work coordinate system {other:?}"))),
    };
    let grbl = GrblProfile {
        max_spindle: d.max_spindle,
        laser_mode: d.laser_mode,
        power_mode,
        air_assist: d.air_assist,
        work_coordinate,
        decimals: d.decimals,
    };
    grbl.validate().map_err(|e| bad("max_spindle", e.to_string()))?;
    Ok(grbl)
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "profile library", detail }
    } else {
        LoadError::Json { entry: "profile library", detail }
    }
}

/// One machine of the profile library: its profile and, for a GRBL machine, the dialect settings.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryProfile {
    pub profile: MachineProfile,
    /// `None` is a Ruida machine.
    pub grbl: Option<GrblProfile>,
    /// What the user declared about the controller (never what a controller replied). Blank text
    /// is not a declaration.
    pub model: Option<String>,
    pub firmware: Option<String>,
}

/// Blank text is "not declared".
fn declared(text: &Option<String>) -> Option<String> {
    text.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// Bytes → profiles, fully validated. The dialect settings of GRBL machines are not part of the
/// result; use [`decode_profile_library_entries`] for them.
pub fn decode_profile_library(bytes: &[u8]) -> Result<Vec<MachineProfile>, LoadError> {
    Ok(decode_profile_library_entries(bytes)?.into_iter().map(|e| e.profile).collect())
}

/// Bytes → machines with their dialect settings, fully validated.
pub fn decode_profile_library_entries(bytes: &[u8]) -> Result<Vec<LibraryProfile>, LoadError> {
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
        out.push(LibraryProfile { profile, grbl: None, model: None, firmware: None });
    }
    let mut setting_ids = BTreeSet::new();
    for (i, d) in dto.settings.iter().enumerate() {
        let grbl = d.grbl.as_ref().map(|g| grbl_from_dto(i, g)).transpose()?;
        let Some(entry) = out.iter_mut().find(|e| e.profile.revision().id == d.id) else {
            return Err(LoadError::InvalidValue {
                path: format!("settings[{i}].id"),
                reason: format!("settings for unknown machine {:?}", d.id),
            });
        };
        if !setting_ids.insert(d.id.clone()) {
            return Err(LoadError::InvalidValue {
                path: format!("settings[{i}].id"),
                reason: format!("two sets of settings for machine {:?}", d.id),
            });
        }
        entry.grbl = grbl;
        entry.model = declared(&d.model);
        entry.firmware = declared(&d.firmware);
    }
    Ok(out)
}

/// Profiles → bytes (compact JSON), without dialect settings (schema 1).
pub fn encode_profile_library(profiles: &[MachineProfile]) -> Result<Vec<u8>, SaveError> {
    let entries: Vec<LibraryProfile> =
        profiles.iter().cloned().map(|profile| LibraryProfile { profile, grbl: None, model: None, firmware: None }).collect();
    encode_profile_library_entries(&entries)
}

/// Machines → bytes (compact JSON). Schema 2 only when a machine has dialect settings.
pub fn encode_profile_library_entries(entries: &[LibraryProfile]) -> Result<Vec<u8>, SaveError> {
    let settings: Vec<SettingsDto> = entries
        .iter()
        .filter_map(|e| {
            let (model, firmware) = (declared(&e.model), declared(&e.firmware));
            if e.grbl.is_none() && model.is_none() && firmware.is_none() {
                return None;
            }
            Some(SettingsDto {
                id: e.profile.revision().id.clone(),
                grbl: e.grbl.as_ref().map(grbl_to_dto),
                model,
                firmware,
            })
        })
        .collect();
    let dto = FileDto {
        format: FORMAT.to_owned(),
        schema: if settings.is_empty() { SCHEMA_PLAIN } else { PROFILE_LIBRARY_SCHEMA },
        profiles: entries.iter().map(|e| profile_to_dto(&e.profile)).collect(),
        settings,
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
/// Atomic save of machines with their dialect settings; like [`save_profile_library`] the bytes
/// are decoded again first, so an invalid file is never written.
pub fn save_profile_library_entries(
    fs: &dyn FileSystem,
    path: &Path,
    entries: &[LibraryProfile],
) -> Result<(), SaveError> {
    let bytes = encode_profile_library_entries(entries)?;
    decode_profile_library_entries(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_profile_library_entries(fs: &dyn FileSystem, path: &Path) -> Result<Vec<LibraryProfile>, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_profile_library_entries(&bytes)
}
