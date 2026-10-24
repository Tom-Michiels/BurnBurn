//! LF9: rulers above and left of the canvas. The layout (which marks, which numbers) is pure and
//! tested per zoom level; the picture is tested offscreen on the real GPU (SKIPPED without an
//! adapter, never a pass; `BURNBURN_REQUIRE_GPU=1` makes that a failure).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_editor::{Camera, SceneSnapshot, Viewport};
use burnburn_geometry::Point;
use burnburn_render::gpu::{GpuContext, GpuError, Image, render_offscreen};
use burnburn_render::{
    CanvasChrome, MarkKind, RULER_THICKNESS_PT, Rgba, RulerAxis, RulerUnit, Rulers, Theme, build_packet_with,
    contrast_ratio, ruler_marks, ruler_step,
};
use burnburn_types::{DocumentId, RevisionId, StableId};

/// Mantissa (1 up to 10) of a positive number.
fn mantissa(v: f64) -> f64 {
    let m = v / 10f64.powf(v.log10().floor());
    if m > 9.999_999 { 1.0 } else { m }
}

fn is_nice(v: f64) -> bool {
    let m = mantissa(v);
    [1.0, 2.0, 5.0].iter().any(|n| (m - n).abs() < 1e-9)
}

#[test]
fn the_major_step_is_a_nice_number_at_every_zoom_level_and_the_finest_that_fits() {
    let mut ppu = 0.003;
    while ppu < 5000.0 {
        let step = ruler_step(ppu);
        assert!(is_nice(step.major), "{ppu} pt/unit: major {} is not 1, 2 or 5 x 10^n", step.major);
        assert!(step.major * ppu >= burnburn_render::RULER_MIN_MAJOR_GAP_PT - 1e-9, "{ppu}: labels too close");
        // The finest: the next smaller nice number would crowd the labels.
        let smaller = match mantissa(step.major) {
            m if (m - 1.0).abs() < 1e-9 => step.major / 2.0, // 1 -> 0.5
            m if (m - 2.0).abs() < 1e-9 => step.major / 2.0, // 2 -> 1
            _ => step.major * 2.0 / 5.0,                     // 5 -> 2
        };
        assert!(smaller * ppu < burnburn_render::RULER_MIN_MAJOR_GAP_PT, "{ppu}: {} would also fit", smaller);
        // The minor step is a nice number too and divides the major exactly.
        assert!(step.subdivisions >= 2);
        let minor = step.major / f64::from(step.subdivisions);
        assert!(is_nice(minor), "{ppu}: minor {minor}");
        ppu *= 1.37;
    }
}

#[test]
fn known_zoom_levels_give_the_expected_steps() {
    // 4 pt per mm: 10 mm is 40 pt (too close), 20 mm is 80 pt.
    assert_eq!(ruler_step(4.0).major, 20.0);
    // 1 pt per mm: 100 mm is 100 pt; 50 mm (50 pt) is too close.
    assert_eq!(ruler_step(1.0).major, 100.0);
    // 10 pt per mm: 10 mm = 100 pt, 5 mm = 50 pt is too close.
    assert_eq!(ruler_step(10.0).major, 10.0);
    // 25.4 * 4 pt per inch: 1 in = 101.6 pt, 0.5 in = 50.8 pt is too close.
    assert_eq!(ruler_step(25.4 * 4.0).major, 1.0);
}

fn scene() -> SceneSnapshot {
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(0.0, 0.0), 4.0).unwrap(),
        viewport: Viewport::new(300.0, 200.0, 2.0).unwrap(),
        items: vec![],
        overlays: vec![],
    }
}

#[test]
fn marks_sit_at_the_projected_values_and_scale_with_the_zoom() {
    let s = scene();
    let top = ruler_marks(&s.camera, &s.viewport, RulerAxis::Horizontal, RulerUnit::Millimetre);
    let majors: Vec<_> = top.iter().filter(|m| m.kind == MarkKind::Major).collect();
    // World x = 0 is the middle of the 300 pt view.
    let zero = majors.iter().find(|m| m.value == 0.0).expect("a major mark at 0");
    assert!((zero.at_pt - 150.0).abs() < 1e-9);
    // 20 mm steps of 80 pt.
    let pts: Vec<f64> = majors.iter().map(|m| m.at_pt).collect();
    assert!(pts.windows(2).all(|w| (w[1] - w[0] - 80.0).abs() < 1e-6), "{pts:?}");
    assert!(top.iter().all(|m| (0.0..=300.0).contains(&m.at_pt)));
    // Minor marks between two majors: subdivisions - 1 of them.
    let between = top.iter().filter(|m| m.kind == MarkKind::Minor && m.at_pt > 150.0 && m.at_pt < 230.0).count();
    assert_eq!(between as u32, ruler_step(4.0).subdivisions - 1);
    // Zoomed in 5x the steps get finer: more marks, smaller values between them.
    let zoomed = Camera::new(Point::new(0.0, 0.0), 20.0).unwrap();
    let fine = ruler_marks(&zoomed, &s.viewport, RulerAxis::Horizontal, RulerUnit::Millimetre);
    let fine_step = fine.iter().filter(|m| m.kind == MarkKind::Major).map(|m| m.value).collect::<Vec<_>>();
    assert_eq!(fine_step[1] - fine_step[0], 5.0, "5 mm = 100 pt at 20 pt/mm");
    // Vertical: world y up, screen y down; y = 20 mm is 80 pt above the middle (100 pt).
    let left = ruler_marks(&s.camera, &s.viewport, RulerAxis::Vertical, RulerUnit::Millimetre);
    let twenty = left.iter().find(|m| m.kind == MarkKind::Major && m.value == 20.0).expect("20 mm");
    assert!((twenty.at_pt - 20.0).abs() < 1e-9, "{}", twenty.at_pt);
    // Inch: the major mark 1 in lies 25.4 mm from the origin.
    let inch = ruler_marks(&s.camera, &s.viewport, RulerAxis::Horizontal, RulerUnit::Inch);
    let one = inch.iter().find(|m| m.kind == MarkKind::Major && m.value == 1.0).expect("1 in");
    assert!((one.at_pt - (150.0 + 25.4 * 4.0)).abs() < 1e-9);
}

#[test]
fn ruler_numbers_and_ink_are_readable_in_both_themes() {
    for (name, t) in [("light", Theme::light()), ("dark", Theme::dark())] {
        assert!(contrast_ratio(t.ruler_ink, t.ruler_background) >= 4.5, "{name}: ruler text contrast");
        assert!(contrast_ratio(t.ruler_ink, t.ruler_background) >= 4.5);
        assert_ne!(t.ruler_background, t.background, "{name}: the ruler is a band of its own");
    }
}

// ---- pixels ---------------------------------------------------------------------------------

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

fn render(ctx: &GpuContext, theme: &Theme, rulers: Option<Rulers>) -> Image {
    let chrome = CanvasChrome { rulers, ..CanvasChrome::default() };
    render_offscreen(ctx, &build_packet_with(&scene(), theme, &chrome)).unwrap()
}

fn near(p: [u8; 4], c: Rgba, tol: f32) -> bool {
    let ch = |v: u8| f32::from(v) / 255.0;
    (ch(p[0]) - c.r).abs() <= tol && (ch(p[1]) - c.g).abs() <= tol && (ch(p[2]) - c.b).abs() <= tol
}

fn rulers(unit: RulerUnit, cursor_mm: Option<Point>) -> Option<Rulers> {
    Some(Rulers { unit, cursor_mm, decimal: ',' })
}

const S: f64 = 2.0;

#[test]
fn the_rulers_are_bands_above_and_left_of_the_canvas() {
    let Some(ctx) = context() else { return };
    for (name, t) in [("light", Theme::light()), ("dark", Theme::dark())] {
        let image = render(&ctx, &t, rulers(RulerUnit::Millimetre, None));
        let px = |x: f64, y: f64| image.pixel((x * S) as u32, (y * S) as u32).unwrap();
        let thick = RULER_THICKNESS_PT;
        // Away from marks and numbers: just inside the bands, and in the canvas.
        assert!(near(px(150.5, 1.5), t.ruler_background, 0.03), "{name}: top band");
        assert!(near(px(1.5, 150.5), t.ruler_background, 0.03), "{name}: left band");
        assert!(near(px(150.0, thick + 30.0), t.background, 0.03), "{name}: canvas below the top band");
        assert!(near(px(thick + 30.0, 150.0), t.background, 0.03), "{name}: canvas right of the left band");
        // Without rulers the canvas reaches the edge.
        let bare = render(&ctx, &t, None);
        assert!(near(bare.pixel(300, 3).unwrap(), t.background, 0.03), "{name}: no rulers by default");
    }
}

#[test]
fn major_marks_and_numbers_are_inked_and_follow_the_unit() {
    let Some(ctx) = context() else { return };
    let t = Theme::light();
    let mm = render(&ctx, &t, rulers(RulerUnit::Millimetre, None));
    let inch = render(&ctx, &t, rulers(RulerUnit::Inch, None));
    let ink = |image: &Image, x0: f64, x1: f64, y0: f64, y1: f64| {
        let mut n = 0;
        for y in (y0 * S) as u32..(y1 * S) as u32 {
            for x in (x0 * S) as u32..(x1 * S) as u32 {
                if !near(image.pixel(x, y).unwrap(), t.ruler_background, 0.2) {
                    n += 1;
                }
            }
        }
        n
    };
    // The major mark at x = 0 (150 pt): a tick that reaches into the lower half of the top band.
    let tick = mm.pixel((150.0 * S) as u32, ((RULER_THICKNESS_PT - 3.0) * S) as u32).unwrap();
    assert!(near(tick, t.ruler_ink, 0.25), "tick pixel {tick:?}");
    // Numbers: ink in the band beside the ticks, and the unit corner is inked too.
    assert!(ink(&mm, 20.0, 300.0, 0.0, 12.0) > 100, "numbers in the top band");
    assert!(ink(&mm, 0.0, 20.0, 20.0, 200.0) > 100, "numbers in the left band");
    assert!(ink(&mm, 0.0, 20.0, 0.0, 20.0) > 20, "the unit in the corner");
    // The unit changes the numbers (and the corner): the pictures differ.
    assert_ne!(mm.rgba, inch.rgba);
    assert_ne!(ink(&mm, 0.0, 20.0, 0.0, 20.0), ink(&inch, 0.0, 20.0, 0.0, 20.0), "'mm' is not 'in'");
}

#[test]
fn the_cursor_position_is_marked_on_both_rulers() {
    let Some(ctx) = context() else { return };
    for (name, t) in [("light", Theme::light()), ("dark", Theme::dark())] {
        // Cursor at world (10, 5) mm: x = 150 + 40 = 190 pt, y = 100 - 20 = 80 pt.
        let image = render(&ctx, &t, rulers(RulerUnit::Millimetre, Some(Point::new(10.0, 5.0))));
        let bare = render(&ctx, &t, rulers(RulerUnit::Millimetre, None));
        let at = |img: &Image, x: f64, y: f64| img.pixel((x * S) as u32, (y * S) as u32).unwrap();
        assert!(near(at(&image, 190.0, 4.0), t.selection, 0.12), "{name}: marker on the top ruler");
        assert!(near(at(&image, 4.0, 80.0), t.selection, 0.12), "{name}: marker on the left ruler");
        assert!(!near(at(&bare, 190.0, 4.0), t.selection, 0.12));
        assert!(!near(at(&bare, 4.0, 80.0), t.selection, 0.12));
    }
}
