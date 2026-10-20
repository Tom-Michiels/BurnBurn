//! The camera picture on the bed (W3-UI-camera): scene translation of `Overlay::BedImage` into an
//! image primitive in device pixels, and the real GPU pass that draws it beneath the design.
//!
//! Without a GPU adapter the GPU test prints `SKIPPED` and returns; that is not a pass. Set
//! `BURNBURN_REQUIRE_GPU=1` (macOS runners) to turn a missing adapter into a failure.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_editor::{BedPicture, BedQuad, Camera, Overlay, SceneItem, SceneSnapshot, StyleRole, Viewport};
use burnburn_geometry::{Affine, Path, Point, Rect};
use burnburn_render::gpu::{GpuContext, GpuError, render_offscreen};
use burnburn_render::{Primitive, Rgba, Theme, build_packet};
use burnburn_types::{DocumentId, GeometryId, Length, RevisionId, StableId};

fn picture(id: u64, w: u32, h: u32, rgba: Vec<u8>) -> Arc<BedPicture> {
    Arc::new(BedPicture::new(id, w, h, rgba.into()).unwrap())
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

fn images(p: &[Primitive]) -> Vec<(&BedPicture, Vec<[[f32; 2]; 4]>, f32)> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Image { picture, cells, opacity } => {
                Some((&**picture, cells.iter().map(|c| c.corners_px).collect(), *opacity))
            }
            _ => None,
        })
        .collect()
}

/// 1 mm = 1 px here. The picture covers x −10..10 mm, y 10..−10 mm (top to bottom), which is
/// 40..60 px on both axes with the y axis flipped; the texture coordinates travel unchanged.
#[test]
fn a_bed_image_is_translated_to_device_pixels_and_painted_before_everything_else() {
    let pic = picture(7, 1, 1, vec![9, 8, 7, 255]);
    let s = scene(
        vec![rect_item(0.0, 0.0, 5.0, 5.0)],
        vec![Overlay::BedImage {
            picture: Arc::clone(&pic),
            quads: vec![quad((-10.0, 10.0), (10.0, -10.0))].into(),
            opacity: 0.5,
        }],
    );

    let packet = build_packet(&s, &Theme::light());

    let found = images(&packet.primitives);
    assert_eq!(found.len(), 1);
    let (got, cells, opacity) = &found[0];
    assert_eq!((got.id, got.width, got.height), (7, 1, 1));
    assert_eq!(*opacity, 0.5);
    assert_eq!(cells, &vec![[[40.0, 40.0], [60.0, 40.0], [60.0, 60.0], [40.0, 60.0]]]);
    assert!(matches!(packet.primitives[0], Primitive::Image { .. }), "the picture is below grid and design");
    if let Primitive::Image { cells, .. } = &packet.primitives[0] {
        assert_eq!(cells[0].uv, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    }
}

/// A cell with a non-finite corner (a pixel beyond the horizon of the calibration) is dropped;
/// the others stay. A picture with no usable cell, or an opacity that is not a number or is 0,
/// is not drawn at all; an opacity above 1 is 1.
#[test]
fn unusable_cells_and_hidden_pictures_produce_no_work_for_the_gpu() {
    let pic = picture(1, 1, 1, vec![0, 0, 0, 255]);
    let mut bad = quad((0.0, 10.0), (10.0, 0.0));
    bad.bed_mm[2] = Point::new(f64::NAN, 0.0);
    let overlay = |quads: Vec<BedQuad>, opacity: f32| Overlay::BedImage {
        picture: Arc::clone(&pic),
        quads: quads.into(),
        opacity,
    };
    let t = Theme::light();

    let mixed = build_packet(&scene(vec![], vec![overlay(vec![bad, quad((-10.0, 10.0), (0.0, 0.0))], 2.0)]), &t);
    let found = images(&mixed.primitives);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.len(), 1, "only the finite cell");
    assert_eq!(found[0].2, 1.0, "opacity is clamped to 1");

    for hidden in [
        overlay(vec![bad], 1.0),
        overlay(vec![], 1.0),
        overlay(vec![quad((0.0, 1.0), (1.0, 0.0))], 0.0),
        overlay(vec![quad((0.0, 1.0), (1.0, 0.0))], f32::NAN),
    ] {
        let packet = build_packet(&scene(vec![], vec![hidden]), &t);
        assert!(images(&packet.primitives).is_empty());
    }
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

/// A 2 × 2 picture (red, green / blue, white) over the pixel square 40..60: each texel covers a
/// 10 × 10 px quadrant. Texture sampling is linear and clamped at the edge, so the pixels in the
/// outer 5 px ring of the picture sample exactly one texel (41 and 58 below). A design rectangle
/// (filled opaque grey) is drawn over the picture, and the clear colour shows outside it.
#[test]
fn the_gpu_draws_the_picture_beneath_the_design_with_the_requested_opacity() {
    let Some(ctx) = context().unwrap() else { return };
    let texels = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
    let overlay = |opacity| Overlay::BedImage {
        picture: picture(3, 2, 2, texels.clone()),
        quads: vec![quad((-10.0, 10.0), (10.0, -10.0))].into(),
        opacity,
    };
    let mut theme = Theme::light();
    theme.grid = None;
    theme.design.fill = Some(Rgba::new(0.5, 0.5, 0.5, 1.0));
    theme.design.stroke = None;

    // Opaque: the outer ring pixels are the texel colours; the design square (bed 0..4 mm, px 50..54 × 46..50) is grey.
    let opaque =
        render_offscreen(&ctx, &build_packet(&scene(vec![rect_item(0.0, 0.0, 4.0, 4.0)], vec![overlay(1.0)]), &theme))
            .unwrap();
    assert!(close(opaque.pixel(41, 41).unwrap(), [255, 0, 0, 255], 2), "{:?}", opaque.pixel(41, 41));
    assert!(close(opaque.pixel(58, 41).unwrap(), [0, 255, 0, 255], 2));
    assert!(close(opaque.pixel(41, 58).unwrap(), [0, 0, 255, 255], 2));
    assert!(close(opaque.pixel(58, 58).unwrap(), [255, 255, 255, 255], 2));
    assert!(close(opaque.pixel(52, 48).unwrap(), [128, 128, 128, 255], 2), "the design is above the picture");
    assert!(close(opaque.pixel(20, 20).unwrap(), [255, 255, 255, 255], 0), "outside the picture: background");

    // Half transparent: red over the white background is a light red.
    let half = render_offscreen(&ctx, &build_packet(&scene(vec![], vec![overlay(0.5)]), &theme)).unwrap();
    assert!(close(half.pixel(41, 41).unwrap(), [255, 128, 128, 255], 3), "{:?}", half.pixel(41, 41));
    assert!(close(half.pixel(58, 41).unwrap(), [128, 255, 128, 255], 3));
}
