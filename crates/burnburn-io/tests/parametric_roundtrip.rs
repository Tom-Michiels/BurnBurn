//! Public contract (W1-L2b): parametric basic shapes survive save and reopen with their
//! definition, plain objects are written exactly as before, and a file whose definition and
//! outline disagree (or whose values are invalid) is refused with the JSON path.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::cell::RefCell;
use std::path::Path;

use burnburn_document::{
    CommandPayload, DocumentCommand, DocumentSession, GeometryObject, GeometrySource, ObjectKind, ShapeCommand,
};
use burnburn_geometry::{Affine, Bounds, ParametricShape, Point, ShapeParams, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{
    AssetStore, LoadError, Project, ProjectUuid, decode_container, encode_container, load_project, save_project,
};
use burnburn_types::{Angle, DocumentId, GeometryId, RequestId, StableId};
use common::{rewrite_json, sample_project};

fn project_with(session: &DocumentSession) -> Project {
    Project {
        uuid: ProjectUuid::parse("0b0b0b0b-0001-4000-8000-000000000777").unwrap(),
        document: (*session.snapshot()).clone(),
        assets: AssetStore::default(),
        profiles: Vec::new(),
        revisions: Vec::new(),
        library_links: Vec::new(),
        production: None,
        layer_slots: Vec::new(),
    }
}

fn create(s: &mut DocumentSession, params: ShapeParams, b: Bounds) -> GeometryId {
    let shape = ParametricShape::fit(params, b).unwrap();
    s.apply(DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Shape(ShapeCommand::Create { shape, name: "vorm".into() }),
    })
    .unwrap()
    .changes
    .created[0]
}

fn all_kinds() -> (DocumentSession, Vec<GeometryId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let b = |x: f64| Bounds::from_corners(Point::new(x, 0.1 + 0.2), Point::new(x + 30.0, 1.0 / 3.0 + 25.0));
    let ids = vec![
        create(&mut s, ShapeParams::Polygon { sides: 5 }, b(0.0)),
        create(&mut s, ShapeParams::Star { points: 7, ratio: 0.1 + 0.2 }, b(40.0)),
        create(&mut s, ShapeParams::DoubleStar { points: 6, ratio: 0.45 }, b(80.0)),
        create(&mut s, ShapeParams::RoundedRectangle { width: 1.0, height: 1.0, radius: 2.5 }, b(120.0)),
    ];
    // A rotated, mirrored and moved placement must survive exactly too.
    let m = Affine::rotate(Angle::from_degrees(33.0).unwrap())
        .then(&Affine::mirror_x())
        .then(&Affine::translate(Vector::new(-7.0, 11.5)).unwrap());
    s.apply(DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(2),
        payload: CommandPayload::Transform { targets: ids.clone(), transform: m },
    })
    .unwrap();
    (s, ids)
}

#[test]
fn definitions_roundtrip_exactly_and_stay_editable_after_reopen() {
    let (s, ids) = all_kinds();
    let original = project_with(&s);
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    save_project(&fs, Path::new("/p/shapes.burnburn"), &original).unwrap();
    let back = load_project(&fs, Path::new("/p/shapes.burnburn")).unwrap();
    for id in &ids {
        let (a, b) = (original.document.object(*id).unwrap(), back.document.object(*id).unwrap());
        assert!(a.parametric().is_some());
        assert_eq!(a, b, "{id:?}");
    }
    // Reopened document: parameters can still be changed (not just displayed).
    let mut reopened = DocumentSession::from_snapshot(DocumentId::from_raw(1), back.document.clone());
    reopened
        .apply(DocumentCommand {
            document: reopened.id(),
            expected: reopened.revision(),
            request: RequestId::from_raw(3),
            payload: CommandPayload::Shape(ShapeCommand::SetParams {
                target: ids[0],
                params: ShapeParams::Polygon { sides: 9 },
            }),
        })
        .unwrap();
    assert_eq!(reopened.snapshot().object(ids[0]).unwrap().path().subpaths()[0].segments().len() + 1, 9);
}

#[test]
fn plain_objects_gain_no_field_and_shapes_use_the_documented_tags() {
    let plain = encode_container(&sample_project()).unwrap();
    let seen = RefCell::new(String::new());
    rewrite_json(&plain, "document.json", |v| *seen.borrow_mut() = v.to_string());
    assert!(!seen.borrow().contains("parametric"), "plain files must stay byte-compatible");

    let (s, ids) = all_kinds();
    let bytes = encode_container(&project_with(&s)).unwrap();
    let doc = RefCell::new(serde_json::Value::Null);
    rewrite_json(&bytes, "document.json", |v| *doc.borrow_mut() = v.clone());
    let doc = doc.into_inner();
    let objects = doc["objects"].as_array().unwrap();
    let by_id = |id: GeometryId| objects.iter().find(|o| o["id"] == id.raw()).unwrap();
    assert_eq!(by_id(ids[0])["parametric"]["type"], "polygon");
    assert_eq!(by_id(ids[0])["parametric"]["sides"], 5);
    assert_eq!(by_id(ids[1])["parametric"]["type"], "star");
    assert_eq!(by_id(ids[2])["parametric"]["type"], "double_star");
    assert_eq!(by_id(ids[3])["parametric"]["type"], "rounded_rectangle");
    assert_eq!(by_id(ids[3])["parametric"]["frame"].as_array().unwrap().len(), 6);
    assert_eq!(by_id(ids[0])["kind"], "polygon");
}

fn tampered(edit: impl Fn(&mut serde_json::Value)) -> Result<Project, LoadError> {
    let (s, _) = all_kinds();
    let bytes = encode_container(&project_with(&s)).unwrap();
    let find = |v: &mut serde_json::Value, ty: &str| -> usize {
        v["objects"].as_array().unwrap().iter().position(|o| o["parametric"]["type"] == ty).unwrap()
    };
    let edited = rewrite_json(&bytes, "document.json", |v| {
        let i = find(&mut v.clone(), "star");
        let mut obj = v["objects"][i].take();
        edit(&mut obj);
        v["objects"][i] = obj;
    });
    decode_container(&edited)
}

fn star_index() -> usize {
    let (s, _) = all_kinds();
    let bytes = encode_container(&project_with(&s)).unwrap();
    let doc = RefCell::new(serde_json::Value::Null);
    rewrite_json(&bytes, "document.json", |v| *doc.borrow_mut() = v.clone());
    let doc = doc.into_inner();
    doc["objects"].as_array().unwrap().iter().position(|o| o["parametric"]["type"] == "star").unwrap()
}

#[test]
fn invalid_definitions_are_refused_with_their_json_path() {
    let i = star_index();
    let err = tampered(|o| o["parametric"]["points"] = serde_json::json!(2)).unwrap_err();
    assert!(
        matches!(&err, LoadError::InvalidValue { path, .. } if path == &format!("objects[{i}].parametric")),
        "{err:?}"
    );
    let err = tampered(|o| o["parametric"]["ratio"] = serde_json::json!(1.5)).unwrap_err();
    assert!(
        matches!(&err, LoadError::InvalidValue { path, .. } if path == &format!("objects[{i}].parametric")),
        "{err:?}"
    );
    // A damaged number (serde_json writes NaN as null) names the exact field.
    let err = tampered(|o| o["parametric"]["frame"][2] = serde_json::Value::Null).unwrap_err();
    assert_eq!(
        err,
        LoadError::InvalidValue {
            path: format!("objects[{i}].parametric.frame[2]"),
            reason: "not a finite number".into()
        }
    );
    // Unknown shape type or field: reported, never dropped.
    let err = tampered(|o| o["parametric"]["type"] = serde_json::json!("heptagram")).unwrap_err();
    assert!(matches!(err, LoadError::Json { .. } | LoadError::UnknownField { .. }), "{err:?}");
    let err = tampered(|o| o["parametric"]["spikes"] = serde_json::json!(3)).unwrap_err();
    assert!(matches!(err, LoadError::Json { .. } | LoadError::UnknownField { .. }), "{err:?}");
}

#[test]
fn a_definition_that_disagrees_with_its_outline_is_refused() {
    let i = star_index();
    // Moving the stored outline by 1 mm but not the definition: the file contradicts itself.
    let err = tampered(|o| {
        let start = &mut o["subpaths"][0]["start"];
        start[0] = serde_json::json!(start[0].as_f64().unwrap() + 1.0);
    })
    .unwrap_err();
    match err {
        LoadError::InvalidValue { path, reason } => {
            assert_eq!(path, format!("objects[{i}].parametric"));
            assert!(reason.contains("OutlineMismatch"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_plain_object_with_a_shape_kind_still_loads_without_definition() {
    let circle = burnburn_geometry::Path::circle(Point::new(5.0, 5.0), 2.0).unwrap();
    let obj = GeometryObject::new(GeometryId::from_raw(1), "s".into(), ObjectKind::Star, GeometrySource::Drawn, circle);
    let doc = burnburn_document::DocumentSnapshot::from_parts(vec![obj], 2).unwrap();
    let project = Project {
        uuid: ProjectUuid::parse("0b0b0b0b-0001-4000-8000-000000000778").unwrap(),
        document: doc,
        assets: AssetStore::default(),
        profiles: Vec::new(),
        revisions: Vec::new(),
        library_links: Vec::new(),
        production: None,
        layer_slots: Vec::new(),
    };
    let back = decode_container(&encode_container(&project).unwrap()).unwrap();
    assert!(back.document.object(GeometryId::from_raw(1)).unwrap().parametric().is_none());
}
