//! W2-L3: optimization settings and cut priorities persist additively in schema 2. Documents
//! without them keep their file form and hash; stored ones come back unchanged or are refused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::process::{CutPriority, OptimizationSettings, OrderKey, ProcessCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};
use common::rewrite_json;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn apply(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn session() -> (DocumentSession, GeometryId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0];
    (s, g)
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([5; 16]), (*s.snapshot()).clone())
}

fn everything() -> OptimizationSettings {
    OptimizationSettings::default()
        .with_order(vec![OrderKey::Priority, OrderKey::Group, OrderKey::Layer])
        .unwrap()
        .with_inner_first(false)
        .with_reduce_travel(false)
        .with_best_start(false)
        .with_best_direction(false)
        .with_hide_backlash(true)
        .with_remove_overlapping(Some(mm(0.25)))
        .unwrap()
}

#[test]
fn optimization_settings_and_priorities_survive_save_and_reopen() {
    let (mut s, g) = session();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: everything() }));
    let priority = CutPriority::new(-7).unwrap();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority }));
    let p = project(&s);
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.process().optimization(), &everything());
    assert_eq!(back.document.process().priority(g), priority);
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    assert_eq!(encode_container(&back).unwrap(), encode_container(&p).unwrap());
}

#[test]
fn the_project_hash_sees_the_order_hierarchy_and_each_priority() {
    let (mut s, g) = session();
    let mut seen = vec![project_hash(&project(&s)).unwrap()];
    for order in [vec![OrderKey::Layer, OrderKey::Priority], vec![OrderKey::Priority, OrderKey::Layer]] {
        let settings = OptimizationSettings::default().with_order(order).unwrap();
        apply(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings }));
        seen.push(project_hash(&project(&s)).unwrap());
    }
    let priority = CutPriority::new(1).unwrap();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority }));
    seen.push(project_hash(&project(&s)).unwrap());
    let mut unique = seen.clone();
    unique.sort_by_key(|h| h.to_string());
    unique.dedup();
    assert_eq!(unique.len(), seen.len(), "every state hashes differently");
}

#[test]
fn documents_without_the_settings_keep_their_file_form() {
    let (s, _) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("optimization") && !text.contains("priorities"), "default form is unchanged");
}

fn stored(s: &DocumentSession) -> Vec<u8> {
    encode_container(&project(s)).unwrap()
}

#[test]
fn a_stored_priority_for_a_missing_shape_or_an_unknown_level_is_refused() {
    let (mut s, g) = session();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: everything() }));
    let priority = CutPriority::new(2).unwrap();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority }));

    let missing = rewrite_json(&stored(&s), "document.json", |v| v["priorities"][0]["geometry"] = 99.into());
    assert!(matches!(decode_container(&missing), Err(LoadError::InvalidValue { .. })), "unknown shape");

    let unknown = rewrite_json(&stored(&s), "document.json", |v| v["optimization"]["order_by"][0] = "depth".into());
    assert!(decode_container(&unknown).is_err(), "unknown level");

    let duplicate =
        rewrite_json(&stored(&s), "document.json", |v| v["optimization"]["order_by"][1] = "priority".into());
    assert!(matches!(decode_container(&duplicate), Err(LoadError::InvalidValue { .. })), "duplicate level");

    let range = rewrite_json(&stored(&s), "document.json", |v| v["priorities"][0]["priority"] = 1_000_000.into());
    assert!(matches!(decode_container(&range), Err(LoadError::InvalidValue { .. })), "priority range");

    let tolerance =
        rewrite_json(&stored(&s), "document.json", |v| v["optimization"]["remove_overlapping_mm"] = (-1.0).into());
    assert!(matches!(decode_container(&tolerance), Err(LoadError::InvalidValue { .. })), "tolerance");
}

#[test]
fn a_focus_offset_survives_save_and_reopen_and_is_validated_on_load() {
    use burnburn_document::process::{AdvancedOptions, Kerf, OperationDraft, OperationMode, ZOffset};
    let (mut s, g) = session();
    let p = burnburn_document::process::PowerPercent::new(10.0).unwrap();
    let draft = OperationDraft {
        name: "Focus".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: burnburn_document::process::ProcessSettings {
            speed: burnburn_document::process::Speed::mm_per_s(10.0).unwrap(),
            power: burnburn_document::process::PowerRange::new(p, p).unwrap(),
            passes: burnburn_document::process::Passes::new(1).unwrap(),
            air_assist: false,
        },
        output_enabled: true,
    };
    let op = apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }))
        .changes
        .operations[0];
    let advanced = AdvancedOptions { z_offset: Some(ZOffset::new(mm(-1.25)).unwrap()), ..Default::default() };
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced: advanced.clone() }));
    let p = project(&s);
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.process().operation(op).unwrap().advanced(), &advanced);
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());

    let zero = rewrite_json(&stored(&s), "document.json", |v| {
        v["operations"][0]["process"]["advanced"]["z_offset_mm"] = 0.0.into();
    });
    assert!(matches!(decode_container(&zero), Err(LoadError::InvalidValue { .. })));
}
