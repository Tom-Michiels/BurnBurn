//! The remnant library (G7-werk, W07): leftover sheets that outlive the session, with material,
//! thickness, contour and the zones that stay out of use. A nest on a remnant (G6-werk) uses the
//! contour as the sheet and the zones as keep-out areas; what is cut from it is the operator's
//! business, orders count only recorded executions.
//!
//! JSON (`format`, `schema`, `next_id`, `remnants`), written atomically through the [`FileSystem`]
//! port. A remnant that would mean nothing (no name, no material, a thickness that is not a number,
//! a contour without area or crossing itself, a zone outside the sheet) is never written, and a file
//! that holds one is a typed error and never an empty library. Names are unique (case ignored) so an
//! order can name its remnant; ids are never reused, also not after a removal.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::store::write_atomic;

pub const REMNANT_LIBRARY_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-remnant-library";
const MAX_REMNANTS: usize = 10_000;
/// A sheet of a workshop is never described by more points than this.
const MAX_POINTS: usize = 2_000;
const MAX_ZONES: usize = 1_000;
const MAX_BYTES: u64 = 64 << 20;
/// Thickest sheet material a laser plan is made for (mm).
const MAX_THICKNESS_MM: f64 = 100.0;
/// Largest coordinate that is a measurement and not a typing mistake (mm).
const MAX_COORDINATE_MM: f64 = 100_000.0;

/// One remnant as it is stored. Coordinates are millimetres in the remnant's own frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemnantRecord {
    pub id: u64,
    pub name: String,
    pub material: String,
    pub thickness_mm: f64,
    /// The outline of the sheet: at least three corners, no repeated first point.
    pub contour: Vec<[f64; 2]>,
    /// Zones of the sheet that are not to be used (burnt, cracked, already cut).
    pub exclusions: Vec<Vec<[f64; 2]>>,
    pub note: String,
    /// `YYYY-MM-DD`.
    pub date: String,
}

/// The remnants and the next id to hand out. An id is never used twice, also not after the remnant
/// that had it was removed.
#[derive(Debug, Clone, PartialEq)]
pub struct RemnantLibrary {
    pub remnants: Vec<RemnantRecord>,
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
    remnants: Vec<RemnantRecord>,
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "remnant library", detail }
    } else {
        LoadError::Json { entry: "remnant library", detail }
    }
}

/// `YYYY-MM-DD`, nothing else.
fn is_iso_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digits = |r: std::ops::Range<usize>| bytes.get(r).is_some_and(|b| b.iter().all(u8::is_ascii_digit));
    bytes.len() == 10 && digits(0..4) && bytes[4] == b'-' && digits(5..7) && bytes[7] == b'-' && digits(8..10)
}

fn invalid(path: String, reason: &str) -> LoadError {
    LoadError::InvalidValue { path, reason: reason.to_owned() }
}

/// Twice the signed area of a polygon (shoelace).
fn double_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Do the segments `ab` and `cd` cross, or touch, anywhere?
fn segments_meet(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let cross = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0]);
    let on = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| {
        q[0] >= o[0].min(p[0]) && q[0] <= o[0].max(p[0]) && q[1] >= o[1].min(p[1]) && q[1] <= o[1].max(p[1])
    };
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    (d1 == 0.0 && on(c, d, a)) || (d2 == 0.0 && on(c, d, b)) || (d3 == 0.0 && on(a, b, c)) || (d4 == 0.0 && on(a, b, d))
}

/// Why `points` is not a polygon a sheet or a zone can have, if it is not.
fn polygon_problem(points: &[[f64; 2]]) -> Option<&'static str> {
    if points.len() < 3 {
        return Some("an outline has at least three corners");
    }
    if points.len() > MAX_POINTS {
        return Some("an outline has too many corners");
    }
    if points.iter().flatten().any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE_MM) {
        return Some("a coordinate is not a usable number");
    }
    if double_area(points) == 0.0 {
        return Some("an outline encloses an area");
    }
    let n = points.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            let (a, b) = (points[i], points[(i + 1) % n]);
            let (c, d) = (points[j], points[(j + 1) % n]);
            if adjacent {
                // Neighbouring edges share one corner; they must not overlap beyond it.
                let (shared, p, q) = if j == i + 1 { (b, a, d) } else { (a, b, c) };
                let (u, v) = ([p[0] - shared[0], p[1] - shared[1]], [q[0] - shared[0], q[1] - shared[1]]);
                let straight_back = u[0] * v[1] - u[1] * v[0] == 0.0 && u[0] * v[0] + u[1] * v[1] > 0.0;
                if straight_back {
                    return Some("an outline folds back on itself");
                }
            } else if segments_meet(a, b, c, d) {
                return Some("an outline crosses itself");
            }
        }
    }
    None
}

fn bounds_of(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    points.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    })
}

/// Why `remnant` means nothing, if it does.
fn check_one(i: usize, remnant: &RemnantRecord) -> Result<(), LoadError> {
    let at = |field: &str| format!("remnants[{i}].{field}");
    if remnant.name.trim().is_empty() {
        return Err(invalid(at("name"), "a remnant has a name"));
    }
    if remnant.material.trim().is_empty() {
        return Err(invalid(at("material"), "a remnant has a material"));
    }
    if !(remnant.thickness_mm.is_finite() && remnant.thickness_mm > 0.0 && remnant.thickness_mm <= MAX_THICKNESS_MM) {
        return Err(invalid(at("thickness_mm"), "a thickness is above zero and not more than 100 mm"));
    }
    if !is_iso_date(&remnant.date) {
        return Err(invalid(at("date"), "the date is YYYY-MM-DD"));
    }
    if let Some(why) = polygon_problem(&remnant.contour) {
        return Err(invalid(at("contour"), why));
    }
    if remnant.exclusions.len() > MAX_ZONES {
        return Err(LoadError::Limit("remnant exclusion zones"));
    }
    let (lo, hi) = bounds_of(&remnant.contour);
    for (z, zone) in remnant.exclusions.iter().enumerate() {
        if let Some(why) = polygon_problem(zone) {
            return Err(invalid(format!("remnants[{i}].exclusions[{z}]"), why));
        }
        let (zlo, zhi) = bounds_of(zone);
        if zlo[0] < lo[0] || zlo[1] < lo[1] || zhi[0] > hi[0] || zhi[1] > hi[1] {
            return Err(invalid(format!("remnants[{i}].exclusions[{z}]"), "a zone lies within the sheet"));
        }
    }
    Ok(())
}

/// Does this library mean something? The same checks a file is read with and written after.
pub fn validate_remnant_library(library: &RemnantLibrary) -> Result<(), LoadError> {
    if library.remnants.len() > MAX_REMNANTS {
        return Err(LoadError::Limit("remnants"));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for (i, r) in library.remnants.iter().enumerate() {
        check_one(i, r)?;
        if !ids.insert(r.id) {
            return Err(invalid(format!("remnants[{i}].id"), "two remnants with one id"));
        }
        if !names.insert(r.name.trim().to_lowercase()) {
            return Err(invalid(format!("remnants[{i}].name"), "two remnants with one name"));
        }
    }
    let after_highest = library.remnants.iter().map(|r| r.id).max().map_or(1, |m| m.saturating_add(1));
    if library.next_id < after_highest {
        return Err(invalid("next_id".into(), "the next id would be one that is in use"));
    }
    Ok(())
}

/// Bytes → remnants, fully validated.
pub fn decode_remnant_library(bytes: &[u8]) -> Result<RemnantLibrary, LoadError> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(REMNANT_LIBRARY_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: REMNANT_LIBRARY_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(invalid("schema".into(), &format!("unknown schema {schema}")));
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;
    let after_highest = dto.remnants.iter().map(|r| r.id).max().map_or(1, |m| m.saturating_add(1));
    let library = RemnantLibrary { next_id: dto.next_id.unwrap_or(after_highest), remnants: dto.remnants };
    validate_remnant_library(&library)?;
    Ok(library)
}

/// Remnants → bytes (compact JSON). Not validated here: [`save_remnant_library`] decodes what it wrote.
pub fn encode_remnant_library(library: &RemnantLibrary) -> Result<Vec<u8>, SaveError> {
    let dto = FileDto {
        format: FORMAT.to_owned(),
        schema: REMNANT_LIBRARY_SCHEMA,
        next_id: Some(library.next_id),
        remnants: library.remnants.clone(),
    };
    serde_json::to_vec(&dto).map_err(|e| SaveError::Encode(e.to_string()))
}

/// Atomic save. What is written is decoded again first, so a library that would not load (a
/// duplicate name, a contour that crosses itself) is never written and the old file stays.
pub fn save_remnant_library(fs: &dyn FileSystem, path: &Path, library: &RemnantLibrary) -> Result<(), SaveError> {
    let bytes = encode_remnant_library(library)?;
    decode_remnant_library(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_remnant_library(fs: &dyn FileSystem, path: &Path) -> Result<RemnantLibrary, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_remnant_library(&bytes)
}
