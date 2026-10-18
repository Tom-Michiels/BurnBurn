//! UI icon rasteriser for the BurnBurn toolbar and menus.
//!
//! The SVG masters in `assets/icons/ui/svg` are a small, linted subset (absolute `M L C Q A Z`
//! paths, circles, groups with clip or opacity, masks and clip paths). They are embedded at build
//! time (see `build.rs`) together with `palette.toml` (colour roles in light and dark) and
//! `icons.toml` (icon id to file), parsed once and rasterised with tiny-skia. A master never
//! carries a colour of its own: it names a palette role by its stable source hex, or
//! `currentColor` for the `line` role, and the theme picks the display value.
//!
//! Provenance: the parser and rasteriser are adapted from SolidCraft's `ui-egui/src/icon_svg.rs`
//! and the palette format from its `assets/icons/ui/design/palette.toml` (same owner, licensed
//! MIT OR Apache-2.0). egui, accents and colour-vision variants were removed.
//!
//! This crate has no GUI or GPU dependency. Pixel format: [`RgbaImage`] is straight
//! (non-premultiplied) RGBA8, row-major, top row first.

mod palette;
mod svg;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

pub use palette::Palette;

include!(concat!(env!("OUT_DIR"), "/icon_data.rs"));

/// Largest accepted side of a rendered icon, in pixels.
pub const MAX_SIDE: u32 = 2048;

/// Light or dark UI theme; selects the display value of every colour role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Theme {
    Light,
    Dark,
}

/// A square RGBA8 image with **straight** (non-premultiplied) alpha, row-major, top row first.
/// `pixels.len() == width * height * 4`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Why an icon could not be rendered.
#[derive(Clone, Debug, PartialEq)]
pub enum IconError {
    /// No icon with this id is registered.
    UnknownIcon(String),
    /// The SVG master is malformed or uses something outside the supported subset.
    Parse(String),
    /// The requested size (`px * scale`) exceeds [`MAX_SIDE`].
    SizeTooLarge { px: u32, scale: f32 },
    /// `px` is zero, `scale` is not a positive finite number, or the result rounds to zero pixels.
    InvalidSize,
    /// `palette.toml` is malformed or lacks a required role.
    Palette(String),
}

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IconError::UnknownIcon(id) => write!(f, "unknown icon `{id}`"),
            IconError::Parse(m) => write!(f, "icon artwork: {m}"),
            IconError::SizeTooLarge { px, scale } => write!(f, "icon size {px} x {scale} exceeds {MAX_SIDE} pixels"),
            IconError::InvalidSize => f.write_str("icon size must be a positive number of pixels"),
            IconError::Palette(m) => write!(f, "icon palette: {m}"),
        }
    }
}

impl std::error::Error for IconError {}

fn side_for(px: u32, scale: f32) -> Result<u32, IconError> {
    if px == 0 || !scale.is_finite() || scale <= 0.0 {
        return Err(IconError::InvalidSize);
    }
    let side = (f64::from(px) * f64::from(scale)).round();
    if side > f64::from(MAX_SIDE) {
        return Err(IconError::SizeTooLarge { px, scale });
    }
    if side < 1.0 {
        return Err(IconError::InvalidSize);
    }
    Ok(side as u32)
}

fn to_image(pm: &tiny_skia::Pixmap) -> RgbaImage {
    let mut pixels = Vec::with_capacity(pm.pixels().len() * 4);
    for p in pm.pixels() {
        let c = p.demultiply();
        pixels.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    RgbaImage { width: pm.width(), height: pm.height(), pixels }
}

/// Rasterise an SVG master with an explicit palette (used for tests and previews).
///
/// The icon is drawn square at `round(px * scale)` pixels; `scale` is the display scale factor
/// (2.0 on Retina).
pub fn render_svg(svg: &str, palette: &Palette, px: u32, scale: f32, theme: Theme) -> Result<RgbaImage, IconError> {
    let side = side_for(px, scale)?;
    let doc = svg::Doc::parse(svg, palette).map_err(IconError::Parse)?;
    Ok(to_image(&svg::rasterise(&doc, palette, theme, side)?))
}

struct Registry {
    palette: Result<Palette, IconError>,
    icons: BTreeMap<String, Result<svg::Doc, IconError>>,
}

/// `icons.toml`: a `[tools."id"]` (or `[icons]`) table whose entries are `id = "file.svg"` or
/// `id = { file = "file.svg", ... }`; a table without a `file` key is a group. The file is matched
/// by its stem in `svg/`. An entry with `missing = true` or an empty `file` has no artwork and is
/// **not registered**: `render` reports it as unknown so the host can fall back to a text label.
fn collect_ids(table: &toml::Table, out: &mut Vec<(String, String)>) {
    for (id, v) in table {
        let file = match v {
            toml::Value::String(f) => Some(f.as_str()),
            toml::Value::Table(t) if t.get("missing").and_then(toml::Value::as_bool) == Some(true) => None,
            toml::Value::Table(t) => match t.get("file").and_then(toml::Value::as_str) {
                Some(f) => Some(f),
                None => {
                    collect_ids(t, out);
                    None
                }
            },
            _ => None,
        };
        if let Some(file) = file.filter(|f| !f.is_empty()) {
            let name = file.rsplit('/').next().unwrap_or(file);
            let stem = name.strip_suffix(".svg").unwrap_or(name);
            out.push((id.clone(), stem.to_owned()));
        }
    }
}

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| {
        let palette = Palette::parse(PALETTE_TOML);
        let sources: BTreeMap<&str, &str> = SVGS.iter().copied().collect();
        let mut ids: Vec<(String, String)> = Vec::new();
        if let Ok(t) = ICONS_TOML.parse::<toml::Table>() {
            let root = ["tools", "icons"].iter().find_map(|k| t.get(*k).and_then(toml::Value::as_table)).unwrap_or(&t);
            collect_ids(root, &mut ids);
        }
        // Files that icons.toml does not name are still icons, under their file stem.
        for stem in sources.keys() {
            if !ids.iter().any(|(id, _)| id == stem) {
                ids.push(((*stem).to_owned(), (*stem).to_owned()));
            }
        }
        let mut icons = BTreeMap::new();
        for (id, stem) in ids {
            let doc = match (&palette, sources.get(stem.as_str())) {
                (Ok(p), Some(src)) => svg::Doc::parse(src, p).map_err(|e| IconError::Parse(format!("{id}: {e}"))),
                (Err(e), _) => Err(e.clone()),
                (_, None) => Err(IconError::Parse(format!("{id}: svg file `{stem}` is not embedded"))),
            };
            icons.insert(id, doc);
        }
        Registry { palette, icons }
    })
}

/// Render the registered icon `id` as a square image of `round(px * scale)` pixels in `theme`.
pub fn render(id: &str, px: u32, scale: f32, theme: Theme) -> Result<RgbaImage, IconError> {
    let side = side_for(px, scale)?;
    let reg = registry();
    let doc = reg.icons.get(id).ok_or_else(|| IconError::UnknownIcon(id.to_owned()))?;
    let doc = doc.as_ref().map_err(Clone::clone)?;
    let palette = reg.palette.as_ref().map_err(Clone::clone)?;
    Ok(to_image(&svg::rasterise(doc, palette, theme, side)?))
}

/// All registered icon ids in ascending order.
pub fn icon_ids() -> impl Iterator<Item = &'static str> {
    registry().icons.keys().map(String::as_str)
}

/// Whether `id` is a registered icon.
pub fn has_icon(id: &str) -> bool {
    registry().icons.contains_key(id)
}
