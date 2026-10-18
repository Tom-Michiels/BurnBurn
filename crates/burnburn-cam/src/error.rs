use std::fmt;

use burnburn_document::process::ProcessError;
use burnburn_geometry::OffsetError;
use burnburn_image::RasterError;
use burnburn_job::{Feature, PlanError, Support};
use burnburn_types::{Cancelled, ContentHash, GeometryId, OperationId};

/// Why a snapshot could not be compiled. Problems the user can review in a plan (outside
/// bed, open fill contours, speeds above the machine limit) are preflight findings instead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompileError {
    Cancelled,
    /// A process mode this compiler does not know; never replaced by another mode.
    UnsupportedMode {
        operation: OperationId,
    },
    /// The operation needs a feature the machine profile does not report as `Supported`
    /// (IM25 feature matrix). Never replaced by another feature.
    UnsupportedFeature {
        operation: OperationId,
        feature: Feature,
        support: Support,
    },
    /// Tabs, leads, overcut or perforation on a mode that does not follow contours.
    LineOptionsOnNonLineMode {
        operation: OperationId,
    },
    /// The tabs of a contour cover its whole length (for example after the source shrank).
    InvalidTabs {
        operation: OperationId,
        geometry: GeometryId,
    },
    /// The image operation's source pixels are not available to the compiler.
    MissingImageSource {
        operation: OperationId,
        source: ContentHash,
    },
    /// Raster production rejected a setting (tone range, dither size, output size).
    Raster {
        operation: OperationId,
        error: RasterError,
    },
    /// Kerf compensation needs a side: an open contour has none, so it is rejected instead of
    /// being cut at nominal position.
    KerfOnOpenContour {
        operation: OperationId,
        geometry: GeometryId,
    },
    /// A geometric offset (kerf, offset fill, spacing) failed for this geometry.
    Offset {
        operation: OperationId,
        geometry: GeometryId,
        error: OffsetError,
    },
    /// Order constraints could not be satisfied (for example a cycle in a loaded file).
    Order(ProcessError),
    /// An operation references geometry that is not in the snapshot.
    UnknownGeometry {
        operation: OperationId,
        geometry: GeometryId,
    },
    /// The fill would need more scan rows than the compiler bound allows.
    TooManyScanRows {
        operation: OperationId,
        rows: u64,
    },
    /// A computed value did not satisfy the plan contract (non-finite coordinate, …).
    Plan(PlanError),
}

impl From<Cancelled> for CompileError {
    fn from(_: Cancelled) -> Self {
        CompileError::Cancelled
    }
}

impl From<PlanError> for CompileError {
    fn from(e: PlanError) -> Self {
        CompileError::Plan(e)
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Cancelled => f.write_str("compile cancelled"),
            CompileError::UnsupportedMode { operation } => write!(f, "{operation} uses an unsupported mode"),
            CompileError::UnsupportedFeature { operation, feature, support } => {
                write!(f, "{operation} needs {} which this machine reports as {support:?}", feature.tag())
            }
            CompileError::LineOptionsOnNonLineMode { operation } => {
                write!(f, "{operation}: tabs, leads, overcut and perforation need Line mode")
            }
            CompileError::InvalidTabs { operation, geometry } => {
                write!(f, "{operation}: the tabs cover the whole contour of {geometry}")
            }
            CompileError::MissingImageSource { operation, source } => {
                write!(f, "{operation}: source image {source} is not available")
            }
            CompileError::Raster { operation, error } => write!(f, "{operation}: {error}"),
            CompileError::KerfOnOpenContour { operation, geometry } => {
                write!(f, "{operation}: kerf compensation needs closed contours ({geometry} is open)")
            }
            CompileError::Offset { operation, geometry, error } => write!(f, "{operation}, {geometry}: {error}"),
            CompileError::Order(e) => write!(f, "operation order: {e}"),
            CompileError::UnknownGeometry { operation, geometry } => {
                write!(f, "{operation} references missing {geometry}")
            }
            CompileError::TooManyScanRows { operation, rows } => write!(f, "{operation} needs {rows} scan rows"),
            CompileError::Plan(e) => write!(f, "plan: {e}"),
        }
    }
}

impl std::error::Error for CompileError {}
