//! G8-trace, T03: the tangential lens coefficients p1, p2 next to k1, k2 in the preparation panel's
//! intent. The photo is rendered from a straight line through the Brown–Conrady law, inverted by
//! fixed-point iteration here (independent of the code under test).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_presentation::{PrepField, PrepIssue, TracePrepIntent};
use g7_support::*;

/// A 201 × 151 px photo of one vertical line (1.5 px wide at x = 40.5 of the undistorted scene)
/// seen through a lens `(k1, p1, p2)`.
fn decentred_line(k1: f64, p1: f64, p2: f64) -> Vec<u8> {
    let (w, h) = (201usize, 151usize);
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let half_diag = cx.hypot(cy);
    image(w, h, |x, y| {
        let (xd, yd) = ((x as f64 + 0.5 - cx) / half_diag, (y as f64 + 0.5 - cy) / half_diag);
        let (mut xu, mut yu) = (xd, yd);
        for _ in 0..200 {
            let r2 = xu * xu + yu * yu;
            let radial = 1.0 + k1 * r2;
            let tx = 2.0 * p1 * xu * yu + p2 * (r2 + 2.0 * xu * xu);
            let ty = p1 * (r2 + 2.0 * yu * yu) + 2.0 * p2 * xu * yu;
            xu = (xd - tx) / radial;
            yu = (yd - ty) / radial;
        }
        if (cx + xu * half_diag - 40.5).abs() < 0.75 { 0 } else { 255 }
    })
}

fn lens(k1: &str, k2: &str, p1: &str, p2: &str) -> TracePrepIntent {
    TracePrepIntent::UseLens { k1: s(k1), k2: s(k2), p1: s(p1), p2: s(p2) }
}

#[test]
fn the_tangential_coefficients_straighten_a_line_that_a_decentred_lens_bowed() {
    let (mut h, doc) = opened_with(&decentred_line(-0.05, 0.03, -0.06));
    let bowed = tracer(&h, doc).result.expect("a trace").width_mm;
    assert!(bowed > 0.45, "the fixture is bowed: {bowed} mm wide for a 0.15 mm line");

    // The radial terms alone do not straighten it.
    send(&mut h, doc, lens("-0.05", "0", "0", "0"));
    let radial_only = tracer(&h, doc).result.expect("a trace").width_mm;
    assert!(radial_only > 0.45, "still bowed: {radial_only} mm");

    send(&mut h, doc, lens("-0.05", "0", "0.03", "-0.06"));
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert!(p.lens.on);
    assert_eq!((p.lens.p1.as_str(), p.lens.p2.as_str()), ("0.03", "-0.06"));
    let straight = tracer(&h, doc).result.expect("a trace").width_mm;
    assert!(straight < 0.35, "straight again: {straight} mm");
}

#[test]
fn tangential_text_that_cannot_be_read_or_is_no_lens_is_refused_where_it_was_typed() {
    let (mut h, doc) = opened_with(&decentred_line(-0.05, 0.03, -0.06));
    for (p1, p2, want) in [
        ("abc", "0", PrepIssue::InvalidNumber(PrepField::LensP1)),
        ("0", "", PrepIssue::InvalidNumber(PrepField::LensP2)),
        ("3", "0", PrepIssue::OutOfRange(PrepField::LensP1)),
        ("0", "-3", PrepIssue::OutOfRange(PrepField::LensP2)),
        // The mapping folds over itself inside the image.
        ("0", "-0.9", PrepIssue::OutOfRange(PrepField::LensP2)),
    ] {
        send(&mut h, doc, lens("0", "0", p1, p2));
        let p = prep(&h, doc);
        assert_eq!(p.issue, Some(want), "{p1:?} {p2:?}");
        assert!(!p.lens.on);
    }
}

#[test]
fn the_tangential_coefficients_are_stored_with_the_applied_trace() {
    use burnburn_presentation::CommandKey;
    let (mut h, doc) = opened_with(&decentred_line(-0.05, 0.03, -0.06));
    send(&mut h, doc, lens("-0.05", "0", "0.03", "-0.06"));
    let straight = tracer(&h, doc).result.unwrap().width_mm;
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let id = vm(&h, doc).objects.last().unwrap().id;
    h.invoke(doc, CommandKey::CloseTracer);
    h.send(burnburn_presentation::UiIntent::Select {
        document: doc,
        ids: vec![id],
        mode: burnburn_presentation::SelectMode::Replace,
    });
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
    let p = prep(&h, doc);
    assert!(p.lens.on);
    assert_eq!((p.lens.p1.as_str(), p.lens.p2.as_str()), ("0.03", "-0.06"));
    assert!((tracer(&h, doc).result.unwrap().width_mm - straight).abs() < 1e-9);
}
