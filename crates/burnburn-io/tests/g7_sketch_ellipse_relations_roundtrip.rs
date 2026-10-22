//! G7-editor S01: the minor-axis dimension of an ellipse, a point on it and a tangent line survive
//! save/reopen (with their parameter text and role), and the project hash sees the dimension.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::Path;

use burnburn_document::sketch::{
    Along, DimensionKind, DimensionRole, Relation, Sketch, SketchCommand, SketchConstraint,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, project_hash, save_project};
use burnburn_types::{DocumentId, RequestId, StableId};

fn sketch(minor_text: &str, role: DimensionRole) -> Sketch {
    let mut k = Sketch::new("Ellips");
    k.set_parameter("kleine_as", "6 mm").unwrap();
    let c = k.point(10.0, 5.0).unwrap();
    let m = k.point(14.0, 5.0).unwrap();
    let e = k.ellipse(c, m, 2.0).unwrap();
    let p = k.point(12.0, 6.6).unwrap();
    let (a, b) = (k.point(5.0, 7.5).unwrap(), k.point(15.0, 7.5).unwrap());
    let line = k.line(a, b).unwrap();
    k.relate(Relation::PointOn { point: p, curve: e }).unwrap();
    k.relate(Relation::Tangent(line, e)).unwrap();
    k.dimension(DimensionKind::Distance { a: c, b: p, along: Along::X }, "2 mm", DimensionRole::Driving).unwrap();
    k.dimension(DimensionKind::MinorDiameter { ellipse: e }, minor_text, role).unwrap();
    k
}

fn project_with(minor_text: &str, role: DimensionRole) -> Project {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let c = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch(minor_text, role)) }),
    };
    s.apply(c).unwrap();
    Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())
}

#[test]
fn the_relations_and_the_minor_axis_survive_save_and_reopen() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/ellipse.burnburn");
    let original = project_with("kleine_as", DimensionRole::Driving);
    save_project(&fs, path, &original).unwrap();
    let back = load_project(&fs, path).unwrap();
    assert_eq!(back.document.sketch_layer(), original.document.sketch_layer());
    let sketch = back.document.sketch_layer().sketches().next().unwrap().1;
    let minor = sketch
        .constraints()
        .find_map(|(_, c)| match c {
            SketchConstraint::Dimension(d) if matches!(d.kind, DimensionKind::MinorDiameter { .. }) => Some(d.clone()),
            _ => None,
        })
        .expect("the minor-axis dimension came back");
    assert_eq!((minor.text.as_str(), minor.role), ("kleine_as", DimensionRole::Driving));
    assert_eq!(sketch.parameters().get("kleine_as"), Some("6 mm"));
    let relations: Vec<_> = sketch
        .constraints()
        .filter_map(|(_, c)| match c {
            SketchConstraint::Relation(r) => Some(*r),
            _ => None,
        })
        .collect();
    assert!(relations.iter().any(|r| matches!(r, Relation::PointOn { .. })));
    assert!(relations.iter().any(|r| matches!(r, Relation::Tangent(..))));
}

#[test]
fn a_driven_minor_axis_keeps_its_role() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/driven.burnburn");
    save_project(&fs, path, &project_with("0 mm", DimensionRole::Driven)).unwrap();
    let back = load_project(&fs, path).unwrap();
    let sketch = back.document.sketch_layer().sketches().next().unwrap().1;
    assert!(sketch.constraints().any(|(_, c)| matches!(
        c,
        SketchConstraint::Dimension(d) if matches!(d.kind, DimensionKind::MinorDiameter { .. }) && d.role == DimensionRole::Driven
    )));
}

#[test]
fn the_project_hash_sees_the_minor_axis_text_and_role() {
    let base = project_hash(&project_with("6 mm", DimensionRole::Driving)).unwrap();
    assert_ne!(base, project_hash(&project_with("7 mm", DimensionRole::Driving)).unwrap());
    assert_ne!(base, project_hash(&project_with("6 mm", DimensionRole::Driven)).unwrap());
}

// --- the solved minor axis in the stored solution ----------------------------------------------------------

mod stored_minor {
    use std::path::Path;

    use burnburn_document::sketch::{Entity, Sketch, SketchCommand, SketchSolution};
    use burnburn_document::{
        CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand, DocumentPatch,
        DocumentSession, DocumentSnapshot, EvalStatus, Evaluator, ObjectKind, OwnerSolution,
    };
    use burnburn_geometry::{Path as GPath, Point};
    use burnburn_io::memory::MemoryFileSystem;
    use burnburn_io::{Project, ProjectUuid, load_project, project_hash, save_project};
    use burnburn_types::{ContentHash, DocumentId, RequestId, StableId};

    /// Solves nothing, but stores `minor_for_ellipses` as the solved radius of every ellipse.
    struct Fake {
        minor: Option<f64>,
    }

    impl Evaluator for Fake {
        fn owner_kind(&self) -> DerivedOwnerKind {
            DerivedOwnerKind::Sketch
        }
        fn evaluate(&self, snap: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
            let DerivedOwner::Sketch(id) = owner else { panic!("only sketch owners") };
            let s = snap.sketch(id).unwrap();
            let mut sol = SketchSolution::default();
            for (eid, e) in s.entities() {
                match e.entity {
                    Entity::Point { at } => {
                        sol.points.insert(eid, at);
                    }
                    Entity::Ellipse { .. } => {
                        if let Some(m) = self.minor {
                            sol.minors.insert(eid, m);
                        }
                    }
                    _ => {}
                }
            }
            DocumentPatch {
                owner,
                inputs: ContentHash::from_bytes([5; 32]),
                outputs: vec![DerivedOutput {
                    key: DerivedKey { owner, local: 0 },
                    kind: ObjectKind::Path,
                    path: GPath::line(Point::new(0.0, 0.0), Point::new(1.0, 0.0)).unwrap(),
                    name: None,
                }],
                status: EvalStatus::Valid,
                solution: OwnerSolution::Sketch(sol),
            }
        }
    }

    fn project(minor: Option<f64>) -> Project {
        let mut k = Sketch::new("Ellips");
        let c = k.point(10.0, 5.0).unwrap();
        let m = k.point(14.0, 5.0).unwrap();
        k.ellipse(c, m, 2.0).unwrap();
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        let cmd = DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(1),
            payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(k) }),
        };
        s.apply_with(cmd, &[&Fake { minor }]).unwrap();
        Project::new(ProjectUuid::from_bytes([4; 16]), (*s.snapshot()).clone())
    }

    #[test]
    fn a_solved_minor_radius_survives_save_and_reopen() {
        let fs = MemoryFileSystem::new();
        fs.create_dir_all_for_test(Path::new("/p"));
        let path = Path::new("/p/minor.burnburn");
        let original = project(Some(3.25));
        save_project(&fs, path, &original).unwrap();
        let back = load_project(&fs, path).unwrap();
        let sketch = back.document.sketch_layer().sketches().next().unwrap().1;
        let minors: Vec<f64> = sketch.solution().unwrap().minors.values().copied().collect();
        assert_eq!(minors, [3.25]);
        assert_eq!(project_hash(&back).unwrap(), project_hash(&original).unwrap());
    }

    #[test]
    fn the_hash_sees_it_and_a_solution_without_one_is_unchanged() {
        let none = project_hash(&project(None)).unwrap();
        let some = project_hash(&project(Some(3.25))).unwrap();
        assert_ne!(none, some);
        assert_ne!(some, project_hash(&project(Some(3.5))).unwrap());
        let fs = MemoryFileSystem::new();
        fs.create_dir_all_for_test(Path::new("/p"));
        let path = Path::new("/p/none.burnburn");
        save_project(&fs, path, &project(None)).unwrap();
        let back = load_project(&fs, path).unwrap();
        let sketch = back.document.sketch_layer().sketches().next().unwrap().1;
        assert!(sketch.solution().unwrap().minors.is_empty());
        assert_eq!(project_hash(&back).unwrap(), none, "no `minors` entry in the file, no change in the hash");
    }

    #[test]
    fn a_stored_minor_for_something_that_is_no_ellipse_is_refused_on_load() {
        // Not constructible through the document: SketchSolution is checked when it is set.
        let mut k = Sketch::new("x");
        let p = k.point(0.0, 0.0).unwrap();
        let mut sol = SketchSolution::default();
        sol.minors.insert(p, 1.0);
        let r = Sketch::from_parts(
            "x".into(),
            k.entities().map(|(i, e)| (i, *e)).collect(),
            Vec::new(),
            k.parameters().clone(),
            k.next_entity_id().raw(),
            k.next_constraint_id().raw(),
            Some(sol),
        );
        assert!(r.is_err());
    }
}
