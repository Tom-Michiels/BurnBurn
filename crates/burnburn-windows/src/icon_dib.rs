//! Tool strip icons as device-independent bitmaps (LF14): the SVG masters of `burnburn-icons`
//! rendered at 16 points times the DPI scale, as 32-bit premultiplied BGRA, top row first (a
//! `BITMAPINFOHEADER` with a negative height; the Win32 layer wraps it in `CreateDIBSection` and
//! gives it to the button with `BM_SETIMAGE`).

use burnburn_icons::{RgbaImage, Theme};

use crate::appearance::Appearance;

/// The size of a tool icon in points (device-independent pixels at 96 dpi).
pub const ICON_POINTS: u32 = 16;

/// A 32-bit premultiplied BGRA bitmap, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dib {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// The side of an icon in pixels at `dpi` (an unknown DPI of 0 counts as 96).
pub fn side_px(dpi: u32) -> u32 {
    let dpi = if dpi == 0 { 96 } else { dpi };
    (ICON_POINTS * dpi + 48) / 96
}

fn theme_of(appearance: Appearance) -> Theme {
    match appearance {
        Appearance::Light => Theme::Light,
        Appearance::Dark => Theme::Dark,
    }
}

/// Straight RGBA to premultiplied BGRA.
fn to_dib(image: &RgbaImage) -> Dib {
    let mut bgra = Vec::with_capacity(image.pixels.len());
    for p in image.pixels.as_chunks::<4>().0 {
        let a = u32::from(p[3]);
        let premultiply = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
        bgra.extend_from_slice(&[premultiply(p[2]), premultiply(p[1]), premultiply(p[0]), p[3]]);
    }
    Dib { width: image.width, height: image.height, bgra }
}

/// The icon `id` for the appearance at `dpi`; `None` when there is no artwork.
pub fn icon_dib(id: &str, appearance: Appearance, dpi: u32) -> Option<Dib> {
    let side = side_px(dpi);
    let scale = side as f32 / ICON_POINTS as f32;
    let image = burnburn_icons::render(id, ICON_POINTS, scale, theme_of(appearance)).ok()?;
    Some(to_dib(&image))
}

/// The drop-down caret of a tool group, drawn here (no glyph character in a button title): a
/// chevron pointing down, mirrored exactly about the middle, in the ink of the appearance.
pub fn caret_dib(appearance: Appearance, dpi: u32) -> Dib {
    let side = side_px(dpi);
    let s = side as f32;
    let ink = match appearance {
        Appearance::Light => 0x40,
        Appearance::Dark => 0xE0,
    };
    let (top, half, thickness) = (0.33 * s, 0.28 * s, (s / 10.0).max(1.2));
    let mut bgra = Vec::with_capacity((side * side * 4) as usize);
    for py in 0..side {
        for px in 0..side {
            let u = ((px as f32 + 0.5) - s / 2.0).abs();
            let v = py as f32 + 0.5;
            let on_line = u <= half + 0.5 && (v - (top + half - u)).abs() <= thickness / 2.0;
            bgra.extend_from_slice(&if on_line { [ink, ink, ink, 255] } else { [0, 0, 0, 0] });
        }
    }
    Dib { width: side, height: side, bgra }
}
