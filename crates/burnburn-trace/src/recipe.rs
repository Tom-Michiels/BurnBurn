use burnburn_image::ImageRecipe;
use burnburn_types::{CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, NonFiniteValue};

use crate::contour::DiagonalPolicy;
use crate::error::TraceError;

/// Version of the tracing engine; part of every recipe key so cached geometry is never reused
/// across algorithm changes.
pub const ENGINE_VERSION: &str = "burnburn-trace/contour-1";

/// Upper bound for the corner window (pixels); larger windows blur distinct corners together.
const MAX_CORNER_WINDOW_PX: f64 = 64.0;

/// Topology settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ContourSettings {
    pub diagonal: DiagonalPolicy,
    /// Parts and holes smaller than this area (mm²) are dropped and reported in
    /// [`crate::TraceResult::removed`]. 0 keeps everything.
    pub min_area_mm2: f64,
    /// Features the area filter never drops (T06): each is named by an anchor, a vertex of its own
    /// outline in adjusted pixels (y down), as reported by [`crate::RemovedFeature::anchor_px`] or
    /// [`crate::TraceResult::contour_anchor_px`]. Keeping a hole or part inside another keeps the
    /// outlines around it too.
    pub keep: Vec<[f64; 2]>,
    /// Corners the fit must keep (G8-trace, T06): each a position in adjusted pixels (y down); the
    /// nearest vertex of an outline within [`CORNER_REACH_PX`] becomes a corner node even where the
    /// turn is milder than `fit.corner_angle_deg` or the tolerance would round it off.
    pub corners: Vec<[f64; 2]>,
}

/// How near (pixels) a protected corner has to be to a vertex of an outline to name it.
pub const CORNER_REACH_PX: f64 = 3.0;

/// How far (pixels) a protected corner is looked up for a local re-trace with another threshold, where the
/// outline may have moved; there the sharpest turn of at least 5 degrees counts, not the nearest vertex.
pub const SHIFTED_CORNER_REACH_PX: f64 = 8.0;

impl Default for ContourSettings {
    fn default() -> Self {
        ContourSettings { diagonal: DiagonalPolicy::Connect, min_area_mm2: 0.0, keep: Vec::new(), corners: Vec::new() }
    }
}

/// Fitting settings. The distance budget is physical (mm); the corner window is in source pixels
/// because it describes how far raster noise reaches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitSettings {
    /// Maximum distance between the sub-pixel source contour and the fitted path.
    pub tolerance_mm: f64,
    /// Turning angle (degrees) above which a contour point is a corner.
    pub corner_angle_deg: f64,
    /// Arc length (pixels) on each side used to measure the turning angle.
    pub corner_window_px: f64,
    /// `false` = straight segments only (polygon simplification within the same budget).
    pub curves: bool,
}

impl Default for FitSettings {
    fn default() -> Self {
        FitSettings { tolerance_mm: 0.05, corner_angle_deg: 60.0, corner_window_px: 2.5, curves: true }
    }
}

/// Every parameter of a trace. Together with the source content hash and [`ENGINE_VERSION`] it
/// determines the output exactly; [`TraceRecipe::key`] is the reproducibility/cache key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TraceRecipe {
    pub image: ImageRecipe,
    pub contour: ContourSettings,
    pub fit: FitSettings,
}

impl TraceRecipe {
    pub fn validate(&self) -> Result<(), TraceError> {
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if !positive(self.fit.tolerance_mm) {
            return Err(TraceError::InvalidRecipe("fit.tolerance_mm"));
        }
        if !(self.fit.corner_angle_deg.is_finite() && (0.0..=180.0).contains(&self.fit.corner_angle_deg)) {
            return Err(TraceError::InvalidRecipe("fit.corner_angle_deg"));
        }
        if !(positive(self.fit.corner_window_px) && self.fit.corner_window_px <= MAX_CORNER_WINDOW_PX) {
            return Err(TraceError::InvalidRecipe("fit.corner_window_px"));
        }
        if !(self.contour.min_area_mm2.is_finite() && self.contour.min_area_mm2 >= 0.0) {
            return Err(TraceError::InvalidRecipe("contour.min_area_mm2"));
        }
        if let Some(lens) = &self.image.lens {
            lens.validate().map_err(|_| TraceError::InvalidRecipe("image.lens"))?;
        }
        if self.contour.keep.iter().any(|a| !(a[0].is_finite() && a[1].is_finite())) {
            return Err(TraceError::InvalidRecipe("contour.keep"));
        }
        if self.contour.corners.iter().any(|a| !(a[0].is_finite() && a[1].is_finite())) {
            return Err(TraceError::InvalidRecipe("contour.corners"));
        }
        Ok(())
    }

    /// Canonical bytes of the recipe (ADR 0004); the persisted form is a DTO in `burnburn-io`.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, NonFiniteValue> {
        let mut w = CanonicalWriter::new();
        self.encode(&mut w);
        w.finish()
    }

    /// `ContentHash::of(TraceKey, canonical(engine version, source hash, recipe))` (DR-17).
    pub fn key(&self, source: ContentHash) -> Result<ContentHash, NonFiniteValue> {
        let mut w = CanonicalWriter::new();
        w.str(ENGINE_VERSION);
        w.bytes(source.as_bytes());
        self.encode(&mut w);
        Ok(ContentHash::of(HashDomain::TraceKey, &w.finish()?))
    }
}

impl CanonicalEncode for TraceRecipe {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("trace-recipe-v1");
        self.image.encode(out);
        out.tag(match self.contour.diagonal {
            DiagonalPolicy::Connect => "diagonal-connect",
            DiagonalPolicy::Separate => "diagonal-separate",
        });
        out.f64(self.contour.min_area_mm2);
        // Only written when there is something to keep, so keys of recipes without it stay unchanged.
        if !self.contour.keep.is_empty() {
            out.tag("contour-keep");
            out.u64(self.contour.keep.len() as u64);
            for a in &self.contour.keep {
                out.f64(a[0]);
                out.f64(a[1]);
            }
        }
        // The same for protected corners (G8-trace, T06).
        if !self.contour.corners.is_empty() {
            out.tag("contour-corners");
            out.u64(self.contour.corners.len() as u64);
            for a in &self.contour.corners {
                out.f64(a[0]);
                out.f64(a[1]);
            }
        }
        out.f64(self.fit.tolerance_mm);
        out.f64(self.fit.corner_angle_deg);
        out.f64(self.fit.corner_window_px);
        out.bool(self.fit.curves);
    }
}
