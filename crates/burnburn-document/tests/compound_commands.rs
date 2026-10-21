//! W4-UI-machine: samengestelde documentcommando's. Beeld importeren (kader + operatie) en Mask
//! Image (toewijzen, loskoppelen, bijwerken) zijn elk één transactie: één revisie, één undo-stap.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::process::{
    DitherMode, ImageRender, ImageSettings, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange,
    ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{
    CommandError, CommandOutcome, CommandPayload, CompoundCommand, CompoundError, CompoundLabel, CompoundStep,
    DocumentCommand, DocumentSession, GeometryCommand, ImportItem, UndoLabel,
};
use burnburn_geometry::{Path, Point, Rect};
use burnburn_types::{ContentHash, DocumentId, GeometryId, HashDomain, Length, OperationId, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd)
}

fn image_draft(name: &str, source: u8) -> OperationDraft {
    let pitch = mm(0.1);
    let settings = ImageSettings::new(
        ContentHash::of(HashDomain::Asset, &[source]),
        pitch,
        pitch,
        ImageRender::Dither(DitherMode::Jarvis),
    )
    .unwrap();
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: name.into(),
        mode: OperationMode::Image(settings),
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

fn frame_import() -> CommandPayload {
    let rect = Rect::new(Point::new(0.0, 0.0), mm(40.0), mm(30.0)).unwrap();
    CommandPayload::Edit(GeometryCommand::Import {
        asset: ContentHash::of(HashDomain::Asset, &[1]),
        items: vec![ImportItem { name: "Beeld".into(), entity: Some("image".into()), path: Path::rectangle(rect) }],
    })
}

fn import_image(draft: OperationDraft) -> CommandPayload {
    CommandPayload::Compound(CompoundCommand {
        label: CompoundLabel::ImportImage,
        steps: vec![CompoundStep::Run(frame_import()), CompoundStep::AddOperationForCreated { draft }],
    })
}

fn rectangle(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: String::new() }).unwrap().changes.created[0]
}

#[test]
fn importing_an_image_is_one_revision_and_one_undo_step_and_redo_restores_all() {
    let mut s = session();
    let before = s.revision();

    let out = run(&mut s, import_image(image_draft("Beeld a.png", 1))).unwrap();

    assert_eq!(out.undo_label, UndoLabel::Compound(CompoundLabel::ImportImage));
    assert_eq!(out.revision, before.next().unwrap(), "one transaction, one new revision");
    assert_eq!(s.undo_labels().len(), 1, "one undo step");
    let snap = s.snapshot();
    assert_eq!(snap.objects().count(), 1, "the frame");
    let frame = out.changes.created[0];
    assert_eq!(out.changes.operations.len(), 1, "the operation is reported as created");
    let op = snap.process().operation(out.changes.operations[0]).unwrap();
    assert_eq!(op.targets(), &[frame], "the operation targets the new frame");

    s.undo(s.revision()).unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.objects().count(), 0, "one undo removes the frame");
    assert_eq!(snap.process().operations().count(), 0, "and the operation");
    assert!(!s.can_undo());

    s.redo(s.revision()).unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.objects().count(), 1, "one redo restores the frame");
    assert_eq!(snap.process().operations().count(), 1, "and the operation");
    assert!(!s.can_redo());
}

#[test]
fn a_failing_step_leaves_no_trace_of_the_earlier_steps() {
    let mut s = session();
    let before = s.revision();
    // The operation names a target that does not exist: the whole compound is refused.
    let payload = CommandPayload::Compound(CompoundCommand {
        label: CompoundLabel::MaskImage,
        steps: vec![
            CompoundStep::Run(frame_import()),
            CompoundStep::Run(CommandPayload::Process(ProcessCommand::AddOperation {
                draft: image_draft("x", 1),
                targets: vec![GeometryId::from_raw(999)],
            })),
        ],
    });

    assert!(run(&mut s, payload).is_err());

    assert_eq!(s.revision(), before);
    assert_eq!(s.snapshot().objects().count(), 0, "the import of the first step is gone too");
    assert!(!s.can_undo());
}

#[test]
fn add_operation_for_created_needs_something_created_by_the_earlier_steps() {
    let mut s = session();
    let payload = CommandPayload::Compound(CompoundCommand {
        label: CompoundLabel::ImportImage,
        steps: vec![CompoundStep::AddOperationForCreated { draft: image_draft("x", 1) }],
    });
    assert_eq!(run(&mut s, payload), Err(CommandError::Compound(CompoundError::NothingCreated)));
    assert!(!s.can_undo());
}

#[test]
fn an_empty_or_nested_compound_is_refused() {
    let mut s = session();
    let empty = CommandPayload::Compound(CompoundCommand { label: CompoundLabel::MaskImage, steps: vec![] });
    assert_eq!(run(&mut s, empty), Err(CommandError::Compound(CompoundError::Empty)));

    let inner = CompoundCommand { label: CompoundLabel::MaskImage, steps: vec![CompoundStep::Run(frame_import())] };
    let nested = CommandPayload::Compound(CompoundCommand {
        label: CompoundLabel::ImportImage,
        steps: vec![CompoundStep::Run(CommandPayload::Compound(inner))],
    });
    assert_eq!(run(&mut s, nested), Err(CommandError::Compound(CompoundError::Nested)));
    assert!(!s.can_undo());
}

#[test]
fn mask_image_assign_unassign_and_update_are_one_undo_step() {
    let mut s = session();
    let imported = run(&mut s, import_image(image_draft("Beeld", 1))).unwrap();
    let frame = imported.changes.created[0];
    let operation: OperationId = imported.changes.operations[0];
    let shape = rectangle(&mut s, 5.0, 5.0, 10.0, 10.0);
    let steps_before = s.undo_labels().len();
    let masked = image_draft("Beeld (masker)", 1);

    let out = run(
        &mut s,
        CommandPayload::Compound(CompoundCommand {
            label: CompoundLabel::MaskImage,
            steps: vec![
                CompoundStep::Run(CommandPayload::Process(ProcessCommand::AssignGeometry {
                    operation,
                    targets: vec![shape],
                })),
                CompoundStep::Run(CommandPayload::Process(ProcessCommand::UnassignGeometry {
                    operation,
                    targets: vec![frame],
                })),
                CompoundStep::Run(CommandPayload::Process(ProcessCommand::UpdateOperation {
                    operation,
                    draft: masked,
                })),
            ],
        }),
    )
    .unwrap();

    assert_eq!(out.undo_label, UndoLabel::Compound(CompoundLabel::MaskImage));
    assert_eq!(s.undo_labels().len(), steps_before + 1, "three edits, one undo step");
    let snap = s.snapshot();
    let op = snap.process().operation(operation).unwrap();
    assert_eq!(op.targets(), &[shape]);
    assert_eq!(op.name(), "Beeld (masker)");

    s.undo(s.revision()).unwrap();
    let snap = s.snapshot();
    let op = snap.process().operation(operation).unwrap();
    assert_eq!(op.targets(), &[frame], "one undo restores the frame as target");
    assert_eq!(op.name(), "Beeld", "and the old settings");

    s.redo(s.revision()).unwrap();
    let snap = s.snapshot();
    let op = snap.process().operation(operation).unwrap();
    assert_eq!(op.targets(), &[shape], "one redo restores the mask");
    assert_eq!(op.name(), "Beeld (masker)");
}
