//! LF9: a snap target is marked by kind (square = endpoint, triangle = midpoint, circle = centre,
//! X = intersection) and carries a label with a halo ("Eindpunt", "Midden", "Raster" ...) next to it.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_editor::{Camera, Overlay, SceneSnapshot, SnapKind, Viewport};
use burnburn_geometry::Point;
use burnburn_render::gpu::{GpuContext, GpuError, render_offscreen};
use burnburn_render::{CanvasChrome, Primitive, Rgba, SnapLabels, Theme, build_packet_with};
use burnburn_types::{DocumentId, RevisionId, StableId};

fn scene(kind: SnapKind) -> SceneSnapshot {
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(0.0, 0.0), 4.0).unwrap(),
        viewport: Viewport::new(100.0, 100.0, 2.0).unwrap(),
        items: vec![],
        overlays: vec![Overlay::SnapCandidate { at: Point::new(0.0, 0.0), kind }],
    }
}

fn packet(kind: SnapKind, chrome: &CanvasChrome) -> Vec<Primitive> {
    build_packet_with(&scene(kind), &Theme::light(), chrome).primitives
}

fn snap_strokes(p: &[Primitive], colour: Rgba) -> Vec<(usize, bool)> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Stroke { points_px, closed, color, .. } if *color == colour => Some((points_px.len(), *closed)),
            _ => None,
        })
        .collect()
}

fn no_labels() -> CanvasChrome {
    CanvasChrome { snap_labels: None, ..CanvasChrome::default() }
}

#[test]
fn each_snap_kind_has_its_own_marker_shape() {
    let colour = Theme::light().snap;
    let chrome = no_labels();
    let endpoint = snap_strokes(&packet(SnapKind::Endpoint, &chrome), colour);
    assert_eq!(endpoint, vec![(4, true)], "a square");
    let midpoint = snap_strokes(&packet(SnapKind::Midpoint, &chrome), colour);
    assert_eq!(midpoint, vec![(3, true)], "a triangle");
    let centre = snap_strokes(&packet(SnapKind::Center, &chrome), colour);
    assert!(centre.len() == 1 && centre[0].1 && centre[0].0 >= 16, "a circle: {centre:?}");
    let cross = snap_strokes(&packet(SnapKind::Intersection, &chrome), colour);
    assert_eq!(cross, vec![(2, false), (2, false)], "an X of two strokes");
}

fn label_fills(p: &[Primitive], colour: Rgba) -> Vec<&Vec<Vec<[f32; 2]>>> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Fill { contours_px, color } if *color == colour => Some(contours_px),
            _ => None,
        })
        .collect()
}

#[test]
fn a_snap_gets_a_label_beside_its_marker_in_the_language_of_the_host() {
    let colour = Theme::light().snap;
    let dutch = CanvasChrome { snap_labels: Some(SnapLabels::DUTCH), ..CanvasChrome::default() };
    let english = CanvasChrome { snap_labels: Some(SnapLabels::ENGLISH), ..CanvasChrome::default() };
    let with = packet(SnapKind::Midpoint, &dutch);
    let without = packet(SnapKind::Midpoint, &no_labels());
    let glyphs = label_fills(&with, colour);
    assert_eq!(glyphs.len(), 1, "the label is one fill of glyph outlines");
    assert!(label_fills(&without, colour).is_empty());
    // The marker is centred on device (100, 100); the label starts to its right.
    let xs: Vec<f32> = glyphs[0].iter().flatten().map(|p| p[0]).collect();
    assert!(xs.iter().cloned().fold(f32::MAX, f32::min) > 100.0, "right of the marker");
    // A halo ring (strokes in the background colour) lies beneath the glyphs.
    let halo = with
        .iter()
        .filter(|p| matches!(p, Primitive::Stroke { color, .. } if *color == Theme::light().background))
        .count();
    assert!(halo >= 3, "halo rings: {halo}");
    assert_ne!(label_fills(&packet(SnapKind::Midpoint, &english), colour), glyphs, "Midpoint is not Midden");
}

#[test]
fn the_labels_name_every_kind_and_the_canvas_font_can_draw_them() {
    for labels in [SnapLabels::DUTCH, SnapLabels::ENGLISH] {
        let mut seen = Vec::new();
        for kind in [
            SnapKind::Endpoint,
            SnapKind::Intersection,
            SnapKind::Midpoint,
            SnapKind::Center,
            SnapKind::Nearest,
            SnapKind::Grid,
        ] {
            let text = labels.get(kind);
            assert!(!text.is_empty());
            assert!(burnburn_render::canvas_text_missing_chars(text).is_empty(), "{text}");
            seen.push(text);
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 6, "six different names");
    }
    assert_eq!(SnapLabels::DUTCH.get(SnapKind::Endpoint), "Eindpunt");
}

/// The label is readable: on the canvas the text pixels differ from the halo around them.
#[test]
fn the_rendered_label_has_ink_pixels_beside_the_marker() {
    let ctx = match GpuContext::headless() {
        Ok(ctx) => ctx,
        Err(GpuError::NoAdapter(e)) if std::env::var_os("BURNBURN_REQUIRE_GPU").is_none() => {
            eprintln!("SKIPPED (not a pass): no GPU adapter on this machine: {e}");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let theme = Theme::light();
    let chrome = CanvasChrome { snap_labels: Some(SnapLabels::DUTCH), ..CanvasChrome::default() };
    let image = render_offscreen(&ctx, &build_packet_with(&scene(SnapKind::Endpoint), &theme, &chrome)).unwrap();
    let region = (112..190u32).flat_map(|x| (60..112u32).map(move |y| (x, y)));
    let near = |p: [u8; 4], c: Rgba| {
        let ch = |v: u8| f32::from(v) / 255.0;
        (ch(p[0]) - c.r).abs() < 0.12 && (ch(p[1]) - c.g).abs() < 0.12 && (ch(p[2]) - c.b).abs() < 0.12
    };
    let ink = region.clone().filter(|&(x, y)| near(image.pixel(x, y).unwrap(), theme.snap)).count();
    assert!(ink >= 40, "label ink pixels: {ink}");
}
