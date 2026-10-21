//! Acceptance plan, machine-setup block: LIB01 (libraries and bundles), CA01/CA02 (camera
//! calibration across heights and sources), REG01 (Print and Cut registration), RO01 (rotary) and
//! MAT01 (material / interval tests). Synthetic ground truth throughout: a pinhole camera in closed
//! form, similarity transforms chosen by the test, circumferences with round numbers.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_cam;
mod acc_machine;
mod common;

use burnburn_cam::test_job::{
    IntervalTestSpec, MaterialTestSpec, TestAxis, TestCellMode, TestParameter, generate_interval_test,
    generate_material_test,
};
use burnburn_geometry::{Point, RegistrationError, RegistrationOptions, ScaleMode, register};
use burnburn_image::calib::{
    AlignmentLens, CalibError, CalibrationStore, CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat,
    Invalidity, LensModel, MarkObservation, MarkRole, StoreMiss, WorkspaceAlignment,
};
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_job::{RotaryConfig, RotaryKind};
use proptest::prelude::*;

// ---------------------------------------------------------------------------------------------
// CA01 / CA02 camera calibration: heights, tolerance, and never on another source
// ---------------------------------------------------------------------------------------------

fn config(device: &str, w: u32, h: u32, format: CameraPixelFormat, fps: u32, lens: &str) -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId(device.into()),
        mode: CameraMode { width: w, height: h, pixel_format: format, frame_rate_mhz: fps },
        lens: lens.into(),
    }
}

fn cam_a() -> CameraConfigKey {
    config("cam-A", 1920, 1080, CameraPixelFormat::Bgra8, 30_000, "standard")
}

/// Pinhole camera at (300, 200, 900) mm looking straight down, focal length 1500 px at 1920 px width.
struct Pinhole {
    scale: f64,
    width: f64,
    height: f64,
}

impl Pinhole {
    fn for_config(cfg: &CameraConfigKey) -> Self {
        let w = f64::from(cfg.mode.width);
        Pinhole { scale: w / 1920.0, width: w, height: f64::from(cfg.mode.height) }
    }
    fn pixel(&self, bed: [f64; 2], z: f64) -> [f64; 2] {
        let (depth, f) = (900.0 - z, 1500.0 * self.scale);
        [self.width / 2.0 + f * (bed[0] - 300.0) / depth, self.height / 2.0 + f * (bed[1] - 200.0) / depth]
    }
    fn bed(&self, pixel: [f64; 2], z: f64) -> [f64; 2] {
        let (depth, f) = (900.0 - z, 1500.0 * self.scale);
        [300.0 + (pixel[0] - self.width / 2.0) * depth / f, 200.0 + (pixel[1] - self.height / 2.0) * depth / f]
    }
}

/// Deterministic pseudo-noise in `[-amp, amp]` (no RNG dependency).
fn noise(seed: u64, amp: f64) -> f64 {
    let x = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    ((x >> 33) as f64 / f64::from(1u32 << 31) - 1.0) * amp
}

fn noisy_marks(cam: &Pinhole, z: f64, amp_px: f64) -> Vec<MarkObservation> {
    let mut obs = Vec::new();
    let mut k = 0;
    for x in [0.0, 200.0, 400.0, 600.0] {
        for y in [0.0, 200.0, 400.0] {
            let p = cam.pixel([x, y], z);
            k += 2;
            let pixel = [p[0] + noise(k, amp_px), p[1] + noise(k + 1, amp_px)];
            obs.push(MarkObservation { pixel, bed_mm: [x, y], role: MarkRole::Fit });
        }
    }
    for bed in [[150.0, 100.0], [450.0, 300.0], [550.0, 50.0]] {
        obs.push(MarkObservation { pixel: cam.pixel(bed, z), bed_mm: bed, role: MarkRole::Check });
    }
    obs
}

fn align(cfg: &CameraConfigKey, z: f64, amp_px: f64) -> Result<WorkspaceAlignment, burnburn_image::calib::AlignError> {
    let cam = Pinhole::for_config(cfg);
    WorkspaceAlignment::align(
        &noisy_marks(&cam, z, amp_px),
        cfg.clone(),
        AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }),
        z,
        0.5,
    )
}

#[test]
fn ca01_positioning_error_stays_inside_the_chosen_tolerance_at_every_calibrated_and_interpolated_height() {
    const TOLERANCE_MM: f64 = 0.5;
    let cfg = cam_a();
    let cam = Pinhole::for_config(&cfg);
    let mut store = CalibrationStore::default();
    for z in [0.0, 10.0, 25.0] {
        let a = align(&cfg, z, 0.3).unwrap(); // +-0.3 px of detection noise on the fit marks
        assert!(a.calibration.residuals_mm.max_mm <= TOLERANCE_MM, "z {z}: {:?}", a.calibration.residuals_mm);
        store.insert_alignment(a);
    }
    // Held-out positions at calibrated heights and in between (linear interpolation along the ray).
    let mut worst = 0.0_f64;
    for z in [0.0, 4.0, 10.0, 17.5, 25.0] {
        for bed in [[75.0, 25.0], [310.0, 190.0], [505.0, 360.0], [20.0, 380.0]] {
            let pixel = cam.pixel(bed, z);
            let got = store.pixel_to_bed_at_height(&cfg, pixel, z).unwrap();
            let truth = cam.bed(pixel, z);
            worst = worst.max(((got[0] - truth[0]).powi(2) + (got[1] - truth[1]).powi(2)).sqrt());
        }
    }
    assert!(worst <= TOLERANCE_MM, "measured positioning error {worst} mm exceeds the tolerance {TOLERANCE_MM} mm");
    // Without noise the error collapses to rounding: the bound above is not slack in the model.
    let exact = align(&cfg, 10.0, 0.0).unwrap();
    assert!(exact.calibration.residuals_mm.max_mm < 1e-6);
}

#[test]
fn ca01_a_mark_that_contradicts_the_others_makes_no_calibration_instead_of_a_bad_one() {
    let cfg = cam_a();
    let cam = Pinhole::for_config(&cfg);
    let mut obs = noisy_marks(&cam, 0.0, 0.0);
    obs[3].pixel[0] += 12.0; // a mis-clicked mark, ~7 mm on the bed
    let r = WorkspaceAlignment::align(&obs, cfg, AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }), 0.0, 0.5);
    assert!(matches!(r, Err(burnburn_image::calib::AlignError::Calibration(CalibError::Inconsistent { .. }))), "{r:?}");
}

#[test]
fn ca02_every_single_change_of_the_camera_configuration_refuses_the_stored_calibration() {
    let base = cam_a();
    let mut store = CalibrationStore::default();
    store.insert_alignment(align(&base, 0.0, 0.0).unwrap());
    let cases: Vec<(&str, CameraConfigKey, StoreMiss)> = vec![
        (
            "resolution",
            config("cam-A", 1280, 720, CameraPixelFormat::Bgra8, 30_000, "standard"),
            StoreMiss::Invalid(Invalidity::ResolutionChanged),
        ),
        (
            "pixel format",
            config("cam-A", 1920, 1080, CameraPixelFormat::Yuv420Video, 30_000, "standard"),
            StoreMiss::Invalid(Invalidity::PixelFormatChanged),
        ),
        (
            "frame rate",
            config("cam-A", 1920, 1080, CameraPixelFormat::Bgra8, 60_000, "standard"),
            StoreMiss::Invalid(Invalidity::FrameRateChanged),
        ),
        (
            "lens profile",
            config("cam-A", 1920, 1080, CameraPixelFormat::Bgra8, 30_000, "wide"),
            StoreMiss::Invalid(Invalidity::LensChanged),
        ),
        ("source", config("cam-B", 1920, 1080, CameraPixelFormat::Bgra8, 30_000, "standard"), StoreMiss::NotCalibrated),
    ];
    for (what, cfg, miss) in cases {
        assert_eq!(store.alignment_for(&cfg, 0.0), Err(miss), "{what}");
    }
    // The alignment itself also refuses to map for another configuration (no silent reuse by a caller
    // that kept a reference).
    let a = store.alignment_for(&base, 0.0).unwrap();
    let other = config("cam-B", 1920, 1080, CameraPixelFormat::Bgra8, 30_000, "standard");
    assert_eq!(a.pixel_to_bed(&other, [960.0, 540.0]), Err(Invalidity::DeviceChanged));
    assert_eq!(a.bed_to_pixel(&other, [100.0, 100.0]), Err(Invalidity::DeviceChanged));
    // And the exact configuration still works.
    assert!(a.pixel_to_bed(&base, [960.0, 540.0]).is_ok());
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

    /// A stored calibration is returned for exactly one configuration: whichever fields differ,
    /// the answer is a miss unless they are all equal.
    #[test]
    fn ca02_property_only_the_identical_configuration_hits(
        w in prop_oneof![Just(1920_u32), Just(1280)], fmt in 0_u8..2, fps in prop_oneof![Just(15_000_u32), Just(30_000)],
        dev in 0_u8..2, lens in 0_u8..2,
    ) {
        let base = cam_a();
        let mut store = CalibrationStore::default();
        store.insert_alignment(align(&base, 0.0, 0.0).unwrap());
        let h = if w == 1920 { 1080 } else { 720 };
        let cfg = config(
            if dev == 0 { "cam-A" } else { "cam-B" }, w, h,
            if fmt == 0 { CameraPixelFormat::Bgra8 } else { CameraPixelFormat::Yuv420Video }, fps,
            if lens == 0 { "standard" } else { "wide" },
        );
        let hit = store.alignment_for(&cfg, 0.0).is_ok();
        prop_assert_eq!(hit, cfg == base);
    }
}

// ---------------------------------------------------------------------------------------------
// REG01 registration: a known transform is found, near-coincident targets are refused
// ---------------------------------------------------------------------------------------------

fn similarity(theta_deg: f64, s: f64, t: (f64, f64), p: Point) -> Point {
    let (sin, cos) = theta_deg.to_radians().sin_cos();
    Point::new(s * (cos * p.x - sin * p.y) + t.0, s * (sin * p.x + cos * p.y) + t.1)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn reg01_property_the_known_rotation_scale_and_offset_are_found(
        theta in -179.0_f64..179.0, s in 0.9_f64..1.1, tx in -500.0_f64..500.0, ty in -300.0_f64..300.0,
        x1 in 5.0_f64..200.0, y1 in -100.0_f64..100.0, extra in any::<bool>(),
    ) {
        let mut design = vec![Point::new(0.0, 0.0), Point::new(x1, y1)];
        if extra { design.push(Point::new(x1 / 2.0 + 30.0, y1 / 2.0 - 40.0)); }
        let measured: Vec<Point> = design.iter().map(|p| similarity(theta, s, (tx, ty), *p)).collect();
        let free = RegistrationOptions { scale: ScaleMode::Uniform { max_deviation: 0.2 }, min_separation: 1.0 };
        let r = register(&design, &measured, &free).unwrap();
        prop_assert!((r.scale - s).abs() < 1e-9, "scale {} vs {s}", r.scale);
        let mut d = (r.rotation.degrees() - theta).rem_euclid(360.0);
        if d > 180.0 { d -= 360.0; }
        prop_assert!(d.abs() < 1e-7, "rotation {} vs {theta}", r.rotation.degrees());
        prop_assert!(r.max_residual() < 1e-7);
        for (dp, mp) in design.iter().zip(&measured) {
            prop_assert!(r.transform.apply(*dp).distance(*mp) < 1e-7);
        }
        // Without scaling, a true scale of s != 1 is not absorbed: it shows up as residual.
        let fixed = RegistrationOptions { scale: ScaleMode::Fixed, min_separation: 1.0 };
        let f = register(&design, &measured, &fixed).unwrap();
        prop_assert_eq!(f.scale, 1.0);
        if (s - 1.0).abs() > 0.01 { prop_assert!(f.max_residual() > 1e-3); }
    }
}

#[test]
fn reg01_targets_that_are_near_coincident_on_either_side_are_refused() {
    let opts = RegistrationOptions { scale: ScaleMode::Fixed, min_separation: 5.0 };
    let far = [Point::new(0.0, 0.0), Point::new(100.0, 0.0)];
    let close = [Point::new(0.0, 0.0), Point::new(4.99, 0.0)];
    assert_eq!(register(&close, &far, &opts).unwrap_err(), RegistrationError::DesignTargetsTooClose);
    assert_eq!(register(&far, &close, &opts).unwrap_err(), RegistrationError::MeasuredTargetsTooClose);
    assert!(
        register(&[Point::new(0.0, 0.0), Point::new(5.0, 0.0)], &[Point::new(1.0, 1.0), Point::new(6.0, 1.0)], &opts)
            .is_ok()
    );
    assert_eq!(register(&far[..1], &far[..1], &opts).unwrap_err(), RegistrationError::TooFewTargets);
}

// REG01 through the app (typed measurements; no scale, scale, reset).
mod reg_app {
    use super::*;
    use acc_machine::rig;
    use burnburn_job::MoveKind;
    use burnburn_presentation::{CommandKey, PrintAndCutIntent, PrintCutScale, SchikkenIntent, UiIntent};

    fn pc(r: &mut acc_machine::Rig, i: PrintAndCutIntent) {
        r.h.send(UiIntent::Schikken(SchikkenIntent::PrintAndCut(i)));
    }

    fn bounds(r: &acc_machine::Rig) -> ((f64, f64), (f64, f64)) {
        let plan = r.h.document(r.doc).unwrap().plan.expect("plan");
        let pts: Vec<(f64, f64)> = plan
            .preview
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Cut)
            .flat_map(|m| [(m.from.x_mm(), m.from.y_mm()), (m.to.x_mm(), m.to.y_mm())])
            .collect();
        let min = pts.iter().fold((f64::MAX, f64::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
        let max = pts.iter().fold((f64::MIN, f64::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
        (min, max)
    }

    fn compile(r: &mut acc_machine::Rig) {
        r.h.invoke(r.doc, CommandKey::Compile);
        r.h.run_tasks();
    }

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    fn targets(r: &mut acc_machine::Rig) {
        let doc = r.doc;
        r.h.invoke(doc, CommandKey::PrintAndCut);
        for (i, (x, y)) in [("10", "10"), ("130", "10")].iter().enumerate() {
            pc(r, PrintAndCutIntent::SetTarget { document: doc, index: i, x: (*x).into(), y: (*y).into() });
        }
    }

    fn measured(r: &mut acc_machine::Rig, a: (&str, &str), b: (&str, &str)) {
        let doc = r.doc;
        pc(r, PrintAndCutIntent::SetMeasured { document: doc, index: 0, x: a.0.into(), y: a.1.into() });
        pc(r, PrintAndCutIntent::SetMeasured { document: doc, index: 1, x: b.0.into(), y: b.1.into() });
    }

    #[test]
    fn reg01_the_found_transform_moves_the_next_compile_scale_is_opt_in_and_reset_undoes_it() {
        let mut r = rig();
        let doc = r.doc;
        targets(&mut r);
        // The sheet lies a quarter turn: marks found 120 apart upwards.
        measured(&mut r, ("200", "40"), ("200", "160"));
        pc(&mut r, PrintAndCutIntent::Register { document: doc });
        let res = r.h.document(doc).unwrap().schikken.print_and_cut.unwrap().result.expect("registered");
        assert!((res.rotation_deg - 90.0).abs() < 1e-9 && (res.scale - 1.0).abs() < 1e-12 && res.rms_mm < 1e-9);
        compile(&mut r);
        let (min, max) = bounds(&r);
        assert!(near(min, (120.0, 40.0)) && near(max, (200.0, 160.0)), "{min:?} {max:?}");

        // A 5 % too big print: refused as a scale by default (shows up as residuals), taken when allowed.
        measured(&mut r, ("200", "40"), ("200", "166"));
        pc(&mut r, PrintAndCutIntent::Register { document: doc });
        let plain = r.h.document(doc).unwrap().schikken.print_and_cut.unwrap().result.expect("registered");
        assert!((plain.scale - 1.0).abs() < 1e-12 && plain.rms_mm > 1.0, "{plain:?}");
        pc(
            &mut r,
            PrintAndCutIntent::SetScale { document: doc, scale: PrintCutScale::Uniform, max_deviation: "0,1".into() },
        );
        pc(&mut r, PrintAndCutIntent::Register { document: doc });
        let scaled = r.h.document(doc).unwrap().schikken.print_and_cut.unwrap().result.expect("registered");
        assert!((scaled.scale - 1.05).abs() < 1e-9 && scaled.rms_mm < 1e-9, "{scaled:?}");

        // Reset: the next compile is the plain one again.
        pc(&mut r, PrintAndCutIntent::Reset { document: doc });
        compile(&mut r);
        let (min, max) = bounds(&r);
        assert!(near(min, (10.0, 10.0)) && near(max, (130.0, 90.0)), "{min:?} {max:?}");
    }

    #[test]
    fn reg01_targets_on_top_of_each_other_are_refused_and_nothing_is_registered() {
        let mut r = rig();
        let doc = r.doc;
        targets(&mut r);
        measured(&mut r, ("200", "40"), ("200.0001", "40"));
        pc(&mut r, PrintAndCutIntent::Register { document: doc });
        let vm = r.h.document(doc).unwrap().schikken.print_and_cut.unwrap();
        assert!(vm.result.is_none() && vm.error.is_some(), "{:?}", vm.error);
        compile(&mut r);
        let (min, _) = bounds(&r);
        assert!(near(min, (10.0, 10.0)), "a refused registration leaves the design where it was");
    }
}

// ---------------------------------------------------------------------------------------------
// RO01 rotary: circumference, mirror, diameters and the switch back to the flat bed
// ---------------------------------------------------------------------------------------------

fn rotary_rd_y_extent(rotary: Option<RotaryConfig>, y0: f64, h: f64) -> (f64, f64) {
    use burnburn_cam::compile;
    let mut s = acc_cam::session();
    let g = acc_cam::rectangle(&mut s, 50.0, y0, 40.0, h);
    acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 20.0, 50.0), vec![g]);
    let profile = match rotary {
        Some(r) => common::profile().with_rotary(r).unwrap(),
        None => common::profile(),
    };
    let plan =
        compile(&s.snapshot(), &profile, &common::options(), &burnburn_types::CancellationToken::never()).unwrap();
    let rd = common::decode(&common::rd_bytes(&plan));
    let (lo, hi) = common::cut_extent(&common::cuts(&rd));
    (lo.y, hi.y)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// The RD's Y motion is surface millimetres times steps / (circumference x steps-per-mm); a
    /// mirrored rotary turns the same object the other way; a roller of the same circumference
    /// behaves like a chuck of the same circumference.
    #[test]
    fn ro01_property_circumference_scale_and_mirror(
        d in 20.0_f64..250.0, steps in 2000_u32..20000, per_mm in 5.0_f64..100.0, mirror in any::<bool>(),
        y_frac in 0.05_f64..0.6, h_frac in 0.05_f64..0.3,
    ) {
        let c = std::f64::consts::PI * d;
        let (y0, h) = (y_frac * c, h_frac * c);
        let chuck = RotaryConfig::new(RotaryKind::Chuck { object_diameter_mm: d }, steps, per_mm, mirror).unwrap();
        let scale = f64::from(steps) / (c * per_mm);
        let (lo, hi) = rotary_rd_y_extent(Some(chuck), y0, h);
        let (exp_lo, exp_hi) = if mirror { ((c - (y0 + h)) * scale, (c - y0) * scale) } else { (y0 * scale, (y0 + h) * scale) };
        let tol = 0.002 + 0.001 * scale;
        prop_assert!((lo - exp_lo).abs() < tol && (hi - exp_hi).abs() < tol, "[{lo}, {hi}] vs [{exp_lo}, {exp_hi}] scale {scale}");
        let roller = RotaryConfig::new(RotaryKind::Roller { roller_diameter_mm: d }, steps, per_mm, mirror).unwrap();
        let (rlo, rhi) = rotary_rd_y_extent(Some(roller), y0, h);
        if mirror {
            // A roller feeds a long object through the unchanged 900 mm bed: it mirrors about that bed.
            let (e_lo, e_hi) = ((900.0 - (y0 + h)) * scale, (900.0 - y0) * scale);
            prop_assert!((rlo - e_lo).abs() < tol && (rhi - e_hi).abs() < tol, "roller [{rlo}, {rhi}] vs [{e_lo}, {e_hi}]");
        } else {
            prop_assert!((rlo - lo).abs() < 1e-9 && (rhi - hi).abs() < 1e-9, "an unmirrored roller of the same circumference is the same motion");
        }
    }
}

#[test]
fn ro01_switching_back_to_the_flat_bed_restores_the_exact_original_output() {
    let mut s = acc_cam::session();
    let g = acc_cam::rectangle(&mut s, 50.0, 20.0, 40.0, 30.0);
    acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 20.0, 50.0), vec![g]);
    let never = burnburn_types::CancellationToken::never();
    let flat = burnburn_cam::compile(&s.snapshot(), &common::profile(), &common::options(), &never).unwrap();
    let flat_rd = common::rd_bytes(&flat);
    let rotary = RotaryConfig::new(RotaryKind::Chuck { object_diameter_mm: 100.0 }, 8000, 20.0, false).unwrap();
    let on = common::profile().with_rotary(rotary).unwrap();
    let plan_on = burnburn_cam::compile(&s.snapshot(), &on, &common::options(), &never).unwrap();
    assert_ne!(plan_on.id(), flat.id());
    assert_ne!(common::rd_bytes(&plan_on), flat_rd, "the rotary job differs from the flat one");
    let back = on.without_rotary();
    let plan_back = burnburn_cam::compile(&s.snapshot(), &back, &common::options(), &never).unwrap();
    assert_eq!(plan_back.id(), flat.id());
    assert_eq!(common::rd_bytes(&plan_back), flat_rd, "byte-identical RD after returning to the flat bed");
}

// ---------------------------------------------------------------------------------------------
// MAT01 material / interval tests
// ---------------------------------------------------------------------------------------------

fn linspace(min: f64, max: f64, n: u32) -> Vec<f64> {
    if n == 1 { vec![min] } else { (0..n).map(|i| min + (max - min) * f64::from(i) / f64::from(n - 1)).collect() }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn mat01_property_cells_carry_exactly_their_axis_values_and_the_source_document_is_untouched(
        s_min in 10.0_f64..200.0, s_span in 0.0_f64..300.0, cols in 1_u32..6,
        p_min in 1.0_f64..40.0, p_span in 0.0_f64..50.0, rows in 1_u32..5,
    ) {
        let base = acc_cam::settings(50.0, 20.0);
        let spec = MaterialTestSpec {
            mode: TestCellMode::Line,
            columns: TestAxis { parameter: TestParameter::Speed, min: s_min, max: s_min + s_span, count: cols },
            rows: Some(TestAxis { parameter: TestParameter::Power, min: p_min, max: p_min + p_span, count: rows }),
            base,
            cell_width: acc_cam::mm(20.0), cell_height: acc_cam::mm(10.0), gap: acc_cam::mm(2.0),
            labels: None, border: None,
        };
        // A running document with its own settings that must not change.
        let mut doc = acc_cam::session();
        let g = acc_cam::rectangle(&mut doc, 0.0, 0.0, 10.0, 10.0);
        acc_cam::add_op(&mut doc, acc_cam::line_op("Snijden", 10.0, 50.0), vec![g]);
        let before = burnburn_cam::source_hash(&doc.snapshot()).unwrap();
        let job = generate_material_test(&spec).unwrap();
        prop_assert_eq!(burnburn_cam::source_hash(&doc.snapshot()).unwrap(), before);
        prop_assert_eq!(job.cells().len() as u32, cols * rows);
        let speeds = linspace(s_min, s_min + s_span, cols);
        let powers = linspace(p_min, p_min + p_span, rows);
        for c in job.cells() {
            let op = job.snapshot().process().operation(c.operation).unwrap();
            let st = op.settings();
            prop_assert!((st.speed.get_mm_per_s() - speeds[c.column as usize]).abs() < 1e-9);
            prop_assert!((st.power.max().get() - powers[c.row as usize]).abs() < 1e-9);
            // Everything else is the base setting.
            prop_assert_eq!(st.passes, base.passes);
        }
        // The RD carries one speed record per cell setting: the test reaches the controller as the cells say.
        let plan = job.compile(&common::profile(), &common::options(), &burnburn_types::CancellationToken::never()).unwrap();
        let rd = common::decode(&common::rd_bytes(&plan));
        let mut in_rd: Vec<u64> = rd.commands().filter_map(|c| match c {
            burnburn_ruida::Command::Speed(s) => Some(s.um_per_s()),
            _ => None,
        }).collect();
        in_rd.dedup();
        let mut expected: Vec<u64> = job.cells().iter().map(|c| (c.column_value * 1000.0).round() as u64).collect();
        expected.dedup();
        for e in &expected {
            prop_assert!(in_rd.contains(e) || in_rd.iter().any(|v| v.abs_diff(*e) <= 1), "speed {e} um/s missing from the RD: {in_rd:?}");
        }
    }
}

#[test]
fn mat01_an_interval_test_scans_every_cell_at_its_own_interval() {
    let spec = IntervalTestSpec {
        min_interval: acc_cam::mm(0.1),
        max_interval: acc_cam::mm(0.4),
        count: 4,
        overscan: acc_cam::mm(0.0),
        base: acc_cam::settings(100.0, 20.0),
        cell_width: acc_cam::mm(20.0),
        cell_height: acc_cam::mm(8.0),
        gap: acc_cam::mm(2.0),
        labels: None,
        border: None,
    };
    let job = generate_interval_test(&spec).unwrap();
    let plan =
        job.compile(&common::profile(), &common::options(), &burnburn_types::CancellationToken::never()).unwrap();
    // Per cell (20 x 8 mm): scan length = area / interval = 160 / interval.
    let mut lengths = Vec::new();
    let mut current = None;
    for e in plan.events() {
        match *e {
            burnburn_job::PlanEvent::OperationBoundary { .. } => {
                current = Some(lengths.len());
                lengths.push(0.0);
            }
            burnburn_job::PlanEvent::ScanRun { line, .. } => {
                if let Some(i) = current {
                    lengths[i] += line.from.distance(line.to);
                }
            }
            _ => {}
        }
    }
    let expected: Vec<f64> = [0.1, 0.2, 0.3, 0.4].iter().map(|i| 160.0 / i).collect();
    assert_eq!(lengths.len(), 4);
    for (got, want) in lengths.iter().zip(expected) {
        // rows lie at half-interval centres: the row count is floor(8 / interval) (+- one row when the
        // interval does not divide 8 mm), each row 20 mm long.
        assert!((got - want).abs() <= 20.0 + 1e-6, "scan length {got} vs {want}");
    }
}

// ---------------------------------------------------------------------------------------------
// LIB01 libraries: a move or a bundle keeps references and preset values valid
// ---------------------------------------------------------------------------------------------

mod lib01 {
    use std::path::Path;

    use burnburn_document::process::{
        Kerf, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, RecipeDraft, Speed,
    };
    use burnburn_io::art_library::{ArtFormat, ArtLibrary, NewArt, load_art_library, save_art_library};
    use burnburn_io::bundle::{
        BundleContents, ImportChoices, ImportTarget, LocalState, apply_import, decode_bundle, encode_bundle,
        preview_import,
    };
    use burnburn_io::material_library::{
        LinkStatus, MaterialLibrary, link_for, link_status, load_material_library, refresh_command,
        save_material_library,
    };
    use burnburn_types::Length;

    use super::*;

    const TRI: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20"><path d="M10 0 L20 20 L0 20 Z"/></svg>"#;

    fn recipe(material: &str, speed: f64, power: f64) -> RecipeDraft {
        let p = PowerPercent::new(power).unwrap();
        RecipeDraft {
            name: format!("{material} 3"),
            material: material.into(),
            thickness: Length::from_mm(3.0).unwrap(),
            machine: "omtech-60w".into(),
            notes: "gemeten".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(speed).unwrap(),
                power: PowerRange::new(p, p).unwrap(),
                passes: Passes::new(2).unwrap(),
                air_assist: true,
            },
        }
    }

    fn arts() -> ArtLibrary {
        let mut lib = ArtLibrary::new("kunst");
        lib.add_group("Vormen").unwrap();
        lib.add_group("Archief").unwrap();
        lib.add_item(NewArt {
            name: "Driehoek",
            group: "Vormen",
            tags: vec!["basis".into()],
            format: ArtFormat::Svg,
            bytes: TRI.as_bytes(),
            dxf_scale: None,
        })
        .unwrap();
        lib
    }

    #[test]
    fn lib01_moving_the_library_files_keeps_entries_links_and_placed_geometry_valid() {
        let fs = MemoryFileSystem::new();
        fs.create_dir_all_for_test(Path::new("/oud"));
        fs.create_dir_all_for_test(Path::new("/nieuw"));
        let mut mats = MaterialLibrary::new("werkplaats");
        let entry = mats.add(recipe("Berken", 25.0, 62.5)).unwrap();
        let art = arts();
        let art_id = art.items()[0].id();
        let placed_before = art.place(art_id).unwrap();

        // A project recipe copied from the library entry, with its link.
        let mut s = acc_cam::session();
        let out = acc_cam::run(
            &mut s,
            burnburn_document::CommandPayload::Process(mats.get(entry).unwrap().add_recipe_command()),
        );
        let recipe_id = out.changes.recipes[0];
        let link = link_for(&mats, entry, recipe_id).unwrap();
        assert_eq!(link_status(&link, &mats), LinkStatus::UpToDate);

        // "Move" = save at the old path, copy the bytes to a new place, delete the old file.
        save_material_library(&fs, Path::new("/oud/materiaal.bblib"), &mats).unwrap();
        save_art_library(&fs, Path::new("/oud/kunst.bbart"), &art).unwrap();
        for (from, to) in
            [("/oud/materiaal.bblib", "/nieuw/materiaal.bblib"), ("/oud/kunst.bbart", "/nieuw/kunst.bbart")]
        {
            let bytes = fs.read(Path::new(from), u64::MAX).unwrap();
            let temp = fs.create_temp_in(Path::new("/nieuw"), ".move").unwrap();
            fs.write_all(&temp, &bytes).unwrap();
            fs.replace(&temp, Path::new(to)).unwrap();
            fs.remove_file(Path::new(from)).unwrap();
        }
        let moved = load_material_library(&fs, Path::new("/nieuw/materiaal.bblib")).unwrap();
        assert_eq!(moved.id(), "werkplaats");
        assert_eq!(moved.get(entry).unwrap().draft(), mats.get(entry).unwrap().draft(), "preset values identical");
        assert_eq!(link_status(&link, &moved), LinkStatus::UpToDate, "the project's link survives the move");
        let moved_art = load_art_library(&fs, Path::new("/nieuw/kunst.bbart")).unwrap();
        let again = moved_art.place(art_id).unwrap();
        assert_eq!(format!("{:?}", again), format!("{:?}", placed_before), "the same geometry from the moved item");
        assert_eq!(moved_art.item(art_id).unwrap().hash(), art.item(art_id).unwrap().hash());

        // An edit in the library is then seen as an update and refreshes the project recipe explicitly.
        let mut edited = moved.clone();
        edited.update(entry, recipe("Berken", 40.0, 70.0)).unwrap();
        assert!(matches!(link_status(&link, &edited), LinkStatus::UpdateAvailable { applied: 1, latest: 2 }));
        let cmd = refresh_command(&link, &edited, recipe_id).unwrap();
        assert!(matches!(cmd, ProcessCommand::UpdateRecipe { .. }));
    }

    #[test]
    fn lib01_a_bundle_into_an_empty_library_reproduces_every_preset_and_art_item() {
        let mut mats = MaterialLibrary::new("werkplaats");
        let a = mats.add(recipe("Berken", 25.0, 62.5)).unwrap();
        let b = mats.add(recipe("Acryl", 10.0, 80.0)).unwrap();
        let art = arts();
        let ids = [a, b];
        let art_ids: Vec<u64> = art.items().iter().map(|i| i.id()).collect();
        let bytes = encode_bundle(&BundleContents {
            materials: Some(&mats.subset(&ids)),
            art: Some(&art.subset(&art_ids)),
            profiles: &[],
            templates: &[],
        })
        .unwrap();
        let bundle = decode_bundle(&bytes).unwrap();
        let mut target_mats = MaterialLibrary::new("laptop");
        let mut target_art = ArtLibrary::new("laptop-kunst");
        let local = LocalState { materials: Some(&target_mats), art: Some(&target_art), ..LocalState::default() };
        let preview = preview_import(&bundle, &local);
        assert!(!preview.materials.is_empty() && !preview.art.is_empty(), "the preview lists what would arrive");
        let target = ImportTarget { materials: &mut target_mats, art: &mut target_art, profiles: &[], templates: &[] };
        apply_import(&bundle, target, &ImportChoices::default()).unwrap();
        assert_eq!(target_mats.entries().len(), 2);
        for e in mats.entries() {
            let same = target_mats.entries().iter().find(|t| t.draft() == e.draft());
            assert!(same.is_some(), "preset {:?} came through with identical values", e.draft().name);
        }
        assert_eq!(target_art.items().len(), 1);
        assert_eq!(target_art.items()[0].hash(), art.items()[0].hash());
    }
}
