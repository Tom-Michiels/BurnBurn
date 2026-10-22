//! Calibration proofs (W09): the kerf, scan-offset and pointer-offset measurements of a machine,
//! kept outside the session in one file next to the other libraries.
//!
//! JSON (`format`, `schema`, `proofs`), written atomically through the [`FileSystem`] port. A proof
//! says what was measured, on which machine, with which material (kerf), at which speed (scan
//! offset), how and when. A list that would not load again (a duplicate id, a value that means
//! nothing) is never written.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::store::write_atomic;

pub const CALIBRATION_LIBRARY_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-calibration-library";
const MAX_PROOFS: usize = 100_000;
const MAX_BYTES: u64 = 64 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofKindRecord {
    Kerf,
    ScanOffset,
    PointerOffset,
}

/// One measurement as it is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofRecord {
    pub id: u64,
    pub kind: ProofKindRecord,
    /// The machine profile id the measurement belongs to.
    pub machine: String,
    /// The material of a kerf measurement; empty for the other kinds.
    pub material: String,
    /// The speed a scan offset was measured at.
    pub speed_mm_s: Option<f64>,
    /// Kerf, scan offset, or the x of the pointer offset (mm).
    pub value_mm: f64,
    /// The y of the pointer offset (mm).
    pub value_y_mm: Option<f64>,
    pub method: String,
    /// `YYYY-MM-DD`.
    pub date: String,
}

/// The proofs and the next id to hand out. An id is never used twice, also not after the proof
/// that had it was removed: a plan names the proofs it used by id.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationLibrary {
    pub proofs: Vec<ProofRecord>,
    pub next_id: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    /// Absent in a file written without it: then it continues after the highest id.
    #[serde(default)]
    next_id: Option<u64>,
    proofs: Vec<ProofRecord>,
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "calibration library", detail }
    } else {
        LoadError::Json { entry: "calibration library", detail }
    }
}

/// `YYYY-MM-DD`, nothing else.
fn is_iso_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digits = |r: std::ops::Range<usize>| bytes.get(r).is_some_and(|b| b.iter().all(u8::is_ascii_digit));
    bytes.len() == 10 && digits(0..4) && bytes[4] == b'-' && digits(5..7) && bytes[7] == b'-' && digits(8..10)
}

/// Why `proof` means nothing, if it does not.
fn problem(proof: &ProofRecord) -> Option<&'static str> {
    let finite = |v: f64| v.is_finite();
    if proof.machine.trim().is_empty() {
        return Some("a proof belongs to a machine");
    }
    if proof.method.trim().is_empty() {
        return Some("the method of a proof is on record");
    }
    if !is_iso_date(&proof.date) {
        return Some("the date is YYYY-MM-DD");
    }
    if !finite(proof.value_mm) {
        return Some("the value is not a number");
    }
    match proof.kind {
        ProofKindRecord::Kerf => {
            if proof.material.trim().is_empty() {
                return Some("a kerf belongs to a material");
            }
            if proof.value_mm < 0.0 {
                return Some("a kerf is not negative");
            }
        }
        ProofKindRecord::ScanOffset => match proof.speed_mm_s {
            Some(speed) if finite(speed) && speed > 0.0 => {}
            _ => return Some("a scan offset is measured at a speed above zero"),
        },
        ProofKindRecord::PointerOffset => match proof.value_y_mm {
            Some(y) if finite(y) => {}
            _ => return Some("a pointer offset has an x and a y"),
        },
    }
    None
}

fn check(proofs: &[ProofRecord]) -> Result<(), LoadError> {
    if proofs.len() > MAX_PROOFS {
        return Err(LoadError::Limit("calibration proofs"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (i, p) in proofs.iter().enumerate() {
        if let Some(reason) = problem(p) {
            return Err(LoadError::InvalidValue { path: format!("proofs[{i}]"), reason: reason.into() });
        }
        if !seen.insert(p.id) {
            return Err(LoadError::InvalidValue {
                path: format!("proofs[{i}].id"),
                reason: format!("two proofs with id {}", p.id),
            });
        }
    }
    Ok(())
}

/// Bytes → proofs, fully validated.
pub fn decode_calibration_library(bytes: &[u8]) -> Result<CalibrationLibrary, LoadError> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(CALIBRATION_LIBRARY_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: CALIBRATION_LIBRARY_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(LoadError::InvalidValue { path: "schema".into(), reason: format!("unknown schema {schema}") });
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;
    check(&dto.proofs)?;
    let after_highest = dto.proofs.iter().map(|p| p.id).max().map_or(1, |m| m.saturating_add(1));
    let next_id = dto.next_id.unwrap_or(after_highest);
    if next_id < after_highest {
        return Err(LoadError::InvalidValue {
            path: "next_id".into(),
            reason: "the next id would be one that is in use".into(),
        });
    }
    Ok(CalibrationLibrary { proofs: dto.proofs, next_id })
}

/// Proofs → bytes (compact JSON).
pub fn encode_calibration_library(library: &CalibrationLibrary) -> Result<Vec<u8>, SaveError> {
    let dto = FileDto {
        format: FORMAT.to_owned(),
        schema: CALIBRATION_LIBRARY_SCHEMA,
        next_id: Some(library.next_id),
        proofs: library.proofs.clone(),
    };
    serde_json::to_vec(&dto).map_err(|e| SaveError::Encode(e.to_string()))
}

/// Atomic save. What is written is decoded again first, so a list that would not load (a
/// duplicate id, a value that means nothing) is never written and the old file stays.
pub fn save_calibration_library(
    fs: &dyn FileSystem,
    path: &Path,
    library: &CalibrationLibrary,
) -> Result<(), SaveError> {
    let bytes = encode_calibration_library(library)?;
    decode_calibration_library(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_calibration_library(fs: &dyn FileSystem, path: &Path) -> Result<CalibrationLibrary, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_calibration_library(&bytes)
}
