//! LF10 (look and feel, canvas): canvas text is glyph outlines from a bundled font, not a stroke
//! toy font, and carries a halo in the background colour so that it stays legible on top of lines.
//! Pixel tests render offscreen on the real GPU (SKIPPED without an adapter, never a pass;
//! `BURNBURN_REQUIRE_GPU=1` makes that a failure).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_editor::{Camera, Overlay, SceneSnapshot, SketchMark, Viewport};
use burnburn_geometry::Point;
use burnburn_render::gpu::{GpuContext, GpuError, Image, render_offscreen};
use burnburn_render::{Rgba, Theme, build_packet};
use burnburn_types::{DocumentId, RevisionId, StableId};

fn context() -> Option<GpuContext> {
    match GpuContext::headless() {
        Ok(ctx) => Some(ctx),
        Err(GpuError::NoAdapter(e)) if std::env::var_os("BURNBURN_REQUIRE_GPU").is_none() => {
            eprintln!("SKIPPED (not a pass): no GPU adapter on this machine: {e}");
            None
        }
        Err(e) => panic!("{e}"),
    }
}

fn themes() -> [(&'static str, Theme); 2] {
    [("light", Theme::light()), ("dark", Theme::dark())]
}

/// 4 device px per logical pt, so the 10 pt label is 40 px high; world (0, 0) in the middle of
/// a 200 x 60 pt view.
const SCALE: f64 = 4.0;
const WIDTH: u32 = 800;
const HEIGHT: u32 = 240;

fn dimension(text: &str, lines: Vec<[Point; 2]>) -> SceneSnapshot {
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(0.0, 0.0), 4.0).unwrap(),
        viewport: Viewport::new(200.0, 60.0, SCALE).unwrap(),
        items: vec![],
        overlays: vec![Overlay::SketchDimension {
            lines,
            label_at: Point::new(0.0, 0.0),
            text: text.to_owned(),
            driven: false,
            mark: SketchMark::Normal,
        }],
    }
}

fn render(ctx: &GpuContext, theme: &Theme, scene: &SceneSnapshot) -> Image {
    let image = render_offscreen(ctx, &build_packet(scene, theme)).unwrap();
    assert_eq!((image.width, image.height), (WIDTH, HEIGHT));
    image
}

fn near(a: [u8; 4], c: Rgba, tol: f32) -> bool {
    let ch = |v: u8| f32::from(v) / 255.0;
    (ch(a[0]) - c.r).abs() <= tol && (ch(a[1]) - c.g).abs() <= tol && (ch(a[2]) - c.b).abs() <= tol
}

/// Bounding boxes (x0, y0, x1, y1; inclusive) of the 8-connected groups of pixels that differ from
/// the background, left to right.
fn components(image: &Image, background: Rgba) -> Vec<[u32; 4]> {
    let (w, h) = (image.width as usize, image.height as usize);
    let ink: Vec<bool> = (0..w * h)
        .map(|i| {
            let p = image.pixel((i % w) as u32, (i / w) as u32).unwrap();
            !near(p, background, 0.25)
        })
        .collect();
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    for start in 0..w * h {
        if !ink[start] || seen[start] {
            continue;
        }
        let mut bbox = [u32::MAX, u32::MAX, 0, 0];
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            bbox = [bbox[0].min(x as u32), bbox[1].min(y as u32), bbox[2].max(x as u32), bbox[3].max(y as u32)];
            for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if ink[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        out.push(bbox);
    }
    out.sort_by_key(|b| b[0]);
    out
}

fn height(b: &[u32; 4]) -> u32 {
    b[3] - b[1] + 1
}

fn width(b: &[u32; 4]) -> u32 {
    b[2] - b[0] + 1
}

/// "R12,5 mm" is seven separate shapes (R 1 2 , 5 m m) drawn from a real font: the lowercase m is
/// lower than the digits and wider than a digit, and the comma hangs below the baseline. A
/// replacement block for a glyph the font lacks would be as tall as a digit.
#[test]
fn r12_5_mm_renders_as_real_glyphs_without_replacement_blocks() {
    let Some(ctx) = context() else { return };
    for (name, theme) in themes() {
        let image = render(&ctx, &theme, &dimension("R12,5 mm", vec![]));
        let parts = components(&image, theme.background);
        assert_eq!(parts.len(), 7, "{name}: R 1 2 , 5 m m expected, got {parts:?}");
        let (r, one, five, comma, m1, m2) = (parts[0], parts[1], parts[4], parts[3], parts[5], parts[6]);
        for m in [m1, m2] {
            assert!(height(&m) * 100 < height(&five) * 85, "{name}: m ({}) not lower than the digit 5 ({})", height(&m), height(&five));
            assert!(width(&m) > width(&five), "{name}: m is wider than a digit");
        }
        assert!(height(&one) >= height(&r) * 9 / 10, "{name}: 1 and R are both cap height");
        assert!(comma[3] > one[3], "{name}: the comma hangs below the baseline");
        assert!(height(&comma) * 100 < height(&five) * 40, "{name}: the comma is small");
        // Inked pixels stay a small part of the label's box: text, not a block.
        let (x0, x1) = (parts[0][0], parts[6][2]);
        let (y0, y1) = (parts.iter().map(|p| p[1]).min().unwrap(), parts.iter().map(|p| p[3]).max().unwrap());
        let inked = (y0..=y1)
            .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
            .filter(|&(x, y)| !near(image.pixel(x, y).unwrap(), theme.background, 0.25))
            .count();
        let area = ((x1 - x0 + 1) * (y1 - y0 + 1)) as usize;
        assert!(inked * 100 < area * 45, "{name}: {inked} of {area} px inked");
    }
}

/// The halo: next to the glyphs, where a line runs underneath, the pixels have the background
/// colour (the line is cut away around the text), not the line colour.
#[test]
fn the_halo_has_the_background_colour_where_a_line_crosses_the_text() {
    let Some(ctx) = context() else { return };
    for (name, theme) in themes() {
        let line = vec![[Point::new(-45.0, 0.0), Point::new(45.0, 0.0)]];
        let bare = render(&ctx, &theme, &dimension("", line.clone()));
        let with_text = render(&ctx, &theme, &dimension("R12,5 mm", line));
        let line_colour = theme.sketch_dimension;
        let glyphs = components(&render(&ctx, &theme, &dimension("R12,5 mm", vec![])), theme.background);
        let (x0, x1) = (glyphs[0][0], glyphs[glyphs.len() - 1][2]);
        let row = HEIGHT / 2;
        // Pixels on the line, inside the text's extent, that the text turned into plain background.
        let cut = (x0..=x1)
            .filter(|&x| near(bare.pixel(x, row).unwrap(), line_colour, 0.05) && near(with_text.pixel(x, row).unwrap(), theme.background, 0.02))
            .count();
        assert!(cut >= 8, "{name}: only {cut} halo pixels on the line row");
        // Far from the text the line is untouched.
        assert!(near(with_text.pixel(WIDTH / 2 + 170, row).unwrap(), line_colour, 0.05) || near(with_text.pixel(WIDTH / 2 - 170, row).unwrap(), line_colour, 0.05), "{name}: the line outside the label");
    }
}

/// A label sits on the bed in practice: the halo then has the bed colour, not the canvas colour.
#[test]
fn on_the_bed_the_halo_takes_the_bed_colour() {
    let Some(ctx) = context() else { return };
    for (name, theme) in themes() {
        let mut scene = dimension("R12,5 mm", vec![[Point::new(-45.0, 0.0), Point::new(45.0, 0.0)]]);
        scene.overlays.insert(0, Overlay::Bed { width_mm: 100.0, height_mm: 40.0, origin: burnburn_editor::BedOrigin::BottomLeft });
        scene.camera = Camera::new(Point::new(50.0, 20.0), 4.0).unwrap();
        // The label and its line are at the world origin: shift them onto the bed's middle.
        for o in &mut scene.overlays {
            if let Overlay::SketchDimension { lines, label_at, .. } = o {
                *label_at = Point::new(50.0, 20.0);
                *lines = vec![[Point::new(5.0, 20.0), Point::new(95.0, 20.0)]];
            }
        }
        let image = render(&ctx, &theme, &scene);
        let row = HEIGHT / 2;
        let halo = (300..500).filter(|&x| near(image.pixel(x, row).unwrap(), theme.bed_fill, 0.02)).count();
        let canvas_coloured = (300..500).filter(|&x| near(image.pixel(x, row).unwrap(), theme.background, 0.02)).count();
        assert!(halo >= 8, "{name}: only {halo} bed-coloured pixels on the line row");
        assert_eq!(canvas_coloured, 0, "{name}: canvas-coloured halo on the bed");
    }
}

/// The bundled font draws the label vocabulary; a character it lacks is reported (and left out of
/// the picture), never replaced by a block.
#[test]
fn the_canvas_font_covers_the_labels_and_reports_what_it_lacks() {
    for text in ["R12,5 mm", "\u{D8} 30,0 mm", "45,0\u{B0}", "Eindpunt", "Midden", "Raster", "-12.5 in", "(8,0 mm)"] {
        assert_eq!(burnburn_render::canvas_text_missing_chars(text), Vec::<char>::new(), "{text}");
    }
    assert_eq!(burnburn_render::canvas_text_missing_chars("R5\u{4E2D}"), vec!['\u{4E2D}']);
}
