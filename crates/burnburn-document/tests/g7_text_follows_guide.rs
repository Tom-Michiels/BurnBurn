//! G7-editor E08: a text bound to a guide object (`TextObject::path`) is evaluated again in the
//! same transaction whenever its guide changes, and only then. The evaluator here is a fake whose
//! output is a 1 x 1 square on the guide's lower-left corner, so "the text follows" is visible
//! without fonts; the real bending is tested in `burnburn-generators`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;

use burnburn_document::structure::{EvalMode, Modifier, ModifierChain, ModifierStep, StructureCommand};
use burnburn_document::text::{FontRef, FontSize, FontStyle, PathBinding, TextCommand, TextError, TextId, TextObject};
use burnburn_document::{
    CommandError, CommandOutcome, CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind,
    DocumentCommand, DocumentPatch, DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus, Evaluator,
    GeometryCommand, GeometrySource, ObjectKind, OwnerSolution,
};
use burnburn_geometry::{Affine, NodeEdit, NodeRef, Path, Point, ProductionTolerance, Rect, Vector};
use burnburn_types::{ContentHash, DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

/// Counts evaluations; a bound text becomes a unit square at its guide's lower-left corner.
struct Follower {
    calls: Cell<usize>,
}

impl Evaluator for Follower {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Text
    }
    fn evaluate(&self, snap: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        self.calls.set(self.calls.get() + 1);
        let DerivedOwner::Text(id) = owner else { panic!("only texts") };
        let t = snap.text(id).unwrap();
        let inputs = ContentHash::from_bytes([1; 32]);
        let at = match t.path {
            None => Point::ORIGIN,
            Some(b) => match snap.object(b.guide) {
                Some(g) => g.bounds().unwrap().min(),
                None => {
                    return DocumentPatch {
                        owner,
                        inputs,
                        outputs: vec![],
                        status: EvalStatus::Failed {
                            diagnosis: EvalDiagnosis::MissingSources { objects: vec![b.guide] },
                        },
                        solution: OwnerSolution::Keep,
                    };
                }
            },
        };
        let path = Path::rectangle(Rect::new(at, mm(1.0), mm(1.0)).unwrap());
        DocumentPatch {
            owner,
            inputs,
            outputs: vec![DerivedOutput {
                key: DerivedKey { owner, local: 0 },
                kind: ObjectKind::Path,
                path,
                name: None,
            }],
            status: EvalStatus::Valid,
            solution: OwnerSolution::Text { missing: vec![] },
        }
    }
}

fn run(
    s: &mut DocumentSession,
    ev: &Follower,
    payload: CommandPayload,
) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply_with(cmd, &[ev])
}

fn rect(s: &mut DocumentSession, ev: &Follower, x: f64, name: &str) -> GeometryId {
    let rect = Rect::new(Point::new(x, 0.0), mm(10.0), mm(10.0)).unwrap();
    run(s, ev, CommandPayload::CreateRectangle { rect, name: name.into() }).unwrap().changes.created[0]
}

fn binding(guide: GeometryId) -> PathBinding {
    PathBinding { guide, start_mm: 0.0, offset_mm: 0.0, reverse: false, warp: false }
}

fn bound_text(s: &mut DocumentSession, ev: &Follower, guide: GeometryId) -> (TextId, GeometryId) {
    let mut t = TextObject::new("tekst", FontRef::new("F", FontStyle::Regular), FontSize::from_mm(mm(5.0)).unwrap());
    t.path = Some(binding(guide));
    let out = run(s, ev, CommandPayload::Text(TextCommand::Create { text: Box::new(t) })).unwrap();
    let id = out.changes.texts[0];
    let output = output_of(s, id);
    (id, output)
}

fn output_of(s: &DocumentSession, text: TextId) -> GeometryId {
    s.snapshot()
        .objects()
        .find(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == DerivedOwner::Text(text)))
        .map(|(id, _)| id)
        .expect("the text has an outline")
}

fn corner(s: &DocumentSession, id: GeometryId) -> Point {
    s.snapshot().object(id).unwrap().bounds().unwrap().min()
}

fn move_by(s: &mut DocumentSession, ev: &Follower, id: GeometryId, dx: f64, dy: f64) {
    let transform = Affine::translate(Vector::new(dx, dy)).unwrap();
    run(s, ev, CommandPayload::Transform { targets: vec![id], transform }).unwrap();
}

fn setup() -> (DocumentSession, Follower, GeometryId, TextId, GeometryId) {
    let ev = Follower { calls: Cell::new(0) };
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let guide = rect(&mut s, &ev, 5.0, "Pad");
    let (text, output) = bound_text(&mut s, &ev, guide);
    (s, ev, guide, text, output)
}

#[test]
fn the_text_follows_a_guide_that_is_moved_in_the_same_transaction() {
    let (mut s, ev, guide, _, output) = setup();
    assert_eq!(corner(&s, output), Point::new(5.0, 0.0));
    let before = s.revision();
    move_by(&mut s, &ev, guide, 20.0, 3.0);
    assert_eq!(corner(&s, output), Point::new(25.0, 3.0), "the outline moved with the guide");
    assert_eq!(s.revision(), before.next().unwrap(), "one revision: guide and text together");
}

#[test]
fn only_a_change_of_the_guide_evaluates_the_text_again() {
    let (mut s, ev, guide, _, _) = setup();
    let other = rect(&mut s, &ev, 100.0, "Ander");
    let calls = ev.calls.get();
    move_by(&mut s, &ev, other, 1.0, 1.0);
    assert_eq!(ev.calls.get(), calls, "an unrelated object does not wake the text");
    move_by(&mut s, &ev, guide, 1.0, 1.0);
    assert_eq!(ev.calls.get(), calls + 1, "the guide does, exactly once");
}

#[test]
fn reshaping_the_guide_counts_as_a_change_too() {
    let ev = Follower { calls: Cell::new(0) };
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let path = Path::polyline(&[Point::new(10.0, 10.0), Point::new(30.0, 10.0)], false).unwrap();
    let guide = run(&mut s, &ev, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: "Lijn".into() }))
        .unwrap()
        .changes
        .created[0];
    let (_, output) = bound_text(&mut s, &ev, guide);
    assert_eq!(corner(&s, output), Point::new(10.0, 10.0));
    let edit = NodeEdit::MoveNode { node: NodeRef { subpath: 0, node: 0 }, to: Point::new(4.0, -3.0) };
    run(&mut s, &ev, CommandPayload::Edit(GeometryCommand::EditNodes { target: guide, edit })).unwrap();
    assert_eq!(corner(&s, output), Point::new(4.0, -3.0), "a node of the guide moved, the text follows");
}

#[test]
fn undo_takes_the_guide_and_the_text_back_together() {
    let (mut s, ev, guide, _, output) = setup();
    move_by(&mut s, &ev, guide, 20.0, 0.0);
    s.undo(s.revision()).unwrap();
    assert_eq!(corner(&s, guide), Point::new(5.0, 0.0));
    assert_eq!(corner(&s, output), Point::new(5.0, 0.0));
    s.redo(s.revision()).unwrap();
    assert_eq!(corner(&s, output), Point::new(25.0, 0.0));
}

#[test]
fn a_deleted_guide_keeps_the_last_outline_and_blocks_production() {
    let (mut s, ev, guide, text, output) = setup();
    run(&mut s, &ev, CommandPayload::Delete { targets: vec![guide] }).unwrap();
    let snap = s.snapshot();
    assert!(snap.object(output).is_some(), "the last outline stays");
    assert_eq!(corner(&s, output), Point::new(5.0, 0.0));
    let record = snap.derived_record(DerivedOwner::Text(text)).unwrap();
    assert_eq!(record.status, EvalStatus::Failed { diagnosis: EvalDiagnosis::MissingSources { objects: vec![guide] } });
    assert!(snap.production_blocked(output));
    // Undo brings the guide back and the text is valid again.
    s.undo(s.revision()).unwrap();
    assert!(!s.snapshot().production_blocked(output));
}

#[test]
fn unbinding_returns_the_text_to_its_own_place_and_stops_following() {
    let (mut s, ev, guide, text, output) = setup();
    let mut free = s.snapshot().text(text).unwrap().clone();
    free.path = None;
    run(&mut s, &ev, CommandPayload::Text(TextCommand::Set { text, source: Box::new(free) })).unwrap();
    assert_eq!(corner(&s, output), Point::ORIGIN);
    let calls = ev.calls.get();
    move_by(&mut s, &ev, guide, 9.0, 9.0);
    assert_eq!(ev.calls.get(), calls, "no binding, no following");
}

#[test]
fn a_guide_that_is_computed_by_a_modifier_is_followed_after_the_modifier_ran() {
    let ev = Follower { calls: Cell::new(0) };
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let source = rect(&mut s, &ev, 0.0, "Bron");
    let chain = ModifierChain {
        name: "Reeks".into(),
        source,
        steps: vec![ModifierStep {
            modifier: Modifier::LinearArray { count: 2, step: Vector::new(30.0, 0.0) },
            enabled: true,
        }],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    };
    let made = run(&mut s, &ev, CommandPayload::Structure(StructureCommand::CreateModifier { chain })).unwrap();
    let guide = made.changes.created[0];
    let (_, output) = bound_text(&mut s, &ev, guide);
    assert_eq!(corner(&s, output), Point::new(0.0, 0.0));
    // The source moves; the chain recomputes its output, the text follows that.
    move_by(&mut s, &ev, source, 0.0, 12.0);
    assert_eq!(corner(&s, guide), Point::new(0.0, 12.0));
    assert_eq!(corner(&s, output), Point::new(0.0, 12.0), "the text follows the recomputed guide");
}

#[test]
fn a_text_cannot_follow_its_own_outline() {
    let (mut s, ev, _, text, output) = setup();
    let mut mine = s.snapshot().text(text).unwrap().clone();
    mine.path = Some(binding(output));
    let revision = s.revision();
    let err = run(&mut s, &ev, CommandPayload::Text(TextCommand::Set { text, source: Box::new(mine) })).unwrap_err();
    assert!(matches!(err, CommandError::Text(TextError::GuideIsOwnOutput(_))), "{err:?}");
    assert_eq!(s.revision(), revision);
}

#[test]
fn a_binding_with_a_negative_start_is_refused() {
    let ev = Follower { calls: Cell::new(0) };
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let guide = rect(&mut s, &ev, 0.0, "Pad");
    let mut t = TextObject::new("x", FontRef::new("F", FontStyle::Regular), FontSize::from_mm(mm(5.0)).unwrap());
    t.path = Some(PathBinding { start_mm: -1.0, ..binding(guide) });
    let err = run(&mut s, &ev, CommandPayload::Text(TextCommand::Create { text: Box::new(t) })).unwrap_err();
    assert!(matches!(err, CommandError::Text(TextError::InvalidLayout)), "{err:?}");
}
