//! Generator definitions in the document (IM22, C-2) evaluate through `GeneratorEvaluator` in
//! the same transaction: outputs are derived objects with stable keys; a bad parameter keeps the
//! last valid output, names the parameter and blocks production.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorCommand, GeneratorDefinition, GeneratorId, generator_inputs};
use burnburn_document::{
    CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus,
    Evaluator, GeometrySource,
};
use burnburn_generators::{GENERATOR_ENGINE, GeneratorEvaluator, QrEcc, qr_encode};
use burnburn_geometry::{Affine, Vector};
use burnburn_types::{DocumentId, GeometryId, RequestId, StableId};

fn apply(s: &mut DocumentSession, c: GeneratorCommand) {
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Generator(c),
    };
    let ev: [&dyn Evaluator; 1] = [&GeneratorEvaluator];
    s.apply_with(cmd, &ev).unwrap();
}

fn outputs(snap: &DocumentSnapshot, g: GeneratorId) -> Vec<(u64, GeometryId)> {
    snap.objects()
        .filter_map(|(id, o)| match o.source() {
            GeometrySource::Derived(k) if k.owner == DerivedOwner::Generator(g) => Some((k.local, id)),
            _ => None,
        })
        .collect()
}

fn qr_def(module: f64) -> GeneratorDefinition {
    GeneratorDefinition {
        name: "QR".into(),
        generator: "qr".into(),
        recipe: EngineRecipe::new(GENERATOR_ENGINE)
            .with("data", ParamValue::Text("https://burnburn.example/p/42".into()))
            .with("ecc", ParamValue::Text("M".into()))
            .with("module", ParamValue::Number(module))
            .with("quiet_modules", ParamValue::Integer(4))
            .with("min_module", ParamValue::Number(0.2)),
        placement: Affine::translate(Vector::new(100.0, 50.0)).unwrap(),
    }
}

fn first(s: &DocumentSession) -> GeneratorId {
    s.snapshot().engine_layer().generators().next().unwrap().0
}

#[test]
fn qr_generator_evaluates_to_placed_modules_in_the_same_transaction() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(qr_def(0.5)) });
    let g = first(&s);
    let snap = s.snapshot();
    let record = snap.derived_record(DerivedOwner::Generator(g)).unwrap();
    assert_eq!(record.status, EvalStatus::Valid);
    assert_eq!(record.inputs, Some(generator_inputs(g, snap.generator(g).unwrap())));
    let out = outputs(&snap, g);
    assert_eq!(out.len(), 1);
    let obj = snap.object(out[0].1).unwrap();
    let code = qr_encode(b"https://burnburn.example/p/42", QrEcc::M).unwrap();
    let n = code.size();
    let dark = (0..n).flat_map(|r| (0..n).map(move |c| (r, c))).filter(|&(r, c)| code.is_dark(r, c)).count();
    let area: f64 = obj.path().subpaths().iter().map(|sp| sp.signed_area().unwrap().abs()).sum();
    assert!((area - dark as f64 * 0.25).abs() < 1e-9);
    let b = obj.bounds().unwrap();
    assert!(b.min().x >= 102.0 - 1e-9 && b.min().y >= 52.0 - 1e-9, "placed, inside the quiet zone: {b:?}");
    assert!(!snap.production_blocked(out[0].1));
}

#[test]
fn invalid_generator_parameter_keeps_the_last_output_and_names_the_parameter() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(qr_def(0.5)) });
    let g = first(&s);
    let before = outputs(&s.snapshot(), g);
    let path_before = s.snapshot().object(before[0].1).unwrap().path().clone();
    apply(&mut s, GeneratorCommand::Set { generator: g, definition: Box::new(qr_def(0.1)) });
    let snap = s.snapshot();
    assert_eq!(outputs(&snap, g), before);
    assert_eq!(snap.object(before[0].1).unwrap().path(), &path_before, "last valid output stays");
    assert!(snap.production_blocked(before[0].1));
    assert_eq!(
        snap.derived_record(DerivedOwner::Generator(g)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("module".into()) } }
    );
}

fn box_def(thickness: f64) -> GeneratorDefinition {
    GeneratorDefinition {
        name: "Doos".into(),
        generator: "finger-box".into(),
        recipe: EngineRecipe::new(GENERATOR_ENGINE)
            .with("width", ParamValue::Number(100.0))
            .with("depth", ParamValue::Number(60.0))
            .with("height", ParamValue::Number(40.0))
            .with("thickness", ParamValue::Number(thickness))
            .with("finger_width", ParamValue::Number(10.0))
            .with("lid", ParamValue::Text("closed".into()))
            .with("spacing", ParamValue::Number(5.0)),
        placement: Affine::IDENTITY,
    }
}

#[test]
fn finger_box_panels_keep_their_ids_when_the_thickness_changes() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(box_def(3.0)) });
    let g = first(&s);
    let before = outputs(&s.snapshot(), g);
    assert_eq!(before.len(), 6, "closed box: six panels");
    apply(&mut s, GeneratorCommand::Set { generator: g, definition: Box::new(box_def(4.0)) });
    let snap = s.snapshot();
    assert_eq!(outputs(&snap, g), before);
    assert_eq!(snap.derived_record(DerivedOwner::Generator(g)).unwrap().status, EvalStatus::Valid);
    let mut open = box_def(4.0);
    open.recipe = open.recipe.with("lid", ParamValue::Text("open".into()));
    apply(&mut s, GeneratorCommand::Set { generator: g, definition: Box::new(open) });
    assert_eq!(outputs(&s.snapshot(), g).len(), 5, "the top panel goes, the others keep their ids");
}

#[test]
fn unknown_generator_or_other_engine_version_is_an_invalid_definition() {
    for def in [
        GeneratorDefinition { generator: "teapot".into(), ..qr_def(0.5) },
        GeneratorDefinition {
            recipe: EngineRecipe { engine: "burnburn-generators/0".into(), ..qr_def(0.5).recipe },
            ..qr_def(0.5)
        },
    ] {
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        apply(&mut s, GeneratorCommand::Create { definition: Box::new(def) });
        let g = first(&s);
        let snap = s.snapshot();
        assert_eq!(
            snap.derived_record(DerivedOwner::Generator(g)).unwrap().status,
            EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidDefinition }
        );
        assert!(outputs(&snap, g).is_empty());
    }
}
