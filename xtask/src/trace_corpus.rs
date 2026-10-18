//! `cargo xtask gen-trace-corpus`: the synthetic trace corpus in `test-data/trace/`.
//!
//! Every image is produced from an explicit vector reference (written next to it as JSON) by a
//! small deterministic rasterizer: 4 x 4 point samples per pixel at fixed offsets, coverage →
//! 8-bit gray (ink 0 = black on paper 255 = white). Only `+ - * /` and comparisons are used (no
//! trigonometry, no `sqrt`), so the bytes are identical on every IEEE-754 platform. The PNG
//! writer is equally minimal: one IDAT with stored (uncompressed) deflate blocks.
//!
//! Expected topology (components, holes, corners, skeleton junctions) is written by hand in
//! [`cases`] *before* any tracer exists (01-test-driven-design.md § Tracer). The unit tests check
//! it independently against the raster with a connected-component count.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

pub type Pt = [f64; 2];

/// One closed boundary of a filled shape.
#[derive(Debug, Clone, PartialEq)]
pub enum Ring {
    Polygon(Vec<Pt>),
    Circle { center: Pt, r: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Even-odd fill of the rings (a ring inside another ring is a hole).
    Fill(Vec<Ring>),
    /// Round-capped, round-joined stroke: every point within `width / 2` of a segment is ink.
    Stroke { segments: Vec<[Pt; 2]>, width: f64 },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Expected {
    pub components: u32,
    pub holes: u32,
    /// Sharp corners of the outline, in pixel coordinates.
    pub corners: Vec<Pt>,
    /// Centerline graph for stroke cases: (point, degree). Degree 1 = stroke end.
    pub skeleton_nodes: Vec<(Pt, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub name: &'static str,
    pub purpose: &'static str,
    pub width: u32,
    pub height: u32,
    /// Explicit scale: one pixel edge in millimetres.
    pub pixel_size_mm: f64,
    /// Union of the shapes.
    pub shapes: Vec<Shape>,
    pub expected: Expected,
}

/// Samples per pixel along each axis.
const SS: u32 = 4;

/// The corpus. Coordinates are pixels, origin top-left, y down; pixel (i, j) covers
/// `[i, i + 1) x [j, j + 1)`.
pub fn cases() -> Vec<Case> {
    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Ring {
        Ring::Polygon(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
    }
    let stroke = |segments: Vec<[Pt; 2]>| Shape::Stroke { segments, width: 6.0 };
    vec![
        Case {
            name: "ring",
            purpose: "One component with one hole (annulus); first red test of IM12.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![Shape::Fill(vec![
                Ring::Circle { center: [32.0, 32.0], r: 24.0 },
                Ring::Circle { center: [32.0, 32.0], r: 12.0 },
            ])],
            expected: Expected { components: 1, holes: 1, ..Expected::default() },
        },
        Case {
            name: "separate-components",
            purpose: "Four disjoint shapes (square, circle, triangle, rectangle): one contour each.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![
                Shape::Fill(vec![rect(6.0, 6.0, 22.0, 22.0)]),
                Shape::Fill(vec![Ring::Circle { center: [46.0, 15.0], r: 9.0 }]),
                Shape::Fill(vec![Ring::Polygon(vec![[8.0, 58.0], [28.0, 58.0], [18.0, 36.0]])]),
                Shape::Fill(vec![rect(38.0, 42.0, 57.0, 58.0)]),
            ],
            expected: Expected { components: 4, holes: 0, ..Expected::default() },
        },
        Case {
            name: "square-sharp-corners",
            purpose: "Axis-aligned square with edges on quarter pixels: four 90 degree corners must stay sharp.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![Shape::Fill(vec![rect(12.25, 12.25, 51.75, 51.75)])],
            expected: Expected {
                components: 1,
                holes: 0,
                corners: vec![[12.25, 12.25], [51.75, 12.25], [51.75, 51.75], [12.25, 51.75]],
                ..Expected::default()
            },
        },
        Case {
            name: "letter-a",
            purpose: "Letter-like outline with a triangular counter: concave and convex corners plus a hole.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![Shape::Fill(vec![
                Ring::Polygon(vec![
                    [8.0, 58.0],
                    [26.0, 6.0],
                    [38.0, 6.0],
                    [56.0, 58.0],
                    [44.0, 58.0],
                    [39.5, 44.0],
                    [24.5, 44.0],
                    [20.0, 58.0],
                ]),
                Ring::Polygon(vec![[27.0, 34.0], [37.0, 34.0], [32.0, 18.0]]),
            ])],
            expected: Expected {
                components: 1,
                holes: 1,
                corners: vec![
                    [8.0, 58.0],
                    [26.0, 6.0],
                    [38.0, 6.0],
                    [56.0, 58.0],
                    [44.0, 58.0],
                    [39.5, 44.0],
                    [24.5, 44.0],
                    [20.0, 58.0],
                    [27.0, 34.0],
                    [37.0, 34.0],
                    [32.0, 18.0],
                ],
                ..Expected::default()
            },
        },
        Case {
            name: "stroke-t",
            purpose: "Centerline (later): T junction, one degree-3 node and three ends.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![stroke(vec![
                [[10.0, 12.0], [32.0, 12.0]],
                [[32.0, 12.0], [54.0, 12.0]],
                [[32.0, 12.0], [32.0, 56.0]],
            ])],
            expected: Expected {
                components: 1,
                holes: 0,
                skeleton_nodes: vec![([32.0, 12.0], 3), ([10.0, 12.0], 1), ([54.0, 12.0], 1), ([32.0, 56.0], 1)],
                ..Expected::default()
            },
        },
        Case {
            name: "stroke-x",
            purpose: "Centerline (later): X crossing, one degree-4 node and four ends.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![stroke(vec![
                [[10.0, 10.0], [32.0, 32.0]],
                [[32.0, 32.0], [54.0, 54.0]],
                [[54.0, 10.0], [32.0, 32.0]],
                [[32.0, 32.0], [10.0, 54.0]],
            ])],
            expected: Expected {
                components: 1,
                holes: 0,
                skeleton_nodes: vec![
                    ([32.0, 32.0], 4),
                    ([10.0, 10.0], 1),
                    ([54.0, 54.0], 1),
                    ([54.0, 10.0], 1),
                    ([10.0, 54.0], 1),
                ],
                ..Expected::default()
            },
        },
        Case {
            name: "stroke-y",
            purpose: "Centerline (later): Y fork with acute branch angle, one degree-3 node and three ends.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![stroke(vec![
                [[32.0, 58.0], [32.0, 32.0]],
                [[32.0, 32.0], [12.0, 8.0]],
                [[32.0, 32.0], [52.0, 8.0]],
            ])],
            expected: Expected {
                components: 1,
                holes: 0,
                skeleton_nodes: vec![([32.0, 32.0], 3), ([32.0, 58.0], 1), ([12.0, 8.0], 1), ([52.0, 8.0], 1)],
                ..Expected::default()
            },
        },
        Case {
            name: "strokes-near-touching",
            purpose: "Two parallel strokes with a 2 px gap: must stay two components / two centerlines.",
            width: 64,
            height: 64,
            pixel_size_mm: 0.1,
            shapes: vec![stroke(vec![[[24.0, 8.0], [24.0, 56.0]]]), stroke(vec![[[32.0, 8.0], [32.0, 56.0]]])],
            expected: Expected {
                components: 2,
                holes: 0,
                skeleton_nodes: vec![([24.0, 8.0], 1), ([24.0, 56.0], 1), ([32.0, 8.0], 1), ([32.0, 56.0], 1)],
                ..Expected::default()
            },
        },
    ]
}

/// Holdout split (01-test-driven-design.md § Tracer: "manifest, bronbeelden, referenties, holdout"):
/// cases that tracer development and tuning must not look at. They are written to
/// `test-data/trace/holdout/` and listed under `holdout` in the manifest, never under `cases`, so
/// a corpus test that iterates `cases` cannot tune against them. They differ from the dev cases
/// in scale and placement (off-grid centres, other mm/px) so that a tracer overfitted to the dev
/// cases shows it. Judged only by the release evaluation (`burnburn-scenarios`, ignored test).
pub fn holdout_cases() -> Vec<Case> {
    vec![
        Case {
            name: "plate-two-holes",
            purpose: "Holdout: off-grid rectangle with two unequal circular holes at 0.25 mm/px (coarser than every dev case).",
            width: 64,
            height: 48,
            pixel_size_mm: 0.25,
            shapes: vec![Shape::Fill(vec![
                Ring::Polygon(vec![[6.5, 8.25], [57.75, 8.25], [57.75, 40.5], [6.5, 40.5]]),
                Ring::Circle { center: [20.3, 24.1], r: 6.2 },
                Ring::Circle { center: [43.7, 24.6], r: 7.1 },
            ])],
            expected: Expected {
                components: 1,
                holes: 2,
                corners: vec![[6.5, 8.25], [57.75, 8.25], [57.75, 40.5], [6.5, 40.5]],
                ..Expected::default()
            },
        },
        Case {
            name: "ring-offcentre-fine",
            purpose: "Holdout: annulus with an off-grid centre at 0.05 mm/px (finer than every dev case).",
            width: 64,
            height: 64,
            pixel_size_mm: 0.05,
            shapes: vec![Shape::Fill(vec![
                Ring::Circle { center: [29.37, 34.81], r: 22.6 },
                Ring::Circle { center: [29.37, 34.81], r: 9.85 },
            ])],
            expected: Expected { components: 1, holes: 1, ..Expected::default() },
        },
    ]
}

/// Even-odd parity of `p` against one ring (half-open crossing rule, so shared edges of
/// adjacent pixels never count twice).
fn ring_crossings(ring: &Ring, p: Pt) -> bool {
    match ring {
        Ring::Circle { center, r } => {
            let (dx, dy) = (p[0] - center[0], p[1] - center[1]);
            dx * dx + dy * dy < r * r
        }
        Ring::Polygon(points) => {
            let mut inside = false;
            let n = points.len();
            for i in 0..n {
                let (a, b) = (points[i], points[(i + 1) % n]);
                if (a[1] > p[1]) != (b[1] > p[1]) {
                    let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                    if p[0] < x {
                        inside = !inside;
                    }
                }
            }
            inside
        }
    }
}

fn distance_sq_to_segment(p: Pt, [a, b]: [Pt; 2]) -> f64 {
    let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
    let (vx, vy) = (p[0] - a[0], p[1] - a[1]);
    let len_sq = ux * ux + uy * uy;
    let t = if len_sq > 0.0 { ((vx * ux + vy * uy) / len_sq).clamp(0.0, 1.0) } else { 0.0 };
    let (dx, dy) = (vx - t * ux, vy - t * uy);
    dx * dx + dy * dy
}

fn shape_contains(shape: &Shape, p: Pt) -> bool {
    match shape {
        Shape::Fill(rings) => rings.iter().filter(|r| ring_crossings(r, p)).count() % 2 == 1,
        Shape::Stroke { segments, width } => {
            let half = width / 2.0;
            segments.iter().any(|s| distance_sq_to_segment(p, *s) <= half * half)
        }
    }
}

/// 8-bit gray pixels, row-major, ink 0 and paper 255.
pub fn rasterize(case: &Case) -> Vec<u8> {
    let mut out = Vec::with_capacity((case.width * case.height) as usize);
    for j in 0..case.height {
        for i in 0..case.width {
            let mut covered = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = [
                        f64::from(i) + (f64::from(sx) + 0.5) / f64::from(SS),
                        f64::from(j) + (f64::from(sy) + 0.5) / f64::from(SS),
                    ];
                    if case.shapes.iter().any(|s| shape_contains(s, p)) {
                        covered += 1;
                    }
                }
            }
            let samples = SS * SS;
            let ink = (255 * covered + samples / 2) / samples;
            out.push(u8::try_from(255 - ink).unwrap_or(0));
        }
    }
    out
}

/// CRC-32 (ISO-HDLC, reflected polynomial 0xEDB88320) as used by PNG chunks.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

pub fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// PNG, 8-bit grayscale, no interlace, filter 0 on every row, stored deflate blocks.
pub fn encode_png_gray8(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(pixels.len() + height as usize);
    for row in pixels.chunks(width.max(1) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (k, block) in blocks.iter().enumerate() {
        z.push(u8::from(k + 1 == blocks.len()));
        let len = u16::try_from(block.len()).unwrap_or(u16::MAX);
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn pt(p: Pt) -> Value {
    json!(p)
}

/// The vector reference written as `<name>.json`.
pub fn reference_json(case: &Case) -> Value {
    let shapes: Vec<Value> = case
        .shapes
        .iter()
        .map(|s| match s {
            Shape::Fill(rings) => json!({
                "kind": "fill",
                "fill_rule": "evenodd",
                "rings": rings.iter().map(|r| match r {
                    Ring::Polygon(points) => json!({ "polygon": points.iter().map(|p| pt(*p)).collect::<Vec<_>>() }),
                    Ring::Circle { center, r } => json!({ "circle": { "center": pt(*center), "r": r } }),
                }).collect::<Vec<_>>(),
            }),
            Shape::Stroke { segments, width } => json!({
                "kind": "stroke",
                "cap": "round",
                "join": "round",
                "width": width,
                "segments": segments.iter().map(|s| json!([pt(s[0]), pt(s[1])])).collect::<Vec<_>>(),
            }),
        })
        .collect();
    json!({
        "schema": 1,
        "name": case.name,
        "units": "px",
        "coordinate_system": "origin top-left, y down; pixel (i, j) covers [i, i+1) x [j, j+1)",
        "width": case.width,
        "height": case.height,
        "pixel_size_mm": case.pixel_size_mm,
        "union_of": shapes,
        "rasterization": "4x4 point samples per pixel at offsets (k + 0.5) / 4; gray = 255 - round(255 * covered / 16); ink 0 on paper 255",
    })
}

fn expected_json(e: &Expected) -> Value {
    let mut v = json!({ "components": e.components, "holes": e.holes });
    if !e.corners.is_empty() {
        v["corners_px"] = json!(e.corners);
    }
    if !e.skeleton_nodes.is_empty() {
        v["skeleton_nodes_px"] =
            json!(e.skeleton_nodes.iter().map(|(p, d)| json!({ "at": pt(*p), "degree": d })).collect::<Vec<_>>());
    }
    v
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn pretty(v: &Value) -> Result<Vec<u8>, String> {
    let mut out = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    out.push(b'\n');
    Ok(out)
}

/// Every generated file (relative path → bytes), including `manifest.json`.
pub fn generate() -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let mut entries = Vec::new();
    let mut manifest_cases = Vec::new();
    let mut holdout_entries = Vec::new();
    let splits =
        cases().into_iter().map(|c| ("synthetic", c)).chain(holdout_cases().into_iter().map(|c| ("holdout", c)));
    for (dir, case) in splits {
        let png = encode_png_gray8(case.width, case.height, &rasterize(&case));
        let reference = pretty(&reference_json(&case))?;
        let image_path = format!("{dir}/{}.png", case.name);
        let reference_path = format!("{dir}/{}.json", case.name);
        for (path, role, bytes) in [(&image_path, "image", &png), (&reference_path, "reference", &reference)] {
            entries.push(json!({ "path": path, "role": role, "bytes": bytes.len(), "sha256": sha256_hex(bytes) }));
        }
        let entry = json!({
            "name": case.name,
            "purpose": case.purpose,
            "image": image_path,
            "reference": reference_path,
            "pixel_size_mm": case.pixel_size_mm,
            "expected": expected_json(&case.expected),
        });
        if dir == "holdout" {
            holdout_entries.push(entry)
        } else {
            manifest_cases.push(entry)
        }
        files.insert(image_path, png);
        files.insert(reference_path, reference);
    }
    let manifest = json!({
        "schema": 1,
        "description": "Synthetic trace corpus. Each PNG is rasterized from the vector reference next to it by `cargo xtask gen-trace-corpus` (xtask/src/trace_corpus.rs); sizes and SHA-256 are checked by burnburn-test-support (fixtures_match_manifests) and the bytes by xtask (committed_corpus_matches_generator).",
        "generator": "cargo xtask gen-trace-corpus",
        "golden_policy": "Never regenerate to make a test pass. A changed image needs a reviewed explanation of the rasterizer or case change (AGENTS.md). Expected topology was written before any tracer existed.",
        "judging": "Judge traces against the vector reference (max/bidirectional distance, holes, graph structure), not pixel overlap (01-test-driven-design.md § Tracer).",
        "files": entries,
        "cases": manifest_cases,
        "holdout_policy": "Cases under `holdout` (files in holdout/) are not for tracer development or tuning: do not iterate them in crate tests or look at their traces while changing the tracer. Only the release evaluation reads them (crates/burnburn-scenarios/tests/trace_holdout.rs). Report holdout results, never adjust them.",
        "holdout": holdout_entries,
    });
    files.insert("manifest.json".to_owned(), pretty(&manifest)?);
    Ok(files)
}

/// Writes the corpus into `dir` (normally `test-data/trace/`).
pub fn write(dir: &Path) -> Result<usize, String> {
    let files = generate()?;
    for (relative, bytes) in &files {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    fn case(width: u32, height: u32, shapes: Vec<Shape>) -> Case {
        Case { name: "t", purpose: "", width, height, pixel_size_mm: 0.1, shapes, expected: Expected::default() }
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
        Shape::Fill(vec![Ring::Polygon(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]])])
    }

    #[test]
    fn crc32_and_adler32_match_published_check_values() {
        // CRC-32/ISO-HDLC check value (used by PNG) and the Adler-32 example from RFC 1950 literature.
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn png_has_signature_header_and_trailer() {
        let png = encode_png_gray8(3, 2, &[0, 128, 255, 1, 2, 3]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        // IHDR: length 13, type, width 3, height 2, bit depth 8, colour type 0 (gray), 0, 0, 0.
        assert_eq!(&png[8..16], b"\x00\x00\x00\x0dIHDR");
        assert_eq!(&png[16..29], &[0, 0, 0, 3, 0, 0, 0, 2, 8, 0, 0, 0, 0]);
        assert_eq!(u32::from_be_bytes(png[29..33].try_into().unwrap()), crc32(&png[12..29]));
        assert_eq!(&png[png.len() - 12..], b"\x00\x00\x00\x00IEND\xae\x42\x60\x82");
    }

    #[test]
    fn png_idat_holds_filtered_rows_in_stored_zlib() {
        let png = encode_png_gray8(3, 2, &[0, 128, 255, 1, 2, 3]);
        let idat_len = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
        assert_eq!(&png[37..41], b"IDAT");
        let z = &png[41..41 + idat_len];
        assert_eq!(&z[..2], &[0x78, 0x01], "zlib header, no dictionary, fastest");
        // One final stored block: BFINAL=1/BTYPE=00, LEN=8, NLEN=!8.
        assert_eq!(&z[2..7], &[0x01, 8, 0, 0xf7, 0xff]);
        let raw = [0u8, 0, 128, 255, 0, 1, 2, 3];
        assert_eq!(&z[7..15], &raw);
        assert_eq!(&z[15..], &adler32(&raw).to_be_bytes());
    }

    #[test]
    fn pixel_aligned_square_is_solid_inside_and_paper_outside() {
        let px = rasterize(&case(8, 8, vec![square(2.0, 2.0, 6.0, 6.0)]));
        for y in 0..8 {
            for x in 0..8 {
                let inside = (2..6).contains(&x) && (2..6).contains(&y);
                assert_eq!(px[y * 8 + x], if inside { 0 } else { 255 }, "pixel ({x}, {y})");
            }
        }
    }

    #[test]
    fn half_and_quarter_pixel_edges_give_partial_coverage() {
        let px = rasterize(&case(8, 8, vec![square(2.5, 2.0, 6.0, 5.25)]));
        // Column 2 is half covered: 8 of 16 samples → 255 - round(127.5) = 127 (round half up gives 128 ink).
        assert_eq!(px[3 * 8 + 2], 127);
        // Row 5 is a quarter covered: 4 of 16 samples → 255 - round(63.75) = 191.
        assert_eq!(px[5 * 8 + 4], 191);
        // Corner pixel (2, 5): 2 of 16 samples → 255 - round(31.875) = 223.
        assert_eq!(px[5 * 8 + 2], 223);
    }

    fn ink(px: &[u8]) -> f64 {
        px.iter().map(|&v| f64::from(255 - v) / 255.0).sum()
    }

    #[test]
    fn circle_and_ring_ink_approximate_their_area() {
        let disc = rasterize(&case(64, 64, vec![Shape::Fill(vec![Ring::Circle { center: [32.0, 32.0], r: 20.0 }])]));
        let area = std::f64::consts::PI * 400.0;
        assert!((ink(&disc) - area).abs() / area < 0.005, "disc ink {} vs {area}", ink(&disc));
        let ring = rasterize(&case(
            64,
            64,
            vec![Shape::Fill(vec![
                Ring::Circle { center: [32.0, 32.0], r: 24.0 },
                Ring::Circle { center: [32.0, 32.0], r: 12.0 },
            ])],
        ));
        let area = std::f64::consts::PI * (576.0 - 144.0);
        assert!((ink(&ring) - area).abs() / area < 0.005, "ring ink {} vs {area}", ink(&ring));
        assert_eq!(ring[32 * 64 + 32], 255, "even-odd: the centre is a hole");
    }

    #[test]
    fn stroke_covers_exactly_the_points_within_half_width() {
        let px =
            rasterize(&case(64, 20, vec![Shape::Stroke { segments: vec![[[5.0, 10.0], [55.0, 10.0]]], width: 4.0 }]));
        // Samples of rows 8..=11 lie within 2 px of y = 10; rows 7 and 12 are just outside.
        for y in 8..12 {
            assert_eq!(px[y * 64 + 30], 0, "row {y}");
        }
        assert_eq!(px[7 * 64 + 30], 255);
        assert_eq!(px[12 * 64 + 30], 255);
        // Round cap: (3, 10) is 2 px left of the start point, so its samples straddle the cap.
        assert!(px[10 * 64 + 3] > 0 && px[10 * 64 + 3] < 255);
        assert_eq!(px[10 * 64 + 2], 255);
    }

    /// Ink = gray < 128. Components use 8-connectivity, holes are 4-connected paper regions that
    /// do not touch the border (the usual dual pair, so a diagonal ink gap closes a hole).
    fn topology(width: usize, height: usize, px: &[u8]) -> (u32, u32) {
        let count = |is_ink: bool, neighbours: &[(i64, i64)]| {
            let mut seen = vec![false; px.len()];
            let mut regions = 0;
            let mut enclosed = 0;
            for start in 0..px.len() {
                if seen[start] || (px[start] < 128) != is_ink {
                    continue;
                }
                regions += 1;
                let mut touches_border = false;
                let mut stack = vec![start];
                seen[start] = true;
                while let Some(i) = stack.pop() {
                    let (x, y) = ((i % width) as i64, (i / width) as i64);
                    if x == 0 || y == 0 || x == width as i64 - 1 || y == height as i64 - 1 {
                        touches_border = true;
                    }
                    for (dx, dy) in neighbours {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                            continue;
                        }
                        let j = ny as usize * width + nx as usize;
                        if !seen[j] && (px[j] < 128) == is_ink {
                            seen[j] = true;
                            stack.push(j);
                        }
                    }
                }
                if !touches_border {
                    enclosed += 1;
                }
            }
            (regions, enclosed)
        };
        let eight = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
        let four = [(0, -1), (-1, 0), (1, 0), (0, 1)];
        (count(true, &eight).0, count(false, &four).1)
    }

    #[test]
    fn every_case_raster_has_the_hand_written_topology() {
        for c in cases() {
            let px = rasterize(&c);
            assert_eq!(px.len(), (c.width * c.height) as usize);
            let (components, holes) = topology(c.width as usize, c.height as usize, &px);
            assert_eq!((components, holes), (c.expected.components, c.expected.holes), "case {}", c.name);
        }
    }

    #[test]
    fn holdout_is_listed_separately_and_disjoint_from_dev_cases() {
        let holdout = holdout_cases();
        assert!(holdout.len() >= 2, "holdout split has {} cases", holdout.len());
        let dev: Vec<_> = cases().iter().map(|c| c.name).collect();
        for c in &holdout {
            assert!(!dev.contains(&c.name), "{} is in both splits", c.name);
            assert!(cases().iter().all(|d| d.pixel_size_mm != c.pixel_size_mm || d.shapes != c.shapes));
            let px = rasterize(c);
            assert_eq!(
                topology(c.width as usize, c.height as usize, &px),
                (c.expected.components, c.expected.holes),
                "case {}",
                c.name
            );
        }
        let files = generate().unwrap();
        let manifest: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
        let listed: Vec<&str> =
            manifest["holdout"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
        assert_eq!(listed, holdout.iter().map(|c| c.name).collect::<Vec<_>>());
        let dev_listed = manifest["cases"].as_array().unwrap();
        assert!(dev_listed.iter().all(|c| !c["image"].as_str().unwrap().starts_with("holdout/")));
        for c in manifest["holdout"].as_array().unwrap() {
            assert!(
                c["image"].as_str().unwrap().starts_with("holdout/")
                    && files.contains_key(c["image"].as_str().unwrap())
            );
        }
    }

    #[test]
    fn rasterization_is_deterministic_and_names_are_unique() {
        let mut names: Vec<_> = cases().iter().map(|c| c.name).collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n);
        assert_eq!(generate().unwrap(), generate().unwrap());
    }

    /// The committed `test-data/trace/` is exactly what the generator produces. A failure means
    /// the generator or a case changed: review the images before running gen-trace-corpus.
    #[test]
    fn committed_corpus_matches_generator() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-data/trace");
        for (relative, bytes) in generate().unwrap() {
            let on_disk = std::fs::read(dir.join(&relative)).unwrap_or_else(|e| panic!("{relative}: {e}"));
            assert!(on_disk == bytes, "{relative} differs from the generator output");
        }
    }
}
