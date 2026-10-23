//! Scene translation contract: `SceneSnapshot` (world mm) → `RenderPacket` (device px), with
//! analytic expectations for camera, viewport, backing scale, flattening and the grid.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_editor::{Camera, Overlay, SceneItem, SceneSnapshot, StyleRole, Viewport};
use burnburn_geometry::{Affine, Bounds, Path, PathBuilder, Point, Rect};
use burnburn_render::{Primitive, Rgba, Theme, build_packet};
use burnburn_types::{DocumentId, GeometryId, Length, RevisionId, StableId};

const GREY: Rgba = Rgba::new(0.5, 0.5, 0.5, 1.0);
const INK: Rgba = Rgba::new(0.0, 0.0, 0.0, 1.0);

/// A theme with distinct, easy colours and no grid unless a test asks for it.
fn theme() -> Theme {
    let mut t = Theme::light();
    t.design.fill = Some(GREY);
    t.design.stroke = Some(INK);
    t.selected.fill = Some(GREY);
    t.grid = None;
    t
}

fn rect_path(x: f64, y: f64, w: f64, h: f64) -> Arc<Path> {
    Arc::new(Path::rectangle(
        Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap(),
    ))
}

fn scene(
    camera: Camera,
    viewport: Viewport,
    items: Vec<(Arc<Path>, StyleRole)>,
    overlays: Vec<Overlay>,
) -> SceneSnapshot {
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera,
        viewport,
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
        overlays,
    }
}

/// Corners of a polygon as a sorted set (start point and direction are the geometry's business).
fn corners(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut c: Vec<[f32; 2]> = points.to_vec();
    if c.len() > 1 && c.first() == c.last() {
        c.pop();
    }
    c.sort_by(|a, b| a.partial_cmp(b).unwrap());
    c
}

fn fills(p: &[Primitive]) -> Vec<(Vec<Vec<[f32; 2]>>, Rgba)> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Fill { contours_px, color } => Some((contours_px.iter().map(|c| corners(c)).collect(), *color)),
            _ => None,
        })
        .collect()
}

fn strokes(p: &[Primitive]) -> Vec<(Vec<[f32; 2]>, bool, f32, Rgba)> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Stroke { points_px, closed, width_px, color } => {
                Some((points_px.clone(), *closed, *width_px, *color))
            }
            _ => None,
        })
        .collect()
}

/// 800 × 600 pt at backing scale 2 is 1600 × 1200 px with the centre at (800, 600) px. The camera
/// centres (60, 40) mm at 2 pt/mm, so 1 mm = 4 px. The 120 × 80 mm rectangle from the origin spans
/// x 0..120 mm → 560..1040 px and y 80..0 mm (y up) → 440..760 px (y down). Selected: the theme's
/// selection outline, 1 pt = 2 px wide on the same corners, after the fill.
#[test]
fn scene_projects_selected_rectangle_at_retina_scale() {
    let viewport = Viewport::new(800.0, 600.0, 2.0).unwrap();
    let camera = Camera::new(Point::new(60.0, 40.0), 2.0).unwrap();
    let t = theme();
    let s = scene(camera, viewport, vec![(rect_path(0.0, 0.0, 120.0, 80.0), StyleRole::Selected)], vec![]);

    let packet = build_packet(&s, &t);

    assert_eq!(packet.size_px, [1600, 1200]);
    assert_eq!(packet.clear, t.background);
    let box_px = vec![[560.0, 440.0], [560.0, 760.0], [1040.0, 440.0], [1040.0, 760.0]];
    assert_eq!(fills(&packet.primitives), vec![(vec![box_px.clone()], GREY)]);
    let outlines = strokes(&packet.primitives);
    assert_eq!(outlines.len(), 1, "{outlines:?}");
    let (points, closed, width, color) = &outlines[0];
    assert_eq!((corners(points), *closed, *width, *color), (box_px, true, 2.0, t.selection));
    assert!(matches!(packet.primitives[0], Primitive::Fill { .. }), "fill under its outline");
}

/// An unselected rectangle gets the design stroke (1 pt) and no selection outline.
#[test]
fn unselected_rectangle_has_design_stroke_only() {
    let viewport = Viewport::new(100.0, 100.0, 1.0).unwrap();
    let camera = Camera::new(Point::new(0.0, 0.0), 1.0).unwrap();
    let s = scene(camera, viewport, vec![(rect_path(-10.0, -10.0, 20.0, 20.0), StyleRole::Design)], vec![]);
    let packet = build_packet(&s, &theme());
    let box_px = vec![[40.0, 40.0], [40.0, 60.0], [60.0, 40.0], [60.0, 60.0]];
    assert_eq!(fills(&packet.primitives), vec![(vec![box_px.clone()], GREY)]);
    let outlines: Vec<_> =
        strokes(&packet.primitives).into_iter().map(|(p, c, w, col)| (corners(&p), c, w, col)).collect();
    assert_eq!(outlines, vec![(box_px, true, 1.0, INK)]);
}

/// 0.25 mm detail one kilometre from the origin stays exact on screen: the projection happens in
/// f64 relative to the camera; a naive f32 world coordinate (ulp 0.0625 mm at 1e6) would not.
#[test]
fn far_from_origin_keeps_subpixel_precision() {
    let viewport = Viewport::new(100.0, 100.0, 2.0).unwrap();
    let far = 1_000_000.0;
    let camera = Camera::new(Point::new(far, far), 10.0).unwrap();
    let s = scene(camera, viewport, vec![(rect_path(far + 0.25, far, 0.25, 0.25), StyleRole::Design)], vec![]);
    let packet = build_packet(&s, &theme());
    // 1 mm = 20 px; centre at (100, 100) px.
    assert_eq!(
        fills(&packet.primitives)[0].0,
        vec![vec![[105.0, 95.0], [105.0, 100.0], [110.0, 95.0], [110.0, 100.0]]]
    );
}

/// Curves are flattened for the screen within a quarter device pixel at the current zoom, and an
/// open path is stroked but not filled.
#[test]
fn curves_flatten_within_a_quarter_pixel_and_open_paths_are_not_filled() {
    let viewport = Viewport::new(400.0, 400.0, 2.0).unwrap();
    let camera = Camera::new(Point::new(0.0, 0.0), 4.0).unwrap();
    let (p0, p1, p2, p3) =
        (Point::new(-40.0, 0.0), Point::new(-40.0, 40.0), Point::new(40.0, 40.0), Point::new(40.0, 0.0));
    let curve = Arc::new(PathBuilder::new().move_to(p0).cubic_to(p1, p2, p3).build().unwrap());
    let s = scene(camera, viewport, vec![(curve, StyleRole::Design)], vec![]);
    let packet = build_packet(&s, &theme());
    assert!(fills(&packet.primitives).is_empty(), "open path: no fill");
    let outlines = strokes(&packet.primitives);
    assert_eq!(outlines.len(), 1);
    let (points, closed, _, _) = &outlines[0];
    assert!(!closed);
    assert!(points.len() > 16, "a 320 px wide arc needs many chords, got {}", points.len());
    // Dense samples of the exact curve, in px, are within 0.25 px (+ f32 slack) of the polyline.
    let to_px = |p: Point| [400.0 + p.x * 8.0, 400.0 - p.y * 8.0];
    let bez = |t: f64| {
        let u = 1.0 - t;
        let b = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        Point::new(
            b[0] * p0.x + b[1] * p1.x + b[2] * p2.x + b[3] * p3.x,
            b[0] * p0.y + b[1] * p1.y + b[2] * p2.y + b[3] * p3.y,
        )
    };
    let dist = |q: [f64; 2]| {
        points
            .windows(2)
            .map(|w| {
                let (a, b) = ([f64::from(w[0][0]), f64::from(w[0][1])], [f64::from(w[1][0]), f64::from(w[1][1])]);
                let ab = [b[0] - a[0], b[1] - a[1]];
                let len2 = ab[0] * ab[0] + ab[1] * ab[1];
                let t = (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
                ((q[0] - a[0] - ab[0] * t).powi(2) + (q[1] - a[1] - ab[1] * t).powi(2)).sqrt()
            })
            .fold(f64::INFINITY, f64::min)
    };
    for i in 0..=1000 {
        let d = dist(to_px(bez(f64::from(i) / 1000.0)));
        assert!(d <= 0.25 + 1e-3, "sample {i}: {d} px from the drawn polyline");
    }
}

/// The grid sits under everything; lines every `spacing_mm` across the visible area, centred on
/// device pixels (crisp 1 px hairlines). 100 × 100 pt at 2 pt/mm shows x −25..25 mm: 11 lines at
/// 5 mm, i.e. every 10 px from 0 px, drawn at k·10 + 0.5 px.
#[test]
fn grid_lines_follow_spacing_under_the_items_and_are_pixel_aligned() {
    let viewport = Viewport::new(100.0, 100.0, 1.0).unwrap();
    let camera = Camera::new(Point::new(0.0, 0.0), 2.0).unwrap();
    let mut t = theme();
    let grid = Rgba::new(0.9, 0.9, 0.9, 1.0);
    t.grid = Some(grid);
    let s = scene(
        camera,
        viewport,
        vec![(rect_path(0.0, 0.0, 10.0, 10.0), StyleRole::Design)],
        vec![Overlay::Grid { spacing_mm: 5.0, major_every: 0 }],
    );
    let packet = build_packet(&s, &t);
    let lines: Vec<_> = strokes(&packet.primitives).into_iter().filter(|s| s.3 == grid).collect();
    let vertical: Vec<f32> = lines.iter().filter(|l| l.0[0][0] == l.0[1][0]).map(|l| l.0[0][0]).collect();
    let mut horizontal: Vec<f32> = lines.iter().filter(|l| l.0[0][1] == l.0[1][1]).map(|l| l.0[0][1]).collect();
    horizontal.sort_by(f32::total_cmp); // world y is up: rows come bottom-first
    let expected: Vec<f32> = (0..=10).map(|k| k as f32 * 10.0 + 0.5).filter(|x| *x < 100.0).collect();
    assert_eq!(vertical, expected);
    assert_eq!(horizontal, expected);
    assert!(lines.iter().all(|l| l.2 == 1.0 && !l.1));
    let first_item = packet.primitives.iter().position(|p| matches!(p, Primitive::Fill { .. })).unwrap();
    assert!(
        packet.primitives[..first_item].iter().all(|p| matches!(p, Primitive::Stroke { color, .. } if *color == grid))
    );
    // A theme without grid colour, or an absurdly dense grid, draws no grid lines.
    t.grid = None;
    assert!(strokes(&build_packet(&s, &t).primitives).iter().all(|l| l.3 != grid));
    t.grid = Some(grid);
    let dense = scene(camera, viewport, vec![], vec![Overlay::Grid { spacing_mm: 1e-6, major_every: 0 }]);
    assert!(build_packet(&dense, &t).primitives.is_empty());
}

/// Overlays draw above the items: selection bounds as an outline, the rubber band as a translucent
/// fill with outline, a drag preview as a preview-coloured stroke.
#[test]
fn overlays_draw_above_items() {
    let viewport = Viewport::new(100.0, 100.0, 1.0).unwrap();
    let camera = Camera::new(Point::new(0.0, 0.0), 1.0).unwrap();
    let t = theme();
    let bounds = Bounds::from_corners(Point::new(-10.0, -10.0), Point::new(10.0, 10.0));
    let s = scene(
        camera,
        viewport,
        vec![(rect_path(-5.0, -5.0, 10.0, 10.0), StyleRole::Design)],
        vec![
            Overlay::SelectionBounds { bounds },
            Overlay::RubberBand { bounds },
            Overlay::DragPreview { path: rect_path(0.0, 0.0, 20.0, 20.0), transform: Affine::IDENTITY },
        ],
    );
    let packet = build_packet(&s, &t);
    let box_px = vec![[40.0, 40.0], [40.0, 60.0], [60.0, 40.0], [60.0, 60.0]];
    let s_out: Vec<_> =
        strokes(&packet.primitives).into_iter().map(|(p, c, w, col)| (corners(&p), c, w, col)).collect();
    assert!(s_out.contains(&(box_px.clone(), true, 1.0, t.selection)), "{s_out:?}");
    assert!(fills(&packet.primitives).contains(&(vec![box_px], t.rubber_band_fill)));
    assert!(s_out.contains(&(vec![[50.0, 30.0], [50.0, 50.0], [70.0, 30.0], [70.0, 50.0]], true, 1.0, t.preview)));
    let item =
        packet.primitives.iter().position(|p| matches!(p, Primitive::Fill { color, .. } if *color == GREY)).unwrap();
    let overlay = packet
        .primitives
        .iter()
        .position(|p| matches!(p, Primitive::Stroke { color, .. } if *color == t.selection))
        .unwrap();
    assert!(item < overlay);
}

/// Roles map to theme styles; process layers cycle through the palette.
#[test]
fn roles_map_to_theme_styles() {
    let t = Theme::light();
    assert_eq!(t.role(StyleRole::Design), t.design);
    assert_eq!(t.role(StyleRole::Selected), t.selected);
    assert_eq!(t.role(StyleRole::Hovered), t.hovered);
    assert_eq!(t.role(StyleRole::Disabled), t.disabled);
    assert_eq!(t.role(StyleRole::ProcessLayer(1)).stroke, Some(t.process_layers[1]));
    let n = t.process_layers.len() as u16;
    assert_eq!(t.role(StyleRole::ProcessLayer(n + 1)).stroke, Some(t.process_layers[1]));
}

/// One colour per layer-palette slot (presentation's `LAYER_SLOTS`, 30 like LightBurn's palette),
/// all different and opaque, so every layer is recognisable on the canvas.
#[test]
fn layer_palette_has_thirty_two_distinct_opaque_colours() {
    // 30 layer colours and the two tool layers T1/T2 (G5-cam).
    let t = Theme::light();
    assert_eq!(t.process_layers.len(), 32);
    let key = |c: &burnburn_render::Rgba| [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8);
    let distinct: std::collections::BTreeSet<_> = t.process_layers.iter().map(key).collect();
    assert_eq!(distinct.len(), 32, "colours repeat");
    assert!(t.process_layers.iter().all(|c| c.a == 1.0));
    assert_ne!(key(&t.process_layers[0]), key(&t.background), "a layer colour equal to the background");
}

/// W2-L1-bewerken: the wireframe view draws outlines only; the strokes stay as they are.
#[test]
fn the_wireframe_theme_draws_no_fills_but_the_same_strokes() {
    let viewport = Viewport::new(100.0, 100.0, 1.0).unwrap();
    let camera = Camera::new(Point::new(0.0, 0.0), 1.0).unwrap();
    let items = vec![
        (rect_path(0.0, 0.0, 10.0, 10.0), StyleRole::Design),
        (rect_path(5.0, 5.0, 10.0, 10.0), StyleRole::Selected),
    ];
    let sc = scene(camera, viewport, items, Vec::new());
    let count = |packet: &burnburn_render::RenderPacket, fills: bool| {
        packet.primitives.iter().filter(|p| matches!(p, Primitive::Fill { .. }) == fills).count()
    };
    let filled = build_packet(&sc, &theme());
    let wire = build_packet(&sc, &theme().wireframe());
    assert_eq!(count(&filled, true), 2);
    assert_eq!(count(&wire, true), 0, "no fill in wireframe");
    assert_eq!(count(&wire, false), count(&filled, false), "every outline is still drawn");
}

/// Display quality (X02): a larger tolerance draws a curve with fewer chords, and the polyline
/// still stays within that tolerance of the exact circle.
#[test]
fn a_coarser_display_tolerance_draws_fewer_chords_within_its_own_bound() {
    let viewport = Viewport::new(400.0, 400.0, 1.0).unwrap();
    let camera = Camera::new(Point::new(0.0, 0.0), 4.0).unwrap();
    let circle = Arc::new(Path::circle(Point::new(0.0, 0.0), 40.0).unwrap());
    let s = scene(camera, viewport, vec![(circle, StyleRole::Design)], vec![]);
    let chords = |theme: &Theme| -> Vec<[f32; 2]> {
        strokes(&build_packet(&s, theme).primitives).into_iter().next().expect("an outline").0
    };
    let fine = chords(&theme());
    let coarse = chords(&theme().with_tolerance_px(2.0));
    assert!(coarse.len() < fine.len() / 2, "{} vs {}", coarse.len(), fine.len());
    // The exact circle has radius 160 px about (200, 200); chords of a polygon inscribed in it sag
    // at most `tolerance` below the circle, and every vertex lies on it.
    for [x, y] in &coarse {
        let r = (f64::from(*x) - 200.0).hypot(f64::from(*y) - 200.0);
        assert!((r - 160.0).abs() < 0.01, "vertex off the circle: {r}");
    }
    let sag = |points: &[[f32; 2]]| {
        points
            .windows(2)
            .map(|w| {
                let mid =
                    [(f64::from(w[0][0]) + f64::from(w[1][0])) / 2.0, (f64::from(w[0][1]) + f64::from(w[1][1])) / 2.0];
                160.0 - (mid[0] - 200.0).hypot(mid[1] - 200.0)
            })
            .fold(0.0, f64::max)
    };
    assert!(sag(&coarse) <= 2.0 + 0.01, "coarse sag {}", sag(&coarse));
    assert!(sag(&fine) <= 0.25 + 0.01, "fine sag {}", sag(&fine));
}
