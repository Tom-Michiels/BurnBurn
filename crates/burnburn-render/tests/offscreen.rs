//! Real GPU rendering (Metal on macOS) into an offscreen texture, read back and checked per pixel.
//!
//! Without a GPU adapter the test prints `SKIPPED` and returns; that is not a pass. Set
//! `BURNBURN_REQUIRE_GPU=1` (macOS runners) to turn a missing adapter into a failure.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_render::gpu::{GpuContext, GpuError, Image, render_offscreen};
use burnburn_render::{Primitive, RenderPacket, Rgba};

const WHITE: [u8; 4] = [255, 255, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

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

/// Channel tolerance: 8-bit unorm quantisation of exact 0.0/1.0 colours is exact, but drivers may
/// round a converted float by one step; anything beyond ±1 is a real colour error.
fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1)
}

fn count(image: &Image, color: [u8; 4], mut at: impl FnMut(u32, u32)) -> usize {
    let mut n = 0;
    for y in 0..image.height {
        for x in 0..image.width {
            if image.pixel(x, y).is_some_and(|p| close(p, color)) {
                at(x, y);
                n += 1;
            }
        }
    }
    n
}

/// Coverage is decided at pixel centres without multisampling. The red rectangle's edges lie on
/// pixel boundaries (10..30 × 10..20 px) so exactly 20 × 10 = 200 pixels are red. The blue 1 px
/// line runs on y = 40.5 from x 5.5 to 59.5; square caps make it cover 5..60 × 40..41, so exactly
/// 55 pixels of row 40 are blue and rows 39 and 41 stay white. Pixel counts are exact; only the
/// channel values have the ±1 tolerance above.
#[test]
fn offscreen_render_fills_rectangle_pixels() {
    let Some(ctx) = context().unwrap() else { return };
    if cfg!(target_os = "macos") {
        assert_eq!(ctx.adapter.get_info().backend, wgpu::Backend::Metal, "must be real Metal, not a fallback");
    }
    let packet = RenderPacket {
        size_px: [64, 48],
        clear: Rgba::new(1.0, 1.0, 1.0, 1.0),
        primitives: vec![
            Primitive::Fill {
                contours_px: vec![vec![[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]]],
                color: Rgba::new(1.0, 0.0, 0.0, 1.0),
            },
            Primitive::Stroke {
                points_px: vec![[5.5, 40.5], [59.5, 40.5]],
                closed: false,
                width_px: 1.0,
                color: Rgba::new(0.0, 0.0, 1.0, 1.0),
            },
        ],
    };

    let image = render_offscreen(&ctx, &packet).unwrap();

    assert_eq!((image.width, image.height, image.rgba.len()), (64, 48, 64 * 48 * 4));
    let reds = count(&image, RED, |x, y| assert!((10..30).contains(&x) && (10..20).contains(&y), "red at {x},{y}"));
    assert_eq!(reds, 200);
    let blues = count(&image, BLUE, |x, y| assert!((5..60).contains(&x) && y == 40, "blue at {x},{y}"));
    assert_eq!(blues, 55);
    assert_eq!(count(&image, WHITE, |_, _| {}), 64 * 48 - 200 - 55, "everything else is the clear colour");
}

/// Device loss is observable and turns rendering into a typed error, never a panic or a hang;
/// a new context renders again (the host's recovery path).
#[test]
fn lost_device_is_reported_and_a_new_context_renders() {
    let Some(ctx) = context().unwrap() else { return };
    assert!(!ctx.is_lost());
    ctx.device.destroy();
    let _ = ctx
        .device
        .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(5)) });
    assert!(ctx.is_lost(), "destroy() must be reported as device loss");
    let packet = RenderPacket { size_px: [8, 8], clear: Rgba::new(1.0, 1.0, 1.0, 1.0), primitives: Vec::new() };
    assert!(matches!(render_offscreen(&ctx, &packet), Err(GpuError::DeviceLost)));
    let fresh = context().unwrap().unwrap();
    let image = render_offscreen(&fresh, &packet).unwrap();
    assert_eq!(image.pixel(3, 3), Some(WHITE));
}

/// The whole scene pipeline headless: SceneSnapshot → build_packet → lyon → Metal → pixels. A
/// 20 × 10 mm rectangle at 1 pt/mm, scale 1, camera centred on (32, 24) mm in a 64 × 48 pt view
/// covers x 10..30 px and y 24..34 px (world y up): exactly 200 red pixels.
#[test]
fn scene_snapshot_renders_offscreen_to_exact_pixels() {
    use burnburn_editor::{Camera, SceneItem, SceneSnapshot, StyleRole, Viewport};
    use burnburn_geometry::{Affine, Path, Point, Rect};
    use burnburn_render::{Theme, build_packet};
    use burnburn_types::{DocumentId, GeometryId, Length, RevisionId, StableId};

    let Some(ctx) = context().unwrap() else { return };
    let rect =
        Rect::new(Point::new(10.0, 14.0), Length::from_mm(20.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    let scene = SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(32.0, 24.0), 1.0).unwrap(),
        viewport: Viewport::new(64.0, 48.0, 1.0).unwrap(),
        items: vec![SceneItem {
            source: GeometryId::from_raw(1),
            path: std::sync::Arc::new(Path::rectangle(rect)),
            transform: Affine::IDENTITY,
            role: StyleRole::Design,
        }],
        overlays: Vec::new(),
    };
    let mut theme = Theme::light();
    theme.design.fill = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
    theme.design.stroke = None;
    theme.grid = None;
    // These pixel expectations assume a white canvas and a filled object (LF2: the light theme now has a grey canvas and no fills).
    theme.background = Rgba::new(1.0, 1.0, 1.0, 1.0);
    let image = render_offscreen(&ctx, &build_packet(&scene, &theme)).unwrap();
    let reds = count(&image, RED, |x, y| assert!((10..30).contains(&x) && (24..34).contains(&y), "red at {x},{y}"));
    assert_eq!(reds, 200);
    assert_eq!(count(&image, WHITE, |_, _| {}), 64 * 48 - 200);
}
