//! GPU-ready derivations of a scene: viewport-relative device pixels in `f32`.
//!
//! A packet is not the document, nor a job plan; it is derived for display only.

use crate::input::Rgba;

/// One thing to draw, in device pixels relative to the viewport's top-left (y down).
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    /// A filled area bounded by closed contours (even-odd: a contour inside another is a hole).
    Fill { contours_px: Vec<Vec<[f32; 2]>>, color: Rgba },
    /// A stroked polyline; `width_px` is the full line width in device pixels.
    Stroke { points_px: Vec<[f32; 2]>, closed: bool, width_px: f32, color: Rgba },
}

/// Everything one frame draws, in painting order.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPacket {
    pub size_px: [u32; 2],
    pub clear: Rgba,
    pub primitives: Vec<Primitive>,
}
