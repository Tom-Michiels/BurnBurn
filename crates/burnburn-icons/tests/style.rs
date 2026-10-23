//! LF8: the UI icon set is one family. Every master uses the agreed stroke set and attributes, and
//! every icon renders at the toolstrip/menu sizes and display scales without blank or clipped output.

#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)] // test helpers outside #[test] fns

use std::fs;
use std::path::PathBuf;

use burnburn_icons::{RgbaImage, Theme, icon_ids, render};

/// Stroke widths on the 64 grid: fine detail (geometry) and the standard action stroke.
const STROKES: [&str; 2] = ["2.55", "4.25"];
/// Dot and dash patterns for construction and guide geometry.
const DASHES: [&str; 2] = ["0.1 5", "2.4 4.2"];
const ELEMENTS: [&str; 7] = ["svg", "title", "path", "circle", "defs", "mask", "g"];
const ATTRIBUTES: [&str; 19] = [
    "xmlns", "viewBox", "d", "cx", "cy", "r", "fill", "stroke", "stroke-width", "stroke-linejoin", "stroke-linecap",
    "stroke-dasharray", "mask", "id", "maskUnits", "x", "y", "width", "height",
];
/// Point sizes of menu/table (16), ribbon (20) and toolbar (24), at 1x, 1.5x, 2x and 3x.
const POINTS: [u32; 3] = [16, 20, 24];
const SCALES: [f32; 4] = [1.0, 1.5, 2.0, 3.0];

fn svg_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/ui/svg")
}

fn masters() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(svg_dir())
        .expect("svg dir")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "svg"))
        .map(|e| {
            (
                e.path().file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                fs::read_to_string(e.path()).expect("read svg"),
            )
        })
        .collect();
    out.sort();
    out
}

/// Opening tags as (name, attributes). Deliberately independent of the crate's own parser.
fn tags(src: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out = Vec::new();
    for chunk in src.split('<').skip(1) {
        let Some(body) = chunk.split('>').next() else { continue };
        if body.starts_with('/') || body.starts_with('?') || body.starts_with('!') {
            continue;
        }
        let body = body.trim_end_matches('/');
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let mut attrs = Vec::new();
        let mut rest = &body[name_end..];
        while let Some(eq) = rest.find("=\"") {
            let key = rest[..eq].trim().to_owned();
            let after = &rest[eq + 2..];
            let end = after.find('"').expect("closing quote");
            attrs.push((key, after[..end].to_owned()));
            rest = &after[end + 1..];
        }
        out.push((body[..name_end].to_owned(), attrs));
    }
    out
}

#[test]
fn every_master_uses_only_the_agreed_stroke_set() {
    let mut bad = Vec::new();
    for (name, src) in masters() {
        for (el, attrs) in tags(&src) {
            for (k, v) in &attrs {
                let ok = match k.as_str() {
                    "stroke-width" => STROKES.contains(&v.as_str()),
                    "stroke-linecap" | "stroke-linejoin" => v == "round",
                    "stroke-dasharray" => DASHES.contains(&v.as_str()),
                    "viewBox" => v == "0 0 64 64",
                    _ => true,
                };
                if !ok {
                    bad.push(format!("{name}: <{el}> {k}=\"{v}\""));
                }
            }
        }
    }
    assert!(bad.is_empty(), "off-style attributes:\n{}", bad.join("\n"));
}

#[test]
fn every_master_uses_only_agreed_elements_and_attributes() {
    let mut bad = Vec::new();
    for (name, src) in masters() {
        let all = tags(&src);
        assert!(all.iter().any(|(e, _)| e == "title"), "{name}: missing <title> (accessible name)");
        for (el, attrs) in all {
            if !ELEMENTS.contains(&el.as_str()) {
                bad.push(format!("{name}: element <{el}>"));
            }
            for (k, _) in attrs {
                if !ATTRIBUTES.contains(&k.as_str()) {
                    bad.push(format!("{name}: <{el}> attribute {k}"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "unexpected markup:\n{}", bad.join("\n"));
}

#[test]
fn stroked_shapes_always_use_round_caps_and_joins() {
    let mut bad = Vec::new();
    for (name, src) in masters() {
        for (el, attrs) in tags(&src) {
            let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
            if get("stroke").is_some_and(|s| s != "none")
                && (get("stroke-linecap") != Some("round") || get("stroke-linejoin") != Some("round"))
            {
                bad.push(format!("{name}: <{el}> stroke without round cap and join"));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn path_coordinates_have_at_most_two_decimals_and_stay_on_the_canvas() {
    // Drawn on a 64 grid: no coordinate outside 0..=64, none with sub-hundredth noise.
    let mut bad = Vec::new();
    for (name, src) in masters() {
        // Mask definitions legitimately cover more than the canvas.
        let visible = match (src.find("<mask"), src.find("</mask>")) {
            (Some(a), Some(b)) => format!("{}{}", &src[..a], &src[b + "</mask>".len()..]),
            _ => src.clone(),
        };
        for (el, attrs) in tags(&visible) {
            for (k, v) in &attrs {
                if !matches!(k.as_str(), "d" | "cx" | "cy") {
                    continue;
                }
                for tok in v.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).filter(|s| !s.is_empty()) {
                    let decimals = tok.split('.').nth(1).map_or(0, str::len);
                    if decimals > 2 {
                        bad.push(format!("{name}: <{el}> {k} number {tok}"));
                    }
                    if tok.parse::<f32>().is_ok_and(|n| !(-0.01..=64.01).contains(&n)) && !v.contains('A') {
                        bad.push(format!("{name}: <{el}> {k} number {tok} off canvas"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

fn alpha_bounds(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    let mut any = false;
    for y in 0..img.height {
        for x in 0..img.width {
            if img.pixels[((y * img.width + x) * 4 + 3) as usize] > 8 {
                any = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    any.then_some((x0, y0, x1, y1))
}

#[test]
fn every_icon_renders_at_all_sizes_and_scales_with_the_right_pixel_size_and_content() {
    let ids: Vec<&str> = icon_ids().collect();
    assert!(ids.len() >= 90);
    let mut bad: Vec<String> = Vec::new();
    for id in ids {
        for theme in [Theme::Light, Theme::Dark] {
            for px in POINTS {
                for scale in SCALES {
                    let img = render(id, px, scale, theme).unwrap_or_else(|e| panic!("{id} {px}@{scale}: {e}"));
                    let side = (f64::from(px) * f64::from(scale)).round() as u32;
                    assert_eq!((img.width, img.height), (side, side), "{id} {px}@{scale}");
                    assert_eq!(img.pixels.len() as u32, side * side * 4);
                    let Some((x0, y0, x1, y1)) = alpha_bounds(&img) else {
                        bad.push(format!("{id} {px}@{scale} ({theme:?}): blank"));
                        continue;
                    };
                    // Not clipped: the outermost pixel ring stays (nearly) transparent.
                    if edge_alpha(&img) > EDGE_ALPHA_MAX {
                        bad.push(format!("{id} {px}@{scale} ({theme:?}): artwork reaches the edge (alpha {})", edge_alpha(&img)));
                    }
                    // Not a speck: the artwork covers at least half of the width or height.
                    if (x1 - x0 + 1).max(y1 - y0 + 1) * 2 < side {
                        bad.push(format!("{id} {px}@{scale} ({theme:?}): too small in its frame"));
                    }
                }
            }
        }
    }
    bad.sort();
    bad.dedup();
    assert!(bad.is_empty(), "{} problems:\n{}", bad.len(), bad.join("\n"));
}

/// A pixel on the outer ring may carry antialiasing but no solid artwork.
const EDGE_ALPHA_MAX: u8 = 96;

fn edge_alpha(img: &RgbaImage) -> u8 {
    let mut m = 0;
    for i in 0..img.width {
        for (x, y) in [(i, 0), (i, img.height - 1), (0, i), (img.width - 1, i)] {
            m = m.max(img.pixels[((y * img.width + x) * 4 + 3) as usize]);
        }
    }
    m
}

// --- review aid -----------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// Minimal PNG writer (stored deflate blocks) so the test needs no extra dependency.
fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let mut raw = Vec::with_capacity(rgba.len() + h as usize);
    for row in rgba.chunks_exact(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65_535).peekable();
    while let Some(c) = chunks.next() {
        z.push(u8::from(chunks.peek().is_none()));
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &z);
    png_chunk(&mut out, b"IEND", &[]);
    fs::write(path, out).expect("write png");
}

#[test]
fn contact_sheet_for_review() {
    // Review aid, not a check: set BURNBURN_ICON_SHEET_DIR to write PNG contact sheets of the set.
    let Ok(dir) = std::env::var("BURNBURN_ICON_SHEET_DIR") else { return };
    fs::create_dir_all(&dir).expect("sheet dir");
    let ids: Vec<&str> = icon_ids().collect();
    let (cell, cols) = (80u32, 12u32);
    let rows = (ids.len() as u32).div_ceil(cols);
    let (w, h) = (cols * cell, rows * cell);
    for (theme, name, bg) in [(Theme::Light, "light", [246u8, 247, 249]), (Theme::Dark, "dark", [30u8, 37, 48])] {
        let mut sheet: Vec<u8> = (0..w * h).flat_map(|_| [bg[0], bg[1], bg[2], 255]).collect();
        for (i, id) in ids.iter().enumerate() {
            let img = render(id, 48, 1.0, theme).expect("render");
            let (ox, oy) = ((i as u32 % cols) * cell + 16, (i as u32 / cols) * cell + 16);
            for y in 0..img.height {
                for x in 0..img.width {
                    let s = ((y * img.width + x) * 4) as usize;
                    let d = (((oy + y) * w + ox + x) * 4) as usize;
                    let a = u32::from(img.pixels[s + 3]);
                    for c in 0..3 {
                        sheet[d + c] = ((u32::from(img.pixels[s + c]) * a + u32::from(sheet[d + c]) * (255 - a)) / 255) as u8;
                    }
                }
            }
        }
        write_png(&PathBuf::from(&dir).join(format!("sheet-{name}.png")), w, h, &sheet);
    }
}
