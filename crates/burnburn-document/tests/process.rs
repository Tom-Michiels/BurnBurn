//! Public contract of the process layer (IM13, owner CAM/job): operations reference design
//! geometry by id, edits are document transactions with undo/redo.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, OrderConstraint, Passes, PowerPercent, PowerRange,
    ProcessCommand, ProcessError, ProcessLabel, ProcessSettings, RecipeDraft, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::{
    Anchor, Axis, CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel,
};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{DocumentId, GeometryId, Length, OperationId, RequestId, StableId};

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

fn process(s: &mut DocumentSession, cmd: ProcessCommand) -> Result<CommandOutcome, CommandError> {
    run(s, CommandPayload::Process(cmd))
}

fn rectangle(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: String::new() }).unwrap().changes.created[0]
}

fn settings(speed: f64, power: f64) -> ProcessSettings {
    let p = PowerPercent::new(power).unwrap();
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    }
}

fn line(name: &str) -> OperationDraft {
    OperationDraft {
        name: name.into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(100.0, 20.0),
        output_enabled: true,
    }
}

fn fill(name: &str) -> OperationDraft {
    let f = FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    OperationDraft {
        name: name.into(),
        mode: OperationMode::Fill(f),
        settings: settings(300.0, 15.0),
        output_enabled: true,
    }
}

fn add(s: &mut DocumentSession, draft: OperationDraft, targets: Vec<GeometryId>) -> OperationId {
    let out = process(s, ProcessCommand::AddOperation { draft, targets }).unwrap();
    assert_eq!(out.changes.operations.len(), 1, "one operation created");
    out.changes.operations[0]
}

#[test]
fn two_operations_share_one_geometry_without_duplicate_source() {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 20.0, 30.0, 15.0);
    let mark = add(&mut s, fill("Markeren"), vec![g]);
    let cut = add(&mut s, line("Snijden"), vec![g]);

    let snap = s.snapshot();
    assert_eq!(snap.len(), 1, "no geometry copy for the second operation");
    assert_ne!(mark, cut);
    assert!(mark.raw() != g.raw() && cut.raw() != g.raw(), "ids share one counter, no clash");
    let layer = snap.process();
    assert_eq!(layer.operations().map(|o| o.id()).collect::<Vec<_>>(), vec![mark, cut]);
    assert_eq!(layer.operation(mark).unwrap().targets(), &[g]);
    assert_eq!(layer.operation(cut).unwrap().targets(), &[g]);
    assert_eq!(layer.operations_using(g), vec![mark, cut], "multiply linked geometry is inspectable");

    // Editing the source reaches both operations: they still name the same, now resized, object.
    run(&mut s, CommandPayload::SetDimension { target: g, axis: Axis::Width, value: mm(60.0), anchor: Anchor::Min })
        .unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap.object(g).unwrap().bounds().unwrap().width(), 60.0);
    assert_eq!(snap.process().operations_using(g), vec![mark, cut]);
}

#[test]
fn hard_order_constraints_override_list_order_and_cycles_name_operations() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let a = add(&mut s, line("A"), vec![g]);
    let b = add(&mut s, line("B"), vec![g]);
    let c = add(&mut s, line("C"), vec![g]);
    assert_eq!(s.snapshot().process().execution_order().unwrap(), vec![a, b, c]);

    process(&mut s, ProcessCommand::AddOrderConstraint { before: c, after: a }).unwrap();
    // Stable: only what the constraint forces moves; B keeps its place relative to A.
    assert_eq!(s.snapshot().process().execution_order().unwrap(), vec![b, c, a]);
    process(&mut s, ProcessCommand::AddOrderConstraint { before: a, after: b }).unwrap();
    assert_eq!(s.snapshot().process().execution_order().unwrap(), vec![c, a, b]);

    // b → c would close c → a → b → c.
    let rev = s.revision();
    let err = process(&mut s, ProcessCommand::AddOrderConstraint { before: b, after: c }).unwrap_err();
    assert_eq!(err, CommandError::Process(ProcessError::OrderCycle { operations: vec![a, b, c] }));
    assert_eq!(s.revision(), rev, "rejected constraint changes nothing");
    assert_eq!(
        s.snapshot().process().constraints(),
        &[OrderConstraint { before: c, after: a }, OrderConstraint { before: a, after: b }]
    );
    let err = process(&mut s, ProcessCommand::AddOrderConstraint { before: a, after: a }).unwrap_err();
    assert_eq!(err, CommandError::Process(ProcessError::SelfConstraint(a)));
}

#[test]
fn deleting_geometry_removes_its_operation_refs_and_undo_restores_them() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let h = rectangle(&mut s, 20.0, 0.0, 10.0, 10.0);
    let op = add(&mut s, line("Snijden"), vec![g, h]);

    let out = run(&mut s, CommandPayload::Delete { targets: vec![g] }).unwrap();
    assert_eq!(out.changes.deleted, vec![g]);
    assert_eq!(out.changes.operations, vec![op], "the operation changed with the delete");
    assert_eq!(s.snapshot().process().operation(op).unwrap().targets(), &[h], "no dangling reference");

    s.undo(s.revision()).unwrap();
    let snap = s.snapshot();
    assert!(snap.object(g).is_some());
    assert_eq!(snap.process().operation(op).unwrap().targets(), &[g, h], "undo restores object and reference");
}

fn recipe(notes: &str, speed: f64) -> RecipeDraft {
    RecipeDraft {
        name: "Populier 3 mm snijden".into(),
        material: "populier multiplex".into(),
        thickness: mm(3.0),
        machine: "ruida-1500x900".into(),
        notes: notes.into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(speed, 60.0),
    }
}

#[test]
fn updating_recipe_does_not_change_operation_that_applied_it() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = add(&mut s, fill("Snijden"), vec![g]);
    let r =
        process(&mut s, ProcessCommand::AddRecipe { draft: recipe("testkaart 1", 12.0) }).unwrap().changes.recipes[0];
    assert_eq!(s.snapshot().process().recipe(r).unwrap().revision(), 1);

    process(&mut s, ProcessCommand::ApplyRecipe { recipe: r, operation: op }).unwrap();
    let applied = s.snapshot().process().operation(op).unwrap().clone();
    assert_eq!(applied.settings(), settings(12.0, 60.0));
    assert_eq!(applied.mode(), OperationMode::Line { kerf: Kerf::NONE }, "recipe mode is copied");
    assert_eq!(applied.recipe().map(|x| (x.recipe, x.revision)), Some((r, 1)));
    assert_eq!(applied.targets(), &[g], "targets are not part of a recipe");

    process(&mut s, ProcessCommand::UpdateRecipe { recipe: r, draft: recipe("testkaart 2", 10.0) }).unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.process().recipe(r).unwrap().revision(), 2);
    assert_eq!(snap.process().operation(op).unwrap(), &applied, "library update leaves job settings alone (C08)");
}

#[test]
fn process_values_are_validated_at_the_boundary() {
    assert_eq!(Speed::mm_per_s(0.0), Err(ProcessError::NotPositive { field: "speed" }));
    assert_eq!(Speed::mm_per_s(f64::NAN), Err(ProcessError::NotPositive { field: "speed" }));
    assert_eq!(PowerPercent::new(100.0).map(PowerPercent::get), Ok(100.0), "upper bound is inclusive");
    assert_eq!(PowerPercent::new(100.000_1), Err(ProcessError::OutOfRange { field: "power" }));
    assert_eq!(PowerPercent::new(-0.1), Err(ProcessError::OutOfRange { field: "power" }));
    let (lo, hi) = (PowerPercent::new(10.0).unwrap(), PowerPercent::new(20.0).unwrap());
    assert_eq!(PowerRange::new(hi, lo), Err(ProcessError::MinPowerAboveMax));
    assert_eq!(Passes::new(0), Err(ProcessError::OutOfRange { field: "passes" }));
    assert_eq!(Passes::new(Passes::MAX).map(Passes::get), Ok(Passes::MAX));
    assert_eq!(
        FillSettings::new(mm(0.0), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)),
        Err(ProcessError::OutOfRange { field: "interval" })
    );
    assert_eq!(
        FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(-1.0)),
        Err(ProcessError::OutOfRange { field: "overscan" })
    );
    assert_eq!(Kerf::width(mm(-0.1)), Err(ProcessError::OutOfRange { field: "kerf" }));
}

#[test]
fn rejected_process_command_changes_nothing() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = add(&mut s, line("Snijden"), vec![g]);
    let before = s.snapshot();
    let ghost = GeometryId::from_raw(999);

    let cases = [
        (ProcessCommand::AddOperation { draft: line("x"), targets: vec![ghost] }, ProcessError::UnknownGeometry(ghost)),
        (
            ProcessCommand::AddOperation { draft: line("x"), targets: vec![g, g] },
            ProcessError::DuplicateTarget { operation: OperationId::from_raw(3), geometry: g },
        ),
        (
            ProcessCommand::AssignGeometry { operation: op, targets: vec![g] },
            ProcessError::DuplicateTarget { operation: op, geometry: g },
        ),
        (
            ProcessCommand::RemoveOperation { operation: OperationId::from_raw(77) },
            ProcessError::UnknownOperation(OperationId::from_raw(77)),
        ),
    ];
    for (cmd, expected) in cases {
        assert_eq!(process(&mut s, cmd).unwrap_err(), CommandError::Process(expected));
        assert_eq!(s.revision(), before.revision());
        assert!(s.snapshot().same_content(&before));
    }
    // A failed add does not consume an id either.
    let next = add(&mut s, line("y"), vec![g]);
    assert_eq!(next, OperationId::from_raw(3));
}

#[test]
fn process_edits_undo_redo_and_mark_document_dirty() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    s.mark_saved(s.revision());
    assert!(!s.is_dirty());

    let op = add(&mut s, line("Snijden"), vec![g]);
    assert!(s.is_dirty(), "a process-only edit is unsaved content");
    assert_eq!(s.undo_label(), Some(UndoLabel::Process(ProcessLabel::AddOperation)));

    let r1 = s.revision();
    let undone = s.undo(r1).unwrap();
    assert!(undone.revision > r1, "undo yields a new revision");
    assert!(s.snapshot().process().operation(op).is_none());
    assert!(!s.is_dirty(), "back at the saved content");

    s.redo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().operation(op).unwrap().targets(), &[g], "redo restores identity");
}

#[test]
fn advanced_options_are_one_validated_undoable_transaction() {
    use burnburn_document::process::{AdvancedOptions, LineOptions, SubOperation, TabSpec};
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = add(&mut s, line("Snijden"), vec![g]);
    assert!(TabSpec::new(0, mm(1.0)).is_err() && TabSpec::new(2, mm(0.0)).is_err(), "validated at the boundary");
    let advanced = AdvancedOptions {
        line: LineOptions { tabs: Some(TabSpec::new(2, mm(1.0)).unwrap()), ..Default::default() },
        ..Default::default()
    };
    let out = process(&mut s, ProcessCommand::SetAdvanced { operation: op, advanced: advanced.clone() }).unwrap();
    assert_eq!(out.changes.operations, vec![op]);
    assert_eq!(s.undo_label(), Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
    assert_eq!(s.snapshot().process().operation(op).unwrap().advanced(), &advanced);
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().process().operation(op).unwrap().advanced().is_default());

    let sub = SubOperation { mode: OperationMode::Line { kerf: Kerf::NONE }, settings: settings(10.0, 10.0) };
    let too_many = AdvancedOptions { sub_operations: vec![sub; 17], ..Default::default() };
    let before = s.revision();
    let err = process(&mut s, ProcessCommand::SetAdvanced { operation: op, advanced: too_many }).unwrap_err();
    assert_eq!(err, CommandError::Process(ProcessError::OutOfRange { field: "sub-operations" }));
    assert_eq!(s.revision(), before, "rejected command changes nothing");
}

#[test]
fn operations_without_new_options_keep_their_im13_canonical_layout() {
    use burnburn_document::process::{AdvancedOptions, Overcut};
    use burnburn_types::{CanonicalEncode, CanonicalWriter};
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = add(&mut s, line("Snijden"), vec![g]);
    let encode = |s: &DocumentSession| {
        let mut w = CanonicalWriter::new();
        s.snapshot().process().operation(op).unwrap().encode(&mut w);
        w.finish().unwrap()
    };
    // IM13 layout, written out by hand: id, name, mode, settings, targets, output, recipe marker.
    let mut w = CanonicalWriter::new();
    w.u64(op.raw());
    w.str("Snijden");
    w.tag("line");
    w.f64(0.0);
    for v in [100.0, 20.0, 20.0] {
        w.f64(v);
    }
    w.u64(1);
    w.bool(true);
    w.u64(1);
    w.u64(g.raw());
    w.bool(true);
    w.u64(0);
    assert_eq!(encode(&s), w.finish().unwrap(), "existing project and plan hashes stay valid");

    let advanced = AdvancedOptions {
        line: burnburn_document::process::LineOptions {
            overcut: Some(Overcut::new(mm(1.0)).unwrap()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plain = encode(&s);
    process(&mut s, ProcessCommand::SetAdvanced { operation: op, advanced }).unwrap();
    assert_ne!(encode(&s), plain, "advanced options are content");
}
