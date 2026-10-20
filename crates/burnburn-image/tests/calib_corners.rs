//! W3-UI-camera: detecting the corners of the printed lens-calibration checkerboard in a camera
//! frame. Oracle: boards rendered by an independent forward model (known homography, known radial
//! lens, supersampled edges), so the true corner positions are exact answers; the last test
//! feeds detected corners of three rendered photos through `calibrate_lens` and expects the lens
//! that rendered them. No real camera or printed pattern was used.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_image::SourceImage;
use burnburn_image::calib::{
    CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat, CheckerGrid, CornerError, LensCalibrationRequest,
    LensFitParams, LensOrder, PatternView, calibrate_lens, detect_checkerboard,
};
use burnburn_types::{CancellationSource, CancellationToken, RequestId, StableId};

const W: u32 = 960;
const H: u32 = 540;
const COLS: usize = 9;
const ROWS: usize = 6;
const PITCH: f64 = 30.0;

/// Reference forward model: `p_d = c + (p_u − c)(1 + k1 r²)`, `r` on the half diagonal.
fn distort(u: [f64; 2], k1: f64) -> [f64; 2] {
    let c = [f64::from(W) / 2.0, f64::from(H) / 2.0];
    let radius = c[0].hypot(c[1]);
    let d = [u[0] - c[0], u[1] - c[1]];
    let r2 = (d[0] * d[0] + d[1] * d[1]) / (radius * radius);
    [c[0] + d[0] * (1.0 + k1 * r2), c[1] + d[1] * (1.0 + k1 * r2)]
}

fn undistort(p: [f64; 2], k1: f64) -> [f64; 2] {
    let c = [f64::from(W) / 2.0, f64::from(H) / 2.0];
    let radius = c[0].hypot(c[1]);
    let d = [p[0] - c[0], p[1] - c[1]];
    let mut u = d;
    for _ in 0..40 {
        let r2 = (u[0] * u[0] + u[1] * u[1]) / (radius * radius);
        u = [d[0] / (1.0 + k1 * r2), d[1] / (1.0 + k1 * r2)];
    }
    [c[0] + u[0], c[1] + u[1]]
}

type Mat = [[f64; 3]; 3];

fn project(g: &Mat, p: [f64; 2]) -> [f64; 2] {
    let w = g[2][0] * p[0] + g[2][1] * p[1] + g[2][2];
    [(g[0][0] * p[0] + g[0][1] * p[1] + g[0][2]) / w, (g[1][0] * p[0] + g[1][1] * p[1] + g[1][2]) / w]
}

fn inverse(g: &Mat) -> Mat {
    let det = g[0][0] * (g[1][1] * g[2][2] - g[1][2] * g[2][1]) - g[0][1] * (g[1][0] * g[2][2] - g[1][2] * g[2][0])
        + g[0][2] * (g[1][0] * g[2][1] - g[1][1] * g[2][0]);
    let c = |r: usize, s: usize| {
        let (r1, r2, s1, s2) = ((r + 1) % 3, (r + 2) % 3, (s + 1) % 3, (s + 2) % 3);
        g[r1][s1] * g[r2][s2] - g[r1][s2] * g[r2][s1]
    };
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (s, v) in row.iter_mut().enumerate() {
            *v = c(s, r) / det;
        }
    }
    out
}

/// Where the board's mm coordinates land in the undistorted image.
const POSE: Mat = [[2.4, 0.1, 120.0], [-0.05, 2.3, 100.0], [0.000_05, 0.000_03, 1.0]];

#[derive(Clone, Copy)]
struct Look {
    k1: f64,
    invert: bool,
    /// Deterministic pseudo-noise amplitude in grey levels.
    noise: u32,
}

const CLEAN: Look = Look { k1: 0.0, invert: false, noise: 0 };

fn board_grey(mm: [f64; 2], invert: bool) -> f64 {
    let (a, b) = ((mm[0] / PITCH).floor(), (mm[1] / PITCH).floor());
    let on_board = a >= -1.0 && a <= COLS as f64 - 1.0 && b >= -1.0 && b <= ROWS as f64 - 1.0;
    if !on_board {
        return 255.0;
    }
    let black = (a + b).rem_euclid(2.0) == 0.0;
    if black != invert { 20.0 } else { 235.0 }
}

fn render(pose: &Mat, look: Look) -> SourceImage {
    let inv = inverse(pose);
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    let mut seed = 0x2545_f491_u32;
    for y in 0..H {
        for x in 0..W {
            let sample = |sx: f64, sy: f64| {
                let u = undistort([f64::from(x) + sx, f64::from(y) + sy], look.k1);
                board_grey(project(&inv, u), look.invert)
            };
            // Flat areas take their grey at once; pixels on an edge get 4 × 4 samples, which puts
            // the edge position within 1/16 px.
            let corners = [sample(0.0, 0.0), sample(1.0, 0.0), sample(0.0, 1.0), sample(1.0, 1.0)];
            let mut grey = if corners.iter().all(|c| *c == corners[0]) && sample(0.5, 0.5) == corners[0] {
                corners[0]
            } else {
                let mut sum = 0.0;
                for i in 0..4 {
                    for j in 0..4 {
                        sum += sample((f64::from(i) + 0.5) / 4.0, (f64::from(j) + 0.5) / 4.0);
                    }
                }
                sum / 16.0
            };
            if look.noise > 0 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                grey += f64::from(seed % (2 * look.noise + 1)) - f64::from(look.noise);
            }
            let g = grey.round().clamp(0.0, 255.0) as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    SourceImage::from_rgba8(W, H, rgba).unwrap()
}

/// True corner positions in grid order (row-major), where the renderer put them.
fn truth(pose: &Mat, k1: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for row in 0..ROWS {
        for col in 0..COLS {
            out.push(distort(project(pose, [col as f64 * PITCH, row as f64 * PITCH]), k1));
        }
    }
    out
}

fn grid() -> CheckerGrid {
    CheckerGrid { cols: COLS, rows: ROWS, pitch_mm: PITCH }
}

fn detect(image: &SourceImage) -> Result<PatternView, CornerError> {
    detect_checkerboard(image, grid(), &CancellationToken::never())
}

fn worst_error(view: &PatternView, truth: &[[f64; 2]]) -> f64 {
    assert_eq!(view.points.len(), truth.len());
    view.points.iter().zip(truth).map(|(p, t)| (p.pixel[0] - t[0]).hypot(p.pixel[1] - t[1])).fold(0.0, f64::max)
}

#[test]
fn a_perspective_board_gives_every_corner_in_grid_order_within_a_third_of_a_pixel() {
    let image = render(&POSE, CLEAN);
    let view = detect(&image).unwrap();
    assert_eq!(view.points.len(), COLS * ROWS);
    assert!(worst_error(&view, &truth(&POSE, 0.0)) < 0.33, "{}", worst_error(&view, &truth(&POSE, 0.0)));
    // Physical positions follow the pitch, row by row.
    for (i, p) in view.points.iter().enumerate() {
        assert_eq!(p.grid_mm, [(i % COLS) as f64 * PITCH, (i / COLS) as f64 * PITCH]);
    }
}

#[test]
fn a_board_with_inverted_squares_is_found_at_the_same_places() {
    let image = render(&POSE, Look { invert: true, ..CLEAN });
    let view = detect(&image).unwrap();
    assert!(worst_error(&view, &truth(&POSE, 0.0)) < 0.33);
}

#[test]
fn a_board_turned_half_a_turn_is_returned_with_the_origin_at_the_top_left() {
    // The same lattice seen upside down: the board's far corner is now at the top left.
    let turned: Mat = [
        [-2.4, -0.1, 120.0 + 2.4 * 240.0 + 0.1 * 150.0],
        [0.05, -2.3, 100.0 + 2.3 * 150.0 - 0.05 * 240.0],
        [0.0, 0.0, 1.0],
    ];
    let image = render(&turned, CLEAN);
    let view = detect(&image).unwrap();
    let first = view.points.first().unwrap().pixel;
    let last = view.points.last().unwrap().pixel;
    assert!(first[0] + first[1] < last[0] + last[1], "origin at the top left: {first:?} vs {last:?}");
    // Whatever the labelling, every corner of the lattice was found exactly once.
    let lattice: Vec<[f64; 2]> =
        (0..ROWS * COLS).map(|i| project(&turned, [(i % COLS) as f64 * PITCH, (i / COLS) as f64 * PITCH])).collect();
    for p in &view.points {
        let nearest = lattice.iter().map(|t| (p.pixel[0] - t[0]).hypot(p.pixel[1] - t[1])).fold(f64::MAX, f64::min);
        assert!(nearest < 0.33, "{nearest}");
    }
}

#[test]
fn barrel_distortion_and_sensor_noise_keep_the_corners_within_half_a_pixel() {
    let look = Look { k1: -0.18, invert: false, noise: 6 };
    let image = render(&POSE, look);
    let view = detect(&image).unwrap();
    let worst = worst_error(&view, &truth(&POSE, -0.18));
    assert!(worst < 0.5, "{worst}");
}

#[test]
fn no_board_a_partial_board_or_the_wrong_size_is_a_typed_refusal() {
    let blank = SourceImage::from_rgba8(W, H, vec![200; (W * H * 4) as usize]).unwrap();
    assert_eq!(detect(&blank), Err(CornerError::NotFound { found: 0 }));

    // The board pushed so far right that its last columns are outside the picture.
    let partial: Mat = [[2.4, 0.1, 420.0], [-0.05, 2.3, 100.0], [0.000_05, 0.000_03, 1.0]];
    assert!(matches!(detect(&render(&partial, CLEAN)), Err(CornerError::NotFound { .. })));

    // A 9 × 6 board asked for as 10 × 6.
    let wrong = CheckerGrid { cols: COLS + 1, rows: ROWS, pitch_mm: PITCH };
    let result = detect_checkerboard(&render(&POSE, CLEAN), wrong, &CancellationToken::never());
    assert!(matches!(result, Err(CornerError::NotFound { .. })), "{result:?}");

    for bad in [
        CheckerGrid { cols: 1, rows: 4, pitch_mm: 10.0 },
        CheckerGrid { cols: 4, rows: 4, pitch_mm: 0.0 },
        CheckerGrid { cols: 4, rows: 4, pitch_mm: f64::NAN },
    ] {
        assert_eq!(detect_checkerboard(&blank, bad, &CancellationToken::never()), Err(CornerError::BadGrid));
    }
}

#[test]
fn a_cancelled_detection_returns_cancelled_and_no_corners() {
    let source = CancellationSource::new();
    source.cancel();
    let result = detect_checkerboard(&render(&POSE, CLEAN), grid(), &source.token());
    assert_eq!(result, Err(CornerError::Cancelled));
}

fn config() -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId("cam-A".into()),
        mode: CameraMode { width: W, height: H, pixel_format: CameraPixelFormat::Bgra8, frame_rate_mhz: 30_000 },
        lens: "standard".into(),
    }
}

/// Three photos of the board from different places, through a lens with k1 = −0.12: detect the
/// corners and fit the lens. The fit must find the lens that rendered the photos.
#[test]
fn corners_detected_in_rendered_photos_recover_the_lens_that_rendered_them() {
    let k1 = -0.12;
    let poses: [Mat; 3] = [
        [[2.4, 0.1, 40.0], [-0.05, 2.3, 60.0], [0.000_05, 0.000_03, 1.0]],
        [[2.3, -0.1, 400.0], [0.06, 2.2, 200.0], [-0.000_04, 0.000_06, 1.0]],
        [[2.5, 0.2, 200.0], [0.1, 2.2, 120.0], [0.000_03, -0.000_04, 1.0]],
    ];
    let views: Vec<PatternView> = poses
        .iter()
        .map(|pose| {
            for t in truth(pose, k1) {
                assert!(t[0] > 5.0 && t[0] < f64::from(W) - 5.0 && t[1] > 5.0 && t[1] < f64::from(H) - 5.0, "{t:?}");
            }
            detect(&render(pose, Look { k1, ..CLEAN })).unwrap()
        })
        .collect();
    let request = LensCalibrationRequest {
        request: RequestId::from_raw(1),
        config: config(),
        views,
        params: LensFitParams { order: LensOrder::K1, max_rms_px: 1.0, max_evaluations: 4_000 },
    };
    let lens = calibrate_lens(&request, &CancellationToken::never()).unwrap();
    assert!((lens.intrinsics.k1 - k1).abs() < 0.02, "k1 {}", lens.intrinsics.k1);
    assert!(lens.report.rms_px < 0.5, "rms {}", lens.report.rms_px);
}
