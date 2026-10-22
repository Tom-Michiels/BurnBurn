//! Camera calibrations kept between sessions (G7-werk W04): the lens calibrations and workspace
//! alignments of the [`CalibrationStore`] in one library file next to the other libraries.
//!
//! JSON (`format`, `schema`, `lenses`, `alignments`), written atomically through the
//! [`FileSystem`] port. Every entry carries the exact camera configuration it was made for (device,
//! resolution, pixel format, frame rate, lens profile); the store serves it for that configuration
//! only, so a calibration read back never applies to another source without a typed miss.
//!
//! The request identity of a lens calibration is a runtime correlation id (stale-result rejection),
//! not content: it is not stored and reads back as `0`. A store that would not load again (a
//! number that is not finite, an empty resolution, two entries for one configuration) is never
//! written; a damaged, foreign or newer file is a typed error and never an empty store.

use std::path::Path;

use burnburn_image::calib::{
    CalibrationStore, CameraCalibration, CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat, CheckPoint,
    Intrinsics, LensCalibration, LensFitReport, ResidualStats, WorkspaceAlignment,
};
use burnburn_types::{RequestId, StableId};
use serde::{Deserialize, Serialize};

use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::store::write_atomic;

pub const CAMERA_CALIBRATION_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-camera-calibrations";
const MAX_ENTRIES: usize = 10_000;
const MAX_BYTES: u64 = 64 << 20;
/// Largest material height the store accepts (a laser bed is never a metre deep).
const MAX_HEIGHT_MM: f64 = 1_000.0;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum PixelFormatDto {
    Bgra8,
    Yuv420Video,
    Other { code: u32 },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigDto {
    pub(crate) device: String,
    width: u32,
    height: u32,
    pixel_format: PixelFormatDto,
    frame_rate_mhz: u32,
    lens: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportDto {
    views: usize,
    points: usize,
    rms_px: f64,
    max_px: f64,
    rms_plane_mm: f64,
    evaluations: usize,
    converged: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LensDto {
    config: ConfigDto,
    center_px: [f64; 2],
    radius_px: f64,
    k1: f64,
    k2: f64,
    report: ReportDto,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatsDto {
    count: usize,
    rms_mm: f64,
    max_mm: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckPointDto {
    pixel: [f64; 2],
    bed_mm: [f64; 2],
    error_mm: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalibrationDto {
    config: ConfigDto,
    center_px: [f64; 2],
    radius_px: f64,
    k1: f64,
    k2: f64,
    homography: [[f64; 3]; 3],
    fit_residuals: StatsDto,
    residuals: StatsDto,
    check_points: Vec<CheckPointDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlignmentDto {
    material_height_mm: f64,
    calibration: CalibrationDto,
    lens_calibration: Option<LensDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    lenses: Vec<LensDto>,
    alignments: Vec<AlignmentDto>,
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "camera calibrations", detail }
    } else {
        LoadError::Json { entry: "camera calibrations", detail }
    }
}

fn invalid(path: String, reason: &str) -> LoadError {
    LoadError::InvalidValue { path, reason: reason.to_owned() }
}

// --- domain -> DTO -------------------------------------------------------------------------------

pub(crate) fn config_dto(c: &CameraConfigKey) -> ConfigDto {
    ConfigDto {
        device: c.device.0.clone(),
        width: c.mode.width,
        height: c.mode.height,
        pixel_format: match c.mode.pixel_format {
            CameraPixelFormat::Bgra8 => PixelFormatDto::Bgra8,
            CameraPixelFormat::Yuv420Video => PixelFormatDto::Yuv420Video,
            CameraPixelFormat::Other(code) => PixelFormatDto::Other { code },
        },
        frame_rate_mhz: c.mode.frame_rate_mhz,
        lens: c.lens.clone(),
    }
}

fn lens_dto(l: &LensCalibration) -> LensDto {
    LensDto {
        config: config_dto(&l.valid_for),
        center_px: l.intrinsics.center_px,
        radius_px: l.intrinsics.radius_px,
        k1: l.intrinsics.k1,
        k2: l.intrinsics.k2,
        report: ReportDto {
            views: l.report.views,
            points: l.report.points,
            rms_px: l.report.rms_px,
            max_px: l.report.max_px,
            rms_plane_mm: l.report.rms_plane_mm,
            evaluations: l.report.evaluations,
            converged: l.report.converged,
        },
    }
}

fn stats_dto(s: ResidualStats) -> StatsDto {
    StatsDto { count: s.count, rms_mm: s.rms_mm, max_mm: s.max_mm }
}

fn alignment_dto(a: &WorkspaceAlignment) -> AlignmentDto {
    let c = &a.calibration;
    AlignmentDto {
        material_height_mm: a.material_height_mm,
        calibration: CalibrationDto {
            config: config_dto(&c.valid_for),
            center_px: c.intrinsics.center_px,
            radius_px: c.intrinsics.radius_px,
            k1: c.intrinsics.k1,
            k2: c.intrinsics.k2,
            homography: c.bed_homography,
            fit_residuals: stats_dto(c.fit_residuals_mm),
            residuals: stats_dto(c.residuals_mm),
            check_points: c
                .check_points
                .iter()
                .map(|p| CheckPointDto { pixel: p.pixel, bed_mm: p.bed_mm, error_mm: p.error_mm })
                .collect(),
        },
        lens_calibration: a.lens_calibration.as_ref().map(lens_dto),
    }
}

// --- DTO -> domain (validated) ----------------------------------------------------------------------

fn finite(path: &str, values: &[f64]) -> Result<(), LoadError> {
    if values.iter().all(|v| v.is_finite()) { Ok(()) } else { Err(invalid(path.to_owned(), "a number is not finite")) }
}

pub(crate) fn config_of(path: &str, c: ConfigDto) -> Result<CameraConfigKey, LoadError> {
    if c.device.trim().is_empty() {
        return Err(invalid(format!("{path}.device"), "a calibration belongs to a camera"));
    }
    if c.width == 0 || c.height == 0 {
        return Err(invalid(format!("{path}.width"), "a picture has a size"));
    }
    if c.frame_rate_mhz == 0 {
        return Err(invalid(format!("{path}.frame_rate_mhz"), "a frame rate is above zero"));
    }
    if c.lens.trim().is_empty() {
        return Err(invalid(format!("{path}.lens"), "a lens profile has a name"));
    }
    let pixel_format = match c.pixel_format {
        PixelFormatDto::Bgra8 => CameraPixelFormat::Bgra8,
        PixelFormatDto::Yuv420Video => CameraPixelFormat::Yuv420Video,
        PixelFormatDto::Other { code } => CameraPixelFormat::Other(code),
    };
    Ok(CameraConfigKey {
        device: CameraDeviceId(c.device),
        mode: CameraMode { width: c.width, height: c.height, pixel_format, frame_rate_mhz: c.frame_rate_mhz },
        lens: c.lens,
    })
}

fn intrinsics_of(path: &str, center_px: [f64; 2], radius_px: f64, k1: f64, k2: f64) -> Result<Intrinsics, LoadError> {
    finite(path, &[center_px[0], center_px[1], radius_px, k1, k2])?;
    if radius_px <= 0.0 {
        return Err(invalid(format!("{path}.radius_px"), "the normalising radius is above zero"));
    }
    Ok(Intrinsics { center_px, radius_px, k1, k2 })
}

fn lens_of(path: &str, l: LensDto) -> Result<LensCalibration, LoadError> {
    let valid_for = config_of(&format!("{path}.config"), l.config)?;
    let intrinsics = intrinsics_of(path, l.center_px, l.radius_px, l.k1, l.k2)?;
    let r = l.report;
    finite(path, &[r.rms_px, r.max_px, r.rms_plane_mm])?;
    Ok(LensCalibration {
        request: RequestId::from_raw(0),
        valid_for,
        intrinsics,
        report: LensFitReport {
            views: r.views,
            points: r.points,
            rms_px: r.rms_px,
            max_px: r.max_px,
            rms_plane_mm: r.rms_plane_mm,
            evaluations: r.evaluations,
            converged: r.converged,
        },
    })
}

fn stats_of(path: &str, s: StatsDto) -> Result<ResidualStats, LoadError> {
    finite(path, &[s.rms_mm, s.max_mm])?;
    Ok(ResidualStats { count: s.count, rms_mm: s.rms_mm, max_mm: s.max_mm })
}

fn alignment_of(path: &str, a: AlignmentDto) -> Result<WorkspaceAlignment, LoadError> {
    if !a.material_height_mm.is_finite() || !(0.0..=MAX_HEIGHT_MM).contains(&a.material_height_mm) {
        return Err(invalid(format!("{path}.material_height_mm"), "a material height is from 0 to 1000 mm"));
    }
    let c = a.calibration;
    let cpath = format!("{path}.calibration");
    let valid_for = config_of(&format!("{cpath}.config"), c.config)?;
    let intrinsics = intrinsics_of(&cpath, c.center_px, c.radius_px, c.k1, c.k2)?;
    finite(&format!("{cpath}.homography"), &c.homography.iter().flatten().copied().collect::<Vec<_>>())?;
    let mut check_points = Vec::with_capacity(c.check_points.len());
    for p in c.check_points {
        finite(&format!("{cpath}.check_points"), &[p.pixel[0], p.pixel[1], p.bed_mm[0], p.bed_mm[1], p.error_mm])?;
        check_points.push(CheckPoint { pixel: p.pixel, bed_mm: p.bed_mm, error_mm: p.error_mm });
    }
    let lens_calibration = match a.lens_calibration {
        Some(l) => {
            let lens = lens_of(&format!("{path}.lens_calibration"), l)?;
            if lens.valid_for != valid_for {
                return Err(invalid(
                    format!("{path}.lens_calibration"),
                    "the lens calibration of an alignment is made for the same camera configuration",
                ));
            }
            Some(lens)
        }
        None => None,
    };
    Ok(WorkspaceAlignment {
        calibration: CameraCalibration {
            valid_for,
            intrinsics,
            bed_homography: c.homography,
            fit_residuals_mm: stats_of(&format!("{cpath}.fit_residuals"), c.fit_residuals)?,
            residuals_mm: stats_of(&format!("{cpath}.residuals"), c.residuals)?,
            check_points,
        },
        material_height_mm: a.material_height_mm,
        lens_calibration,
    })
}

/// Bytes → store, fully validated.
pub fn decode_camera_calibrations(bytes: &[u8]) -> Result<CalibrationStore, LoadError> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(CAMERA_CALIBRATION_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: CAMERA_CALIBRATION_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(invalid("schema".into(), &format!("unknown schema {schema}")));
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;
    if dto.lenses.len() > MAX_ENTRIES || dto.alignments.len() > MAX_ENTRIES {
        return Err(LoadError::Limit("camera calibrations"));
    }
    let mut store = CalibrationStore::default();
    for (i, l) in dto.lenses.into_iter().enumerate() {
        let path = format!("lenses[{i}]");
        if store.insert_lens(lens_of(&path, l)?).is_some() {
            return Err(invalid(path, "a second lens calibration for one camera configuration"));
        }
    }
    for (i, a) in dto.alignments.into_iter().enumerate() {
        let path = format!("alignments[{i}]");
        if store.insert_alignment(alignment_of(&path, a)?).is_some() {
            return Err(invalid(path, "a second alignment for one camera configuration and height"));
        }
    }
    Ok(store)
}

/// Store → bytes (compact JSON, canonical order of the store).
pub fn encode_camera_calibrations(store: &CalibrationStore) -> Result<Vec<u8>, SaveError> {
    let dto = FileDto {
        format: FORMAT.to_owned(),
        schema: CAMERA_CALIBRATION_SCHEMA,
        lenses: store.lenses().map(lens_dto).collect(),
        alignments: store.alignments().map(alignment_dto).collect(),
    };
    serde_json::to_vec(&dto).map_err(|e| SaveError::Encode(e.to_string()))
}

/// Atomic save. What is written is decoded again first, so a store that would not load (a number
/// that is not finite, an empty picture) is never written and the old file stays.
pub fn save_camera_calibrations(fs: &dyn FileSystem, path: &Path, store: &CalibrationStore) -> Result<(), SaveError> {
    let bytes = encode_camera_calibrations(store)?;
    decode_camera_calibrations(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_camera_calibrations(fs: &dyn FileSystem, path: &Path) -> Result<CalibrationStore, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_camera_calibrations(&bytes)
}
