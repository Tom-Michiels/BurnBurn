//! Grid and circular arrays as non-destructive modifiers (CAM → Kern request, IM26 + IM21): the
//! source stays editable, the output follows in the same transaction, copies keep distinct
//! associative names.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_document::structure::{EvalMode, Modifier, ModifierChain, ModifierStep, StructureCommand, StructureError};
use burnburn_document::{CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::array::{CircularArray, GridArray};
use burnburn_geometry::{Affine, Point, ProductionTolerance, Rect, SegmentRef, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd)
}

fn square(s: &mut DocumentSession) -> GeometryId {
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).unwrap().changes.created[0]
}

fn chain(source: GeometryId, modifier: Modifier) -> ModifierChain {
    ModifierChain {
        name: "Reeks".into(),
        source,
        steps: vec![ModifierStep { modifier, enabled: true }],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    }
}

fn grid(columns: u32, rows: u32) -> Modifier {
    Modifier::GridArray(GridArray { columns, rows, pitch_x: mm(20.0), pitch_y: mm(15.0) })
}

#[test]
fn grid_array_modifier_outputs_every_copy_and_follows_source_edits() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let out =
        run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(src, grid(3, 2)) }))
            .unwrap();
    let result = out.changes.created[0];
    let snap = s.snapshot();
    let obj = snap.object(result).unwrap();
    assert_eq!(obj.path().subpaths().len(), 6);
    let b = obj.bounds().unwrap();
    assert_eq!((b.min(), b.max()), (Point::new(0.0, 0.0), Point::new(50.0, 25.0)));
    assert!(!snap.production_blocked(result));
    // Copies are named apart: the first edge of copy 0 and of copy 1 are different references.
    let a = snap.edge_ref(result, SegmentRef { subpath: 0, segment: 0 }).unwrap();
    let c = snap.edge_ref(result, SegmentRef { subpath: 1, segment: 0 }).unwrap();
    assert_ne!(a, c);

    let m = Affine::translate(Vector::new(5.0, 0.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![src], transform: m }).unwrap();
    let b = s.snapshot().object(result).unwrap().bounds().unwrap();
    assert_eq!((b.min().x, b.max().x), (5.0, 55.0), "the output follows the editable source, same id");
}

#[test]
fn circular_array_modifier_turns_copies_about_the_centre() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let circle = Modifier::CircularArray(CircularArray {
        count: 4,
        center: Point::new(0.0, 0.0),
        sweep_deg: 360.0,
        rotate_items: true,
        reference: Point::new(0.0, 0.0),
    });
    let out =
        run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(src, circle) })).unwrap();
    let obj = s.snapshot().object(out.changes.created[0]).unwrap().clone();
    assert_eq!(obj.path().subpaths().len(), 4);
    let b = obj.bounds().unwrap();
    assert_eq!((b.min(), b.max()), (Point::new(-10.0, -10.0), Point::new(10.0, 10.0)), "exact quarter turns");
}

#[test]
fn invalid_array_parameters_are_refused_and_change_nothing() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let rev = s.revision();
    let bad = [
        grid(0, 2),
        Modifier::CircularArray(CircularArray {
            count: 3,
            center: Point::new(0.0, 0.0),
            sweep_deg: 0.0,
            rotate_items: false,
            reference: Point::new(0.0, 0.0),
        }),
    ];
    for m in bad {
        let err = run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(src, m) }));
        assert_eq!(err.unwrap_err(), CommandError::Structure(StructureError::InvalidValue { field: "array" }));
    }
    assert_eq!(s.revision(), rev);
}
