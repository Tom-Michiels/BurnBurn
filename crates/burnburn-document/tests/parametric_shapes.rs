//! Public contract: parametric basic shapes in the document (W1-L2b). A shape stays editable
//! (identity, placement, parameters) until it is converted to a plain path; every command is one
//! transaction and one history entry, a failed command changes nothing, undo restores content.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::{
    Anchor, Axis, CommandError, CommandPayload, DocumentCommand, DocumentSession, GeometryCommand, ObjectKind,
    ShapeCommand, ShapeError, ShapeLabel, UndoLabel,
};
use burnburn_geometry::{
    Affine, Bounds, NodeEdit, ParametricShape, Point, ShapeParamError, ShapeParams, Vector,
};
use burnburn_types::{Angle, CanonicalEncode, CanonicalWriter, DocumentId, GeometryId, Length, RequestId, StableId};

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<burnburn_document::CommandOutcome, CommandError> {
    s.apply(DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload,
    })
}

fn shape_cmd(s: &mut DocumentSession, c: ShapeCommand) -> Result<burnburn_document::CommandOutcome, CommandError> {
    run(s, CommandPayload::Shape(c))
}

fn bounds(x0: f64, y0: f64, x1: f64, y1: f64) -> Bounds {
    Bounds::from_corners(Point::new(x0, y0), Point::new(x1, y1))
}

fn create(s: &mut DocumentSession, params: ShapeParams, b: Bounds, name: &str) -> GeometryId {
    let shape = ParametricShape::fit(params, b).unwrap();
    shape_cmd(s, ShapeCommand::Create { shape, name: name.into() }).unwrap().changes.created[0]
}

fn encoded(o: &burnburn_document::GeometryObject) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    o.encode(&mut w);
    w.finish().unwrap()
}

fn vertex_count(s: &DocumentSession, id: GeometryId) -> usize {
    s.snapshot().object(id).unwrap().path().subpaths()[0].segments().len() + 1
}

#[test]
fn created_shapes_are_named_objects_of_their_kind_with_their_definition() {
    let mut s = session();
    let tri = create(&mut s, ShapeParams::Polygon { sides: 3 }, bounds(0.0, 0.0, 30.0, 26.0), "Driehoek");
    let star = create(&mut s, ShapeParams::Star { points: 5, ratio: 0.4 }, bounds(40.0, 0.0, 80.0, 40.0), "");
    let dbl = create(&mut s, ShapeParams::DoubleStar { points: 6, ratio: 0.5 }, bounds(0.0, 50.0, 40.0, 90.0), "");
    let rr = create(
        &mut s,
        ShapeParams::RoundedRectangle { width: 1.0, height: 1.0, radius: 2.0 },
        bounds(50.0, 50.0, 90.0, 80.0),
        "",
    );
    let snap = s.snapshot();
    let kinds: Vec<_> = [tri, star, dbl, rr].iter().map(|id| snap.object(*id).unwrap().kind()).collect();
    assert_eq!(kinds, vec![ObjectKind::Polygon, ObjectKind::Star, ObjectKind::Star, ObjectKind::Rectangle]);
    let t = snap.object(tri).unwrap();
    assert_eq!(t.name(), "Driehoek");
    assert_eq!(*t.parametric().unwrap().params(), ShapeParams::Polygon { sides: 3 });
    // The stored outline is the resolved definition, bounds exactly as drawn.
    assert_eq!(**t.path(), t.parametric().unwrap().to_path().unwrap());
    let b = t.bounds().unwrap();
    assert!((b.width() - 30.0).abs() < 1e-9 && (b.height() - 26.0).abs() < 1e-9);
    // The rounded rectangle took its size from the drag and kept the (clamped) radius.
    assert_eq!(
        *snap.object(rr).unwrap().parametric().unwrap().params(),
        ShapeParams::RoundedRectangle { width: 40.0, height: 30.0, radius: 2.0 }
    );
    assert_eq!(snap.object(dbl).unwrap().path().subpaths().len(), 2);
    assert_eq!(s.undo_label(), Some(UndoLabel::Shape(ShapeLabel::Create)));
}

#[test]
fn creation_is_one_undoable_entry_that_restores_identity_on_redo() {
    let mut s = session();
    let id = create(&mut s, ShapeParams::Polygon { sides: 5 }, bounds(0.0, 0.0, 20.0, 20.0), "");
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().object(id).is_none());
    s.redo(s.revision()).unwrap();
    let again = s.snapshot();
    assert_eq!(again.object(id).unwrap().parametric().unwrap().params(), &ShapeParams::Polygon { sides: 5 });
    assert!(!s.can_undo() || s.undo(s.revision()).is_ok());
}

#[test]
fn editing_parameters_regenerates_the_outline_and_keeps_identity_and_placement() {
    let mut s = session();
    let id = create(&mut s, ShapeParams::Polygon { sides: 5 }, bounds(10.0, 10.0, 50.0, 50.0), "P");
    let frame = *s.snapshot().object(id).unwrap().parametric().unwrap().frame();
    let before = s.revision();
    let out = shape_cmd(&mut s, ShapeCommand::SetParams { target: id, params: ShapeParams::Polygon { sides: 8 } })
        .unwrap();
    assert_eq!(out.changes.modified, vec![id]);
    assert_eq!(s.undo_label(), Some(UndoLabel::Shape(ShapeLabel::SetParams)));
    let snap = s.snapshot();
    let obj = snap.object(id).unwrap();
    assert_eq!(obj.name(), "P");
    assert_eq!(*obj.parametric().unwrap().frame(), frame);
    assert_eq!(vertex_count(&s, id), 8);
    assert_eq!(**obj.path(), obj.parametric().unwrap().to_path().unwrap());
    // One entry: undo returns to the pentagon in a single step.
    s.undo(s.revision()).unwrap();
    assert_eq!(vertex_count(&s, id), 5);
    assert!(s.revision() != before);
}

#[test]
fn invalid_parameters_are_rejected_without_touching_the_document() {
    let mut s = session();
    let id = create(&mut s, ShapeParams::Star { points: 5, ratio: 0.5 }, bounds(0.0, 0.0, 20.0, 20.0), "");
    let rev = s.revision();
    let r = shape_cmd(&mut s, ShapeCommand::SetParams { target: id, params: ShapeParams::Star { points: 5, ratio: 1.5 } });
    assert_eq!(r.unwrap_err(), CommandError::Shape(ShapeError::Param(ShapeParamError::RatioOutOfRange)));
    // Same parameters: no history entry.
    let same = shape_cmd(&mut s, ShapeCommand::SetParams { target: id, params: ShapeParams::Star { points: 5, ratio: 0.5 } });
    assert_eq!(same.unwrap_err(), CommandError::Shape(ShapeError::NothingChanged));
    assert_eq!(s.revision(), rev);
    // A shape cannot be created with an invalid definition either: validation happens in the constructor.
    assert!(ParametricShape::new(ShapeParams::Polygon { sides: 2 }, Affine::IDENTITY).is_err());
}

#[test]
fn moving_rotating_and_resizing_keep_the_definition_editable() {
    let mut s = session();
    let id = create(&mut s, ShapeParams::Star { points: 6, ratio: 0.5 }, bounds(0.0, 0.0, 40.0, 40.0), "");
    let params = ShapeParams::Star { points: 6, ratio: 0.5 };
    let m = Affine::rotate(Angle::from_degrees(25.0).unwrap()).then(&Affine::translate(Vector::new(100.0, -20.0)).unwrap());
    run(&mut s, CommandPayload::Transform { targets: vec![id], transform: m }).unwrap();
    let after_move = s.snapshot();
    let obj = after_move.object(id).unwrap();
    assert_eq!(*obj.parametric().unwrap().params(), params);
    assert_eq!(**obj.path(), obj.parametric().unwrap().to_path().unwrap());
    // Bounding-box resize (width field of the inspector) is an affine map too.
    run(
        &mut s,
        CommandPayload::SetDimension {
            target: id,
            axis: Axis::Width,
            value: Length::from_mm(60.0).unwrap(),
            anchor: Anchor::Min,
        },
    )
    .unwrap();
    let snap = s.snapshot();
    let obj = snap.object(id).unwrap();
    assert!((obj.bounds().unwrap().width() - 60.0).abs() < 1e-7);
    assert_eq!(*obj.parametric().unwrap().params(), params);
    // And the sides can still be changed afterwards, in the transformed placement.
    shape_cmd(&mut s, ShapeCommand::SetParams { target: id, params: ShapeParams::Star { points: 7, ratio: 0.5 } }).unwrap();
    assert_eq!(s.snapshot().object(id).unwrap().path().subpaths()[0].segments().len() + 1, 14);
}

#[test]
fn a_shear_turns_a_rounded_rectangle_into_a_plain_path_but_not_a_polygon() {
    let mut s = session();
    let rr = create(
        &mut s,
        ShapeParams::RoundedRectangle { width: 1.0, height: 1.0, radius: 3.0 },
        bounds(0.0, 0.0, 40.0, 20.0),
        "",
    );
    let tri = create(&mut s, ShapeParams::Polygon { sides: 3 }, bounds(0.0, 0.0, 40.0, 20.0), "");
    let shear = Affine::new([1.0, 0.0, 0.5, 1.0, 0.0, 0.0]).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![rr, tri], transform: shear }).unwrap();
    let snap = s.snapshot();
    assert!(snap.object(rr).unwrap().parametric().is_none());
    assert!(snap.object(tri).unwrap().parametric().is_some());
    // The outline is still the sheared rounded rectangle. Its width is twice the support function in
    // direction (1, ½) of the centred 40 × 20 box with corner radius 3: the corner arcs cut the
    // sheared corner, so it is less than 40 + 10.
    let (w, h, r) = (40.0, 20.0, 3.0);
    let support = (w / 2.0 - r) + 0.5 * (h / 2.0 - r) + r * 1.25f64.sqrt();
    assert!((snap.object(rr).unwrap().bounds().unwrap().width() - 2.0 * support).abs() < 1e-9);
}

#[test]
fn convert_to_path_keeps_the_outline_drops_the_definition_and_is_undoable() {
    let mut s = session();
    let a = create(&mut s, ShapeParams::Polygon { sides: 6 }, bounds(0.0, 0.0, 20.0, 20.0), "");
    let b = create(&mut s, ShapeParams::Star { points: 5, ratio: 0.4 }, bounds(30.0, 0.0, 50.0, 20.0), "");
    let paths: Vec<_> = [a, b].iter().map(|id| s.snapshot().object(*id).unwrap().path().clone()).collect();
    let out = shape_cmd(&mut s, ShapeCommand::ConvertToPath { targets: vec![a, b] }).unwrap();
    assert_eq!(out.changes.modified, vec![a, b]);
    assert_eq!(s.undo_label(), Some(UndoLabel::Shape(ShapeLabel::ConvertToPath)));
    let snap = s.snapshot();
    for (id, path) in [a, b].iter().zip(&paths) {
        let o = snap.object(*id).unwrap();
        assert!(o.parametric().is_none());
        assert_eq!(o.path(), path);
    }
    // Nothing left to convert; parameters can no longer be edited.
    let rev = s.revision();
    assert_eq!(
        shape_cmd(&mut s, ShapeCommand::ConvertToPath { targets: vec![a] }).unwrap_err(),
        CommandError::Shape(ShapeError::NotParametric(a))
    );
    assert_eq!(
        shape_cmd(&mut s, ShapeCommand::SetParams { target: a, params: ShapeParams::Polygon { sides: 7 } }).unwrap_err(),
        CommandError::Shape(ShapeError::NotParametric(a))
    );
    assert_eq!(s.revision(), rev);
    // Undo brings the definition back.
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().object(a).unwrap().parametric().is_some());
    assert!(s.snapshot().object(b).unwrap().parametric().is_some());
}

#[test]
fn converting_a_mixed_selection_fails_atomically() {
    let mut s = session();
    let a = create(&mut s, ShapeParams::Polygon { sides: 6 }, bounds(0.0, 0.0, 20.0, 20.0), "");
    let plain = run(
        &mut s,
        CommandPayload::Edit(GeometryCommand::CreatePath {
            path: burnburn_geometry::Path::line(Point::ORIGIN, Point::new(5.0, 5.0)).unwrap(),
            name: String::new(),
        }),
    )
    .unwrap()
    .changes
    .created[0];
    let rev = s.revision();
    let err = shape_cmd(&mut s, ShapeCommand::ConvertToPath { targets: vec![a, plain] }).unwrap_err();
    assert_eq!(err, CommandError::Shape(ShapeError::NotParametric(plain)));
    assert_eq!(s.revision(), rev);
    assert!(s.snapshot().object(a).unwrap().parametric().is_some());
}

#[test]
fn editing_nodes_converts_implicitly_so_stale_parameters_never_overwrite_the_edit() {
    // Regression guard: an outline edit without a matching definition must drop the definition.
    let mut s = session();
    let id = create(&mut s, ShapeParams::Polygon { sides: 4 }, bounds(0.0, 0.0, 20.0, 20.0), "");
    let target = s.snapshot().object(id).unwrap().path().subpaths()[0].start();
    run(
        &mut s,
        CommandPayload::Edit(GeometryCommand::EditNodes {
            target: id,
            edit: NodeEdit::MoveNode {
                node: burnburn_geometry::NodeRef { subpath: 0, node: 0 },
                to: Point::new(target.x + 3.0, target.y + 3.0),
            },
        }),
    )
    .unwrap();
    let snap = s.snapshot();
    let o = snap.object(id).unwrap();
    assert!(o.parametric().is_none());
    assert!(o.path().subpaths()[0].start().distance(Point::new(target.x + 3.0, target.y + 3.0)) < 1e-9);
}

#[test]
fn the_definition_is_part_of_the_document_content_identity() {
    // Converting changes no outline but it changes what the document is (and what it saves).
    let mut s = session();
    let id = create(&mut s, ShapeParams::Polygon { sides: 6 }, bounds(0.0, 0.0, 20.0, 20.0), "");
    let before = s.snapshot();
    shape_cmd(&mut s, ShapeCommand::ConvertToPath { targets: vec![id] }).unwrap();
    let after = s.snapshot();
    let hash = |snap: &burnburn_document::DocumentSnapshot| encoded(snap.object(id).unwrap());
    assert_ne!(hash(&before), hash(&after));
    // The outline-only content of a plain object is unchanged by this feature: a stored
    // non-parametric object hashes the same as the converted one from a plain path.
    let plain = burnburn_document::GeometryObject::new(
        id,
        String::new(),
        ObjectKind::Polygon,
        burnburn_document::GeometrySource::Drawn,
        (**after.object(id).unwrap().path()).clone(),
    );
    assert_eq!(encoded(&plain), hash(&after));
}
