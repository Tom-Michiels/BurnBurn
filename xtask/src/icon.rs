//! The BurnBurn app icon: rasterises the SVG sources in `assets/app-icon/` (own design) with
//! `tiny-skia` and packs them as `.icns` (macOS), `.ico` (Windows exe resource) and PNG logos (MSIX).
//!
//! Only the small SVG subset the sources use is supported (paths of absolute M L H V C Q Z, circles,
//! rects, `g` with translate/scale, linear and radial gradients, opacity, round-cap strokes); anything
//! else is an error, never silently dropped.

/// The macOS app icon: a squircle plate on a 1024 px canvas.
pub const APP_SVG: &str = include_str!("../../assets/app-icon/burnburn.svg");
/// The `.burnburn` project document icon.
pub const DOCUMENT_SVG: &str = include_str!("../../assets/app-icon/burnburn-document.svg");

/// Committed copy of the 512 px icon the macOS host embeds (it sets the Dock icon of unbundled runs).
pub const HOST_PNG: &str = "assets/app-icon/burnburn-512.png";
/// Committed Windows exe resource.
pub const WINDOWS_ICO: &str = "crates/burnburn-windows/res/burnburn.ico";

/// Which part of the 1024 canvas is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    /// The whole canvas (macOS: the plate keeps its margin and shadow room).
    Canvas,
    /// The plate only (`100 100 824 824`): Windows icons fill their square.
    Plate,
}

/// A square RGBA8 image, straight alpha, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub size: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// RGBA of the pixel at `(x, y)`.
    #[cfg(test)]
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.size + x) * 4) as usize;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }
}

/// Largest side `render` accepts (the icon set tops out at 1024).
const MAX_SIDE: u32 = 4096;

/// Rasterises one of the icon SVGs at `size` x `size` pixels.
pub fn render(svg: &str, size: u32, frame: Frame) -> Result<Image, String> {
    if size == 0 || size > MAX_SIDE {
        return Err(format!("icon size {size} is outside 1..={MAX_SIDE}"));
    }
    let mut pixmap = sk::Pixmap::new(size, size).ok_or("cannot allocate the icon bitmap")?;
    Painter::new(size, frame).paint(svg, &mut pixmap)?;
    let mut pixels = Vec::with_capacity(pixmap.pixels().len() * 4);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        pixels.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(Image { size, pixels })
}

/// A compressed PNG of `image`.
pub fn png(image: &Image) -> Result<Vec<u8>, String> {
    let buffer = image::RgbaImage::from_raw(image.size, image.size, image.pixels.clone())
        .ok_or("pixel buffer does not match the image size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    buffer.write_to(&mut out, image::ImageFormat::Png).map_err(|e| format!("encoding PNG: {e}"))?;
    Ok(out.into_inner())
}

// ---------------------------------------------------------------------------------------------
// The SVG subset

use std::collections::HashMap;

use tiny_skia as sk;

struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, &'a str)>,
    close: bool,
    self_close: bool,
}

impl<'a> Tag<'a> {
    fn get(&self, key: &str) -> Option<&'a str> {
        self.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    fn num(&self, key: &str) -> Result<f32, String> {
        let v = self.get(key).ok_or_else(|| format!("<{}> without {key}", self.name))?;
        v.trim().parse::<f32>().map_err(|_| format!("<{}> {key}=`{v}` is not a number", self.name))
    }

    fn num_or(&self, key: &str, default: f32) -> Result<f32, String> {
        if self.get(key).is_some() { self.num(key) } else { Ok(default) }
    }
}

fn tags(src: &str) -> Result<Vec<Tag<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after.find("-->").ok_or("unterminated comment")?;
            rest = &after[end + 3..];
            continue;
        }
        let end = rest.find('>').ok_or("unterminated tag")?;
        let inner = &rest[1..end];
        rest = &rest[end + 1..];
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        let (close, inner) = match inner.strip_prefix('/') {
            Some(i) => (true, i),
            None => (false, inner),
        };
        let (self_close, inner) = match inner.strip_suffix('/') {
            Some(i) => (true, i),
            None => (false, inner),
        };
        let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
        let name = &inner[..name_end];
        let mut attrs = Vec::new();
        let mut a = inner[name_end..].trim_start();
        while !a.is_empty() {
            let eq = a.find('=').ok_or_else(|| format!("<{name}>: attribute without a value"))?;
            let key = a[..eq].trim();
            let value = a[eq + 1..].trim_start();
            let quote = value.chars().next().filter(|q| *q == '"' || *q == '\'').ok_or("unquoted attribute")?;
            let value = &value[1..];
            let close_quote = value.find(quote).ok_or("unterminated attribute")?;
            attrs.push((key, &value[..close_quote]));
            a = value[close_quote + 1..].trim_start();
        }
        out.push(Tag { name, attrs, close, self_close });
    }
    Ok(out)
}

/// Path data of absolute `M L H V C Q Z`; implicit repetition works as in SVG.
fn parse_path(d: &str) -> Result<sk::Path, String> {
    enum Tok {
        Cmd(char),
        Num(f32),
    }
    let mut toks = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() {
            toks.push(Tok::Cmd(c));
            i += 1;
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            let s = i;
            i += 1;
            let mut dot = c == '.';
            while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !dot)) {
                dot |= b[i] == b'.';
                i += 1;
            }
            let n = d[s..i].parse::<f32>().map_err(|_| format!("path: bad number `{}`", &d[s..i]))?;
            toks.push(Tok::Num(n));
        } else if c.is_whitespace() || c == ',' {
            i += 1;
        } else {
            return Err(format!("path: unexpected `{c}`"));
        }
    }
    let mut pb = sk::PathBuilder::new();
    let mut k = 0;
    let mut cmd = ' ';
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut open = false;
    let take = |k: &mut usize, n: usize| -> Result<Vec<f32>, String> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            match toks.get(*k) {
                Some(Tok::Num(x)) => v.push(*x),
                _ => return Err("path data ends early".into()),
            }
            *k += 1;
        }
        Ok(v)
    };
    while k < toks.len() {
        if let Tok::Cmd(c) = toks[k] {
            cmd = c;
            k += 1;
            if c == 'Z' {
                pb.close();
                open = false;
                continue;
            }
        } else if cmd == 'Z' || cmd == ' ' {
            return Err("path: number without a command".into());
        }
        match cmd {
            'M' => {
                let v = take(&mut k, 2)?;
                (x, y) = (v[0], v[1]);
                pb.move_to(x, y);
                open = true;
                cmd = 'L';
            }
            'L' => {
                let v = take(&mut k, 2)?;
                (x, y) = (v[0], v[1]);
                pb.line_to(x, y);
            }
            'H' => {
                x = take(&mut k, 1)?[0];
                pb.line_to(x, y);
            }
            'V' => {
                y = take(&mut k, 1)?[0];
                pb.line_to(x, y);
            }
            'C' => {
                let v = take(&mut k, 6)?;
                pb.cubic_to(v[0], v[1], v[2], v[3], v[4], v[5]);
                (x, y) = (v[4], v[5]);
            }
            'Q' => {
                let v = take(&mut k, 4)?;
                pb.quad_to(v[0], v[1], v[2], v[3]);
                (x, y) = (v[2], v[3]);
            }
            other => return Err(format!("path command {other} is not supported")),
        }
    }
    let _ = open;
    pb.finish().ok_or_else(|| "path is empty".to_owned())
}

#[derive(Clone, Copy, PartialEq)]
enum Fill {
    None,
    Solid([u8; 3]),
    Gradient(usize),
}

struct Gradient {
    radial: bool,
    /// Linear: x1 y1 x2 y2. Radial: cx cy r 0.
    geometry: [f32; 4],
    stops: Vec<(f32, [u8; 3], f32)>,
}

fn parse_hex(v: &str) -> Result<[u8; 3], String> {
    let digits = v.strip_prefix('#').filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()));
    let digits = digits.ok_or_else(|| format!("colour `{v}` is not #rrggbb"))?;
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).map_err(|_| format!("colour `{v}`"));
    Ok([byte(0)?, byte(2)?, byte(4)?])
}

fn parse_transform(v: &str) -> Result<sk::Transform, String> {
    let mut ts = sk::Transform::identity();
    let mut rest = v.trim();
    while !rest.is_empty() {
        let open = rest.find('(').ok_or("transform: missing (")?;
        let close = rest.find(')').ok_or("transform: missing )")?;
        let args: Vec<f32> = rest[open + 1..close]
            .split([' ', ','])
            .filter(|p| !p.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| format!("transform `{v}`: bad number"))?;
        let step = match (rest[..open].trim(), args.as_slice()) {
            ("translate", [x, y]) => sk::Transform::from_translate(*x, *y),
            ("translate", [x]) => sk::Transform::from_translate(*x, 0.0),
            ("scale", [s]) => sk::Transform::from_scale(*s, *s),
            ("scale", [sx, sy]) => sk::Transform::from_scale(*sx, *sy),
            (other, _) => return Err(format!("transform `{other}` is not supported")),
        };
        ts = ts.pre_concat(step);
        rest = rest[close + 1..].trim_start();
    }
    Ok(ts)
}

struct Painter {
    base: sk::Transform,
    /// Canvas side of the SVG's viewBox, to scale the frame.
    frame: Frame,
    size: u32,
}

impl Painter {
    fn new(size: u32, frame: Frame) -> Self {
        Painter { base: sk::Transform::identity(), frame, size }
    }

    fn paint(&mut self, svg: &str, pm: &mut sk::Pixmap) -> Result<(), String> {
        let tags = tags(svg)?;
        let root = tags.first().filter(|t| t.name == "svg" && !t.close).ok_or("not an SVG document")?;
        let view: Vec<f32> = root
            .get("viewBox")
            .ok_or("<svg> without viewBox")?
            .split([' ', ','])
            .filter(|p| !p.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| "viewBox is not four numbers")?;
        let side = match view.as_slice() {
            [0.0, 0.0, w, h] if w == h && *w > 0.0 => *w,
            _ => return Err("viewBox must be a square `0 0 N N`".into()),
        };
        let (origin, extent) = match self.frame {
            Frame::Canvas => (0.0, side),
            Frame::Plate => (side * 100.0 / 1024.0, side * 824.0 / 1024.0),
        };
        let scale = self.size as f32 / extent;
        self.base = sk::Transform::from_row(scale, 0.0, 0.0, scale, -origin * scale, -origin * scale);

        let mut gradients: Vec<Gradient> = Vec::new();
        let mut ids: HashMap<&str, usize> = HashMap::new();
        let mut building: Option<(&str, Gradient)> = None;
        let mut groups: Vec<(sk::Transform, f32)> = vec![(self.base, 1.0)];
        for t in &tags[1..] {
            match (t.name, t.close) {
                ("svg", true) | ("defs", _) => {}
                ("linearGradient" | "radialGradient", false) => {
                    if t.get("gradientUnits") != Some("userSpaceOnUse") {
                        return Err("gradients must use gradientUnits=\"userSpaceOnUse\"".into());
                    }
                    let id = t.get("id").ok_or("gradient without id")?;
                    let (radial, geometry) = if t.name == "radialGradient" {
                        (true, [t.num("cx")?, t.num("cy")?, t.num("r")?, 0.0])
                    } else {
                        (false, [t.num("x1")?, t.num("y1")?, t.num("x2")?, t.num("y2")?])
                    };
                    building = Some((id, Gradient { radial, geometry, stops: Vec::new() }));
                }
                ("linearGradient" | "radialGradient", true) => {
                    let (id, g) = building.take().ok_or("unbalanced gradient")?;
                    gradients.push(g);
                    ids.insert(id, gradients.len() - 1);
                }
                ("stop", false) => {
                    let (_, g) = building.as_mut().ok_or("<stop> outside a gradient")?;
                    let colour = parse_hex(t.get("stop-color").ok_or("<stop> without stop-color")?)?;
                    g.stops.push((t.num("offset")?, colour, t.num_or("stop-opacity", 1.0)?));
                }
                ("stop", true) => {}
                ("g", false) => {
                    let (parent_ts, parent_op) = *groups.last().ok_or("group stack")?;
                    let ts = match t.get("transform") {
                        Some(v) => parent_ts.pre_concat(parse_transform(v)?),
                        None => parent_ts,
                    };
                    let op = parent_op * t.num_or("opacity", 1.0)?;
                    if !t.self_close {
                        groups.push((ts, op));
                    }
                }
                ("g", true) => {
                    if groups.len() < 2 {
                        return Err("unbalanced </g>".into());
                    }
                    groups.pop();
                }
                ("path" | "circle", false) => {
                    let path = if t.name == "path" {
                        parse_path(t.get("d").ok_or("path without d")?)?
                    } else {
                        sk::PathBuilder::from_circle(t.num("cx")?, t.num("cy")?, t.num("r")?).ok_or("bad circle")?
                    };
                    let (ts, op) = *groups.last().ok_or("group stack")?;
                    let opacity = op * t.num_or("opacity", 1.0)?;
                    let fill = self.paint_of(t.get("fill").unwrap_or("#000000"), &ids)?;
                    if let Some(paint) = shader_paint(fill, opacity, &gradients) {
                        pm.fill_path(&path, &paint, sk::FillRule::Winding, ts, None);
                    }
                    let stroke_fill = self.paint_of(t.get("stroke").unwrap_or("none"), &ids)?;
                    if let Some(paint) = shader_paint(stroke_fill, opacity, &gradients) {
                        let stroke = sk::Stroke {
                            width: t.num_or("stroke-width", 1.0)?,
                            line_cap: match t.get("stroke-linecap") {
                                Some("round") => sk::LineCap::Round,
                                Some("square") => sk::LineCap::Square,
                                _ => sk::LineCap::Butt,
                            },
                            ..sk::Stroke::default()
                        };
                        pm.stroke_path(&path, &paint, &stroke, ts, None);
                    }
                }
                (other, _) => return Err(format!("element <{other}> is not supported")),
            }
        }
        if building.is_some() || groups.len() != 1 {
            return Err("unbalanced tags".into());
        }
        Ok(())
    }

    fn paint_of(&self, v: &str, ids: &HashMap<&str, usize>) -> Result<Fill, String> {
        if v == "none" {
            return Ok(Fill::None);
        }
        if let Some(id) = v.strip_prefix("url(#").and_then(|r| r.strip_suffix(')')) {
            return ids.get(id).map(|i| Fill::Gradient(*i)).ok_or_else(|| format!("unknown gradient `{id}`"));
        }
        parse_hex(v).map(Fill::Solid)
    }
}

/// The tiny-skia paint for `fill` with every alpha scaled by `opacity` (None = nothing to draw).
fn shader_paint(fill: Fill, opacity: f32, gradients: &[Gradient]) -> Option<sk::Paint<'static>> {
    let mut paint = sk::Paint { anti_alias: true, ..sk::Paint::default() };
    let colour = |c: [u8; 3], a: f32| sk::Color::from_rgba8(c[0], c[1], c[2], (a * 255.0).round() as u8);
    match fill {
        Fill::None => return None,
        Fill::Solid(c) => paint.set_color(colour(c, opacity)),
        Fill::Gradient(i) => {
            let g = gradients.get(i)?;
            let stops = g.stops.iter().map(|(o, c, a)| sk::GradientStop::new(*o, colour(*c, a * opacity))).collect();
            let [a, b, c, d] = g.geometry;
            paint.shader = if g.radial {
                sk::RadialGradient::new(
                    sk::Point::from_xy(a, b),
                    0.0,
                    sk::Point::from_xy(a, b),
                    c,
                    stops,
                    sk::SpreadMode::Pad,
                    sk::Transform::identity(),
                )?
            } else {
                sk::LinearGradient::new(
                    sk::Point::from_xy(a, b),
                    sk::Point::from_xy(c, d),
                    stops,
                    sk::SpreadMode::Pad,
                    sk::Transform::identity(),
                )?
            };
        }
    }
    Some(paint)
}

/// `(icns type code, pixel size)` of every entry of the macOS icon, in file order.
pub const ICNS_ENTRIES: [(&[u8; 4], u32); 10] = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"ic11", 32),
    (b"ic12", 64),
    (b"ic07", 128),
    (b"ic13", 256),
    (b"ic08", 256),
    (b"ic14", 512),
    (b"ic09", 512),
    (b"ic10", 1024),
];

/// Pixel sizes of the Windows `.ico`.
pub const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// The PNGs of `sizes`, each rendered once.
fn pngs(svg: &str, sizes: &[u32], frame: Frame) -> Result<HashMap<u32, Vec<u8>>, String> {
    let mut out = HashMap::new();
    for &size in sizes {
        if let std::collections::hash_map::Entry::Vacant(slot) = out.entry(size) {
            slot.insert(png(&render(svg, size, frame)?)?);
        }
    }
    Ok(out)
}

/// An `.icns` with PNG entries for [`ICNS_ENTRIES`] (the whole 1024 canvas, so the plate keeps its margin).
pub fn icns(svg: &str) -> Result<Vec<u8>, String> {
    let sizes: Vec<u32> = ICNS_ENTRIES.iter().map(|e| e.1).collect();
    let pngs = pngs(svg, &sizes, Frame::Canvas)?;
    let mut body = Vec::new();
    for (kind, size) in ICNS_ENTRIES {
        let data = pngs.get(&size).ok_or("icon size missing")?;
        body.extend_from_slice(kind);
        body.extend_from_slice(&u32::try_from(data.len() + 8).map_err(|_| "icon too large")?.to_be_bytes());
        body.extend_from_slice(data);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&u32::try_from(body.len() + 8).map_err(|_| "icon too large")?.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// A Windows `.ico` with PNG entries for [`ICO_SIZES`] (the plate edge to edge).
pub fn ico(svg: &str) -> Result<Vec<u8>, String> {
    let pngs = pngs(svg, &ICO_SIZES, Frame::Plate)?;
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(ICO_SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * ICO_SIZES.len();
    for size in ICO_SIZES {
        let data = pngs.get(&size).ok_or("icon size missing")?;
        let side = if size >= 256 { 0 } else { size as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&u32::try_from(data.len()).map_err(|_| "icon too large")?.to_le_bytes());
        out.extend_from_slice(&u32::try_from(offset).map_err(|_| "icon too large")?.to_le_bytes());
        offset += data.len();
    }
    for size in ICO_SIZES {
        out.extend_from_slice(pngs.get(&size).ok_or("icon size missing")?);
    }
    Ok(out)
}

/// The logo PNG for the MSIX package at `size` px (plate edge to edge; `document` selects the file icon).
pub fn logo_png(size: u32, document: bool) -> Result<Vec<u8>, String> {
    if document { png(&render(DOCUMENT_SVG, size, Frame::Canvas)?) } else { png(&render(APP_SVG, size, Frame::Plate)?) }
}

/// `--preview <dir>`: where to write inspection PNGs instead of rewriting the committed files.
fn preview_dir(args: &[String]) -> Result<Option<std::path::PathBuf>, String> {
    match args {
        [] => Ok(None),
        [flag, dir] if flag == "--preview" => Ok(Some(dir.into())),
        _ => Err("usage: cargo xtask icon [--preview <dir>]".into()),
    }
}

/// Inspection PNGs: the app icon on the canvas at 1024/128/64/32/16, the Windows frame at 256/32/16, the document icon at 512/64.
fn write_previews(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let mut jobs: Vec<(String, Vec<u8>)> = Vec::new();
    for size in [1024, 128, 64, 32, 16] {
        jobs.push((format!("app-{size}.png"), png(&render(APP_SVG, size, Frame::Canvas)?)?));
    }
    for size in [256, 32, 16] {
        jobs.push((format!("windows-{size}.png"), logo_png(size, false)?));
    }
    for size in [512, 64] {
        jobs.push((format!("document-{size}.png"), logo_png(size, true)?));
    }
    for (name, bytes) in jobs {
        let path = dir.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))?;
        println!("icon: {}", path.display());
    }
    Ok(())
}

/// `cargo xtask icon`: rewrites the committed derived files (the host's 512 px PNG and the Windows `.ico`).
pub fn run(root: &std::path::Path, args: &[String]) -> Result<(), String> {
    if let Some(dir) = preview_dir(args)? {
        return write_previews(&dir);
    }
    let write = |rel: &str, bytes: Vec<u8>| -> Result<(), String> {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))?;
        println!("icon: {}", path.display());
        Ok(())
    };
    write(HOST_PNG, png(&render(APP_SVG, 512, Frame::Canvas)?)?)?;
    write(WINDOWS_ICO, ico(APP_SVG)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn luma(p: [u8; 4]) -> u32 {
        (u32::from(p[0]) * 3 + u32::from(p[1]) * 6 + u32::from(p[2])) / 10
    }

    fn be32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    fn le16(b: &[u8], at: usize) -> u32 {
        u32::from(u16::from_le_bytes([b[at], b[at + 1]]))
    }

    fn le32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// (width, height) from a PNG's IHDR, after checking the signature.
    fn png_dimensions(png: &[u8]) -> (u32, u32) {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
        assert_eq!(&png[12..16], b"IHDR");
        (be32(png, 16), be32(png, 20))
    }

    #[test]
    fn the_macos_canvas_keeps_the_squircle_margin_and_the_plate_is_opaque() {
        let img = render(APP_SVG, 1024, Frame::Canvas).unwrap();
        assert_eq!(img.at(8, 8)[3], 0, "canvas corner outside the plate");
        assert_eq!(img.at(512, 4)[3], 0, "the 100 px margin above the plate stays clear");
        assert_eq!(img.at(512, 140)[3], 255, "inside the plate near its top edge");
        // The plate is a squircle, not a square: its corner is cut away.
        assert_eq!(img.at(104, 104)[3], 0, "the plate's corner is rounded off");
    }

    #[test]
    fn the_windows_frame_fills_the_square_edge_to_edge() {
        let img = render(APP_SVG, 256, Frame::Plate).unwrap();
        assert_eq!(img.at(0, 128)[3], 255, "left edge middle is plate");
        assert_eq!(img.at(255, 128)[3], 255, "right edge middle is plate");
        assert_eq!(img.at(128, 0)[3], 255, "top edge middle is plate");
        assert_eq!(img.at(0, 0)[3], 0, "the corner is rounded off");
    }

    #[test]
    fn the_plate_is_dark_and_the_flame_is_warm_and_bright() {
        let img = render(APP_SVG, 512, Frame::Canvas).unwrap();
        let plate = img.at(512 / 2, 90);
        assert!(luma(plate) < 90, "dark plate {plate:?}");
        let flame = img.at(256, 300);
        assert!(flame[0] > 200 && flame[0] > flame[2] + 80, "warm flame {flame:?}");
        let core = img.at(256, 330);
        assert!(luma(core) > 170, "bright flame core {core:?}");
    }

    #[test]
    fn gradients_run_top_to_bottom_and_opacity_blends() {
        let img = render(APP_SVG, 512, Frame::Canvas).unwrap();
        // Plate gradient is lighter at the top than at the bottom corners (away from the glow).
        assert!(luma(img.at(110, 100)) > luma(img.at(110, 410)), "top lighter than bottom");
        // The soft shadow under the plate is translucent, not opaque and not absent.
        let shadow = img.at(256, 475);
        assert!(shadow[3] > 0 && shadow[3] < 200, "shadow alpha {shadow:?}");
    }

    #[test]
    fn the_document_icon_is_a_light_page_with_a_folded_corner() {
        let img = render(DOCUMENT_SVG, 512, Frame::Canvas).unwrap();
        assert_eq!(img.at(20, 20)[3], 0, "outside the page");
        assert!(luma(img.at(120, 400)) > 200, "paper is light");
        assert_eq!(img.at(500, 30)[3], 0, "the top-right corner is folded away");
        let flame = img.at(256, 380);
        assert!(flame[0] > 200 && flame[2] < 150, "flame on the page {flame:?}");
    }

    #[test]
    fn unsupported_svg_is_an_error_not_a_blank_icon() {
        let svg = r#"<svg viewBox="0 0 10 10"><text x="1" y="1">x</text></svg>"#;
        assert!(render(svg, 16, Frame::Canvas).unwrap_err().contains("text"));
        let svg = r#"<svg viewBox="0 0 10 10"><path d="M1 1 A 2 2 0 0 1 4 4"/></svg>"#;
        assert!(render(svg, 16, Frame::Canvas).is_err());
        let svg = r##"<svg viewBox="0 0 10 10"><path d="M1 1 L 4" fill="#ff0000"/></svg>"##;
        assert!(render(svg, 16, Frame::Canvas).is_err(), "truncated path data");
        assert!(render("not svg", 16, Frame::Canvas).is_err());
        assert!(render(APP_SVG, 0, Frame::Canvas).is_err());
    }

    #[test]
    fn png_output_decodes_to_the_same_pixels() {
        let img = render(APP_SVG, 64, Frame::Plate).unwrap();
        let bytes = png(&img).unwrap();
        assert_eq!(png_dimensions(&bytes), (64, 64));
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap().to_rgba8();
        assert_eq!(decoded.as_raw(), &img.pixels);
        assert!(bytes.len() < 64 * 64 * 4, "compressed, not stored blocks: {}", bytes.len());
    }

    /// The icns container is self-consistent and holds every size macOS asks for, 16-1024 px, at 1x and 2x.
    #[test]
    fn the_icns_holds_every_size_from_16_to_1024_as_png() {
        let icns = icns(APP_SVG).unwrap();
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(be32(&icns, 4) as usize, icns.len(), "total length");
        let mut at = 8;
        let mut found: Vec<([u8; 4], u32)> = Vec::new();
        while at < icns.len() {
            let kind = [icns[at], icns[at + 1], icns[at + 2], icns[at + 3]];
            let len = be32(&icns, at + 4) as usize;
            let (w, h) = png_dimensions(&icns[at + 8..at + len]);
            assert_eq!(w, h);
            found.push((kind, w));
            at += len;
        }
        assert_eq!(at, icns.len(), "no trailing bytes");
        let expected: Vec<([u8; 4], u32)> = ICNS_ENTRIES.iter().map(|(k, s)| (**k, *s)).collect();
        assert_eq!(found, expected);
        let sizes: std::collections::BTreeSet<u32> = found.iter().map(|f| f.1).collect();
        assert_eq!(sizes.into_iter().collect::<Vec<_>>(), vec![16, 32, 64, 128, 256, 512, 1024]);
    }

    /// Independent oracle: macOS's own `iconutil` reads the icns back into a 10-image iconset.
    #[test]
    fn iconutil_unpacks_the_icns_into_an_iconset() {
        if !cfg!(target_os = "macos") {
            println!("SKIPPED (not macOS; not a pass)");
            return;
        }
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-iconutil-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("BurnBurn.icns");
        std::fs::write(&file, icns(APP_SVG).unwrap()).unwrap();
        let out = std::process::Command::new("iconutil")
            .args(["--convert", "iconset", "--output"])
            .arg(dir.join("BurnBurn.iconset"))
            .arg(&file)
            .output()
            .unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "{text}");
        let mut names: Vec<String> = std::fs::read_dir(dir.join("BurnBurn.iconset"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names.len(), 10, "{names:?}");
        assert!(names.contains(&"icon_16x16.png".to_owned()) && names.contains(&"icon_512x512@2x.png".to_owned()));
    }

    /// The `.ico` directory lists 16-256 px PNG entries that point at real image data.
    #[test]
    fn the_ico_holds_the_windows_sizes_as_png() {
        let ico = ico(APP_SVG).unwrap();
        assert_eq!((le16(&ico, 0), le16(&ico, 2)), (0, 1), "reserved 0, type 1 = icon");
        let count = le16(&ico, 4) as usize;
        assert_eq!(count, ICO_SIZES.len());
        let mut sizes = Vec::new();
        for i in 0..count {
            let e = 6 + i * 16;
            let (w, h) = (ico[e], ico[e + 1]);
            let (len, offset) = (le32(&ico, e + 8) as usize, le32(&ico, e + 12) as usize);
            assert!(offset + len <= ico.len(), "entry {i} inside the file");
            let real = png_dimensions(&ico[offset..offset + len]);
            let declared = if w == 0 { 256 } else { u32::from(w) };
            assert_eq!(real, (declared, declared), "directory size equals PNG size");
            assert_eq!(h, w);
            assert_eq!(le16(&ico, e + 6), 32, "32 bits per pixel");
            sizes.push(declared);
        }
        assert_eq!(sizes, ICO_SIZES.to_vec());
    }

    /// The committed artefacts are exactly what the sources render to (`cargo xtask icon` rewrites them).
    #[test]
    fn committed_icon_files_are_current() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let png512 = std::fs::read(root.join(HOST_PNG)).unwrap_or_default();
        assert!(!png512.is_empty(), "{HOST_PNG} is missing: run `cargo xtask icon`");
        assert!(
            png512 == png(&render(APP_SVG, 512, Frame::Canvas).unwrap()).unwrap(),
            "{HOST_PNG}: run `cargo xtask icon`"
        );
        let ico_file = std::fs::read(root.join(WINDOWS_ICO)).unwrap_or_default();
        assert!(!ico_file.is_empty(), "{WINDOWS_ICO} is missing: run `cargo xtask icon`");
        assert!(ico_file == ico(APP_SVG).unwrap(), "{WINDOWS_ICO}: run `cargo xtask icon`");
    }
}
