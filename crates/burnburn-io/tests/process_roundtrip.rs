//! The process layer (IM13, owner CAM/job) survives save and reopen with its full meaning.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::Path;

use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, RecipeDraft, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, decode_container, encode_container, load_project, project_hash, save_project};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn settings(speed: f64, lo: f64, hi: f64, passes: u32, air: bool) -> ProcessSettings {
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(PowerPercent::new(lo).unwrap(), PowerPercent::new(hi).unwrap()).unwrap(),
        passes: Passes::new(passes).unwrap(),
        air_assist: air,
    }
}

/// One plate, two operations on it (fill mark, then line cut), an order constraint and a recipe
/// that was applied to the cut.
fn session_with_process() -> DocumentSession {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(10.0, 20.0), mm(120.0), mm(80.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }).changes.created[0];
    let fill = FillSettings::new(mm(0.1), ScanAxis::Y, ScanDirection::Unidirectional, mm(2.5)).unwrap();
    let mark = OperationDraft {
        name: "Markeren".into(),
        mode: OperationMode::Fill(fill),
        settings: settings(300.0, 10.0, 15.5, 1, false),
        output_enabled: true,
    };
    let cut = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::width(mm(0.15)).unwrap() },
        settings: settings(12.0, 55.0, 60.0, 2, true),
        output_enabled: false,
    };
    let p = |c| CommandPayload::Process(c);
    let m = apply(&mut s, p(ProcessCommand::AddOperation { draft: mark, targets: vec![g] })).changes.operations[0];
    let c = apply(&mut s, p(ProcessCommand::AddOperation { draft: cut, targets: vec![g] })).changes.operations[0];
    apply(&mut s, p(ProcessCommand::AddOrderConstraint { before: m, after: c }));
    let draft = RecipeDraft {
        name: "Populier 3 mm".into(),
        material: "populier".into(),
        thickness: mm(3.0),
        machine: "ruida-1500x900".into(),
        notes: "testkaart 2026-10-18".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(10.0, 50.0, 65.0, 1, true),
    };
    let r = apply(&mut s, p(ProcessCommand::AddRecipe { draft })).changes.recipes[0];
    apply(&mut s, p(ProcessCommand::ApplyRecipe { recipe: r, operation: c }));
    s
}

#[test]
fn process_layer_roundtrips_with_settings_constraints_and_recipes() {
    let s = session_with_process();
    let project = Project::new(ProjectUuid::from_bytes(*b"burnburn-process"), (*s.snapshot()).clone());
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/process.burnburn");
    save_project(&fs, path, &project).unwrap();
    let back = load_project(&fs, path).unwrap();

    assert!(!back.document.process().is_empty());
    assert_eq!(back.document.process(), project.document.process(), "operations, order and recipes are kept");
    assert!(back.document.same_content(&project.document));
    assert_eq!(project_hash(&back).unwrap(), project_hash(&project).unwrap());
}

#[test]
fn process_settings_are_part_of_the_project_hash() {
    let s = session_with_process();
    let a = Project::new(ProjectUuid::from_bytes([1; 16]), (*s.snapshot()).clone());
    let mut t = session_with_process();
    let op = t.snapshot().process().operations().next().unwrap().id();
    let mut draft = t.snapshot().process().operation(op).unwrap().draft();
    draft.settings.speed = Speed::mm_per_s(301.0).unwrap();
    let cmd = DocumentCommand {
        document: t.id(),
        expected: t.revision(),
        request: RequestId::from_raw(2),
        payload: CommandPayload::Process(ProcessCommand::UpdateOperation { operation: op, draft }),
    };
    t.apply(cmd).unwrap();
    let b = Project::new(ProjectUuid::from_bytes([1; 16]), (*t.snapshot()).clone());
    assert_ne!(project_hash(&a).unwrap(), project_hash(&b).unwrap(), "a speed change is a meaning change");
    assert_eq!(
        project_hash(&a).unwrap(),
        project_hash(&decode_container(&encode_container(&a).unwrap()).unwrap()).unwrap()
    );
}

#[test]
fn schema_1_and_2_operation_forms_cannot_be_mixed() {
    let bytes = encode_container(&common::sample_project()).unwrap();
    // Schema 2 with a schema-1 bare-number target.
    let legacy_target = common::rewrite_json(&bytes, "document.json", |v| v["operations"][0]["targets"][0] = 1.into());
    let err = decode_container(&legacy_target).unwrap_err();
    assert!(
        matches!(&err, burnburn_io::LoadError::InvalidValue { path, .. } if path == "operations[0].targets[0]"),
        "{err:?}"
    );
    // Schema 1 manifest with schema-2 process data.
    let old_manifest = common::rewrite_json(&bytes, "manifest.json", |v| v["schema"] = 1.into());
    let err = decode_container(&old_manifest).unwrap_err();
    assert!(
        matches!(&err, burnburn_io::LoadError::InvalidValue { path, .. } if path == "operations[0].targets[0]"),
        "{err:?}"
    );
    // Settings are validated through the domain constructors, with a path.
    let bad_power = common::rewrite_json(&bytes, "document.json", |v| {
        v["operations"][1]["process"]["settings"]["power_max_pct"] = 101.0.into()
    });
    let err = decode_container(&bad_power).unwrap_err();
    assert!(
        matches!(&err, burnburn_io::LoadError::InvalidValue { path, .. } if path == "operations[1].process.settings.power_max_pct"),
        "{err:?}"
    );
}

/// IM25 options are stored additively in schema 2 (integratie-datamodel): never refused, never
/// written as something else and never silently dropped (a reopened file would cut differently).
#[test]
fn process_options_are_kept_not_dropped() {
    use burnburn_document::process::{AdvancedOptions, LineOptions, OffsetFillOrder, OffsetFillSettings, Overcut};
    let reopened = |s: &DocumentSession| {
        let bytes = encode_container(&Project::new(ProjectUuid::from_bytes([7; 16]), (*s.snapshot()).clone())).unwrap();
        decode_container(&bytes).unwrap().document
    };
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let fresh = || {
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
        let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0];
        (s, g)
    };
    let draft = |mode| OperationDraft {
        name: "x".into(),
        mode,
        settings: settings(10.0, 5.0, 5.0, 1, false),
        output_enabled: true,
    };
    let same = |s: &DocumentSession| {
        let back = reopened(s);
        assert_eq!(back.process(), s.snapshot().process());
    };

    // Advanced options (tabs, overcut, …).
    let (mut s, g) = fresh();
    let op = apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::AddOperation {
            draft: draft(OperationMode::Line { kerf: Kerf::NONE }),
            targets: vec![g],
        }),
    )
    .changes
    .operations[0];
    let advanced = AdvancedOptions {
        line: LineOptions { overcut: Some(Overcut::new(mm(1.0)).unwrap()), ..Default::default() },
        ..Default::default()
    };
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }));
    same(&s);

    // A rotated fill.
    let (mut s, g) = fresh();
    let angled = FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0))
        .unwrap()
        .with_angle(30.0)
        .unwrap();
    apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::AddOperation {
            draft: draft(OperationMode::Fill(angled)),
            targets: vec![g],
        }),
    );
    same(&s);

    // New modes.
    let (mut s, g) = fresh();
    let ring = OffsetFillSettings::new(mm(1.0), OffsetFillOrder::OutsideIn).unwrap();
    apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::AddOperation {
            draft: draft(OperationMode::OffsetFill(ring)),
            targets: vec![g],
        }),
    );
    same(&s);
}
