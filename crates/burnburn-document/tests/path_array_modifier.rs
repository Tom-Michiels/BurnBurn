//! Copy along a path as a non-destructive modifier (AR02, G5-cam): the path is another object,
//! the output follows both the source and the path, and the chain refuses what cannot work.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_document::structure::{EvalMode, Modifier, ModifierChain, ModifierStep, StructureCommand, StructureError};
use burnburn_document::{
    CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, GeometryCommand,
};
use burnburn_geometry::{Affine, Path, Point, ProductionTolerance, Rect};
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

fn path_object(s: &mut DocumentSession, points: &[(f64, f64)]) -> GeometryId {
    let path = Path::polyline(&points.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), false).unwrap();
    run(s, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: "Pad".into() })).unwrap().changes.created[0]
}

fn chain(source: GeometryId, modifier: Modifier) -> ModifierChain {
    ModifierChain {
        name: "Langs pad".into(),
        source,
        steps: vec![ModifierStep { modifier, enabled: true }],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    }
}

fn along(g: GeometryId, count: u32, orient: bool) -> Modifier {
    Modifier::PathArray { count, along: g, orient }
}

fn create(s: &mut DocumentSession, source: GeometryId, modifier: Modifier) -> Result<CommandOutcome, CommandError> {
    run(s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(source, modifier) }))
}

#[test]
fn copies_follow_the_path_and_the_output_follows_both_the_source_and_the_path() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let pad = path_object(&mut s, &[(0.0, 0.0), (60.0, 0.0)]);
    let result = create(&mut s, src, along(pad, 4, false)).unwrap().changes.created[0];
    let snap = s.snapshot();
    let obj = snap.object(result).unwrap();
    assert_eq!(obj.path().subpaths().len(), 4);
    let b = obj.bounds().unwrap();
    assert_eq!((b.min(), b.max()), (Point::new(0.0, 0.0), Point::new(70.0, 10.0)), "4 squares 20 mm apart");

    // Stretching the path to twice its length moves the copies apart, in the same transaction.
    let stretch = Affine::new([2.0, 0.0, 0.0, 1.0, 0.0, 0.0]).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![pad], transform: stretch }).unwrap();
    let b = s.snapshot().object(result).unwrap().bounds().unwrap();
    assert_eq!(b.max().x, 130.0, "the output follows the path: copies 40 mm apart");
}

#[test]
fn oriented_copies_turn_with_the_path() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let pad = path_object(&mut s, &[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)]);
    let result = create(&mut s, src, along(pad, 3, true)).unwrap().changes.created[0];
    let snap = s.snapshot();
    let obj = snap.object(result).unwrap();
    // The third copy sits at the end of the path and is turned a quarter: the source corner
    // (0,0) is at (20,20) and the square extends to the left and up.
    let last = &obj.path().subpaths()[2];
    assert!(last.start().distance(Point::new(20.0, 20.0)) < 1e-9, "{:?}", last.start());
    let b = obj.bounds().unwrap();
    assert!(b.min().distance(Point::new(0.0, 0.0)) < 1e-9 && b.max().distance(Point::new(20.0, 30.0)) < 1e-9, "{b:?}");
}

#[test]
fn a_missing_path_a_bad_count_and_a_point_as_path_are_refused_and_change_nothing() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let pad = path_object(&mut s, &[(0.0, 0.0), (10.0, 0.0)]);
    let dot = path_object(&mut s, &[(3.0, 3.0), (3.0, 3.0)]);
    let rev = s.revision();
    assert!(create(&mut s, src, along(pad, 0, false)).is_err());
    assert!(matches!(
        create(&mut s, src, along(GeometryId::from_raw(9999), 3, false)),
        Err(CommandError::Structure(StructureError::UnknownGeometry(_)))
    ));
    assert!(matches!(
        create(&mut s, src, along(dot, 3, false)),
        Err(CommandError::Structure(StructureError::InvalidValue { .. }))
    ));
    assert_eq!(s.revision(), rev);
}

#[test]
fn undo_removes_the_modifier_and_its_output() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let pad = path_object(&mut s, &[(0.0, 0.0), (40.0, 0.0)]);
    let before = s.snapshot().objects().count();
    create(&mut s, src, along(pad, 3, false)).unwrap();
    assert_eq!(s.snapshot().objects().count(), before + 1);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().objects().count(), before);
}
