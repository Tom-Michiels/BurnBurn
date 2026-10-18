//! Sketch data (contract §12) survives save/reopen: definition with expression texts, the last
//! valid solution, evaluation records and derived outputs with their keys. Contributed by the
//! Sketch role (DR-12); the evaluator here is a fake so io stays below `burnburn-sketch`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::Path;

use burnburn_document::sketch::{
    Along, DimensionKind, DimensionRole, Relation, Sketch, SketchCommand, SketchId, SketchSolution,
};
use burnburn_document::{
    CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand, DocumentPatch,
    DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_geometry::{EllipticalArc, PathBuilder, Point};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{AssetStore, Project, ProjectUuid, load_project, project_hash, save_project};
use burnburn_types::{Angle, ContentHash, DocumentId, RequestId, StableId};

/// Valid while parameter `w` parses; output: a slot with an arc end, plus the solution.
struct Fake;

impl Evaluator for Fake {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Sketch
    }
    fn evaluate(&self, snap: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let DerivedOwner::Sketch(id) = owner else { panic!("only sketch owners") };
        let s = snap.sketch(id).unwrap();
        let inputs = ContentHash::from_bytes([7; 32]);
        let Some(w) = s.parameters().get("w").and_then(|t| t.trim_end_matches(" mm").parse::<f64>().ok()) else {
            return DocumentPatch {
                owner,
                inputs,
                outputs: vec![],
                status: EvalStatus::Failed {
                    diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("w".into()) },
                },
                solution: OwnerSolution::Keep,
            };
        };
        let arc = EllipticalArc::circle(
            Point::new(w, 5.0),
            5.0,
            Angle::from_degrees(-90.0).unwrap(),
            Angle::from_degrees(180.0).unwrap(),
        )
        .unwrap();
        let path = PathBuilder::new()
            .move_to(Point::new(0.1 + 0.2, 0.0))
            .line_to(Point::new(w, 0.0))
            .arc(arc)
            .line_to(Point::new(0.0, 10.0))
            .close()
            .build()
            .unwrap();
        let mut sol = SketchSolution::default();
        for (eid, e) in s.entities() {
            if let burnburn_document::sketch::Entity::Point { at } = e.entity {
                sol.points.insert(eid, Point::new(at.x / 3.0, at.y));
            }
            if let burnburn_document::sketch::Entity::Circle { radius, .. } = e.entity {
                sol.radii.insert(eid, radius + 1.0 / 7.0);
            }
        }
        sol.dof = 2;
        DocumentPatch {
            owner,
            inputs,
            outputs: vec![DerivedOutput {
                key: DerivedKey { owner, local: 3 },
                kind: ObjectKind::Path,
                path,
                name: Some("Sleuf".into()),
            }],
            status: EvalStatus::Valid,
            solution: OwnerSolution::Sketch(sol),
        }
    }
}

fn cmd(s: &DocumentSession, c: SketchCommand) -> DocumentCommand {
    DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(c),
    }
}

fn every_kind_sketch() -> Sketch {
    let mut k = Sketch::new("Alles");
    let a = k.point(0.0, 0.0).unwrap();
    let b = k.point(120.0, 0.1).unwrap();
    let c = k.point(60.0, 40.0).unwrap();
    let d = k.point(70.0, 40.0).unwrap();
    let l = k.line(a, b).unwrap();
    let axis = k.construction_line(c, d).unwrap();
    let circle = k.circle(c, 2.0).unwrap();
    let arc = k.arc(c, d, b, true).unwrap();
    for r in [
        Relation::Coincident(a, b),
        Relation::PointOn { point: c, curve: l },
        Relation::Horizontal(l),
        Relation::Vertical(axis),
        Relation::Parallel(l, axis),
        Relation::Perpendicular(l, axis),
        Relation::Tangent(l, arc),
        Relation::Equal(circle, arc),
        Relation::Midpoint { point: c, line: l },
        Relation::Symmetric { a, b, about: axis },
        Relation::Concentric(circle, arc),
        Relation::Fix { point: a, at: Point::new(0.0, -1.0 / 3.0) },
    ] {
        k.relate(r).unwrap();
    }
    k.dimension(DimensionKind::Distance { a, b, along: Along::X }, "w", DimensionRole::Driving).unwrap();
    k.dimension(DimensionKind::Distance { a, b: c, along: Along::Direct }, "", DimensionRole::Driven).unwrap();
    k.dimension(DimensionKind::Length { line: l }, "w / 2 + 1in", DimensionRole::Driving).unwrap();
    k.dimension(DimensionKind::Radius { curve: arc }, "3 mm", DimensionRole::Driven).unwrap();
    k.dimension(DimensionKind::Diameter { curve: circle }, "4 mm", DimensionRole::Driving).unwrap();
    k.dimension(DimensionKind::Angle { from: l, to: axis }, "90 deg", DimensionRole::Driving).unwrap();
    k.set_parameter("w", "120 mm").unwrap();
    k.set_parameter("gat", "w/2 - 15 mm").unwrap();
    k
}

fn saved_and_reopened(doc: DocumentSnapshot) -> (Project, Project) {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/sketch.burnburn");
    let original = Project::new(ProjectUuid::from_bytes([3; 16]), doc);
    let _ = AssetStore::default();
    save_project(&fs, path, &original).unwrap();
    let back = load_project(&fs, path).unwrap();
    (original, back)
}

#[test]
fn sketch_survives_save_and_reopen_with_identical_derived_geometry() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let out = s.apply_with(cmd(&s, SketchCommand::Create { sketch: Box::new(every_kind_sketch()) }), &[&Fake]).unwrap();
    let valid = out.changes.sketches[0];
    // A second sketch whose last evaluation failed: its record and last valid output must survive too.
    let mut k = Sketch::new("Kapot");
    let p = k.point(1.0, 2.0).unwrap();
    k.relate(Relation::Fix { point: p, at: Point::new(1.0, 2.0) }).unwrap();
    k.set_parameter("w", "10 mm").unwrap();
    let out = s.apply_with(cmd(&s, SketchCommand::Create { sketch: Box::new(k) }), &[&Fake]).unwrap();
    let failed = out.changes.sketches[0];
    let c = cmd(&s, SketchCommand::SetParameter { sketch: failed, name: "w".into(), text: "kapot".into() });
    s.apply_with(c, &[&Fake]).unwrap();

    let snap = (*s.snapshot()).clone();
    let (original, back) = saved_and_reopened(snap.clone());
    let doc = &back.document;
    assert_eq!(doc.sketch_layer(), snap.sketch_layer(), "definitions, solutions and records");
    let a: Vec<_> = snap.objects().map(|(_, o)| o.clone()).collect();
    let b: Vec<_> = doc.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(b, a, "derived outputs keep id, key and bit-identical geometry");
    for (id, _) in doc.objects() {
        assert_eq!(doc.production_blocked(id), snap.production_blocked(id));
    }
    assert!(doc.objects().any(|(id, _)| doc.production_blocked(id)), "the failed sketch stays blocked");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&original).unwrap());
    assert!(doc.sketch(valid).unwrap().solution().is_some());
    let _: SketchId = failed;
}

#[test]
fn project_hash_covers_sketch_definitions() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let out = s.apply(cmd(&s, SketchCommand::Create { sketch: Box::new(every_kind_sketch()) })).unwrap();
    let id = out.changes.sketches[0];
    let before = project_hash(&Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())).unwrap();
    s.apply(cmd(&s, SketchCommand::SetParameter { sketch: id, name: "w".into(), text: "150 mm".into() })).unwrap();
    let after = project_hash(&Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())).unwrap();
    assert_ne!(before, after);
}
