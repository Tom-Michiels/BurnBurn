//! LFB-rest: an object that a Fill / Offset Fill operation fills gets a light fill in the layer colour
//! on the canvas; a line-only object stays an outline. Packet tests are headless; the pixel test
//! renders offscreen on the real GPU (SKIPPED without an adapter, never a pass).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_editor::{Camera, SceneItem, SceneSnapshot, StyleRole, Viewport};
use burnburn_geometry::{Affine, Path, Point, Rect};
use burnburn_render::gpu::{GpuContext, GpuError, render_offscreen};
use burnburn_render::{Primitive, Theme, build_packet};
use burnburn_types::{DocumentId, GeometryId, Length, RevisionId, StableId};

const SLOT: u16 = 2;

fn rect(x: f64, y: f64) -> Arc<Path> {
    Arc::new(Path::rectangle(
        Rect::new(Point::new(x, y), Length::from_mm(20.0).unwrap(), Length::from_mm(20.0).unwrap()).unwrap(),
    ))
}

/// Two 20 mm squares at 2 px/mm: the left one filled by an operation of slot 2, the right one line-only.
fn scene(left_fill: Option<u16>) -> SceneSnapshot {
    let item = |id: u64, path, fill_slot| SceneItem {
        source: GeometryId::from_raw(id),
        path,
        transform: Affine::IDENTITY,
        role: StyleRole::ProcessLayer(SLOT),
        fill_slot,
    };
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(25.0, 10.0), 2.0).unwrap(),
        viewport: Viewport::new(100.0, 40.0, 1.0).unwrap(),
        items: vec![item(1, rect(0.0, 0.0), left_fill), item(2, rect(30.0, 0.0), None)],
        overlays: Vec::new(),
    }
}

fn fills(theme: &Theme, scene: &SceneSnapshot) -> Vec<(Vec<Vec<[f32; 2]>>, burnburn_render::Rgba)> {
    build_packet(scene, theme)
        .primitives
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Fill { contours_px, color } => Some((contours_px, color)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_filled_object_gets_a_light_fill_in_its_layer_colour_and_a_line_object_none() {
    for theme in [Theme::light(), Theme::dark()] {
        let layer = theme.process_layers[usize::from(SLOT)];
        let all = fills(&theme, &scene(Some(SLOT)));
        assert_eq!(all.len(), 1, "only the filled object has a fill");
        let (contours, colour) = &all[0];
        assert_eq!((colour.r, colour.g, colour.b), (layer.r, layer.g, layer.b), "the layer colour");
        assert!(colour.a > 0.05 && colour.a <= 0.4, "a light tint, not a solid: alpha {}", colour.a);
        // It covers the left square only: x from 0 mm (-25*2 + 50 = 0 px) to 40 px.
        let xs: Vec<f32> = contours[0].iter().map(|p| p[0]).collect();
        assert!(xs.iter().all(|x| (-0.5..=40.5).contains(x)), "{xs:?}");
        assert!(fills(&theme, &scene(None)).is_empty(), "no operation fill, no canvas fill");
    }
}

#[test]
fn the_fill_lies_beneath_the_outline_and_wireframe_draws_none() {
    let theme = Theme::light();
    let packet = build_packet(&scene(Some(SLOT)), &theme);
    let first_fill = packet.primitives.iter().position(|p| matches!(p, Primitive::Fill { .. })).unwrap();
    let first_stroke = packet.primitives.iter().position(|p| matches!(p, Primitive::Stroke { .. })).unwrap();
    assert!(first_fill < first_stroke, "fill first, outline on top");
    assert!(fills(&theme.wireframe(), &scene(Some(SLOT))).is_empty(), "draadmodel: outlines only");
}

#[test]
fn offscreen_pixels_show_the_tint_inside_the_filled_square_only() {
    let ctx = match GpuContext::headless() {
        Ok(ctx) => ctx,
        Err(GpuError::NoAdapter(e)) if std::env::var_os("BURNBURN_REQUIRE_GPU").is_none() => {
            eprintln!("SKIPPED (not a pass): no GPU adapter on this machine: {e}");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let theme = Theme::light();
    let layer = theme.process_layers[usize::from(SLOT)];
    let image = render_offscreen(&ctx, &build_packet(&scene(Some(SLOT)), &theme)).unwrap();
    // Centres of the squares: (20, 20) px and (80, 20) px.
    let tinted = image.pixel(20, 20).unwrap();
    let plain = image.pixel(80, 20).unwrap();
    let bg = [theme.background.r, theme.background.g, theme.background.b].map(|v| (v * 255.0).round() as i32);
    for (i, c) in [layer.r, layer.g, layer.b].into_iter().enumerate() {
        assert!((i32::from(plain[i]) - bg[i]).abs() <= 2, "line-only square stays the canvas colour");
        // The tint moves the pixel from the background towards the layer colour.
        let towards = (c * 255.0).round() as i32 - bg[i];
        let moved = i32::from(tinted[i]) - bg[i];
        assert!(
            towards.abs() < 8 || (moved != 0 && moved.signum() == towards.signum()),
            "channel {i}: {moved} vs {towards}"
        );
    }
    assert_ne!(tinted[..3], plain[..3], "filled and line-only squares differ");
}
