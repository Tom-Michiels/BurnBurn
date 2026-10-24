//! LF9 (look and feel, canvas): selection grips. The rotate grip is round and hangs on a stem, the
//! scale grips are squares, and the grip under the pointer is drawn larger. Pixel tests render
//! offscreen on the real GPU (SKIPPED without an adapter, never a pass; `BURNBURN_REQUIRE_GPU=1`
//! makes that a failure).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_editor::{Camera, Handle, Overlay, SceneSnapshot, ScaleGrip, Viewport};
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

const SCALE: f64 = 4.0;
/// 100 x 100 pt view at 4 device px per pt: 400 x 400 px, world (0, 0) in the middle, 4 pt per mm.
fn scene(overlays: Vec<Overlay>) -> SceneSnapshot {
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(0.0, 0.0), 4.0).unwrap(),
        viewport: Viewport::new(100.0, 100.0, SCALE).unwrap(),
        items: vec![],
        overlays,
    }
}

fn handle(at: Point, handle: Handle, stem_from: Option<Point>, hovered: bool) -> Overlay {
    Overlay::Handle { at, handle, stem_from, hovered }
}

fn render(ctx: &GpuContext, theme: &Theme, overlays: Vec<Overlay>) -> Image {
    render_offscreen(ctx, &build_packet(&scene(overlays), theme)).unwrap()
}

fn near(a: [u8; 4], c: Rgba, tol: f32) -> bool {
    let ch = |v: u8| f32::from(v) / 255.0;
    (ch(a[0]) - c.r).abs() <= tol && (ch(a[1]) - c.g).abs() <= tol && (ch(a[2]) - c.b).abs() <= tol
}

fn themes() -> [(&'static str, Theme); 2] {
    [("light", Theme::light()), ("dark", Theme::dark())]
}

/// Device pixels that differ from the clear colour.
fn painted(image: &Image, theme: &Theme) -> usize {
    (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| !near(image.pixel(x, y).unwrap(), theme.background, 0.02))
        .count()
}

#[test]
fn the_rotate_grip_is_round_and_the_scale_grip_is_square() {
    let Some(ctx) = context() else { return };
    for (name, theme) in themes() {
        let size_px = theme.grip_size_pt * SCALE;
        // 40% of the grip's size from its centre along both axes: inside a square, outside a circle.
        let corner = (size_px * 0.4) as u32;
        let (cx, cy) = (200u32, 200u32);
        let square = render(
            &ctx,
            &theme,
            vec![handle(Point::new(0.0, 0.0), Handle::Scale(ScaleGrip::TopRight), None, false)],
        );
        let round = render(&ctx, &theme, vec![handle(Point::new(0.0, 0.0), Handle::Rotate, None, false)]);
        for (label, image, expect_corner_filled) in [("scale", &square, true), ("rotate", &round, false)] {
            assert!(near(image.pixel(cx, cy).unwrap(), theme.bed_fill, 0.03), "{name} {label}: centre is the grip fill");
            for (dx, dy) in [(1i32, 1i32), (-1, 1), (1, -1), (-1, -1)] {
                let p = image.pixel((cx as i32 + dx * corner as i32) as u32, (cy as i32 + dy * corner as i32) as u32).unwrap();
                assert_eq!(
                    near(p, theme.bed_fill, 0.03),
                    expect_corner_filled,
                    "{name} {label}: corner ({dx},{dy}) = {p:?}"
                );
            }
        }
    }
}

#[test]
fn the_hovered_grip_is_drawn_larger() {
    let Some(ctx) = context() else { return };
    for (name, theme) in themes() {
        for grip in [Handle::Scale(ScaleGrip::Left), Handle::Rotate] {
            let calm = painted(&render(&ctx, &theme, vec![handle(Point::new(0.0, 0.0), grip, None, false)]), &theme);
            let hot = painted(&render(&ctx, &theme, vec![handle(Point::new(0.0, 0.0), grip, None, true)]), &theme);
            assert!(calm > 0, "{name} {grip:?}: nothing drawn");
            // 9 pt against 7 pt is 1.65 times the area; allow for the outline and anti-aliasing.
            assert!(hot as f64 >= calm as f64 * 1.4, "{name} {grip:?}: hovered {hot} px against {calm} px");
        }
    }
}

#[test]
fn the_rotate_grip_hangs_on_a_stem_drawn_in_the_selection_colour() {
    let Some(ctx) = context() else { return };
    for (name, theme) in themes() {
        // Grip 6 mm (24 pt, 96 px) above the stem's start; the stem is vertical through x = 200.
        let image = render(
            &ctx,
            &theme,
            vec![handle(Point::new(0.0, 6.0), Handle::Rotate, Some(Point::new(0.0, 0.0)), false)],
        );
        // Half way between grip and top edge, away from both markers.
        let (x, y) = (200u32, 200 - 48);
        let hit = (x - 2..=x + 2).any(|xx| near(image.pixel(xx, y).unwrap(), theme.selection, 0.12));
        assert!(hit, "{name}: no stem pixel in the selection colour near {x},{y}");
        // And not a stem where no grip hangs.
        let bare = render(&ctx, &theme, vec![handle(Point::new(0.0, 6.0), Handle::Rotate, None, false)]);
        assert!(!(x - 2..=x + 2).any(|xx| near(bare.pixel(xx, y).unwrap(), theme.selection, 0.12)), "{name}: stem without stem_from");
    }
}
