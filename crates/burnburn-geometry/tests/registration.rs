//! W2-L2 Print and Cut registration and Center Finder (IM27, story W05). Oracles are transforms
//! chosen first and applied to the targets, circles chosen first and sampled, and the least-squares
//! optimality conditions, not the fit's own formulas.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{
    Affine, CircleFitError, Point, RegistrationError, RegistrationOptions, ScaleMode, Vector, fit_circle, register,
};
use burnburn_types::Angle;
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn near(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

fn near_pt(a: Point, b: Point, eps: f64) -> bool {
    near(a.x, b.x, eps) && near(a.y, b.y, eps)
}

fn similarity(deg: f64, scale: f64, t: Vector) -> Affine {
    Affine::scale(scale, scale)
        .unwrap()
        .then(&Affine::rotate(Angle::from_degrees(deg).unwrap()))
        .then(&Affine::translate(t).unwrap())
}

fn free() -> RegistrationOptions {
    RegistrationOptions { scale: ScaleMode::Uniform { max_deviation: 0.5 }, min_separation: 1.0 }
}

fn fixed() -> RegistrationOptions {
    RegistrationOptions { scale: ScaleMode::Fixed, min_separation: 1.0 }
}

#[test]
fn two_targets_give_the_exact_rotation_and_translation() {
    let truth = similarity(30.0, 1.0, Vector::new(10.0, 20.0));
    let design = [p(0.0, 0.0), p(100.0, 0.0)];
    let measured = design.map(|d| truth.apply(d));
    for options in [free(), fixed()] {
        let r = register(&design, &measured, &options).unwrap();
        assert!(near(r.rotation.degrees(), 30.0, 1e-9), "{:?}", r.rotation);
        assert!(near(r.scale, 1.0, 1e-12));
        for (d, m) in design.iter().zip(&measured) {
            assert!(near_pt(r.transform.apply(*d), *m, 1e-9));
        }
        assert!(r.residuals.iter().all(|e| *e < 1e-9), "{:?}", r.residuals);
        assert!(r.rms() < 1e-9 && r.max_residual() < 1e-9);
    }
}

#[test]
fn a_uniform_scale_is_recovered_unless_scaling_is_switched_off() {
    let design = [p(0.0, 0.0), p(100.0, 0.0)];
    let truth = similarity(-12.0, 1.02, Vector::new(-5.0, 7.0));
    let measured = design.map(|d| truth.apply(d));
    let r = register(&design, &measured, &free()).unwrap();
    assert!(near(r.scale, 1.02, 1e-12), "{}", r.scale);
    assert!(r.max_residual() < 1e-9);

    // No-scaling: the scale stays 1; the 2 mm disagreement is split over both targets.
    let r = register(&design, &measured, &fixed()).unwrap();
    assert_eq!(r.scale, 1.0);
    assert!(near(r.rotation.degrees(), -12.0, 1e-9));
    assert!(r.residuals.iter().all(|e| near(*e, 1.0, 1e-9)), "{:?}", r.residuals);
    assert!(near(r.rms(), 1.0, 1e-9));
    // The midpoint of the design lands on the midpoint of the measurement.
    let mid = r.transform.apply(p(50.0, 0.0));
    assert!(near_pt(mid, p((measured[0].x + measured[1].x) / 2.0, (measured[0].y + measured[1].y) / 2.0), 1e-9));
}

#[test]
fn a_scale_outside_the_allowed_range_is_refused() {
    let design = [p(0.0, 0.0), p(100.0, 0.0)];
    let measured = [p(0.0, 0.0), p(130.0, 0.0)];
    let options = RegistrationOptions { scale: ScaleMode::Uniform { max_deviation: 0.1 }, min_separation: 1.0 };
    match register(&design, &measured, &options) {
        Err(RegistrationError::ScaleOutOfRange { scale }) => assert!(near(scale, 1.3, 1e-12)),
        other => panic!("{other:?}"),
    }
    // The same measurement is fine without scaling: the misfit is reported as residuals instead.
    let r = register(&design, &measured, &fixed()).unwrap();
    assert!(near(r.max_residual(), 15.0, 1e-9));
}

#[test]
fn coinciding_or_very_near_targets_are_refused_on_either_side() {
    let ok = [p(0.0, 0.0), p(50.0, 0.0)];
    let same = [p(3.0, 3.0), p(3.0, 3.0)];
    let near_by = [p(3.0, 3.0), p(3.0, 3.4)];
    assert_eq!(register(&same, &ok, &free()), Err(RegistrationError::DesignTargetsTooClose));
    assert_eq!(register(&ok, &same, &free()), Err(RegistrationError::MeasuredTargetsTooClose));
    assert_eq!(register(&near_by, &ok, &free()), Err(RegistrationError::DesignTargetsTooClose));
    assert_eq!(register(&ok, &near_by, &fixed()), Err(RegistrationError::MeasuredTargetsTooClose));
    // Exactly the minimum separation is accepted.
    let edge = [p(3.0, 3.0), p(3.0, 4.0)];
    assert!(register(&edge, &edge, &fixed()).is_ok());
}

#[test]
fn bad_input_is_a_typed_error() {
    let two = [p(0.0, 0.0), p(10.0, 0.0)];
    assert_eq!(register(&two[..1], &two[..1], &free()), Err(RegistrationError::TooFewTargets));
    assert_eq!(register(&[], &[], &free()), Err(RegistrationError::TooFewTargets));
    assert_eq!(
        register(&two, &[p(0.0, 0.0), p(5.0, 0.0), p(9.0, 9.0)], &free()),
        Err(RegistrationError::CountMismatch)
    );
    assert_eq!(register(&two, &[p(0.0, 0.0), p(f64::NAN, 0.0)], &free()), Err(RegistrationError::NotFinite));
    let bad = RegistrationOptions { scale: ScaleMode::Uniform { max_deviation: -0.1 }, min_separation: 1.0 };
    assert_eq!(register(&two, &two, &bad), Err(RegistrationError::InvalidOptions));
    let bad = RegistrationOptions { scale: ScaleMode::Fixed, min_separation: f64::NAN };
    assert_eq!(register(&two, &two, &bad), Err(RegistrationError::InvalidOptions));
}

#[test]
fn four_noisy_targets_use_least_squares_and_report_the_misfit() {
    let design = [p(0.0, 0.0), p(100.0, 0.0), p(100.0, 80.0), p(0.0, 80.0)];
    let truth = similarity(5.0, 1.0, Vector::new(12.0, -3.0));
    let mut measured = design.map(|d| truth.apply(d));
    measured[2] = measured[2] + Vector::new(0.2, -0.1);
    let r = register(&design, &measured, &fixed()).unwrap();
    // The centroid of the design maps to the centroid of the measurement (a least-squares condition).
    let c_design = p(50.0, 40.0);
    let c_measured =
        p(measured.iter().map(|m| m.x).sum::<f64>() / 4.0, measured.iter().map(|m| m.y).sum::<f64>() / 4.0);
    assert!(near_pt(r.transform.apply(c_design), c_measured, 1e-9));
    // The disturbed target has the largest residual, and it is smaller than its disturbance.
    let worst = r.residuals.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    assert_eq!(worst.0, 2);
    assert!(worst.1 < (0.2f64.powi(2) + 0.1f64.powi(2)).sqrt());
    // No nearby transform fits better.
    let sse = |m: &Affine| -> f64 {
        design
            .iter()
            .zip(&measured)
            .map(|(d, q)| {
                let e = m.apply(*d) - *q;
                e.dot(e)
            })
            .sum()
    };
    let best = sse(&r.transform);
    for (dr, dx, dy) in [(0.01, 0.0, 0.0), (-0.01, 0.0, 0.0), (0.0, 0.01, 0.0), (0.0, 0.0, -0.01)] {
        let nudged = r
            .transform
            .then(&Affine::rotate(Angle::from_degrees(dr).unwrap()))
            .then(&Affine::translate(Vector::new(dx, dy)).unwrap());
        assert!(sse(&nudged) >= best - 1e-12, "{dr} {dx} {dy}");
    }
    assert!(near(r.rms().powi(2) * 4.0, best, 1e-9));
    assert!(r.transform.determinant() > 0.0);
}

proptest! {
    #[test]
    fn a_known_similarity_is_recovered_from_any_number_of_targets(
        deg in -180.0f64..180.0, scale in 0.8f64..1.25, tx in -500.0f64..500.0, ty in -500.0f64..500.0,
        points in prop::collection::vec((-300.0f64..300.0, -300.0f64..300.0), 2..6),
    ) {
        let design: Vec<Point> = points.iter().map(|&(x, y)| p(x, y)).collect();
        let far = design.iter().any(|a| design.iter().any(|b| a.distance(*b) > 5.0));
        prop_assume!(far);
        let truth = similarity(deg, scale, Vector::new(tx, ty));
        let measured: Vec<Point> = design.iter().map(|d| truth.apply(*d)).collect();
        let r = register(&design, &measured, &free()).unwrap();
        prop_assert!(near(r.scale, scale, 1e-9));
        let mut diff = (r.rotation.degrees() - deg).rem_euclid(360.0);
        if diff > 180.0 { diff -= 360.0; }
        prop_assert!(diff.abs() < 1e-7, "rotation {} vs {}", r.rotation.degrees(), deg);
        prop_assert!(r.max_residual() < 1e-7, "{}", r.max_residual());
        prop_assert!(r.transform.determinant() > 0.0, "a registration never mirrors");
    }
}

// ---- Center Finder ----

fn on_circle(c: Point, r: f64, degrees: &[f64]) -> Vec<Point> {
    degrees.iter().map(|d| p(c.x + r * d.to_radians().cos(), c.y + r * d.to_radians().sin())).collect()
}

#[test]
fn three_points_give_the_circle_through_them_with_zero_residuals() {
    let pts = [p(8.0, 4.0), p(3.0, 9.0), p(-2.0, 4.0)];
    let fit = fit_circle(&pts).unwrap();
    assert!(near_pt(fit.circle.center, p(3.0, 4.0), 1e-9), "{fit:?}");
    assert!(near(fit.circle.radius, 5.0, 1e-9));
    assert!(fit.residuals.iter().all(|e| e.abs() < 1e-9));
    assert!(fit.rms() < 1e-9 && fit.max_abs_residual() < 1e-9);
}

#[test]
fn many_points_on_a_short_arc_find_the_centre() {
    let pts = on_circle(p(-12.5, 30.0), 17.0, &[20.0, 35.0, 50.0, 65.0, 80.0, 95.0, 110.0, 130.0]);
    let fit = fit_circle(&pts).unwrap();
    assert!(near_pt(fit.circle.center, p(-12.5, 30.0), 1e-7), "{:?}", fit.circle);
    assert!(near(fit.circle.radius, 17.0, 1e-7));
    assert!(near(fit.arc_span, 110f64.to_radians(), 1e-6), "{}", fit.arc_span);
}

#[test]
fn symmetric_measurement_noise_leaves_the_centre_and_shows_up_as_residuals() {
    // Opposite points are 0.1 mm too far, the other pair 0.1 mm too near.
    let pts = [p(11.1, 2.0), p(1.0, 11.9), p(-9.1, 2.0), p(1.0, -7.9)];
    let fit = fit_circle(&pts).unwrap();
    assert!(near_pt(fit.circle.center, p(1.0, 2.0), 1e-9), "{:?}", fit.circle);
    assert!(near(fit.circle.radius, 10.0, 1e-9));
    for (got, want) in fit.residuals.iter().zip([0.1, -0.1, 0.1, -0.1]) {
        assert!(near(*got, want, 1e-9), "{:?}", fit.residuals);
    }
    assert!(near(fit.rms(), 0.1, 1e-9) && near(fit.max_abs_residual(), 0.1, 1e-9));
}

#[test]
fn collinear_marks_give_no_valid_registration() {
    let line = [p(0.0, 0.0), p(5.0, 5.0), p(10.0, 10.0), p(-3.0, -3.0)];
    assert_eq!(fit_circle(&line).unwrap_err(), CircleFitError::Collinear);
    let horizontal = [p(0.0, 7.0), p(5.0, 7.0), p(1e4, 7.0)];
    assert_eq!(fit_circle(&horizontal).unwrap_err(), CircleFitError::Collinear);
}

#[test]
fn too_few_or_repeated_points_are_refused() {
    assert_eq!(fit_circle(&[]).unwrap_err(), CircleFitError::TooFew);
    assert_eq!(fit_circle(&[p(0.0, 0.0), p(1.0, 1.0)]).unwrap_err(), CircleFitError::TooFew);
    let repeated = [p(0.0, 0.0), p(0.0, 0.0), p(4.0, 1.0)];
    assert_eq!(fit_circle(&repeated).unwrap_err(), CircleFitError::Coincident);
    let all_same = [p(2.0, 2.0); 5];
    assert_eq!(fit_circle(&all_same).unwrap_err(), CircleFitError::Coincident);
    assert_eq!(fit_circle(&[p(0.0, 0.0), p(1.0, 0.0), p(f64::INFINITY, 0.0)]).unwrap_err(), CircleFitError::NotFinite);
}

#[test]
fn a_nearly_straight_arc_is_a_valid_fit_that_reports_its_tiny_span() {
    // 2° of a circle with radius 500 mm: well-defined but very sensitive.
    let pts = on_circle(p(0.0, 0.0), 500.0, &[0.0, 0.5, 1.0, 1.5, 2.0]);
    let fit = fit_circle(&pts).unwrap();
    assert!(fit.arc_span < 0.04, "{}", fit.arc_span);
    assert!(near_pt(fit.circle.center, p(0.0, 0.0), 1e-4), "{:?}", fit.circle);
}

#[test]
fn far_from_the_origin_the_centre_keeps_its_accuracy() {
    let c = p(250_000.0, -120_000.0);
    let pts = on_circle(c, 40.0, &[10.0, 100.0, 190.0, 280.0]);
    let fit = fit_circle(&pts).unwrap();
    assert!(near_pt(fit.circle.center, c, 1e-5), "{:?}", fit.circle);
}

proptest! {
    #[test]
    fn the_centre_of_any_sampled_circle_is_recovered(
        cx in -300.0f64..300.0, cy in -300.0f64..300.0, r in 2.0f64..200.0,
        start in 0.0f64..360.0, span in 60.0f64..340.0, count in 3usize..9,
    ) {
        let degrees: Vec<f64> = (0..count).map(|i| start + span * (i as f64) / ((count - 1) as f64)).collect();
        let fit = fit_circle(&on_circle(p(cx, cy), r, &degrees)).unwrap();
        prop_assert!(near_pt(fit.circle.center, p(cx, cy), 1e-6), "{:?}", fit.circle);
        prop_assert!(near(fit.circle.radius, r, 1e-6));
        prop_assert!(fit.max_abs_residual() < 1e-6);
    }
}
