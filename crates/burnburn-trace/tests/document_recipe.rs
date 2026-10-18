//! The trace recipe as document data (C-9): every field survives the engine-parameter form (so a
//! reopened project re-traces identically), and a result becomes one trace command whose key is
//! the result's recipe key and whose outputs are the components, keyed by outer contour id.

#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use std::sync::Arc;

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::trace::TraceCommand;
use burnburn_document::{CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, GeometrySource};
use burnburn_geometry::{Affine, Vector};
use burnburn_image::{
    AlphaPolicy, ImageRecipe, Morphology, Perspective, PixelRect, PixelScale, QuarterTurn, RegionMask, RegionMode,
    RegionShape, Threshold,
};
use burnburn_trace::{
    ContourSettings, DiagonalPolicy, ENGINE_VERSION, FitSettings, TraceRecipe, TraceRequest, recipe_from_engine,
    recipe_to_engine, trace, trace_command,
};
use burnburn_types::{CancellationToken, ContentHash, DocumentId, RequestId, RevisionId, StableId, TaskId};
use support::*;

fn every_field() -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::from_mm_per_pixel(0.1, 1.0 / 3.0).unwrap()),
            crop: Some(PixelRect { x: 1, y: 2, width: 30, height: 40 }),
            rotation: QuarterTurn::CounterClockwise90,
            alpha: AlphaPolicy::CompositeOver { background: 17 },
            median_radius: 2,
            threshold: Threshold::Adaptive { radius: 5, offset: 9 },
            invert: true,
            regions: vec![
                RegionMask {
                    mode: RegionMode::Exclude,
                    shape: RegionShape::Rect(PixelRect { x: 0, y: 0, width: 3, height: 4 }),
                },
                RegionMask {
                    mode: RegionMode::Include,
                    shape: RegionShape::Polygon(vec![[0.5, 0.25], [10.0, 0.0], [5.0, 1.0 / 7.0]]),
                },
            ],
            morphology: vec![Morphology::Open(1), Morphology::Close(3)],
            perspective: Some(Perspective {
                points: vec![
                    ([0.0, 0.0], [0.0, 0.0]),
                    ([10.0, 0.5], [10.0, 0.0]),
                    ([9.5, 10.0], [10.0, 10.0]),
                    ([0.0, 10.0], [0.0, 10.0]),
                ],
                width: 11,
                height: 12,
            }),
        },
        contour: ContourSettings { diagonal: DiagonalPolicy::Separate, min_area_mm2: 0.75 },
        fit: FitSettings { tolerance_mm: 0.02, corner_angle_deg: 45.0, corner_window_px: 3.5, curves: false },
    }
}

#[test]
fn every_recipe_field_survives_the_engine_parameter_form_with_the_same_key() {
    let source = ContentHash::from_bytes([5; 32]);
    for recipe in [TraceRecipe::default(), every_field()] {
        let engine = recipe_to_engine(&recipe);
        assert_eq!(engine.engine, ENGINE_VERSION);
        engine.validate().unwrap();
        let back = recipe_from_engine(&engine).unwrap();
        assert_eq!(back, recipe);
        assert_eq!(back.key(source).unwrap(), recipe.key(source).unwrap());
    }
    // Two different recipes never share a parameter form.
    assert_ne!(recipe_to_engine(&TraceRecipe::default()), recipe_to_engine(&every_field()));
}

#[test]
fn parameters_of_another_engine_or_with_a_bad_value_are_refused() {
    let mut other = recipe_to_engine(&every_field());
    other.engine = "burnburn-trace/contour-0".into();
    assert!(recipe_from_engine(&other).is_err(), "another engine version is never silently reused");
    let mut bad = recipe_to_engine(&every_field());
    bad.params.insert("fit.tolerance_mm".into(), ParamValue::Text("fine".into()));
    assert!(recipe_from_engine(&bad).is_err());
    let mut missing = recipe_to_engine(&every_field());
    missing.params.remove("fit.curves");
    assert!(recipe_from_engine(&missing).is_err());
    assert!(recipe_from_engine(&EngineRecipe::new(ENGINE_VERSION)).is_err());
}

#[test]
fn a_trace_result_becomes_one_trace_command_keyed_by_contour_ids() {
    let source = rasterize(200, 120, 0.1, |x, y| disc(4.0, 6.0, 3.0)(x, y) || disc(12.0, 6.0, 2.5)(x, y));
    let recipe = TraceRecipe {
        image: ImageRecipe { scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()), ..ImageRecipe::default() },
        ..TraceRecipe::default()
    };
    let request = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(source),
        recipe: recipe.clone(),
    };
    let result = trace(&request, &CancellationToken::never()).unwrap();
    let asset = ContentHash::from_bytes([8; 32]);
    let placement = Affine::translate(Vector::new(5.0, 0.0)).unwrap();
    let TraceCommand::Create { definition, outputs } = trace_command(&result, asset, "Logo", placement, None).unwrap()
    else {
        panic!("a new trace is a Create")
    };
    assert_eq!(definition.key, result.recipe_key);
    assert_eq!(definition.source, asset);
    assert_eq!(recipe_from_engine(&definition.recipe).unwrap(), recipe);
    assert_eq!(outputs.len(), result.components.len());
    let keys: Vec<u64> = result.components.iter().map(|c| result.contours[c.outer].id.0).collect();
    assert_eq!(outputs.iter().map(|o| o.local).collect::<Vec<_>>(), keys);

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
    assert_eq!(derived, 2);
    assert!(matches!(
        trace_command(&result, asset, "Logo", placement, Some(id)).unwrap(),
        TraceCommand::Replace { .. }
    ));
}
