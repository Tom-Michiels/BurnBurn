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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContourSettings {
    pub diagonal: DiagonalPolicy,
    /// Parts and holes smaller than this area (mm²) are dropped and reported in
    /// [`crate::TraceResult::removed`]. 0 keeps everything.
    pub min_area_mm2: f64,
}

impl Default for ContourSettings {
    fn default() -> Self {
        ContourSettings { diagonal: DiagonalPolicy::Connect, min_area_mm2: 0.0 }
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
        out.f64(self.fit.tolerance_mm);
        out.f64(self.fit.corner_angle_deg);
        out.f64(self.fit.corner_window_px);
        out.bool(self.fit.curves);
    }
}
