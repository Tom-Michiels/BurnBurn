//! SVG-subset parser and tiny-skia rasteriser.
//!
//! Adapted from SolidCraft's `ui-egui/src/icon_svg.rs` (same owner, MIT OR Apache-2.0): the
//! parser and drawing code are the same; the egui, accent and colour-vision machinery is gone and
//! colour roles come from a runtime [`Palette`] instead of build-time constants.

use std::collections::HashMap;

use tiny_skia as sk;

use crate::palette::Palette;
use crate::{IconError, Theme};

/// How a shape is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Paint {
    None,
    /// Erase earlier artwork; never paint a theme background into the image.
    Knockout,
    /// A palette role (index into the palette).
    Role(usize),
    /// A fixed colour (mask artwork).
    Fixed([u8; 3]),
}

#[derive(Debug)]
struct Node {
    path: sk::Path,
    fill: Paint,
    rule: sk::FillRule,
    stroke: Paint,
    width: f32,
    dash: Vec<f32>,
    opacity: f32,
    mask: Option<usize>,
}

#[derive(Debug)]
enum Item {
    Node(Node),
    Group { clip: Option<usize>, opacity: f32, items: Vec<Item> },
}

/// A parsed master.
#[derive(Debug)]
pub(crate) struct Doc {
    view: f32,
    items: Vec<Item>,
    masks: Vec<Vec<Node>>,
    clips: Vec<Vec<Node>>,
}

// ---------------------------------------------------------------------------------------------
// Parsing

struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, &'a str)>,
    close: bool,
    selfclose: bool,
}

impl Tag<'_> {
    fn get(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| *n == k).map(|(_, v)| *v)
    }
    fn num(&self, k: &str) -> Result<f32, String> {
        self.get(k).ok_or_else(|| format!("missing {k}"))?.trim().parse::<f32>().map_err(|e| format!("{k}: {e}"))
    }
}

fn tags(src: &str) -> Result<Vec<Tag<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find('<') {
        let after = rest.get(i + 1..).ok_or("truncated")?;
        let j = after.find('>').ok_or("unterminated tag")?;
        let body = after.get(..j).ok_or("truncated")?;
        rest = after.get(j + 1..).ok_or("truncated")?;
        if body.starts_with('?') || body.starts_with('!') {
            continue;
        }
        let (close, body) = match body.strip_prefix('/') {
            Some(b) => (true, b),
            None => (false, body),
        };
        let (selfclose, body) = match body.strip_suffix('/') {
            Some(b) => (true, b),
            None => (false, body),
        };
        let body = body.trim();
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let name = body.get(..name_end).ok_or("bad tag")?;
        let mut attrs = Vec::new();
        let mut a = body.get(name_end..).unwrap_or("");
        while let Some(eq) = a.find('=') {
            let key = a.get(..eq).ok_or("bad attribute")?.trim();
            let v = a.get(eq + 1..).ok_or("bad attribute")?.trim_start();
            let q = v.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or("unquoted attribute")?;
            let v = v.get(1..).ok_or("bad attribute")?;
            let end = v.find(q).ok_or("unterminated attribute")?;
            attrs.push((key, v.get(..end).ok_or("bad attribute")?));
            a = v.get(end + 1..).unwrap_or("");
        }
        out.push(Tag { name, attrs, close, selfclose });
    }
    Ok(out)
}

/// An SVG elliptical arc as cubic Béziers appended to `pb`.
#[allow(clippy::too_many_arguments)]
fn arc(
    pb: &mut sk::PathBuilder,
    x1: f32,
    y1: f32,
    rx: f32,
    ry: f32,
    rot_deg: f32,
    large: bool,
    sweep: bool,
    x2: f32,
    y2: f32,
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-6 || ry < 1e-6 || ((x1 - x2).abs() < 1e-6 && (y1 - y2).abs() < 1e-6) {
        pb.line_to(x2, y2);
        return;
    }
    let phi = rot_deg.to_radians();
    let (sp, cp) = phi.sin_cos();
    let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let (x1p, y1p) = (cp * dx + sp * dy, -sp * dx + cp * dy);
    let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let coef = sign * (num / den).max(0.0).sqrt();
    let (cxp, cyp) = (coef * rx * y1p / ry, -coef * ry * x1p / rx);
    let (cx, cy) = (cp * cxp - sp * cyp + (x1 + x2) / 2.0, sp * cxp + cp * cyp + (y1 + y2) / 2.0);
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let (ux, uy) = ((x1p - cxp) / rx, (y1p - cyp) / ry);
    let (vx, vy) = ((-x1p - cxp) / rx, (-y1p - cyp) / ry);
    let theta = ang(1.0, 0.0, ux, uy);
    let mut dtheta = ang(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    }
    let n = ((dtheta.abs() / std::f32::consts::FRAC_PI_2).ceil() as usize).clamp(1, 8);
    let delta = dtheta / n as f32;
    let t = 4.0 / 3.0 * (delta / 4.0).tan();
    let map = |x: f32, y: f32| (cx + rx * cp * x - ry * sp * y, cy + rx * sp * x + ry * cp * y);
    for i in 0..n {
        let a1 = theta + delta * i as f32;
        let a2 = a1 + delta;
        let (s1, c1) = a1.sin_cos();
        let (s2, c2) = a2.sin_cos();
        let (p1x, p1y) = map(c1 - t * s1, s1 + t * c1);
        let (p2x, p2y) = map(c2 + t * s2, s2 - t * c2);
        let (ex, ey) = map(c2, s2);
        pb.cubic_to(p1x, p1y, p2x, p2y, ex, ey);
    }
}

fn parse_path(d: &str) -> Result<sk::Path, String> {
    let mut toks: Vec<&str> = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = *b.get(i).ok_or("path")?;
        if c.is_ascii_alphabetic() {
            toks.push(d.get(i..=i).ok_or("path")?);
            i += 1;
        } else if c == b'-' || c == b'.' || c.is_ascii_digit() {
            let s = i;
            i += 1;
            while let Some(&n) = b.get(i) {
                if n.is_ascii_digit()
                    || n == b'.'
                    || ((n == b'-' || n == b'+') && matches!(b.get(i - 1), Some(b'e' | b'E')))
                    || n == b'e'
                    || n == b'E'
                {
                    i += 1;
                } else {
                    break;
                }
            }
            toks.push(d.get(s..i).ok_or("path")?);
        } else {
            i += 1;
        }
    }
    let mut pb = sk::PathBuilder::new();
    let (mut k, mut cmd) = (0usize, ' ');
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut open = false;
    let num = |k: &mut usize| -> Result<f32, String> {
        let t = toks.get(*k).ok_or("path: missing number")?;
        *k += 1;
        t.parse::<f32>().map_err(|e| format!("path: {e}"))
    };
    while k < toks.len() {
        let t = *toks.get(k).ok_or("path")?;
        if t.len() == 1 && t.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
            cmd = t.chars().next().ok_or("path")?;
            k += 1;
        }
        match cmd {
            'M' => {
                (x, y) = (num(&mut k)?, num(&mut k)?);
                pb.move_to(x, y);
                open = true;
                cmd = 'L';
            }
            'L' => {
                (x, y) = (num(&mut k)?, num(&mut k)?);
                pb.line_to(x, y);
            }
            'C' => {
                let v = [num(&mut k)?, num(&mut k)?, num(&mut k)?, num(&mut k)?, num(&mut k)?, num(&mut k)?];
                pb.cubic_to(v[0], v[1], v[2], v[3], v[4], v[5]);
                (x, y) = (v[4], v[5]);
            }
            'Q' => {
                let v = [num(&mut k)?, num(&mut k)?, num(&mut k)?, num(&mut k)?];
                pb.quad_to(v[0], v[1], v[2], v[3]);
                (x, y) = (v[2], v[3]);
            }
            'A' => {
                let (rx, ry, rot) = (num(&mut k)?, num(&mut k)?, num(&mut k)?);
                let (fa, fs) = (num(&mut k)? != 0.0, num(&mut k)? != 0.0);
                let (ex, ey) = (num(&mut k)?, num(&mut k)?);
                arc(&mut pb, x, y, rx, ry, rot, fa, fs, ex, ey);
                (x, y) = (ex, ey);
            }
            'Z' => {
                if open {
                    pb.close();
                }
            }
            other => return Err(format!("path command {other} is not supported")),
        }
    }
    pb.finish().ok_or_else(|| "empty path".to_string())
}

fn parse_dash(v: Option<&str>) -> Vec<f32> {
    let mut d: Vec<f32> = v.map(|s| s.split([' ', ',']).filter_map(|n| n.parse().ok()).collect()).unwrap_or_default();
    if d.len() % 2 == 1 {
        d.extend(d.clone());
    }
    d
}

fn url_ref(v: &str) -> Option<&str> {
    v.strip_prefix("url(#")?.strip_suffix(')')
}

fn parse_paint(v: Option<&str>, in_mask: bool, palette: &Palette) -> Result<Paint, String> {
    let Some(v) = v else { return Ok(Paint::None) };
    match v {
        "none" => Ok(Paint::None),
        "currentColor" => Ok(Paint::Role(palette.line_role())),
        "white" if in_mask => Ok(Paint::Fixed([255, 255, 255])),
        "black" if in_mask => Ok(Paint::Fixed([0, 0, 0])),
        _ => palette
            .role_for_source(&v.to_ascii_lowercase())
            .map(|role| if role == palette.gap_role() { Paint::Knockout } else { Paint::Role(role) })
            .ok_or_else(|| format!("colour {v} is not a palette colour")),
    }
}

enum Frame {
    Group { clip: Option<usize>, opacity: f32, items: Vec<Item> },
    Mask(usize),
    Clip(usize),
    Defs,
}

impl Doc {
    pub(crate) fn parse(src: &str, palette: &Palette) -> Result<Doc, String> {
        let mut doc = Doc { view: 64.0, items: Vec::new(), masks: Vec::new(), clips: Vec::new() };
        let (mut mask_ids, mut clip_ids): (HashMap<String, usize>, HashMap<String, usize>) =
            (HashMap::new(), HashMap::new());
        let mut stack: Vec<Frame> = Vec::new();
        for t in tags(src)? {
            match (t.name, t.close) {
                ("svg", false) => {
                    if let Some(vb) = t.get("viewBox") {
                        doc.view = parse_view_box(vb)?;
                    }
                }
                ("svg" | "title", _) => {}
                ("defs", false) => stack.push(Frame::Defs),
                ("mask", false) => {
                    doc.masks.push(Vec::new());
                    mask_ids.insert(t.get("id").ok_or("mask without id")?.to_string(), doc.masks.len() - 1);
                    stack.push(Frame::Mask(doc.masks.len() - 1));
                }
                ("clipPath", false) => {
                    doc.clips.push(Vec::new());
                    clip_ids.insert(t.get("id").ok_or("clipPath without id")?.to_string(), doc.clips.len() - 1);
                    stack.push(Frame::Clip(doc.clips.len() - 1));
                }
                ("g", false) => {
                    let clip = match t.get("clip-path").and_then(url_ref) {
                        Some(id) => Some(*clip_ids.get(id).ok_or("unknown clip")?),
                        None => None,
                    };
                    let opacity = t.get("opacity").and_then(|o| o.parse().ok()).unwrap_or(1.0);
                    stack.push(Frame::Group { clip, opacity, items: Vec::new() });
                }
                ("defs" | "mask" | "clipPath" | "g", true) => match stack.pop() {
                    Some(Frame::Group { clip, opacity, items }) => {
                        let item = Item::Group { clip, opacity, items };
                        match stack.last_mut() {
                            Some(Frame::Group { items, .. }) => items.push(item),
                            _ => doc.items.push(item),
                        }
                    }
                    Some(_) => {}
                    None => return Err("unbalanced tags".into()),
                },
                ("path" | "circle", _) => {
                    let in_mask = matches!(stack.last(), Some(Frame::Mask(_)));
                    let path = if t.name == "path" {
                        parse_path(t.get("d").ok_or("path without d")?)?
                    } else {
                        sk::PathBuilder::from_circle(t.num("cx")?, t.num("cy")?, t.num("r")?).ok_or("bad circle")?
                    };
                    let mask = match t.get("mask").and_then(url_ref) {
                        Some(id) => Some(*mask_ids.get(id).ok_or("unknown mask")?),
                        None => None,
                    };
                    let dash = parse_dash(t.get("stroke-dasharray"));
                    let mut stroke = parse_paint(t.get("stroke"), in_mask, palette)?;
                    let visible_stroke = match stroke {
                        Paint::Role(role) => {
                            !matches!(palette.name(role), Some("gap" | "active-1" | "active-2" | "active-3"))
                        }
                        _ => false,
                    };
                    // Non-blue dotted/dashed references share the guide colour.
                    // Blue source paints retain their semantic accent; keep
                    // background knockouts and literal mask paints unchanged.
                    if !dash.is_empty() && visible_stroke && !in_mask {
                        stroke = Paint::Role(palette.guide_role());
                    }
                    let node = Node {
                        path,
                        fill: if t.get("fill").is_none() && in_mask {
                            Paint::None
                        } else {
                            parse_paint(t.get("fill").or(Some("none")), in_mask, palette)?
                        },
                        rule: if t.get("fill-rule") == Some("evenodd") {
                            sk::FillRule::EvenOdd
                        } else {
                            sk::FillRule::Winding
                        },
                        stroke,
                        width: t.get("stroke-width").and_then(|w| w.parse().ok()).unwrap_or(1.0),
                        dash,
                        opacity: t.get("opacity").and_then(|o| o.parse().ok()).unwrap_or(1.0),
                        mask,
                    };
                    match stack.last_mut() {
                        Some(Frame::Mask(i)) => doc.masks.get_mut(*i).ok_or("mask")?.push(node),
                        Some(Frame::Clip(i)) => doc.clips.get_mut(*i).ok_or("clip")?.push(node),
                        Some(Frame::Group { items, .. }) => items.push(Item::Node(node)),
                        _ => doc.items.push(Item::Node(node)),
                    }
                }
                (other, _) => return Err(format!("element <{other}> is not supported")),
            }
            let _ = t.selfclose;
        }
        if !stack.is_empty() {
            return Err("unbalanced tags".into());
        }
        Ok(doc)
    }
}

fn parse_view_box(v: &str) -> Result<f32, String> {
    let n: Vec<f32> = v
        .split([' ', ','])
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| format!("viewBox `{v}`"))?;
    match n.as_slice() {
        [0.0, 0.0, w, h] if w == h && *w > 0.0 && w.is_finite() => Ok(*w),
        _ => Err(format!("viewBox `{v}` is not a square `0 0 N N`")),
    }
}

// ---------------------------------------------------------------------------------------------
// Rendering

fn paint_of(c: [u8; 3], alpha: f32) -> sk::Paint<'static> {
    let mut p = sk::Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], (alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
    p.anti_alias = true;
    p
}

struct Look<'a> {
    palette: &'a Palette,
    theme: Theme,
}

impl Look<'_> {
    fn shape_paint(&self, paint: Paint, alpha: f32) -> Option<sk::Paint<'static>> {
        match paint {
            Paint::None => None,
            Paint::Knockout => {
                let mut p = paint_of([0, 0, 0], alpha);
                p.blend_mode = sk::BlendMode::DestinationOut;
                Some(p)
            }
            Paint::Fixed(c) => Some(paint_of(c, alpha)),
            Paint::Role(r) => self.palette.rgb(r, self.theme).map(|c| paint_of(c, alpha)),
        }
    }
}

fn draw_node(
    pm: &mut sk::Pixmap,
    n: &Node,
    look: &Look,
    tf: sk::Transform,
    doc: &Doc,
    clip: Option<&sk::Mask>,
    opacity: f32,
) {
    let own;
    let mask = if let Some(m) = n.mask {
        own = doc.masks.get(m).and_then(|nodes| {
            let mut sub = sk::Pixmap::new(pm.width(), pm.height())?;
            for node in nodes {
                draw_node(&mut sub, node, look, tf, doc, None, 1.0);
            }
            let mut own = sk::Mask::from_pixmap(sub.as_ref(), sk::MaskType::Luminance);
            if let Some(parent) = clip {
                for (value, parent) in own.data_mut().iter_mut().zip(parent.data()) {
                    *value = ((u16::from(*value) * u16::from(*parent) + 127) / 255) as u8;
                }
            }
            Some(own)
        });
        own.as_ref()
    } else {
        clip
    };
    let alpha = n.opacity * opacity;
    if let Some(paint) = look.shape_paint(n.fill, alpha) {
        pm.fill_path(&n.path, &paint, n.rule, tf, mask);
    }
    if let Some(paint) = look.shape_paint(n.stroke, alpha) {
        let stroke = sk::Stroke {
            width: n.width,
            line_cap: sk::LineCap::Round,
            line_join: sk::LineJoin::Round,
            dash: if n.dash.is_empty() { None } else { sk::StrokeDash::new(n.dash.clone(), 0.0) },
            ..sk::Stroke::default()
        };
        pm.stroke_path(&n.path, &paint, &stroke, tf, mask);
    }
}

fn draw_items(
    pm: &mut sk::Pixmap,
    items: &[Item],
    look: &Look,
    tf: sk::Transform,
    doc: &Doc,
    clip: Option<&sk::Mask>,
    opacity: f32,
) {
    for item in items {
        match item {
            Item::Node(n) => draw_node(pm, n, look, tf, doc, clip, opacity),
            Item::Group { clip: c, opacity: o, items } => {
                let mask = c.and_then(|c| doc.clips.get(c)).and_then(|nodes| {
                    let mut m = sk::Mask::new(pm.width(), pm.height())?;
                    for n in nodes {
                        m.fill_path(&n.path, n.rule, true, tf);
                    }
                    Some(m)
                });
                draw_items(pm, items, look, tf, doc, mask.as_ref().or(clip), opacity * o);
            }
        }
    }
}

/// Rasterise `doc` into a square `side` x `side` pixmap (premultiplied, tiny-skia's native format).
pub(crate) fn rasterise(doc: &Doc, palette: &Palette, theme: Theme, side: u32) -> Result<sk::Pixmap, IconError> {
    let mut pm = sk::Pixmap::new(side, side).ok_or(IconError::InvalidSize)?;
    let s = side as f32 / doc.view;
    let look = Look { palette, theme };
    draw_items(&mut pm, &doc.items, &look, sk::Transform::from_scale(s, s), doc, None, 1.0);
    Ok(pm)
}
