//! G6-werk T04: the operator's decisions on a centreline trace (which branches continue through a
//! junction, which chain became an exact primitive, which two ends were joined) belong to the stored
//! recipe, so tracing the same source again applies them. A decision that no longer fits the new
//! graph is skipped and reported, never applied somewhere else.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::engine::ParamValue;
use burnburn_document::trace::TraceCommand;
use burnburn_geometry::Affine;
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, SourceImage, Threshold, decode};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_trace::{
    CenterlineDecision, CenterlineRequest, CenterlineResult, CenterlineSettings, Connections, NodeKind, SkipReason,
    centerline_command, centerline_decisions_from_engine, trace_centerline,
};
use burnburn_types::{CancellationToken, ContentHash, RevisionId, StableId, TaskId};

fn image_recipe() -> ImageRecipe {
    ImageRecipe {
        scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()),
        threshold: Threshold::Fixed(127),
        ..ImageRecipe::default()
    }
}

fn run(name: &str) -> CenterlineResult {
    let source: SourceImage =
        decode(&read_fixture("trace", &format!("synthetic/{name}.png")).unwrap(), DecodeLimits::default()).unwrap();
    let req = CenterlineRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(source),
        image: image_recipe(),
        settings: CenterlineSettings::default(),
    };
    trace_centerline(&req, &CancellationToken::never()).unwrap()
}

fn recipe_of(result: &CenterlineResult) -> burnburn_document::engine::EngineRecipe {
    let TraceCommand::Create { definition, .. } =
        centerline_command(result, &image_recipe(), ContentHash::from_bytes([1; 32]), "x", Affine::IDENTITY, None)
            .unwrap()
    else {
        panic!("a new trace is a Create")
    };
    definition.recipe
}

/// The T junction resolved with its second-best pairing (not the default, so a replay that falls
/// back to "the best one" would be caught).
fn t_with_second_best_pairing() -> (CenterlineResult, CenterlineResult) {
    let plain = run("stroke-t");
    let junction = plain.nodes.iter().find(|n| n.kind == NodeKind::Junction).unwrap();
    let Connections::Ambiguous { candidates } = &junction.connections else { panic!() };
    let edited = plain.resolve_junction(junction.id, candidates[1].pairs.clone()).unwrap();
    (plain, edited)
}

fn stroke_paths(r: &CenterlineResult) -> Vec<burnburn_geometry::Subpath> {
    r.strokes().into_iter().map(|s| s.path).collect()
}

#[test]
fn a_junction_decision_is_in_the_recipe_and_tracing_again_applies_it() {
    let (plain, edited) = t_with_second_best_pairing();
    assert_eq!(edited.decisions().len(), 1);
    assert_eq!(plain.decisions().len(), 0);
    let decisions = centerline_decisions_from_engine(&recipe_of(&edited)).unwrap();
    assert_eq!(decisions, edited.decisions());

    // A fresh trace of the same source, then the stored decisions.
    let replayed = run("stroke-t").replay(&decisions);
    assert!(replayed.skipped.is_empty(), "{:?}", replayed.skipped);
    assert_eq!(replayed.applied, 1);
    assert_eq!(stroke_paths(&replayed.result), stroke_paths(&edited), "the same strokes as the operator made");
    assert_ne!(stroke_paths(&replayed.result), stroke_paths(&plain), "and not the undecided ones");
    assert_eq!(replayed.result.decisions(), edited.decisions(), "the replayed result records them again");
}

#[test]
fn a_confirmed_primitive_and_a_join_are_stored_and_replayed_in_order() {
    let plain = run("stroke-t");
    let chain = plain.chains.iter().find(|c| c.candidates.iter().any(|k| k.within_tolerance)).unwrap().id;
    let proposal = plain.propose_replacement(chain).unwrap();
    let confirmed = plain.confirm_replacement(&proposal).unwrap();
    let junction = confirmed.nodes.iter().find(|n| n.kind == NodeKind::Junction).unwrap();
    let Connections::Ambiguous { candidates } = &junction.connections else { panic!() };
    let edited = confirmed.resolve_junction(junction.id, candidates[0].pairs.clone()).unwrap();
    assert_eq!(edited.decisions().len(), 2);
    assert!(matches!(edited.decisions()[0], CenterlineDecision::ConfirmPrimitive { .. }));
    assert!(matches!(edited.decisions()[1], CenterlineDecision::ResolveJunction { .. }));

    let stored = centerline_decisions_from_engine(&recipe_of(&edited)).unwrap();
    let replayed = run("stroke-t").replay(&stored);
    assert_eq!((replayed.applied, replayed.skipped.len()), (2, 0));
    assert_eq!(stroke_paths(&replayed.result), stroke_paths(&edited));
    assert_eq!(
        replayed.result.chain(chain).unwrap().confirmed,
        edited.chain(chain).unwrap().confirmed,
        "the accepted primitive is the same"
    );
}

#[test]
fn a_decision_that_does_not_fit_the_new_graph_is_skipped_and_named_never_applied_elsewhere() {
    let (_, edited) = t_with_second_best_pairing();
    let decisions = edited.decisions().to_vec();
    // The Y stroke has its junction somewhere else.
    let y = run("stroke-y");
    let replayed = y.replay(&decisions);
    assert_eq!(replayed.applied, 0);
    assert_eq!(replayed.skipped.len(), 1);
    assert_eq!(replayed.skipped[0].index, 0);
    assert_eq!(replayed.skipped[0].reason, SkipReason::NothingThere);
    assert_eq!(replayed.result.revision, y.revision, "nothing was decided");
    assert_eq!(stroke_paths(&replayed.result), stroke_paths(&y));
    assert!(replayed.result.decisions().is_empty());
}

#[test]
fn a_recipe_without_decisions_reads_as_none_and_a_damaged_list_is_refused() {
    let plain = run("stroke-t");
    assert_eq!(centerline_decisions_from_engine(&recipe_of(&plain)).unwrap(), Vec::new());

    let (_, edited) = t_with_second_best_pairing();
    let mut damaged = recipe_of(&edited);
    damaged.params.insert("centerline.decisions".into(), ParamValue::Text("junction".into()));
    assert!(centerline_decisions_from_engine(&damaged).is_err(), "not a list");
    let mut unknown = recipe_of(&edited);
    unknown.params.insert(
        "centerline.decisions".into(),
        ParamValue::List(vec![ParamValue::List(vec![ParamValue::Text("x".into())])]),
    );
    assert!(centerline_decisions_from_engine(&unknown).is_err(), "unknown kind of decision");
    let mut short = recipe_of(&edited);
    short.params.insert(
        "centerline.decisions".into(),
        ParamValue::List(vec![ParamValue::List(vec![ParamValue::Text("junction".into()), ParamValue::Number(1.0)])]),
    );
    assert!(centerline_decisions_from_engine(&short).is_err(), "a decision with missing values");
}

#[test]
fn decisions_in_the_recipe_make_it_differ_from_the_undecided_one() {
    let (plain, edited) = t_with_second_best_pairing();
    assert_ne!(recipe_of(&plain), recipe_of(&edited));
}

#[test]
fn a_join_of_two_stroke_ends_is_stored_and_replayed() {
    let plain = run("strokes-near-touching");
    let ends: Vec<_> = plain.nodes.iter().filter(|n| n.kind == NodeKind::End).collect();
    // The closest pair of ends that the repair rule accepts.
    let mut best: Option<(f64, burnburn_trace::NodeId, burnburn_trace::NodeId)> = None;
    for (i, a) in ends.iter().enumerate() {
        for b in &ends[i + 1..] {
            let d = a.at.distance(b.at);
            if plain.join_endpoints(a.id, b.id).is_ok() && best.is_none_or(|(bd, ..)| d < bd) {
                best = Some((d, a.id, b.id));
            }
        }
    }
    let (_, a, b) = best.expect("two joinable ends in the near-touching fixture");
    let joined = plain.join_endpoints(a, b).unwrap();
    assert!(matches!(joined.decisions(), [CenterlineDecision::JoinEndpoints { .. }]));
    let stored = centerline_decisions_from_engine(&recipe_of(&joined)).unwrap();
    let replayed = run("strokes-near-touching").replay(&stored);
    assert_eq!((replayed.applied, replayed.skipped.len()), (1, 0));
    assert_eq!(stroke_paths(&replayed.result), stroke_paths(&joined));
    assert_eq!(replayed.result.chains.len(), plain.chains.len() + 1, "the repair chain is back");
}
