//! Traces and generators (C-2, C-9) survive save and reopen: definition (source asset, recipe
//! with engine version, key, placement), evaluation record and derived outputs with their keys.
//! Additive in schema 2: files without them are unchanged (see `migration_fixtures.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorCommand, GeneratorDefinition, generator_inputs};
use burnburn_document::trace::{TraceCommand, TraceDefinition, TraceOutput};
use burnburn_document::{
    CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand, DocumentPatch,
    DocumentSession, DocumentSnapshot, EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_geometry::{Affine, Path as GPath, Point, Rect, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{AssetStore, LoadError, Project, ProjectUuid, decode_container, encode_container, load_project};
use burnburn_io::{project_hash, save_project};
use burnburn_types::{ContentHash, DocumentId, Length, RequestId, StableId};

struct FakeGenerator;

impl Evaluator for FakeGenerator {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Generator
    }
    fn evaluate(&self, snap: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let DerivedOwner::Generator(id) = owner else { panic!("only generators") };
        let def = snap.generator(id).unwrap();
        DocumentPatch {
            owner,
            inputs: generator_inputs(id, def),
            outputs: vec![DerivedOutput {
                key: DerivedKey { owner, local: 3 },
                kind: ObjectKind::Path,
                path: square(1.0 / 3.0, 12.5).transform(&def.placement),
                name: Some("bar".into()),
            }],
            status: EvalStatus::Valid,
            solution: OwnerSolution::Keep,
        }
    }
}

fn square(x: f64, size: f64) -> GPath {
    let l = Length::from_mm(size).unwrap();
    GPath::rectangle(Rect::new(Point::new(x, 0.1 + 0.2), l, l).unwrap())
}

fn cmd(s: &DocumentSession, payload: CommandPayload) -> DocumentCommand {
    DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload }
}

fn every_kind_recipe(engine: &str) -> EngineRecipe {
    EngineRecipe::new(engine)
        .with("fit.tolerance_mm", ParamValue::Number(1.0 / 3.0))
        .with("image.median_radius", ParamValue::Integer(-2))
        .with("fit.curves", ParamValue::Bool(false))
        .with("image.threshold", ParamValue::Text("adaptive".into()))
        .with(
            "image.regions",
            ParamValue::List(vec![ParamValue::List(vec![ParamValue::Number(0.5), ParamValue::Integer(4)])]),
        )
}

/// A project with one trace (two contours, moved) and one evaluated generator.
fn project() -> Project {
    let mut assets = AssetStore::default();
    let png = assets.add("image/png", b"\x89PNG fake source pixels");
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let trace = TraceDefinition {
        name: "Logo".into(),
        source: png,
        recipe: every_kind_recipe("burnburn-trace/contour-1"),
        key: ContentHash::from_bytes([3; 32]),
        placement: Affine::translate(Vector::new(10.0 / 3.0, -2.5)).unwrap(),
    };
    let outputs = vec![
        TraceOutput { local: 4, kind: ObjectKind::Path, path: square(0.0, 10.0), name: Some("c4".into()) },
        TraceOutput { local: 9, kind: ObjectKind::Path, path: square(20.0, 1.0 / 7.0), name: None },
    ];
    let c = TraceCommand::Create { definition: Box::new(trace), outputs };
    s.apply(cmd(&s, CommandPayload::Trace(c))).unwrap();
    let def = GeneratorDefinition {
        name: "Code".into(),
        generator: "code128".into(),
        recipe: every_kind_recipe("burnburn-generators/1").with("data", ParamValue::Text("AB-12".into())),
        placement: Affine::translate(Vector::new(50.0, 0.0)).unwrap(),
    };
    let c = GeneratorCommand::Create { definition: Box::new(def) };
    s.apply_with(cmd(&s, CommandPayload::Generator(c)), &[&FakeGenerator]).unwrap();
    let mut p = Project::new(ProjectUuid::from_bytes([6; 16]), (*s.snapshot()).clone());
    p.assets = assets;
    p
}

#[test]
fn trace_and_generator_survive_save_and_reopen_with_records_and_outputs() {
    let original = project();
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/engines.burnburn");
    save_project(&fs, path, &original).unwrap();
    let back = load_project(&fs, path).unwrap();
    assert_eq!(back.document.engine_layer(), original.document.engine_layer(), "definitions and records");
    let a: Vec<_> = original.document.objects().map(|(_, o)| o.clone()).collect();
    let b: Vec<_> = back.document.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(b, a, "outputs keep id, owner, key and bit-identical geometry");
    assert!(back.document.objects().all(|(id, _)| !back.document.production_blocked(id)));
    assert_eq!(project_hash(&back).unwrap(), project_hash(&original).unwrap());
    // The reopened session keeps working: the trace can be moved by its definition.
    let trace = back.document.engine_layer().traces().next().unwrap().0;
    let mut s = DocumentSession::from_snapshot(DocumentId::from_raw(2), back.document.clone());
    let c = TraceCommand::SetPlacement { trace, placement: Affine::IDENTITY };
    s.apply(cmd(&s, CommandPayload::Trace(c))).unwrap();
}

#[test]
fn project_hash_covers_trace_recipe_and_generator_parameters() {
    let original = project();
    let base = project_hash(&original).unwrap();
    let mut s = DocumentSession::from_snapshot(DocumentId::from_raw(1), original.document.clone());
    let (g, def) = {
        let snap = s.snapshot();
        let (g, d) = snap.engine_layer().generators().next().unwrap();
        (g, d.clone())
    };
    let mut changed = def.clone();
    changed.recipe = changed.recipe.with("data", ParamValue::Text("AB-13".into()));
    let c = GeneratorCommand::Set { generator: g, definition: Box::new(changed) };
    s.apply_with(cmd(&s, CommandPayload::Generator(c)), &[&FakeGenerator]).unwrap();
    let mut p = Project::new(original.uuid, (*s.snapshot()).clone());
    p.assets = original.assets.clone();
    assert_ne!(project_hash(&p).unwrap(), base);
}

#[test]
fn trace_whose_source_asset_is_missing_is_refused_on_load() {
    let mut p = project();
    p.assets = AssetStore::default();
    let bytes = encode_container(&p).unwrap();
    assert!(matches!(decode_container(&bytes), Err(LoadError::MissingAsset(_))));
}
