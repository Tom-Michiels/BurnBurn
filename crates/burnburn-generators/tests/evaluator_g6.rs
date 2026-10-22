//! G6-editor (E10): any barcode type and the living hinge are generators of the document, so a
//! barcode is an object with a stable key (editable after creation), not a baked path.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorCommand, GeneratorDefinition, GeneratorId};
use burnburn_document::{
    CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus,
    Evaluator, GeometrySource,
};
use burnburn_generators::{
    GENERATOR_ENGINE, GeneratorEvaluator, LivingHinge, Symbology, SymbologyParams, encode_symbol, living_hinge,
};
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

fn first(s: &DocumentSession) -> GeneratorId {
    s.snapshot().engine_layer().generators().next().unwrap().0
}

fn symbol_def(symbology: Symbology, payload: &str, module: f64) -> GeneratorDefinition {
    GeneratorDefinition {
        name: "Streepjescode".into(),
        generator: "symbology".into(),
        recipe: EngineRecipe::new(GENERATOR_ENGINE)
            .with("symbology", ParamValue::Text(symbology.stable_name().into()))
            .with("payload", ParamValue::Text(payload.into()))
            .with("module", ParamValue::Number(module))
            .with("min_module", ParamValue::Number(0.2))
            .with("height", ParamValue::Number(12.0)),
        placement: Affine::translate(Vector::new(40.0, 25.0)).unwrap(),
    }
}

#[test]
fn a_barcode_generator_gives_the_domain_symbol_placed_under_one_stable_key() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(symbol_def(Symbology::Code128, "ABC-123", 0.4)) });
    let g = first(&s);
    let snap = s.snapshot();
    assert_eq!(snap.derived_record(DerivedOwner::Generator(g)).unwrap().status, EvalStatus::Valid);
    let out = outputs(&snap, g);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, 0, "the key is fixed");
    // Independent reference: the domain encoder with the same parameters, moved by the placement.
    let mut params = SymbologyParams::new(0.4, 0.2);
    params.height_mm = 12.0;
    let reference = encode_symbol(Symbology::Code128, "ABC-123", &params).unwrap();
    let want = reference.path.bounds().unwrap();
    let got = snap.object(out[0].1).unwrap().bounds().unwrap();
    assert!((got.min().x - (want.min().x + 40.0)).abs() < 1e-9 && (got.min().y - (want.min().y + 25.0)).abs() < 1e-9);
    assert!((got.width() - want.width()).abs() < 1e-9 && (got.height() - want.height()).abs() < 1e-9);
}

#[test]
fn editing_the_payload_keeps_the_object_id() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(symbol_def(Symbology::QrCode, "eerste", 0.5)) });
    let g = first(&s);
    let before = outputs(&s.snapshot(), g);
    apply(
        &mut s,
        GeneratorCommand::Set {
            generator: g,
            definition: Box::new(symbol_def(Symbology::QrCode, "tweede en langer", 0.5)),
        },
    );
    assert_eq!(outputs(&s.snapshot(), g), before, "same key, same id: operations keep pointing at it");
}

#[test]
fn a_bad_payload_or_module_names_its_parameter_and_keeps_the_last_symbol() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(symbol_def(Symbology::Ean, "5901234123457", 0.33)) });
    let g = first(&s);
    let before = outputs(&s.snapshot(), g);
    // A wrong check digit.
    apply(
        &mut s,
        GeneratorCommand::Set { generator: g, definition: Box::new(symbol_def(Symbology::Ean, "5901234123450", 0.33)) },
    );
    let snap = s.snapshot();
    assert_eq!(
        snap.derived_record(DerivedOwner::Generator(g)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("payload".into()) } }
    );
    assert_eq!(outputs(&snap, g), before);
    assert!(snap.production_blocked(before[0].1));
    // A module below the minimum is never rounded up.
    apply(
        &mut s,
        GeneratorCommand::Set { generator: g, definition: Box::new(symbol_def(Symbology::Ean, "5901234123457", 0.1)) },
    );
    assert_eq!(
        s.snapshot().derived_record(DerivedOwner::Generator(g)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("module".into()) } }
    );
}

fn hinge_def(cut_length: f64, height: f64) -> GeneratorDefinition {
    GeneratorDefinition {
        name: "Scharnier".into(),
        generator: "living-hinge".into(),
        recipe: EngineRecipe::new(GENERATOR_ENGINE)
            .with("width", ParamValue::Number(30.0))
            .with("height", ParamValue::Number(height))
            .with("cut_length", ParamValue::Number(cut_length))
            .with("bridge", ParamValue::Number(2.0))
            .with("spacing", ParamValue::Number(3.0)),
        placement: Affine::IDENTITY,
    }
}

#[test]
fn a_living_hinge_generator_is_one_object_holding_every_cut() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(hinge_def(10.0, 40.0)) });
    let g = first(&s);
    let snap = s.snapshot();
    assert_eq!(snap.derived_record(DerivedOwner::Generator(g)).unwrap().status, EvalStatus::Valid);
    let out = outputs(&snap, g);
    assert_eq!(out.len(), 1, "one object, however many cuts");
    let cuts =
        living_hinge(&LivingHinge { width: 30.0, height: 40.0, cut_length: 10.0, bridge: 2.0, spacing: 3.0 }).unwrap();
    let object = snap.object(out[0].1).unwrap();
    assert_eq!(object.path().subpaths().len(), cuts.len());
    assert!(object.path().subpaths().iter().all(|sp| !sp.is_closed()), "cut lines are open");
}

#[test]
fn an_impossible_hinge_names_the_parameter_that_is_too_small() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    apply(&mut s, GeneratorCommand::Create { definition: Box::new(hinge_def(10.0, 40.0)) });
    let g = first(&s);
    apply(&mut s, GeneratorCommand::Set { generator: g, definition: Box::new(hinge_def(10.0, 3.0)) });
    assert_eq!(
        s.snapshot().derived_record(DerivedOwner::Generator(g)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("height".into()) } }
    );
}
