//! A picture laid on the bed (the camera overlay): pixels plus a mesh that places them in world
//! millimetres. Pure data for the scene; the renderer draws it beneath the design geometry.
//!
//! The picture is a visual placement aid. Nothing in here is document data, and no job is ever
//! derived from it.

use std::sync::Arc;

use burnburn_geometry::Point;

/// RGBA8 pixels, top row first. `id` changes whenever the pixels do (a new frame), so a renderer
/// can keep its texture for an unchanged picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BedPicture {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

impl BedPicture {
    /// `None` when the buffer does not hold `width × height` RGBA pixels.
    pub fn new(id: u64, width: u32, height: u32, rgba: Arc<[u8]>) -> Option<BedPicture> {
        let wanted = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        (width > 0 && height > 0 && rgba.len() == wanted).then_some(BedPicture { id, width, height, rgba })
    }

    /// A gray8 bitmap (one byte per pixel, top row first) as an opaque RGBA picture.
    pub fn from_gray8(id: u64, width: u32, height: u32, gray8: &[u8]) -> Option<BedPicture> {
        let pixels = (width as usize).checked_mul(height as usize)?;
        if width == 0 || height == 0 || gray8.len() != pixels {
            return None;
        }
        let rgba: Vec<u8> = gray8.iter().flat_map(|g| [*g, *g, *g, 255]).collect();
        BedPicture::new(id, width, height, rgba.into())
    }
}

/// One mesh cell: where the corners of a texture cell land on the bed. Corner order is
/// top-left, top-right, bottom-right, bottom-left *of the picture*; `uv` is in 0..=1 with the
/// origin at the picture's top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BedQuad {
    pub bed_mm: [Point; 4],
    pub uv: [[f32; 2]; 4],
}
