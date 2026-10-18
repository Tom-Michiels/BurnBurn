use std::fmt;

use burnburn_image::ImageError;
use burnburn_types::{Cancelled, ContentHash, TaskId};

use crate::result::ContourId;

/// Why a trace produced no result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TraceError {
    /// Preparing the source failed (includes the typed "needs scale").
    Image(ImageError),
    /// A recipe field is out of range; the name is the field path.
    InvalidRecipe(&'static str),
    /// A coordinate became non-finite (extreme scale); nothing is returned.
    NonFinite,
    Cancelled(Cancelled),
    /// A local correction does not fit its base: different source, frame (scale/crop/rotation),
    /// diagonal policy or size. The name says which.
    CorrectionMismatch(&'static str),
    /// A correction refers to a contour id the base result does not contain.
    UnknownContour(ContourId),
    /// A protected contour would have changed; the correction is refused rather than applied.
    ProtectedChanged(ContourId),
}

impl From<Cancelled> for TraceError {
    fn from(c: Cancelled) -> Self {
        TraceError::Cancelled(c)
    }
}

impl From<ImageError> for TraceError {
    fn from(e: ImageError) -> Self {
        match e {
            ImageError::Cancelled(c) => TraceError::Cancelled(c),
            other => TraceError::Image(other),
        }
    }
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraceError::Image(e) => e.fmt(f),
            TraceError::InvalidRecipe(field) => write!(f, "trace recipe value out of range: {field}"),
            TraceError::NonFinite => f.write_str("trace produced a non-finite coordinate"),
            TraceError::Cancelled(c) => c.fmt(f),
            TraceError::CorrectionMismatch(what) => write!(f, "local correction does not match its base: {what}"),
            TraceError::UnknownContour(id) => write!(f, "unknown contour {}", id.0),
            TraceError::ProtectedChanged(id) => write!(f, "protected contour {} would change", id.0),
        }
    }
}

impl std::error::Error for TraceError {}

/// Why a finished trace must not be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleTrace {
    /// Source image, recipe, crop/scale or engine changed since submission (recipe keys differ).
    InputsChanged { traced: ContentHash, current: ContentHash },
    /// A newer trace task (other recipe or region) replaced this one.
    Superseded { result: TaskId, latest: TaskId },
}

impl fmt::Display for StaleTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StaleTrace::InputsChanged { traced, current } => {
                write!(f, "trace inputs changed (traced {traced}, current {current})")
            }
            StaleTrace::Superseded { result, latest } => write!(f, "trace {result} was superseded by {latest}"),
        }
    }
}

impl std::error::Error for StaleTrace {}
