//! Workspace alignments and the calibration registry (W2-L5e).
//!
//! A *workspace alignment* ties a camera configuration (and optionally the lens calibration made for
//! it) to bed millimetres at one material height. The [`CalibrationStore`] keeps many of them, keyed by
//! the exact camera configuration, and refuses to serve one for any other configuration: a changed
//! device, resolution, pixel format, frame rate or lens profile is a typed [`StoreMiss`], never a
//! silent reuse. Keys are `Ord`, the maps are `BTreeMap`s, and [`CanonicalEncode`] gives a byte form that
//! does not depend on insertion order (a future io DTO maps onto the same content).

use std::collections::BTreeMap;

use burnburn_types::{CanonicalEncode, CanonicalWriter};

use super::lens::LensCalibration;
use super::{
    CalibError, CalibParams, CameraCalibration, CameraConfigKey, CameraDeviceId, Invalidity, LensModel,
    MarkObservation, calibrate, config_validity,
};

/// Largest material height accepted (a laser bed is never a metre deep).
const MAX_HEIGHT_MM: f64 = 1_000.0;
/// Heights are keyed in whole micrometres so that equal heights compare equal.
const HEIGHT_KEY_PER_MM: f64 = 1_000.0;

fn height_key(mm: f64) -> Option<i64> {
    (mm.is_finite() && (0.0..=MAX_HEIGHT_MM).contains(&mm)).then(|| (mm * HEIGHT_KEY_PER_MM).round() as i64)
}

/// Camera pixels to bed millimetres at one material height, with the lens it was made with.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceAlignment {
    /// Homography, intrinsics, residuals and the configuration it is valid for.
    pub calibration: CameraCalibration,
    /// Height of the material surface above the bed the marks were placed on.
    pub material_height_mm: f64,
    /// Set when the radial coefficients came from a pattern calibration rather than a profile.
    pub lens_calibration: Option<LensCalibration>,
}

/// Where the lens model of an alignment comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum AlignmentLens {
    /// A lens profile or an estimate from the marks themselves.
    Profile(LensModel),
    /// A pattern calibration of the same camera configuration.
    Calibrated(LensCalibration),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlignError {
    Calibration(CalibError),
    /// Negative, non-finite or absurd material height.
    BadHeight,
    /// The lens calibration was made for another camera configuration.
    LensCalibrationInvalid(Invalidity),
}

impl WorkspaceAlignment {
    /// Fits the alignment from user-placed marks (see [`calibrate`]) at `material_height_mm`.
    pub fn align(
        observations: &[MarkObservation],
        cfg: CameraConfigKey,
        lens: AlignmentLens,
        material_height_mm: f64,
        max_fit_residual_mm: f64,
    ) -> Result<WorkspaceAlignment, AlignError> {
        if height_key(material_height_mm).is_none() {
            return Err(AlignError::BadHeight);
        }
        let (model, lens_calibration) = match lens {
            AlignmentLens::Profile(model) => (model, None),
            AlignmentLens::Calibrated(cal) => {
                config_validity(&cal.valid_for, &cfg).map_err(AlignError::LensCalibrationInvalid)?;
                (LensModel::FixedRadial { k1: cal.intrinsics.k1, k2: cal.intrinsics.k2 }, Some(cal))
            }
        };
        let calibration = calibrate(observations, cfg, CalibParams { lens: model, max_fit_residual_mm })
            .map_err(AlignError::Calibration)?;
        Ok(WorkspaceAlignment { calibration, material_height_mm, lens_calibration })
    }

    /// The configuration this alignment is valid for.
    pub fn config(&self) -> &CameraConfigKey {
        &self.calibration.valid_for
    }

    /// Bed position (mm, at this alignment's material height) of an image pixel; refused when the
    /// camera configuration changed.
    pub fn pixel_to_bed(&self, current: &CameraConfigKey, pixel: [f64; 2]) -> Result<[f64; 2], Invalidity> {
        self.calibration.pixel_to_bed(current, pixel)
    }

    /// Image pixel that shows a bed position (the inverse of [`WorkspaceAlignment::pixel_to_bed`]);
    /// `None` when the position cannot be projected.
    pub fn bed_to_pixel(&self, current: &CameraConfigKey, bed: [f64; 2]) -> Result<Option<[f64; 2]>, Invalidity> {
        self.calibration.bed_to_pixel(current, bed)
    }
}

/// Why the store has no usable entry for a request.
#[derive(Debug, Clone, PartialEq)]
pub enum StoreMiss {
    /// Nothing was ever calibrated for this device.
    NotCalibrated,
    /// The device has calibrations, but none for this configuration; the reason is the first difference
    /// to the closest stored configuration (fewest differing settings).
    Invalid(Invalidity),
    /// The configuration is calibrated, but not at this material height.
    NoAlignmentForHeight { available_mm: Vec<f64> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeightLookupError {
    Miss(StoreMiss),
    /// Heights are interpolated between two alignments, never extrapolated.
    OutsideCalibratedRange {
        min_mm: f64,
        max_mm: f64,
    },
    /// A stored alignment could not project the pixel (degenerate homography).
    Unmappable,
}

/// All lens calibrations and workspace alignments, per exact camera configuration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CalibrationStore {
    lenses: BTreeMap<CameraConfigKey, LensCalibration>,
    /// Keyed by configuration and material height in micrometres.
    alignments: BTreeMap<(CameraConfigKey, i64), WorkspaceAlignment>,
}

/// Number of settings in which two configurations differ (0 = identical).
fn differences(a: &CameraConfigKey, b: &CameraConfigKey) -> usize {
    usize::from(a.device != b.device)
        + usize::from((a.mode.width, a.mode.height) != (b.mode.width, b.mode.height))
        + usize::from(a.mode.pixel_format != b.mode.pixel_format)
        + usize::from(a.mode.frame_rate_mhz != b.mode.frame_rate_mhz)
        + usize::from(a.lens != b.lens)
}

/// The miss for `cfg` given the stored configurations (in canonical order).
fn miss<'a>(stored: impl Iterator<Item = &'a CameraConfigKey>, cfg: &CameraConfigKey) -> StoreMiss {
    let closest = stored.filter(|k| k.device == cfg.device).min_by_key(|k| differences(k, cfg)); // `min_by_key` keeps the first of equals: canonical order
    match closest {
        None => StoreMiss::NotCalibrated,
        Some(k) => match config_validity(k, cfg) {
            Err(why) => StoreMiss::Invalid(why),
            // Identical configuration: the caller found it, so this is only reached defensively.
            Ok(()) => StoreMiss::NotCalibrated,
        },
    }
}

impl CalibrationStore {
    /// Stores a lens calibration for its configuration; returns the one it replaced.
    pub fn insert_lens(&mut self, lens: LensCalibration) -> Option<LensCalibration> {
        self.lenses.insert(lens.valid_for.clone(), lens)
    }

    /// Stores an alignment for its configuration and material height; returns the one it replaced.
    /// An alignment with an impossible height (not constructible through [`WorkspaceAlignment::align`])
    /// is not stored.
    pub fn insert_alignment(&mut self, alignment: WorkspaceAlignment) -> Option<WorkspaceAlignment> {
        let key = height_key(alignment.material_height_mm)?;
        self.alignments.insert((alignment.config().clone(), key), alignment)
    }

    pub fn lens_count(&self) -> usize {
        self.lenses.len()
    }

    pub fn alignment_count(&self) -> usize {
        self.alignments.len()
    }

    /// The lens calibration made for exactly this configuration.
    pub fn lens_for(&self, cfg: &CameraConfigKey) -> Result<&LensCalibration, StoreMiss> {
        self.lenses.get(cfg).ok_or_else(|| miss(self.lenses.keys(), cfg))
    }

    /// The alignment made for exactly this configuration at exactly this material height.
    pub fn alignment_for(&self, cfg: &CameraConfigKey, height_mm: f64) -> Result<&WorkspaceAlignment, StoreMiss> {
        let configured = self.alignments_for_config(cfg);
        if let Some(key) = height_key(height_mm)
            && let Some(found) = self.alignments.get(&(cfg.clone(), key))
        {
            return Ok(found);
        }
        if configured.is_empty() {
            return Err(miss(self.alignments.keys().map(|(k, _)| k), cfg));
        }
        Err(StoreMiss::NoAlignmentForHeight { available_mm: configured.iter().map(|a| a.material_height_mm).collect() })
    }

    /// Alignments of exactly this configuration, by ascending material height.
    pub fn alignments_for_config(&self, cfg: &CameraConfigKey) -> Vec<&WorkspaceAlignment> {
        let mut found: Vec<(&i64, &WorkspaceAlignment)> =
            self.alignments.iter().filter(|((k, _), _)| k == cfg).map(|((_, h), a)| (h, a)).collect();
        found.sort_by_key(|(h, _)| **h);
        found.into_iter().map(|(_, a)| a).collect()
    }

    /// Every alignment of a camera at a resolution, whatever the frame rate, pixel format, lens profile
    /// or height, in canonical order (pixel format, frame rate, lens, height).
    pub fn alignments_for_resolution(
        &self,
        device: &CameraDeviceId,
        width: u32,
        height: u32,
    ) -> Vec<&WorkspaceAlignment> {
        self.alignments
            .iter()
            .filter(|((k, _), _)| k.device == *device && (k.mode.width, k.mode.height) == (width, height))
            .map(|(_, a)| a)
            .collect()
    }

    /// Every lens calibration of a camera at a resolution, in canonical order.
    pub fn lenses_for_resolution(&self, device: &CameraDeviceId, width: u32, height: u32) -> Vec<&LensCalibration> {
        self.lenses
            .iter()
            .filter(|(k, _)| k.device == *device && (k.mode.width, k.mode.height) == (width, height))
            .map(|(_, l)| l)
            .collect()
    }

    /// Bed position of a pixel for material at `height_mm`: the stored alignment at exactly that height,
    /// or the linear blend of the two alignments around it. A camera ray crosses the planes at different
    /// heights along a straight line, so the bed position is linear in the height. Never extrapolates.
    pub fn pixel_to_bed_at_height(
        &self,
        cfg: &CameraConfigKey,
        pixel: [f64; 2],
        height_mm: f64,
    ) -> Result<[f64; 2], HeightLookupError> {
        let set = self.alignments_for_config(cfg);
        let (Some(first), Some(last)) = (set.first(), set.last()) else {
            return Err(HeightLookupError::Miss(miss(self.alignments.keys().map(|(k, _)| k), cfg)));
        };
        let (min_mm, max_mm) = (first.material_height_mm, last.material_height_mm);
        if !height_mm.is_finite() || height_mm < min_mm || height_mm > max_mm {
            return Err(HeightLookupError::OutsideCalibratedRange { min_mm, max_mm });
        }
        let at = |a: &WorkspaceAlignment| -> Result<[f64; 2], HeightLookupError> {
            let p = a.pixel_to_bed(cfg, pixel).map_err(|_| HeightLookupError::Unmappable)?;
            if p.iter().all(|v| v.is_finite()) { Ok(p) } else { Err(HeightLookupError::Unmappable) }
        };
        let upper = set.iter().position(|a| a.material_height_mm >= height_mm).unwrap_or(set.len() - 1);
        let hi = set[upper];
        if hi.material_height_mm == height_mm || upper == 0 {
            return at(hi);
        }
        let lo = set[upper - 1];
        let t = (height_mm - lo.material_height_mm) / (hi.material_height_mm - lo.material_height_mm);
        let (a, b) = (at(lo)?, at(hi)?);
        Ok([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])])
    }
}

impl CanonicalEncode for CameraConfigKey {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("camera-config-v1");
        out.str(&self.device.0);
        out.u64(u64::from(self.mode.width));
        out.u64(u64::from(self.mode.height));
        match self.mode.pixel_format {
            super::CameraPixelFormat::Bgra8 => out.u64(0),
            super::CameraPixelFormat::Yuv420Video => out.u64(1),
            super::CameraPixelFormat::Other(code) => {
                out.u64(2);
                out.u64(u64::from(code));
            }
        }
        out.u64(u64::from(self.mode.frame_rate_mhz));
        out.str(&self.lens);
    }
}

impl CanonicalEncode for CameraCalibration {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("camera-calibration-v1");
        self.valid_for.encode(out);
        for v in self.intrinsics.center_px {
            out.f64(v);
        }
        out.f64(self.intrinsics.radius_px);
        out.f64(self.intrinsics.k1);
        out.f64(self.intrinsics.k2);
        for v in self.bed_homography.iter().flatten() {
            out.f64(*v);
        }
        for stats in [self.fit_residuals_mm, self.residuals_mm] {
            out.u64(stats.count as u64);
            out.f64(stats.rms_mm);
            out.f64(stats.max_mm);
        }
        out.u64(self.check_points.len() as u64);
        for c in &self.check_points {
            for v in c.pixel.iter().chain(&c.bed_mm) {
                out.f64(*v);
            }
            out.f64(c.error_mm);
        }
    }
}

/// The request identity is a runtime correlation id (stale-result rejection), not content: it is not
/// part of the canonical form.
impl CanonicalEncode for LensCalibration {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("lens-calibration-v1");
        self.valid_for.encode(out);
        for v in self.intrinsics.center_px {
            out.f64(v);
        }
        out.f64(self.intrinsics.radius_px);
        out.f64(self.intrinsics.k1);
        out.f64(self.intrinsics.k2);
        out.u64(self.report.views as u64);
        out.u64(self.report.points as u64);
        out.f64(self.report.rms_px);
        out.f64(self.report.max_px);
        out.f64(self.report.rms_plane_mm);
        out.u64(self.report.evaluations as u64);
        out.bool(self.report.converged);
    }
}

impl CanonicalEncode for WorkspaceAlignment {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("workspace-alignment-v1");
        self.calibration.encode(out);
        out.f64(self.material_height_mm);
        out.bool(self.lens_calibration.is_some());
        if let Some(lens) = &self.lens_calibration {
            lens.encode(out);
        }
    }
}

impl CanonicalEncode for CalibrationStore {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("camera-calibration-store-v1");
        out.seq(self.lenses.values());
        out.seq(self.alignments.values());
    }
}
