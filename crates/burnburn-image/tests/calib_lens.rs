//! W2-L5e lens calibration from a pattern. Oracle: a synthetic pattern photographed through a KNOWN
//! lens (the forward model implemented here, independently of the crate's inverse iteration) and KNOWN
//! per-view homographies, so the true coefficients and a zero noise-free residual are exact answers.
//! No real camera or printed pattern was used.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_image::calib::{
    CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat, LensCalibError, LensCalibrationRequest,
    LensFitParams, LensOrder, PatternPoint, PatternView, StaleResult, calibrate_lens,
};
use burnburn_types::{CancellationSource, CancellationToken, Cancelled, RequestId, StableId};

const W: f64 = 1920.0;
const H: f64 = 1080.0;
const COLS: usize = 13;
const ROWS: usize = 9;
const PITCH: f64 = 50.0;

fn config() -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId("cam-A".into()),
        mode: CameraMode { width: 1920, height: 1080, pixel_format: CameraPixelFormat::Bgra8, frame_rate_mhz: 30_000 },
        lens: "standard".into(),
    }
}

/// Reference forward model: `p_d = c + (p_u − c)(1 + k1 r² + k2 r⁴)`, `r` on the half diagonal.
fn distort(u: [f64; 2], k1: f64, k2: f64) -> [f64; 2] {
    let c = [W / 2.0, H / 2.0];
    let radius = c[0].hypot(c[1]);
    let d = [u[0] - c[0], u[1] - c[1]];
    let r2 = (d[0] * d[0] + d[1] * d[1]) / (radius * radius);
    let f = 1.0 + k1 * r2 + k2 * r2 * r2;
    [c[0] + d[0] * f, c[1] + d[1] * f]
}

fn project(g: &[[f64; 3]; 3], p: [f64; 2]) -> [f64; 2] {
    let w = g[2][0] * p[0] + g[2][1] * p[1] + g[2][2];
    [(g[0][0] * p[0] + g[0][1] * p[1] + g[0][2]) / w, (g[1][0] * p[0] + g[1][1] * p[1] + g[1][2]) / w]
}

/// Three photos of a 13 × 9 pattern (600 × 400 mm) from different positions.
const VIEWS: [[[f64; 3]; 3]; 3] = [
    [[2.9, 0.15, 110.0], [-0.08, 2.4, 60.0], [0.000_12, 0.000_05, 1.0]],
    [[2.7, -0.2, 150.0], [0.1, 2.2, 90.0], [-0.000_08, 0.000_15, 1.0]],
    [[2.5, 0.3, 200.0], [0.15, 2.25, 50.0], [0.000_05, -0.000_1, 1.0]],
];

/// Row-major corners, as a checkerboard detector reports them.
fn view_through(g: &[[f64; 3]; 3], k1: f64, k2: f64) -> PatternView {
    let mut points = Vec::new();
    for row in 0..ROWS {
        for col in 0..COLS {
            let grid_mm = [col as f64 * PITCH, row as f64 * PITCH];
            points.push(PatternPoint { pixel: distort(project(g, grid_mm), k1, k2), grid_mm });
        }
    }
    PatternView { points }
}

fn request(views: Vec<PatternView>, order: LensOrder) -> LensCalibrationRequest {
    LensCalibrationRequest {
        request: RequestId::from_raw(7),
        config: config(),
        views,
        params: LensFitParams { order, max_rms_px: 0.5, max_evaluations: 4_000 },
    }
}

fn run(req: &LensCalibrationRequest) -> Result<burnburn_image::calib::LensCalibration, LensCalibError> {
    calibrate_lens(req, &CancellationToken::never())
}

#[test]
fn lens_calibration_recovers_known_k1_and_k2_from_three_views() {
    let (k1, k2) = (-0.12, 0.03);
    let views: Vec<PatternView> = VIEWS.iter().map(|g| view_through(g, k1, k2)).collect();
    let cal = run(&request(views, LensOrder::K1K2)).unwrap();
    assert!((cal.intrinsics.k1 - k1).abs() < 1e-6, "k1 = {}", cal.intrinsics.k1);
    assert!((cal.intrinsics.k2 - k2).abs() < 1e-6, "k2 = {}", cal.intrinsics.k2);
    assert!(cal.report.rms_px < 1e-6, "{:?}", cal.report);
    assert!(cal.report.rms_plane_mm < 1e-6, "{:?}", cal.report);
    assert!(cal.report.converged);
    assert_eq!((cal.report.views, cal.report.points), (3, 3 * COLS * ROWS));
    assert_eq!(cal.intrinsics.center_px, [960.0, 540.0]);
    assert_eq!(cal.valid_for, config());
    assert_eq!(cal.request, RequestId::from_raw(7));
}

#[test]
fn a_single_view_is_enough_for_the_first_order_model() {
    // Plain scale + offset map: the answer is analytic, the lens is the only curvature in the image.
    let affine = [[2.5, 0.0, 210.0], [0.0, 2.5, 40.0], [0.0, 0.0, 1.0]];
    let k1 = 0.08;
    let cal = run(&request(vec![view_through(&affine, k1, 0.0)], LensOrder::K1)).unwrap();
    assert!((cal.intrinsics.k1 - k1).abs() < 1e-6, "k1 = {}", cal.intrinsics.k1);
    assert_eq!(cal.intrinsics.k2, 0.0, "first-order fit leaves k2 at zero");
    assert!(cal.report.rms_px < 1e-5, "{:?}", cal.report);
}

#[test]
fn an_ideal_lens_gives_zero_coefficients_and_zero_residual() {
    let affine = [[2.5, 0.0, 210.0], [0.0, 2.5, 40.0], [0.0, 0.0, 1.0]];
    for order in [LensOrder::K1, LensOrder::K1K2] {
        let cal = run(&request(vec![view_through(&affine, 0.0, 0.0)], order)).unwrap();
        assert!(cal.intrinsics.k1.abs() < 1e-6 && cal.intrinsics.k2.abs() < 1e-5, "{:?}", cal.intrinsics);
        assert!(cal.report.rms_px < 1e-6, "{:?}", cal.report);
    }
}

#[test]
fn bounded_pixel_noise_gives_a_bounded_coefficient_error_and_a_residual_of_the_noise_size() {
    let (k1, k2) = (-0.10, 0.02);
    let mut views: Vec<PatternView> = VIEWS.iter().map(|g| view_through(g, k1, k2)).collect();
    // Deterministic uniform noise in ±0.15 px (rms 0.0866 px): xorshift, no external randomness.
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64 * 0.3 - 0.15
    };
    for v in &mut views {
        for p in &mut v.points {
            p.pixel[0] += next();
            p.pixel[1] += next();
        }
    }
    let cal = run(&request(views, LensOrder::K1K2)).unwrap();
    assert!((cal.intrinsics.k1 - k1).abs() < 0.01, "k1 = {}", cal.intrinsics.k1);
    assert!((cal.intrinsics.k2 - k2).abs() < 0.02, "k2 = {}", cal.intrinsics.k2);
    assert!(cal.report.rms_px > 0.04 && cal.report.rms_px < 0.15, "{:?}", cal.report);
}

#[test]
fn the_fit_is_deterministic() {
    let views: Vec<PatternView> = VIEWS.iter().map(|g| view_through(g, -0.09, 0.01)).collect();
    let req = request(views, LensOrder::K1K2);
    assert_eq!(run(&req).unwrap(), run(&req).unwrap());
}

#[test]
fn bad_input_gives_a_typed_refusal_and_no_calibration() {
    let good = view_through(&VIEWS[0], -0.05, 0.0);
    let order = LensOrder::K1K2;

    assert_eq!(run(&request(vec![], order)), Err(LensCalibError::TooFewViews { need: 1, got: 0 }));

    let few = PatternView { points: good.points[..5].to_vec() };
    assert!(matches!(
        run(&request(vec![good.clone(), few], order)),
        Err(LensCalibError::TooFewPoints { view: 1, got: 5, .. })
    ));

    // Twelve corners on one line: no plane map can be fitted.
    let line = PatternView {
        points: (0..12)
            .map(|i| {
                let g = [i as f64 * PITCH, 0.0];
                PatternPoint { pixel: distort(project(&VIEWS[0], g), 0.0, 0.0), grid_mm: g }
            })
            .collect(),
    };
    assert_eq!(run(&request(vec![line], order)), Err(LensCalibError::Collinear { view: 0 }));

    let mut nan = good.clone();
    nan.points[3].pixel[0] = f64::NAN;
    assert_eq!(run(&request(vec![nan], order)), Err(LensCalibError::NonFinite));
    let mut inf = good.clone();
    inf.points[3].grid_mm[1] = f64::INFINITY;
    assert_eq!(run(&request(vec![inf], order)), Err(LensCalibError::NonFinite));

    let mut outside = good.clone();
    outside.points[0].pixel = [W + 5.0, 10.0];
    assert_eq!(run(&request(vec![outside], order)), Err(LensCalibError::OutsideImage { view: 0 }));

    let mut req = request(vec![good.clone()], order);
    req.config.mode.width = 0;
    assert_eq!(run(&req), Err(LensCalibError::BadMode));
    let mut req = request(vec![good.clone()], order);
    req.params.max_rms_px = f64::NAN;
    assert_eq!(run(&req), Err(LensCalibError::BadParams));
}

#[test]
fn a_pattern_that_covers_only_the_image_centre_cannot_pin_down_the_lens() {
    // 25 × 25 mm squares near the centre: radial distortion is invisible there.
    let small = [[0.5, 0.0, 860.0], [0.0, 0.5, 440.0], [0.0, 0.0, 1.0]];
    let result = run(&request(vec![view_through(&small, -0.1, 0.0)], LensOrder::K1));
    assert!(matches!(result, Err(LensCalibError::InsufficientCoverage { .. })), "{result:?}");
}

#[test]
fn a_misdetected_corner_exceeds_the_residual_threshold_instead_of_passing() {
    let mut views: Vec<PatternView> = VIEWS.iter().map(|g| view_through(g, -0.08, 0.0)).collect();
    views[1].points[40].pixel[0] += 60.0;
    views[1].points[40].pixel[1] -= 45.0;
    let mut req = request(views, LensOrder::K1);
    req.params.max_rms_px = 0.5;
    match run(&req) {
        Err(LensCalibError::ResidualTooHigh { rms_px, max_rms_px }) => {
            assert!(rms_px > 0.5 && max_rms_px == 0.5, "{rms_px}");
        }
        other => panic!("expected ResidualTooHigh, got {other:?}"),
    }
}

#[test]
fn an_iteration_budget_that_is_too_small_is_refused_not_truncated_silently() {
    let views: Vec<PatternView> = VIEWS.iter().map(|g| view_through(g, -0.08, 0.0)).collect();
    let mut req = request(views, LensOrder::K1K2);
    req.params.max_evaluations = 10;
    assert!(matches!(run(&req), Err(LensCalibError::BudgetExhausted { budget: 10, .. })));
    // The coarse search alone does not locate the minimum: the residual check refuses it.
    req.params.max_evaluations = 230;
    assert!(matches!(run(&req), Err(LensCalibError::ResidualTooHigh { .. })));
    // A budget that ends the refinement early still gives an honest result, flagged `converged = false`.
    req.params.max_evaluations = 300;
    let cal = run(&req).unwrap();
    assert!(!cal.report.converged && cal.report.evaluations == 300, "{:?}", cal.report);
    assert!(cal.report.rms_px < 1e-3 && (cal.intrinsics.k1 + 0.08).abs() < 1e-3, "{:?}", cal);
}

#[test]
fn a_cancelled_task_returns_cancelled_and_no_partial_calibration() {
    let views: Vec<PatternView> = VIEWS.iter().map(|g| view_through(g, -0.08, 0.0)).collect();
    let req = request(views, LensOrder::K1K2);
    let source = CancellationSource::new();
    source.cancel();
    assert_eq!(calibrate_lens(&req, &source.token()), Err(LensCalibError::Cancelled(Cancelled)));
}

#[test]
fn a_result_for_a_superseded_request_is_rejected() {
    let views: Vec<PatternView> = VIEWS.iter().map(|g| view_through(g, -0.08, 0.0)).collect();
    let cal = run(&request(views, LensOrder::K1)).unwrap();
    let newer = RequestId::from_raw(8);
    assert_eq!(cal.clone().accept_for(newer), Err(StaleResult { expected: newer, got: RequestId::from_raw(7) }));
    assert_eq!(cal.clone().accept_for(RequestId::from_raw(7)), Ok(cal));
}

#[test]
fn checkerboard_corners_get_their_physical_positions_from_the_pitch() {
    let corners: Vec<[f64; 2]> = (0..6).map(|i| [100.0 + i as f64, 200.0]).collect();
    let view = PatternView::from_grid(3, 2, 12.5, &corners).unwrap();
    let grid: Vec<[f64; 2]> = view.points.iter().map(|p| p.grid_mm).collect();
    assert_eq!(grid, vec![[0.0, 0.0], [12.5, 0.0], [25.0, 0.0], [0.0, 12.5], [12.5, 12.5], [25.0, 12.5]]);
    assert_eq!(view.points[4].pixel, [104.0, 200.0]);
    assert_eq!(PatternView::from_grid(3, 2, 12.5, &corners[..5]), Err(LensCalibError::BadGrid));
    assert_eq!(PatternView::from_grid(3, 2, 0.0, &corners), Err(LensCalibError::BadGrid));
    assert_eq!(PatternView::from_grid(1, 6, 12.5, &corners), Err(LensCalibError::BadGrid));
}
