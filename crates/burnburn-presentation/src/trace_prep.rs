//! Preparing a photo or an uneven scan for tracing (G6-werk, T03, T05, T06): a local threshold next
//! to the global one, a four-point plane correction that shows its transform, and masks that keep
//! or remove foreground. None of it touches the source pixels: every choice is part of the trace
//! recipe, so the trace that is applied keeps it and tracing again reproduces it.
//!
//! The panel sends raw field text (DR-05); the controller parses it. Coordinates are pixels of the
//! adjusted working image (after crop and rotation; after the correction once there is one), y down.

/// A field of the preparation panel, named in an [`PrepIssue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrepField {
    /// Window radius of the local threshold, pixels.
    Radius,
    /// How far below the local mean a pixel must be to count as foreground, gray levels.
    Offset,
    /// One corner of the correction: index 0…3 (top-left, top-right, bottom-right, bottom-left).
    CornerX(usize),
    CornerY(usize),
    /// Size of the corrected image, pixels.
    OutWidth,
    OutHeight,
    MaskX,
    MaskY,
    MaskWidth,
    MaskHeight,
    /// The gray level a corrected area is traced at (empty = the level of the trace).
    RegionThreshold,
    /// The list of anchor points of a correction, `x, y; x, y`.
    Anchors,
    /// The radial lens coefficients (G7-trace, T03).
    LensK1,
    LensK2,
    /// Node goal of a local correction (G7-trace, T05): most nodes, and the coarsest tolerance (mm).
    NodeLimit,
    NodeTolerance,
    /// The radius of the brush in pixels.
    BrushRadius,
}

/// Why a preparation was refused. The recipe stays as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PrepIssue {
    InvalidNumber(PrepField),
    OutOfRange(PrepField),
    /// The four corners do not span a plane: three are on one line, or two coincide.
    DegenerateCorners,
    /// The mask lies (partly) outside the working image.
    MaskOutsideImage,
    /// There is no mask (or correction) with that number.
    UnknownMask,
    /// A correction needs a finished contour trace of the image as it is now.
    NothingToCorrect,
    /// The corrections stored with the trace could not be read; the trace starts without them.
    StoredCorrectionsUnreadable,
    /// A contour that is not in the result in force (protect, keep).
    UnknownContour,
    /// A dropped piece that is not in the report in force (restore).
    UnknownRemoved,
    /// A click or a stroke while no picking was started.
    PickNotStarted,
    /// A stroke without points, or with points all outside the image.
    InvalidStroke,
    /// The corner points clicked cannot be told apart (two on the same place).
    CornersTooClose,
}

/// What a mask does with the foreground inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MaskMode {
    /// Only what is inside the masks (together) is traced; the rest counts as background.
    #[default]
    Keep,
    /// Whatever is inside is background, whatever the threshold says.
    Remove,
}

/// One preparation action of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TracePrepIntent {
    /// Use the local threshold with this window and offset (replaces the global one).
    UseLocalThreshold {
        radius: String,
        offset: String,
    },
    /// Back to the global threshold of the tracer form.
    UseGlobalThreshold,
    /// The four-point correction: corners as typed `(x, y)`, then the size of the corrected image.
    /// Empty size fields take the longer of each pair of opposite edges.
    ApplyPerspective {
        corners: Box<[[String; 2]; 4]>,
        width: String,
        height: String,
    },
    ClearPerspective,
    AddMask {
        mode: MaskMode,
        x: String,
        y: String,
        width: String,
        height: String,
    },
    RemoveMask {
        index: usize,
    },
    /// Trace the area again with its own gray level (empty: the level of the trace) and optional
    /// anchors `x, y; x, y` the new outline must pass through. Everything outside the area stays.
    RetraceRegion {
        x: String,
        y: String,
        width: String,
        height: String,
        threshold: String,
        anchors: String,
    },
    /// Drop correction number `index`; the trace is made again with the others.
    RemoveCorrection {
        index: usize,
    },
    /// Correct the radial distortion of the lens before anything else (G7-trace, T03): the two
    /// coefficients as typed. Barrel distortion is negative.
    UseLens {
        k1: String,
        k2: String,
    },
    ClearLens,
    /// Contour `id` of the result in force is (`on`) or is no longer protected from the next area
    /// retrace: inside the area it stays exactly as it is (G7-trace, T05).
    ProtectContour {
        id: u64,
        on: bool,
    },
    /// The goal of the next area retrace: at most `max_nodes` nodes in the whole result, reached by
    /// raising the tolerance up to `max_tolerance_mm`. Both empty: no goal.
    SetNodeTarget {
        max_nodes: String,
        max_tolerance_mm: String,
    },
    /// Contour `id` is (`keep`) or is no longer protected from "ignore less than" (G7-trace, T06).
    KeepContour {
        id: u64,
        keep: bool,
    },
    /// Bring back piece number `index` of the report that "ignore less than" dropped (G7-trace, T06).
    RestoreRemoved {
        index: usize,
    },
}

/// The local threshold in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepThreshold {
    /// The local threshold is the one in force (otherwise the global one of the tracer form).
    pub local: bool,
    pub radius: String,
    pub offset: String,
}

/// The plane transform of the correction, photo → corrected pixels, with how well it fits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrepTransform {
    /// Row-major 3×3 with `m[8] = 1`.
    pub matrix: [f64; 9],
    /// The largest distance, in corrected pixels, between a corner and where the transform puts it.
    pub residual_px: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepPerspective {
    pub on: bool,
    /// The corners in force, or the corners of the image when there is no correction.
    pub corners: [[String; 2]; 4],
    pub width: String,
    pub height: String,
    pub transform: Option<PrepTransform>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrepMask {
    /// Radius in pixels when the mask is a brush stroke (x, y, width and height are then its box).
    pub brush_px: Option<f64>,
    pub mode: MaskMode,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The preparation panel while a tracer is open.
#[derive(Debug, Clone, PartialEq)]
pub struct TracePrepViewModel {
    /// Size of the source in pixels.
    pub source_px: (u32, u32),
    pub threshold: PrepThreshold,
    pub perspective: PrepPerspective,
    pub masks: Vec<PrepMask>,
    /// The local corrections in force, in the order they are applied.
    pub corrections: Vec<PrepCorrection>,
    /// The cost of simplifying, once there is a contour result.
    pub report: Option<PrepReport>,
    pub lens: PrepLens,
    /// The contours of the contour result in force (T05, T06).
    pub contours: Vec<PrepContour>,
    pub node_target: Option<PrepNodeTarget>,
    /// What pointing in the image is waiting for, if anything (T03).
    pub pick: Option<crate::trace_pick::PrepPick>,
    pub issue: Option<PrepIssue>,
    /// A trace of the prepared image is running.
    pub computing: bool,
}

/// Where a stored correction stands after the last trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionStatus {
    Applied,
    /// Not applied: the contours it was made on are no longer the same. `lost` contours it was bound
    /// to have no match; `new_in_region` contours in its area it never saw.
    NeedsRelink {
        lost: usize,
        new_in_region: usize,
    },
    /// The retrace itself was refused.
    Failed,
}

/// One local correction.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepCorrection {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The gray level the area was traced at, or the local threshold in words.
    pub threshold: String,
    pub anchors: usize,
    /// How many contours the correction left alone inside its area (T05).
    pub protected: usize,
    /// The node goal and what it reached (T05); `met == false` is said, never hidden.
    pub node_goal: Option<PrepNodeGoal>,
    pub status: CorrectionStatus,
}

/// What a node goal reached.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrepNodeGoal {
    pub max_nodes: usize,
    pub met: bool,
    pub nodes: usize,
    pub tolerance_mm: f64,
}

/// A contour of the result in force, to protect or to keep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrepContour {
    pub id: u64,
    pub hole: bool,
    pub area_mm2: f64,
    /// Middle of its box, millimetres in the frame of the trace (y up).
    pub x_mm: f64,
    pub y_mm: f64,
    /// Left alone by the next area retrace (T05).
    pub protected: bool,
    /// Never dropped by "ignore less than" (T06).
    pub kept: bool,
}

/// The radial lens correction in force (T03).
#[derive(Debug, Clone, PartialEq)]
pub struct PrepLens {
    pub on: bool,
    pub k1: String,
    pub k2: String,
}

/// The goal of the next area retrace, as typed (T05).
#[derive(Debug, Clone, PartialEq)]
pub struct PrepNodeTarget {
    pub max_nodes: String,
    pub max_tolerance_mm: String,
}

/// One of the largest deviations between the fitted outline and the source contour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrepDeviation {
    pub mm: f64,
    /// Where the source point with that deviation is, millimetres in the frame of the trace (y up).
    pub x_mm: f64,
    pub y_mm: f64,
}

/// A part or hole the "ignore less than" filter took away, and where it was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrepRemoved {
    pub hole: bool,
    pub area_mm2: f64,
    /// The middle of its bounding box, millimetres in the frame of the trace (y up).
    pub x_mm: f64,
    pub y_mm: f64,
}

/// What simplifying cost (T06): the fit budget, the deviations that really occur, and what the
/// filter dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepReport {
    /// The fit budget in force, mm.
    pub tolerance_mm: f64,
    pub max_deviation_mm: f64,
    pub p95_deviation_mm: f64,
    /// The largest deviations, largest first.
    pub worst: Vec<PrepDeviation>,
    pub removed: Vec<PrepRemoved>,
    pub parts: usize,
    pub holes: usize,
}
