//! IM23 centerline: skeleton → graph (ends, T/X/Y junctions) → spur control → chain fitting with
//! primitive candidates; ambiguous connections stay visible until the operator resolves them.

#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use burnburn_geometry::Point;
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, SourceImage, Threshold, decode};
use burnburn_test_support::fixtures::{fixture_dir, read_fixture};
use burnburn_trace::{
    CenterlineRequest, CenterlineResult, CenterlineSettings, Connections, NodeKind, trace_centerline,
};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};
use serde_json::Value;

const S: f64 = 0.1;

fn image_recipe() -> ImageRecipe {
    ImageRecipe {
        scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
        threshold: Threshold::Fixed(127),
        ..ImageRecipe::default()
    }
}

fn run(source: SourceImage, settings: CenterlineSettings) -> CenterlineResult {
    let req = CenterlineRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(source),
        image: image_recipe(),
        settings,
    };
    trace_centerline(&req, &CancellationToken::never()).unwrap()
}

fn corpus_image(name: &str) -> SourceImage {
    decode(&read_fixture("trace", &format!("synthetic/{name}.png")).unwrap(), DecodeLimits::default()).unwrap()
}

/// Corpus pixel coordinates (y-down, 64 px high) to millimetres (y-up).
fn px(x: f64, y: f64) -> Point {
    Point::new(x * S, (64.0 - y) * S)
}

#[test]
fn t_junction_retains_three_branches_and_marks_ambiguous_connections() {
    let result = run(corpus_image("stroke-t"), CenterlineSettings::default());
    assert_eq!(result.components, 1);
    let junctions: Vec<_> = result.nodes.iter().filter(|n| n.kind == NodeKind::Junction).collect();
    assert_eq!(junctions.len(), 1, "one T junction: {:?}", result.nodes);
    let j = junctions[0];
    assert_eq!(j.degree(), 3, "three branches");
    assert!(j.at.distance(px(32.0, 12.0)) <= 0.1, "junction at {:?}", j.at);
    let ends: Vec<_> = result.nodes.iter().filter(|n| n.kind == NodeKind::End).collect();
    assert_eq!(ends.len(), 3);
    for want in [px(10.0, 12.0), px(54.0, 12.0), px(32.0, 56.0)] {
        let d = ends.iter().map(|n| n.at.distance(want)).fold(f64::INFINITY, f64::min);
        assert!(d <= 0.2, "end near {want:?}: {d:.3} mm");
    }
    assert_eq!(result.chains.len(), 3);
    for c in &result.chains {
        assert!(!c.closed && !c.path.is_closed(), "open strokes stay open");
        assert!((c.width_mm - 0.6).abs() <= 0.15, "stroke width metadata {:.3}", c.width_mm);
    }

    // Which branch continues which is not decided by the tracer.
    let Connections::Ambiguous { candidates } = &j.connections else { panic!("{:?}", j.connections) };
    assert_eq!(candidates.len(), 3, "each pair of the three branches is a candidate continuation");
    let best = &candidates[0];
    assert_eq!(best.pairs.len(), 1);
    let (a, b) = best.pairs[0];
    let other_end = |r: burnburn_trace::ChainEnd| {
        let chain = result.chain(r.chain).unwrap();
        let far = if r.at_start { chain.to } else { chain.from };
        result.node(far.unwrap()).unwrap().at
    };
    let (pa, pb) = (other_end(a), other_end(b));
    assert!((pa.y - pb.y).abs() < 0.1, "best continuation is the straight bar: {pa:?} – {pb:?}");
    assert!(best.cost_deg < 10.0 && candidates[1].cost_deg > 60.0);

    // Unresolved: three separate strokes, no continuity invented.
    assert_eq!(result.strokes().len(), 3);
    // Operator resolves with the straight bar: two strokes, one through the junction.
    let resolved = result.resolve_junction(j.id, best.pairs.clone()).unwrap();
    assert_eq!(resolved.strokes().len(), 2);
    assert!(matches!(resolved.node(j.id).unwrap().connections, Connections::Resolved { .. }));
}

#[test]
fn corpus_strokes_follow_the_reference_skeleton_graph() {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(fixture_dir("trace").join("manifest.json")).unwrap()).unwrap();
    let mut checked = 0;
    for case in manifest["cases"].as_array().unwrap() {
        let Some(expected_nodes) = case["expected"].get("skeleton_nodes_px").and_then(Value::as_array) else {
            continue;
        };
        let name = case["name"].as_str().unwrap();
        let png = read_fixture("trace", case["image"].as_str().unwrap()).unwrap();
        let result = run(decode(&png, DecodeLimits::default()).unwrap(), CenterlineSettings::default());
        assert_eq!(result.components as u64, case["expected"]["components"].as_u64().unwrap(), "{name}: components");
        assert_eq!(result.nodes.len(), expected_nodes.len(), "{name}: node count {:?}", result.nodes);
        for want in expected_nodes {
            let at = px(want["at"][0].as_f64().unwrap(), want["at"][1].as_f64().unwrap());
            let degree = want["degree"].as_u64().unwrap() as usize;
            let tol = if degree == 1 { 0.2 } else { 0.1 };
            let found = result.nodes.iter().find(|n| n.degree() == degree && n.at.distance(at) <= tol);
            assert!(found.is_some(), "{name}: node degree {degree} near {at:?}; got {:?}", result.nodes);
            if degree >= 3 {
                assert!(matches!(found.unwrap().connections, Connections::Ambiguous { .. }), "{name}");
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 4, "stroke-t, stroke-x, stroke-y, strokes-near-touching");
}

fn raster(w: u32, h: u32, inside: impl Fn(f64, f64) -> bool) -> SourceImage {
    support::rasterize(w, h, S, inside)
}

#[test]
fn near_touching_strokes_stay_separate_and_touching_strokes_are_marked_ambiguous() {
    let near = run(corpus_image("strokes-near-touching"), CenterlineSettings::default());
    assert_eq!(near.components, 2);
    assert!(near.nodes.iter().all(|n| n.kind == NodeKind::End), "no junction between separate strokes");
    assert_eq!(near.near_contacts.len(), 1, "the 2 px gap is shown: {:?}", near.near_contacts);
    let c = near.near_contacts[0];
    assert!((c.gap_mm - 0.2).abs() <= 0.1, "gap {:.3} mm", c.gap_mm);
    assert!((c.at.x - 2.8).abs() <= 0.2, "between the strokes: {:?}", c.at);

    // ")(" — two arcs that touch at one point merge in the mask; the graph must not decide.
    let left = support::arc_stroke(0.6, 3.2, 2.6, -1.0, 2.0, 0.3);
    let right = support::arc_stroke(5.8, 3.2, 2.6, std::f64::consts::PI - 1.0, 2.0, 0.3);
    let touching = run(raster(64, 64, |x, y| left(x, y) || right(x, y)), CenterlineSettings::default());
    assert_eq!(touching.components, 1);
    // The contact zone is ~2.4 mm long, so the merged mask thins to an "H": every junction it
    // creates is ambiguous and no stroke runs through one on its own.
    let j: Vec<_> = touching.nodes.iter().filter(|n| n.kind == NodeKind::Junction).collect();
    assert!(!j.is_empty(), "{:?}", touching.nodes);
    for n in &j {
        assert!(matches!(n.connections, Connections::Ambiguous { .. }), "{n:?}");
    }
    assert_eq!(touching.strokes().len(), touching.chains.len(), "unresolved: no merge invented");
    assert!(touching.strokes().iter().all(|s| s.chains.len() == 1));
    assert_eq!(
        touching.resolve_junction(j[0].id, vec![(j[0].ends[0], j[0].ends[0])]).unwrap_err(),
        burnburn_trace::CenterlineEditError::InvalidPairing
    );
}

#[test]
fn short_spur_is_pruned_and_reported_but_a_protected_spur_is_kept() {
    // Horizontal stroke with a short nub (0.3 mm beyond the edge) and a long branch.
    let bar = support::capsule(1.0, 3.0, 9.0, 3.0, 0.3);
    let nub = support::convex(vec![(3.35, 3.0), (3.65, 3.0), (3.65, 3.6), (3.35, 3.6)]);
    let branch = support::capsule(7.0, 3.0, 7.0, 5.5, 0.3);
    let img = || raster(100, 64, |x, y| bar(x, y) || nub(x, y) || branch(x, y));
    let pruned = run(img(), CenterlineSettings::default());
    let junctions = pruned.nodes.iter().filter(|n| n.kind == NodeKind::Junction).count();
    assert_eq!(junctions, 1, "only the real branch stays: {:?}", pruned.nodes);
    assert_eq!(pruned.spurs_removed.len(), 1, "{:?}", pruned.spurs_removed);
    let spur = pruned.spurs_removed[0];
    assert!((spur.base.x - 3.5).abs() < 0.3 && spur.length_mm < 0.6, "{spur:?}");

    // Protected near the nub tip (pixel coordinates): kept as a branch.
    let settings =
        CenterlineSettings { keep_spurs_near_px: vec![[35.0, 64.0 - 34.5]], ..CenterlineSettings::default() };
    let kept = run(img(), settings);
    assert_eq!(kept.nodes.iter().filter(|n| n.kind == NodeKind::Junction).count(), 2, "{:?}", kept.nodes);
    assert!(kept.spurs_removed.is_empty());
}

#[test]
fn line_arc_and_circle_candidates_carry_residuals_and_replace_only_after_confirmation() {
    use burnburn_trace::{CenterlineEditError, Primitive};
    let line = support::capsule(1.0, 1.0, 9.0, 1.0, 0.3);
    let arc = support::arc_stroke(3.0, 4.5, 2.0, 0.0, std::f64::consts::PI, 0.3);
    let circle = move |x: f64, y: f64| ((x - 9.0).hypot(y - 4.5) - 2.0).abs() <= 0.3;
    let result = run(raster(120, 72, |x, y| line(x, y) || arc(x, y) || circle(x, y)), CenterlineSettings::default());
    assert_eq!(result.components, 3);
    assert_eq!(result.chains.len(), 3, "{:?}", result.nodes);
    let chain_near = |p: Point| {
        result
            .chains
            .iter()
            .min_by(|a, b| {
                let da = a.points.iter().map(|q| q.distance(p)).fold(f64::INFINITY, f64::min);
                let db = b.points.iter().map(|q| q.distance(p)).fold(f64::INFINITY, f64::min);
                da.total_cmp(&db)
            })
            .unwrap()
    };
    let l = chain_near(Point::new(5.0, 1.0));
    let a = chain_near(Point::new(3.0, 6.5));
    let c = chain_near(Point::new(11.0, 4.5));
    assert!(c.closed && !l.closed && !a.closed);

    let best = |chain: &burnburn_trace::Chain| result.propose_replacement(chain.id).unwrap().candidate;
    let Primitive::Line { from, to } = best(l).primitive else { panic!("{:?}", l.candidates) };
    assert!((from.y - 1.0).abs() < 0.05 && (to.y - 1.0).abs() < 0.05 && (from.x - to.x).abs() > 6.0);
    let Primitive::Arc { center, radius, sweep_rad, .. } = best(a).primitive else { panic!("{:?}", a.candidates) };
    assert!(center.distance(Point::new(3.0, 4.5)) < 0.05 && (radius - 2.0).abs() < 0.05, "{center:?} r {radius}");
    assert!((sweep_rad.abs() - std::f64::consts::PI).abs() < 0.25, "sweep {sweep_rad}");
    let Primitive::Circle { center, radius } = best(c).primitive else { panic!("{:?}", c.candidates) };
    assert!(center.distance(Point::new(9.0, 4.5)) < 0.05 && (radius - 2.0).abs() < 0.05);
    for chain in [l, a, c] {
        let cand = best(chain);
        assert!(cand.max_residual_mm <= 0.12 && cand.rms_residual_mm <= cand.max_residual_mm);
        // Free geometry until the operator confirms.
        assert!(chain.confirmed.is_none());
        assert!(!chain.path.segments().iter().any(|s| matches!(s, burnburn_geometry::Segment::Arc(_))));
    }

    let proposal = result.propose_replacement(c.id).unwrap();
    let confirmed = result.confirm_replacement(&proposal).unwrap();
    let c_now = confirmed.chain(c.id).unwrap();
    assert_eq!(c_now.confirmed, Some(proposal.candidate.primitive));
    assert!(matches!(c_now.path.segments(), [burnburn_geometry::Segment::Arc(_)]));
    assert_eq!(confirmed.chain(l.id).unwrap().confirmed, None, "only the confirmed chain changes");
    // A proposal made for an older state is refused.
    assert_eq!(confirmed.confirm_replacement(&proposal).unwrap_err(), CenterlineEditError::Stale);

    // A wavy stroke has no primitive within tolerance.
    let wave = |x: f64, y: f64| (1.0..=9.0).contains(&x) && (y - (3.0 + 0.8 * (x * 1.2).sin())).abs() <= 0.3;
    let wavy = run(raster(100, 64, wave), CenterlineSettings::default());
    let chain = &wavy.chains[0];
    assert_eq!(wavy.propose_replacement(chain.id).unwrap_err(), CenterlineEditError::NoCandidate(chain.id));
    assert!(!chain.candidates.is_empty(), "candidates are still listed with their residuals");
}

#[test]
fn broken_stroke_is_repaired_only_by_an_explicit_join() {
    use burnburn_trace::{CenterlineEditError, ChainOrigin};
    let a = support::capsule(1.0, 3.0, 4.4, 3.0, 0.3);
    let b = support::capsule(5.6, 3.0, 9.0, 3.0, 0.3);
    let result = run(raster(100, 64, |x, y| a(x, y) || b(x, y)), CenterlineSettings::default());
    assert_eq!((result.components, result.chains.len()), (2, 2));
    assert_eq!(result.strokes().len(), 2);
    let end_near =
        |p: Point| result.nodes.iter().min_by(|m, n| m.at.distance(p).total_cmp(&n.at.distance(p))).unwrap().id;
    let (l, r) = (end_near(Point::new(4.4, 3.0)), end_near(Point::new(5.6, 3.0)));
    let joined = result.join_endpoints(l, r).unwrap();
    let repair: Vec<_> = joined.chains.iter().filter(|c| c.origin == ChainOrigin::Repair).collect();
    assert_eq!(repair.len(), 1, "the added piece is distinguishable from observed lines");
    assert_eq!(joined.strokes().len(), 1);
    assert_eq!(joined.chains.iter().filter(|c| c.origin == ChainOrigin::Observed).count(), 2);
    let far = end_near(Point::new(9.0, 3.0));
    assert!(matches!(result.join_endpoints(l, far), Err(CenterlineEditError::TooFar { .. })));
}

#[test]
fn cancelled_centerline_returns_cancelled_and_stale_inputs_are_rejected() {
    let cancel = burnburn_types::CancellationSource::new();
    cancel.cancel();
    let req = CenterlineRequest {
        task: TaskId::from_raw(3),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(corpus_image("stroke-y")),
        image: image_recipe(),
        settings: CenterlineSettings::default(),
    };
    assert!(matches!(trace_centerline(&req, &cancel.token()), Err(burnburn_trace::TraceError::Cancelled(_))));
    let r = trace_centerline(&req, &CancellationToken::never()).unwrap();
    assert!(r.check_fresh(TaskId::from_raw(3), req.key().unwrap()).is_ok());
    let changed = CenterlineRequest {
        settings: CenterlineSettings { min_spur_mm: 0.5, ..CenterlineSettings::default() },
        ..req.clone()
    };
    assert!(r.check_fresh(TaskId::from_raw(3), changed.key().unwrap()).is_err(), "settings are inputs");
    assert!(r.check_fresh(TaskId::from_raw(4), req.key().unwrap()).is_err(), "newer task supersedes");
}

#[test]
fn acute_x_crossing_is_one_degree_four_junction_at_the_crossing() {
    // Two strokes crossing at 40°: thinning splits the crossing into two degree-3 junctions.
    let t = 20.0f64.to_radians();
    let (dx, dy) = (4.0 * t.cos(), 4.0 * t.sin());
    let a = support::capsule(5.0 - dx, 3.2 - dy, 5.0 + dx, 3.2 + dy, 0.3);
    let b = support::capsule(5.0 - dx, 3.2 + dy, 5.0 + dx, 3.2 - dy, 0.3);
    let r = run(raster(100, 64, |x, y| a(x, y) || b(x, y)), CenterlineSettings::default());
    let j: Vec<_> = r.nodes.iter().filter(|n| n.kind == NodeKind::Junction).collect();
    assert_eq!(j.len(), 1, "{:?}", r.nodes);
    assert_eq!(j[0].degree(), 4);
    assert!(j[0].at.distance(Point::new(5.0, 3.2)) <= 0.1, "{:?}", j[0].at);
    let Connections::Ambiguous { candidates } = &j[0].connections else { panic!() };
    // The straight continuations (each stroke with itself) are the cheapest pairing.
    assert!(candidates[0].cost_deg < 15.0, "{:?}", candidates[0]);
}
