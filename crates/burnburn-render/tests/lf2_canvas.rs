//! LF2 (look and feel, canvas): anti-aliasing, the dark canvas theme, the bed with its origin and a
//! two-level grid, and no grey fill in closed shapes. Pixel tests render offscreen on the real GPU
//! (SKIPPED without an adapter, never a pass; `BURNBURN_REQUIRE_GPU=1` makes it a failure); the
//! rest is headless on the render packet.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_editor::{BedOrigin, Camera, Overlay, SceneItem, SceneSnapshot, StyleRole, Viewport};
use burnburn_geometry::{Affine, Path, Point, Rect};
use burnburn_render::gpu::{GpuContext, GpuError, Image, render_offscreen};
use burnburn_render::{Primitive, RenderPacket, Rgba, Theme, build_packet, contrast_ratio, delta_e};
use burnburn_types::{DocumentId, GeometryId, Length, RevisionId, StableId};

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

fn luma(p: [u8; 4]) -> f64 {
    0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
}

/// `top` laid over opaque `under` (straight alpha), as the canvas blends.
fn over(top: Rgba, under: Rgba) -> Rgba {
    let mix = |t: f32, u: f32| t * top.a + u * (1.0 - top.a);
    Rgba::new(mix(top.r, under.r), mix(top.g, under.g), mix(top.b, under.b), 1.0)
}

fn pixels_where(image: &Image, f: impl Fn([u8; 4]) -> bool) -> usize {
    (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| f(image.pixel(x, y).unwrap()))
        .count()
}

// ---- (a) anti-aliasing -------------------------------------------------------------------

/// A slanted black line on white must have edge pixels of intermediate tone (no staircase). Without
/// anti-aliasing every pixel is either fully ink or fully paper.
#[test]
fn slanted_line_has_intermediate_tones_on_its_edge() {
    let Some(ctx) = context() else { return };
    let packet = RenderPacket {
        size_px: [64, 48],
        clear: Rgba::new(1.0, 1.0, 1.0, 1.0),
        primitives: vec![Primitive::Stroke {
            points_px: vec![[4.0, 10.0], [60.0, 30.0]],
            closed: false,
            width_px: 1.5,
            color: Rgba::new(0.0, 0.0, 0.0, 1.0),
        }],
    };
    let image = render_offscreen(&ctx, &packet).unwrap();
    let mid = pixels_where(&image, |p| (40.0..215.0).contains(&luma(p)));
    let ink = pixels_where(&image, |p| luma(p) < 40.0);
    // A 56 x 20 px diagonal crosses ~60 pixel columns, with two edges: well over 3 partial pixels.
    assert!(mid >= 40, "only {mid} intermediate-tone pixels: the edge is a staircase");
    assert!(ink + mid > 60, "the line must still be drawn ({ink} solid, {mid} partial)");
}

/// Edges that lie on pixel boundaries stay exact under multisampling (no blur of crisp rectangles).
#[test]
fn pixel_aligned_rectangle_stays_crisp_with_multisampling() {
    let Some(ctx) = context() else { return };
    let packet = RenderPacket {
        size_px: [32, 32],
        clear: Rgba::new(1.0, 1.0, 1.0, 1.0),
        primitives: vec![Primitive::Fill {
            contours_px: vec![vec![[8.0, 8.0], [24.0, 8.0], [24.0, 24.0], [8.0, 24.0]]],
            color: Rgba::new(0.0, 0.0, 0.0, 1.0),
        }],
    };
    let image = render_offscreen(&ctx, &packet).unwrap();
    assert_eq!(pixels_where(&image, |p| luma(p) < 1.0), 16 * 16);
    assert_eq!(pixels_where(&image, |p| luma(p) > 254.0), 32 * 32 - 16 * 16);
}

// ---- (b) dark theme and contrast ----------------------------------------------------------

#[test]
fn contrast_and_delta_e_follow_their_definitions() {
    let (black, white) = (Rgba::new(0.0, 0.0, 0.0, 1.0), Rgba::new(1.0, 1.0, 1.0, 1.0));
    assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-6, "WCAG black on white is 21:1");
    assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
    // sRGB (119,119,119) against white is the well-known 4.48:1.
    let grey = Rgba::new(119.0 / 255.0, 119.0 / 255.0, 119.0 / 255.0, 1.0);
    assert!((contrast_ratio(grey, white) - 4.48).abs() < 0.02, "{}", contrast_ratio(grey, white));
    // CIE76: black to white is 100 (L* 0 to 100).
    assert!((delta_e(black, white) - 100.0).abs() < 0.5, "{}", delta_e(black, white));
    assert!(delta_e(white, white) < 1e-9);
}

#[test]
fn dark_theme_is_dark_and_its_ink_has_high_contrast() {
    let dark = Theme::dark();
    let l = |c: Rgba| 0.2126 * f64::from(c.r) + 0.7152 * f64::from(c.g) + 0.0722 * f64::from(c.b);
    assert!(l(dark.background) < 0.1, "dark canvas background luminance {}", l(dark.background));
    assert!(l(dark.bed_fill) < 0.2, "the bed is dark too");
    let ink = dark.design.stroke.unwrap();
    assert!(contrast_ratio(ink, dark.bed_fill) >= 7.0, "ink {:.2}:1", contrast_ratio(ink, dark.bed_fill));
    assert_ne!(Theme::light(), dark);
}

#[test]
fn lines_marks_and_layer_colours_are_readable_against_the_bed_in_both_themes() {
    for (name, t) in themes() {
        let bed = t.bed_fill;
        let ink = t.design.stroke.unwrap();
        assert!(contrast_ratio(ink, bed) >= 7.0, "{name}: ink {:.2}", contrast_ratio(ink, bed));
        for (what, c) in [("selection", t.selection), ("hover", t.hovered.stroke.unwrap()), ("origin", t.origin)] {
            assert!(contrast_ratio(c, bed) >= 3.0, "{name}: {what} {:.2}:1", contrast_ratio(c, bed));
        }
        assert!(contrast_ratio(over(t.preview, bed), bed) >= 3.0, "{name}: preview");
        assert!(contrast_ratio(t.bed_border, t.background) >= 1.2, "{name}: bed border against canvas");
        // Grid lines are quiet but visible: majors above minors, both distinct from the bed.
        let minor = t.grid.unwrap();
        assert!(contrast_ratio(minor, bed) >= 1.1, "{name}: minor grid");
        assert!(contrast_ratio(t.grid_major, bed) > contrast_ratio(minor, bed), "{name}: major above minor");
    }
    let dark = Theme::dark();
    for (i, c) in dark.process_layers.iter().enumerate() {
        assert!(
            contrast_ratio(*c, dark.bed_fill) >= 3.0,
            "dark layer slot {i}: {:.2}:1",
            contrast_ratio(*c, dark.bed_fill)
        );
    }
}

// ---- (d) selection, hover and preview are told apart --------------------------------------

#[test]
fn selection_hover_and_preview_differ_by_at_least_20_delta_e() {
    for (name, t) in themes() {
        let sel = t.selection;
        let hov = t.hovered.stroke.unwrap();
        let pre = over(t.preview, t.bed_fill);
        for (an, a, bn, b) in
            [("selection", sel, "hover", hov), ("selection", sel, "preview", pre), ("hover", hov, "preview", pre)]
        {
            assert!(delta_e(a, b) >= 20.0, "{name}: {an} vs {bn} only {:.1}", delta_e(a, b));
        }
    }
}

// ---- (c) bed, origin and grid -------------------------------------------------------------

const PPM: f64 = 4.0;

/// A 100 x 60 mm bed at 4 px/mm (400 x 240 px) centred in a 600 x 400 px view.
fn bed_scene(origin: BedOrigin, items: Vec<(Arc<Path>, StyleRole)>) -> SceneSnapshot {
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(50.0, 30.0), PPM).unwrap(),
        viewport: Viewport::new(600.0, 400.0, 1.0).unwrap(),
        items: items
            .into_iter()
            .enumerate()
            .map(|(i, (path, role))| SceneItem {
                source: GeometryId::from_raw(i as u64 + 1),
                path,
                transform: Affine::IDENTITY,
                role,
                fill_slot: None,
            })
            .collect(),
        overlays: vec![
            Overlay::Bed { width_mm: 100.0, height_mm: 60.0, origin },
            Overlay::Grid { spacing_mm: 5.0, major_every: 2 },
        ],
    }
}

/// The bed's pixel rectangle: x 100..500, y 80..320 (world y up, screen y down).
const BED_PX: [f32; 4] = [100.0, 80.0, 500.0, 320.0];

fn grid_strokes(p: &RenderPacket, t: &Theme) -> Vec<(Vec<[f32; 2]>, Rgba)> {
    p.primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Stroke { points_px, closed: false, color, .. }
                if *color == t.grid.unwrap() || *color == t.grid_major =>
            {
                Some((points_px.clone(), *color))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn grid_lines_stay_inside_the_bed_with_majors_apart_from_minors() {
    for (name, t) in themes() {
        let packet = build_packet(&bed_scene(BedOrigin::BottomLeft, vec![]), &t);
        let lines = grid_strokes(&packet, &t);
        assert!(!lines.is_empty(), "{name}: no grid");
        let [l, tp, r, b] = BED_PX;
        for (pts, _) in &lines {
            for [x, y] in pts {
                assert!(
                    (l - 1.0..=r + 1.0).contains(x) && (tp - 1.0..=b + 1.0).contains(y),
                    "{name}: grid point {x},{y} outside the bed"
                );
            }
        }
        let majors = lines.iter().filter(|(_, c)| *c == t.grid_major).count();
        let minors = lines.iter().filter(|(_, c)| Some(*c) == t.grid).count();
        // 5 mm minors, a major every second line (10 mm): x 0,10..100 = 11 majors, y 0,10..60 = 7.
        assert_eq!(majors, 11 + 7, "{name}: major lines");
        assert_eq!(minors, 10 + 6, "{name}: minor lines (x 5,15..95 and y 5,15..55)");
        // The bed surface is a fill of the bed colour exactly covering the bed.
        let fill = packet
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Fill { contours_px, color } if *color == t.bed_fill => Some(contours_px.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: no bed fill"));
        let xs: Vec<f32> = fill[0].iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = fill[0].iter().map(|p| p[1]).collect();
        let extent =
            |v: &[f32]| (v.iter().cloned().fold(f32::MAX, f32::min), v.iter().cloned().fold(f32::MIN, f32::max));
        assert_eq!(extent(&xs), (l, r));
        assert_eq!(extent(&ys), (tp, b));
    }
}

/// The origin mark sits on the bed corner the machine's origin is: bottom-left of the bed view is
/// pixel (100, 320), and so on for the other three.
#[test]
fn the_origin_mark_sits_on_the_right_bed_corner() {
    let t = Theme::light();
    let [l, tp, r, b] = BED_PX;
    for (origin, corner) in [
        (BedOrigin::BottomLeft, [l, b]),
        (BedOrigin::TopLeft, [l, tp]),
        (BedOrigin::TopRight, [r, tp]),
        (BedOrigin::BottomRight, [r, b]),
    ] {
        let packet = build_packet(&bed_scene(origin, vec![]), &t);
        let pts: Vec<[f32; 2]> = packet
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Stroke { points_px, color, .. } if *color == t.origin => Some(points_px.clone()),
                Primitive::Fill { contours_px, color } if *color == t.origin => Some(contours_px.concat()),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(!pts.is_empty(), "{origin:?}: no origin mark");
        // The mark contains the corner and reaches inward along both edges, never outside the bed.
        assert!(
            pts.iter().any(|p| (p[0] - corner[0]).abs() < 1.0 && (p[1] - corner[1]).abs() < 1.0),
            "{origin:?}: corner {corner:?} not on the mark"
        );
        assert!(
            pts.iter().all(|p| (l - 1.0..=r + 1.0).contains(&p[0]) && (tp - 1.0..=b + 1.0).contains(&p[1])),
            "{origin:?}: mark outside the bed"
        );
        let far = |axis: usize, edge: f32| pts.iter().any(|p| (p[axis] - edge).abs() > 12.0);
        assert!(far(0, corner[0]) && far(1, corner[1]), "{origin:?}: the mark must show both axes");
    }
}

/// Pixels: outside the bed is the canvas background, inside (off the grid) the bed colour, and
/// no grid colour appears outside the bed.
#[test]
fn rendered_bed_has_background_outside_and_bed_colour_inside() {
    let Some(ctx) = context() else { return };
    for (name, t) in themes() {
        let packet = build_packet(&bed_scene(BedOrigin::BottomLeft, vec![]), &t);
        let image = render_offscreen(&ctx, &packet).unwrap();
        let to8 = |c: Rgba| [c.r, c.g, c.b].map(|v| (v * 255.0).round() as i32);
        let near = |p: [u8; 4], c: Rgba| {
            let e = to8(c);
            (0..3).all(|i| (i32::from(p[i]) - e[i]).abs() <= 2)
        };
        assert!(near(image.pixel(20, 20).unwrap(), t.background), "{name}: outside top-left");
        assert!(near(image.pixel(580, 380).unwrap(), t.background), "{name}: outside bottom-right");
        assert!(near(image.pixel(300, 5).unwrap(), t.background), "{name}: above the bed");
        // x = 311 is between the grid lines at 10 mm (x 300) and 15 mm (x 320); y 203 likewise.
        assert!(near(image.pixel(311, 203).unwrap(), t.bed_fill), "{name}: inside the bed {:?}", image.pixel(311, 203));
        // No pixel outside the bed shows a grid colour.
        for y in (0..400).step_by(3) {
            for x in (0..600).step_by(3) {
                let outside = !(98..=502).contains(&x) || !(78..=322).contains(&y);
                if outside {
                    let p = image.pixel(x, y).unwrap();
                    assert!(
                        !near(p, t.grid.unwrap()) && !near(p, t.grid_major),
                        "{name}: grid pixel outside the bed at {x},{y}"
                    );
                }
            }
        }
    }
}

// ---- 4. no grey fill ----------------------------------------------------------------------

fn square() -> Arc<Path> {
    Arc::new(Path::rectangle(
        Rect::new(Point::new(20.0, 20.0), Length::from_mm(30.0).unwrap(), Length::from_mm(20.0).unwrap()).unwrap(),
    ))
}

fn fill_count(p: &RenderPacket, except: &[Rgba]) -> usize {
    p.primitives.iter().filter(|p| matches!(p, Primitive::Fill { color, .. } if !except.contains(color))).count()
}

#[test]
fn a_closed_shape_gets_no_fill_unless_it_means_something() {
    for (name, t) in themes() {
        let bed = [t.bed_fill];
        for role in [StyleRole::Design, StyleRole::ProcessLayer(3), StyleRole::Hovered, StyleRole::Disabled] {
            let packet = build_packet(&bed_scene(BedOrigin::BottomLeft, vec![(square(), role)]), &t);
            assert_eq!(fill_count(&packet, &bed), 0, "{name}: {role:?} must not be filled");
        }
        // A selected shape gets at most a faint hint, in the accent colour.
        let packet = build_packet(&bed_scene(BedOrigin::BottomLeft, vec![(square(), StyleRole::Selected)]), &t);
        for p in &packet.primitives {
            if let Primitive::Fill { color, .. } = p
                && *color != t.bed_fill
            {
                assert!(color.a <= 0.2 && color.a > 0.0, "{name}: selection hint alpha {}", color.a);
                assert_eq!((color.r, color.g, color.b), (t.selection.r, t.selection.g, t.selection.b), "{name}");
            }
        }
    }
}
