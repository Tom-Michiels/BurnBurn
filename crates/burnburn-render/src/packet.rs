//! GPU-ready derivations of a scene: viewport-relative device pixels in `f32`.
//!
//! A packet is not the document, nor a job plan; it is derived for display only.

use std::sync::Arc;

use burnburn_editor::BedPicture;

use crate::input::Rgba;

/// One thing to draw, in device pixels relative to the viewport's top-left (y down).
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    /// A filled area bounded by closed contours (even-odd: a contour inside another is a hole).
    Fill { contours_px: Vec<Vec<[f32; 2]>>, color: Rgba },
    /// A stroked polyline; `width_px` is the full line width in device pixels.
    Stroke { points_px: Vec<[f32; 2]>, closed: bool, width_px: f32, color: Rgba },
    /// A textured mesh (the camera picture on the bed). Every image is painted *beneath* all
    /// fills and strokes, in list order, whatever its place in the list.
    Image { picture: Arc<BedPicture>, cells: Vec<ImageCell>, opacity: f32 },
}

/// One mesh cell in device pixels: corners top-left, top-right, bottom-right, bottom-left of the
/// picture's cell, with the matching texture coordinates (0..=1, picture top-left origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageCell {
    pub corners_px: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
}

/// Everything one frame draws, in painting order.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPacket {
    pub size_px: [u32; 2],
    pub clear: Rgba,
    pub primitives: Vec<Primitive>,
}
