//! A bitmap on top of the design (W4-UI-machine): the live preview of Adjust Image drawn on the
//! bounds of the image object. Scene translation (semantic) and the real GPU pass (pixels, with a
//! tolerance for texture filtering).
//!
//! Without a GPU adapter the GPU test prints `SKIPPED` and returns; that is not a pass. Set
//! `BURNBURN_REQUIRE_GPU=1` (macOS runners) to turn a missing adapter into a failure.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_editor::{BedPicture, BedQuad, Camera, Overlay, SceneItem, SceneSnapshot, StyleRole, Viewport};
use burnburn_geometry::{Affine, Bounds, Path, Point, Rect};
use burnburn_render::gpu::{GpuContext, GpuError, render_offscreen};
use burnburn_render::{Primitive, Rgba, Theme, build_packet};
use burnburn_types::{DocumentId, GeometryId, Length, RevisionId, StableId};

fn gray_picture(id: u64, w: u32, h: u32, gray: &[u8]) -> Arc<BedPicture> {
    Arc::new(BedPicture::from_gray8(id, w, h, gray).unwrap())
}

/// Corners as the picture's top-left, top-right, bottom-right, bottom-left, in bed mm (y up).
fn quad(tl: (f64, f64), br: (f64, f64)) -> BedQuad {
    BedQuad {
        bed_mm: [Point::new(tl.0, tl.1), Point::new(br.0, tl.1), Point::new(br.0, br.1), Point::new(tl.0, br.1)],
        uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    }
}

fn scene(items: Vec<SceneItem>, overlays: Vec<Overlay>) -> SceneSnapshot {
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        // 100 × 100 pt at scale 1, 1 pt per mm, centred on the origin: bed (x, y) → px (50 + x, 50 − y).
        camera: Camera::new(Point::new(0.0, 0.0), 1.0).unwrap(),
        viewport: Viewport::new(100.0, 100.0, 1.0).unwrap(),
        items,
        overlays,
    }
}

fn rect_item(x: f64, y: f64, w: f64, h: f64) -> SceneItem {
    SceneItem {
        source: GeometryId::from_raw(1),
        path: Arc::new(Path::rectangle(
            Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap(),
        )),
        transform: Affine::IDENTITY,
        role: StyleRole::Design,
    }
}

fn preview(gray: &[u8], opacity: f32) -> Overlay {
    Overlay::ImagePreview {
        picture: gray_picture(5, 2, 2, gray),
        quads: vec![quad((-10.0, 10.0), (10.0, -10.0))].into(),
        opacity,
    }
}

type Found = (u64, Vec<[[f32; 2]; 4]>, f32);

fn bitmaps(p: &[Primitive]) -> Vec<Found> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Bitmap { picture, cells, opacity } => {
                Some((picture.id, cells.iter().map(|c| c.corners_px).collect(), *opacity))
            }
            _ => None,
        })
        .collect()
}

/// The bitmap lies on the bounds of the image object: above the object's fill and outline, below
/// the selection marks that belong to it. The picture is not an Image primitive (those are
/// painted beneath everything).
#[test]
fn an_image_preview_is_painted_above_the_design_and_below_the_selection_marks() {
    let bounds = Bounds::from_corners(Point::new(-10.0, -10.0), Point::new(10.0, 10.0));
    let s = scene(
        vec![rect_item(-10.0, -10.0, 20.0, 20.0)],
        vec![Overlay::SelectionBounds { bounds }, preview(&[0, 255, 255, 0], 1.0)],
    );

    let packet = build_packet(&s, &Theme::light());

    let found = bitmaps(&packet.primitives);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, 5);
    assert_eq!(found[0].1, vec![[[40.0, 40.0], [60.0, 40.0], [60.0, 60.0], [40.0, 60.0]]]);
    assert!(!packet.primitives.iter().any(|p| matches!(p, Primitive::Image { .. })));
    let at = packet.primitives.iter().position(|p| matches!(p, Primitive::Bitmap { .. })).unwrap();
    let last_item = packet.primitives.iter().rposition(|p| matches!(p, Primitive::Fill { .. })).unwrap();
    assert!(at > last_item, "above the object's fill");
    let selection = Theme::light().selection;
    let mark = packet
        .primitives
        .iter()
        .position(|p| matches!(p, Primitive::Stroke { color, .. } if *color == selection))
        .unwrap();
    assert!(mark > at, "the selection outline stays visible on top of the bitmap");
}

#[test]
fn an_invisible_or_unusable_preview_produces_no_work() {
    let t = Theme::light();
    for opacity in [0.0, f32::NAN, -1.0] {
        let packet = build_packet(&scene(vec![], vec![preview(&[0, 255, 255, 0], opacity)]), &t);
        assert!(bitmaps(&packet.primitives).is_empty(), "opacity {opacity}");
    }
    let mut bad = quad((-10.0, 10.0), (10.0, -10.0));
    bad.bed_mm[2] = Point::new(f64::NAN, 0.0);
    let overlay = Overlay::ImagePreview { picture: gray_picture(1, 1, 1, &[0]), quads: vec![bad].into(), opacity: 1.0 };
    assert!(bitmaps(&build_packet(&scene(vec![], vec![overlay]), &t).primitives).is_empty());
}

fn context() -> Result<Option<GpuContext>, GpuError> {
    match GpuContext::headless() {
        Ok(ctx) => Ok(Some(ctx)),
        Err(GpuError::NoAdapter(e)) if std::env::var_os("BURNBURN_REQUIRE_GPU").is_none() => {
            eprintln!("SKIPPED (not a pass): no GPU adapter on this machine: {e}");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn close(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}

/// A 2 × 2 gray picture (black, white / white, black) over the pixel square 40..60, on top of an
/// opaque grey design rectangle of the same size. Texture sampling is linear and clamped at the
/// edge, so pixels in the outer 5 px ring of the picture sample exactly one texel (41 and 58).
#[test]
fn the_gpu_draws_the_preview_above_the_object_fill() {
    let Some(ctx) = context().unwrap() else { return };
    let mut theme = Theme::light();
    theme.grid = None;
    theme.design.fill = Some(Rgba::new(0.5, 0.5, 0.5, 1.0));
    theme.design.stroke = None;
    let s = scene(vec![rect_item(-10.0, -10.0, 20.0, 20.0)], vec![preview(&[0, 255, 255, 0], 1.0)]);

    let image = render_offscreen(&ctx, &build_packet(&s, &theme)).unwrap();

    assert!(close(image.pixel(41, 41).unwrap(), [0, 0, 0, 255], 2), "{:?}", image.pixel(41, 41));
    assert!(close(image.pixel(58, 41).unwrap(), [255, 255, 255, 255], 2), "{:?}", image.pixel(58, 41));
    assert!(close(image.pixel(41, 58).unwrap(), [255, 255, 255, 255], 2));
    assert!(close(image.pixel(58, 58).unwrap(), [0, 0, 0, 255], 2));
    assert!(close(image.pixel(20, 20).unwrap(), [255, 255, 255, 255], 0), "outside the picture: background");

    // Without the preview the object's grey shows: the bitmap is what covers it.
    let bare_scene = scene(vec![rect_item(-10.0, -10.0, 20.0, 20.0)], vec![]);
    let bare = render_offscreen(&ctx, &build_packet(&bare_scene, &theme)).unwrap();
    assert!(close(bare.pixel(41, 41).unwrap(), [128, 128, 128, 255], 2));
}
