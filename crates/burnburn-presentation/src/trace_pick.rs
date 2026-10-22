//! Pointing in the image of the open tracer (G7-trace, T03): the four corners of the plane
//! correction are clicked, and masks are painted with a brush. The host reports positions in
//! pixels of the picture it shows (`PrepPick::surface_px`), y down; presentation never parses text
//! for these and never sees the pointer, only the positions.
//!
//! Corners are in pixels of the photo as the tracer sees it before the plane correction (after crop,
//! rotation and the lens correction); strokes are in pixels of the working image (after the plane
//! correction, when there is one).

use std::sync::Arc;

use crate::trace_prep::MaskMode;

/// One pointing action.
#[derive(Debug, Clone, PartialEq)]
pub enum TracePick {
    /// Click the four corners of the plane in the photo, in any order; the fourth click applies the
    /// correction with the size of the longer opposite edges.
    BeginCorners,
    /// Paint masks with a round brush of this radius (pixels); every finished stroke is one mask.
    BeginBrush { mode: MaskMode, radius_px: f64 },
    /// A click in the image.
    Point { x: f64, y: f64 },
    /// One finished brush stroke: the positions the brush went through.
    Stroke { points: Vec<[f64; 2]> },
    /// Stop pointing; what was clicked so far is dropped.
    End,
}

/// What the picture is waiting for.
#[derive(Debug, Clone, PartialEq)]
pub enum PickKind {
    /// Corners clicked so far (fewer than four).
    Corners {
        picked: Vec<[f64; 2]>,
    },
    Brush {
        mode: MaskMode,
        radius_px: f64,
    },
}

/// A small gray picture of what to point at.
#[derive(Debug, Clone, PartialEq)]
pub struct PickPicture {
    pub width: u32,
    pub height: u32,
    pub gray8: Arc<[u8]>,
}

/// The open pointing mode: what is waited for, on which picture, in which pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepPick {
    pub kind: PickKind,
    /// Size in pixels of the image positions refer to.
    pub surface_px: (u32, u32),
    /// Downscaled picture of that image (at most 512 pixels on the long side), for drawing.
    pub picture: Option<PickPicture>,
}
