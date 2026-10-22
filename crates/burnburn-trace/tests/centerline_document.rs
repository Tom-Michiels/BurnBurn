//! G5-UI-gaten T04: a centerline result as a re-editable trace of the document. The recipe (image
//! recipe plus centerline settings) survives the engine-parameter form; the result becomes one
//! trace command whose outputs are the open strokes, keyed by their first chain.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::trace::TraceCommand;
use burnburn_document::{CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, GeometrySource};
use burnburn_geometry::{Affine, Vector};
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, SourceImage, Threshold, decode};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_trace::{
    CENTERLINE_ENGINE_VERSION, CenterlineRequest, CenterlineResult, CenterlineSettings, ENGINE_VERSION,
    centerline_command, centerline_from_engine, centerline_to_engine, recipe_from_engine, trace_centerline,
};
use burnburn_types::{CancellationToken, ContentHash, DocumentId, RequestId, RevisionId, StableId, TaskId};

fn image_recipe() -> ImageRecipe {
    ImageRecipe {
        scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()),
        threshold: Threshold::Fixed(127),
        ..ImageRecipe::default()
    }
}

fn corpus_image(name: &str) -> SourceImage {
    decode(&read_fixture("trace", &format!("synthetic/{name}.png")).unwrap(), DecodeLimits::default()).unwrap()
}

fn run(name: &str, settings: CenterlineSettings) -> CenterlineResult {
    let req = CenterlineRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(corpus_image(name)),
        image: image_recipe(),
        settings,
    };
    trace_centerline(&req, &CancellationToken::never()).unwrap()
}

fn unusual_settings() -> CenterlineSettings {
    CenterlineSettings {
        min_spur_mm: 0.4,
        spur_width_factor: 1.5,
        keep_spurs_near_px: vec![[3.5, 4.25], [10.0, 0.5]],
        junction_merge_factor: 0.75,
        tolerance_mm: 0.05,
        primitive_tolerance_mm: 0.2,
        near_contact_factor: 0.5,
        max_join_factor: 4.0,
    }
}

#[test]
fn the_centerline_recipe_survives_the_engine_parameter_form() {
    for settings in [CenterlineSettings::default(), unusual_settings()] {
        let engine = centerline_to_engine(&image_recipe(), &settings);
        assert_eq!(engine.engine, CENTERLINE_ENGINE_VERSION);
        engine.validate().unwrap();
        let (image, back) = centerline_from_engine(&engine).unwrap();
        assert_eq!(image, image_recipe());
        assert_eq!(back, settings);
    }
    assert_ne!(
        centerline_to_engine(&image_recipe(), &CenterlineSettings::default()),
        centerline_to_engine(&image_recipe(), &unusual_settings()),
        "different settings never share a parameter form"
    );
}

#[test]
fn another_engine_a_missing_or_a_bad_value_is_refused() {
    let good = centerline_to_engine(&image_recipe(), &CenterlineSettings::default());
    let mut other = good.clone();
    other.engine = ENGINE_VERSION.into();
    assert!(centerline_from_engine(&other).is_err(), "a contour recipe is not a centerline recipe");
    assert!(recipe_from_engine(&good).is_err(), "and the other way round");
    let mut bad = good.clone();
    bad.params.insert("centerline.tolerance_mm".into(), ParamValue::Text("fine".into()));
    assert!(centerline_from_engine(&bad).is_err());
    let mut negative = good.clone();
    negative.params.insert("centerline.tolerance_mm".into(), ParamValue::Number(-1.0));
    assert!(centerline_from_engine(&negative).is_err(), "validate() applies");
    let mut missing = good;
    missing.params.remove("centerline.max_join_factor");
    assert!(centerline_from_engine(&missing).is_err());
    assert!(centerline_from_engine(&EngineRecipe::new(CENTERLINE_ENGINE_VERSION)).is_err());
}

#[test]
fn a_centerline_result_becomes_one_trace_command() {
    let result = run("stroke-t", CenterlineSettings::default());
    let strokes = result.strokes();
    assert!(!strokes.is_empty());
    let asset = ContentHash::from_bytes([9; 32]);
    let placement = Affine::translate(Vector::new(2.0, 3.0)).unwrap();
    let TraceCommand::Create { definition, outputs } =
        centerline_command(&result, &image_recipe(), asset, "Schrift", placement, None).unwrap()
    else {
        panic!("a new trace is a Create")
    };
    assert_eq!(definition.key, result.inputs, "the key is the request key of the centerline run");
    assert_eq!(definition.source, asset);
    let (image, settings) = centerline_from_engine(&definition.recipe).unwrap();
    assert_eq!((image, &settings), (image_recipe(), result.settings()));
    assert_eq!(outputs.len(), strokes.len());
    assert!(outputs.iter().all(|o| !o.path.subpaths().iter().any(|s| s.is_closed())), "strokes stay open paths");

    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Trace(TraceCommand::Create { definition, outputs }),
    };
    s.apply(cmd).unwrap();
    let snap = s.snapshot();
    let (id, _) = snap.engine_layer().traces().next().unwrap();
    let derived = snap
        .objects()
        .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == DerivedOwner::Trace(id)))
        .count();
    assert_eq!(derived, strokes.len());
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().objects().count(), 0, "one undo takes the whole trace away");
    assert!(matches!(
        centerline_command(&result, &image_recipe(), asset, "Schrift", placement, Some(id)).unwrap(),
        TraceCommand::Replace { .. }
    ));
}

#[test]
fn an_operator_edited_result_is_stored_with_the_edit_count_so_it_is_not_taken_for_a_plain_run() {
    let result = run("stroke-t", CenterlineSettings::default());
    let plain =
        centerline_command(&result, &image_recipe(), ContentHash::from_bytes([1; 32]), "x", Affine::IDENTITY, None)
            .unwrap();
    let junction = result.nodes.iter().find(|n| n.degree() >= 3).unwrap();
    let burnburn_trace::Connections::Ambiguous { candidates } = &junction.connections else { panic!() };
    let edited = result.resolve_junction(junction.id, candidates[0].pairs.clone()).unwrap();
    assert_eq!(edited.revision, result.revision + 1);
    let TraceCommand::Create { definition, .. } =
        centerline_command(&edited, &image_recipe(), ContentHash::from_bytes([1; 32]), "x", Affine::IDENTITY, None)
            .unwrap()
    else {
        panic!()
    };
    let TraceCommand::Create { definition: plain_definition, .. } = plain else { panic!() };
    assert_ne!(definition.recipe, plain_definition.recipe, "operator decisions are recorded, not silently dropped");
}
