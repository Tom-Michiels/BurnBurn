//! Tool layers T1/T2 (LY01, G5-cam): layers for guides and registration. They never produce
//! output, whatever anyone asks, they keep their geometry by reference like any layer, and they
//! change neither the hash of ordinary operations nor their order.

#![allow(clippy::unwrap_used)]

use burnburn_document::process::{
    AdvancedOptions, DotMode, Kerf, LineOptions, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange,
    ProcessCommand, ProcessError, ProcessSettings, Speed, ToolLayer,
};
use burnburn_document::{CommandError, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{CanonicalEncode, CanonicalWriter, DocumentId, GeometryId, Length, OperationId, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<burnburn_document::CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd)
}

fn rect(s: &mut DocumentSession) -> GeometryId {
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: "Hulplijn".into() }).unwrap().changes.created[0]
}

fn draft() -> OperationDraft {
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: "Gewoon".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

fn add_tool(s: &mut DocumentSession, tool: ToolLayer, g: GeometryId) -> OperationId {
    let before: Vec<OperationId> = s.snapshot().process().operations().map(|o| o.id()).collect();
    run(s, CommandPayload::Process(ProcessCommand::AddToolLayer { tool, targets: vec![g] })).unwrap();
    s.snapshot().process().operations().map(|o| o.id()).find(|id| !before.contains(id)).unwrap()
}

#[test]
fn a_tool_layer_references_its_geometry_and_never_produces_output() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = rect(&mut s);
    let id = add_tool(&mut s, ToolLayer::T1, g);
    let snap = s.snapshot();
    let op = snap.process().operation(id).unwrap();
    assert_eq!(op.tool(), Some(ToolLayer::T1));
    assert_eq!(op.targets(), &[g]);
    assert!(!op.output_enabled(), "a tool layer is never part of a job");
    assert_eq!(op.name(), "T1");
    assert_eq!(snap.process().operations_using(g), vec![id], "the object is still inspectable as linked");
}

#[test]
fn nobody_can_switch_output_on_for_a_tool_layer() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = rect(&mut s);
    let id = add_tool(&mut s, ToolLayer::T2, g);
    let rev = s.revision();
    let on = OperationDraft { output_enabled: true, ..s.snapshot().process().operation(id).unwrap().draft() };
    let r = run(&mut s, CommandPayload::Process(ProcessCommand::UpdateOperation { operation: id, draft: on }));
    assert!(matches!(r, Err(CommandError::Process(ProcessError::ToolLayerNoOutput(_)))), "{r:?}");
    assert_eq!(s.revision(), rev);
    assert!(!s.snapshot().process().operation(id).unwrap().output_enabled());
}

#[test]
fn a_tool_layer_cannot_carry_cut_options_and_an_ordinary_layer_cannot_become_one_with_them() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = rect(&mut s);
    let id = add_tool(&mut s, ToolLayer::T1, g);
    let dot = DotMode::new(mm(1.0), 1.0).unwrap();
    let with_dots = AdvancedOptions {
        tool: Some(ToolLayer::T1),
        line: LineOptions { dot: Some(dot), ..Default::default() },
        ..Default::default()
    };
    let r = run(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: id, advanced: with_dots }));
    assert!(matches!(r, Err(CommandError::Process(ProcessError::ConflictingOptions { .. }))), "{r:?}");
}

#[test]
fn a_tool_layer_is_undoable_in_one_step_and_changes_the_project_hash() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = rect(&mut s);
    let hash_before = canonical(&s);
    let id = add_tool(&mut s, ToolLayer::T1, g);
    assert_ne!(canonical(&s), hash_before);
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().process().operation(id).is_none());
    assert_eq!(canonical(&s), hash_before);
}

#[test]
fn an_ordinary_operation_keeps_its_earlier_hash_and_a_tool_layer_does_not_collide_with_it() {
    // An operation without a tool role is encoded exactly as before the role existed.
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = rect(&mut s);
    run(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft: draft(), targets: vec![g] })).unwrap();
    let ordinary = canonical(&s);
    let mut t = DocumentSession::new(DocumentId::from_raw(1));
    let g = rect(&mut t);
    add_tool(&mut t, ToolLayer::T1, g);
    let tool = canonical(&t);
    assert_ne!(ordinary, tool);
    let mut u = DocumentSession::new(DocumentId::from_raw(1));
    let g = rect(&mut u);
    add_tool(&mut u, ToolLayer::T2, g);
    assert_ne!(canonical(&u), tool, "T1 and T2 differ");
}

#[test]
fn the_role_itself_is_in_the_hash_not_just_the_name() {
    let hash = |tool| {
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        let g = rect(&mut s);
        let id = add_tool(&mut s, tool, g);
        let renamed = OperationDraft { name: "Hulp".into(), ..s.snapshot().process().operation(id).unwrap().draft() };
        run(&mut s, CommandPayload::Process(ProcessCommand::UpdateOperation { operation: id, draft: renamed })).unwrap();
        canonical(&s)
    };
    assert_ne!(hash(ToolLayer::T1), hash(ToolLayer::T2));
}

fn canonical(s: &DocumentSession) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    s.snapshot().process().encode(&mut w);
    w.finish().unwrap()
}
