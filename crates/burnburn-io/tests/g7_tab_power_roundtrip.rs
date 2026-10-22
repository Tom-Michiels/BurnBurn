//! Tab Cut Power (C04, G7-cam) in the project file: the power survives save and reopen with the
//! same hash, a file written before it loads as "tabs are skipped", an unused power writes no key
//! and a power outside (0, 100) is refused with its path.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use burnburn_document::process::{
    AdvancedOptions, Kerf, LineOptions, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, Speed, TabSpec,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, Length, OperationId, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

/// Two line operations: [0] counted tabs at 25 %, [1] spaced tabs that are skipped.
fn session() -> (DocumentSession, Vec<OperationId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(40.0), mm(30.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }).changes.created[0];
    let p = PowerPercent::new(30.0).unwrap();
    let settings = ProcessSettings {
        speed: Speed::mm_per_s(20.0).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: false,
    };
    let tabs = [
        TabSpec::new(3, mm(0.5)).unwrap().with_cut_power(25.0).unwrap(),
        TabSpec::spaced(mm(20.0), mm(0.5)).unwrap(),
    ];
    let mut ops = Vec::new();
    for (i, t) in tabs.into_iter().enumerate() {
        let draft = OperationDraft {
            name: format!("op {i}"),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings,
            output_enabled: true,
        };
        let id = apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }))
            .changes
            .operations[0];
        let advanced =
            AdvancedOptions { line: LineOptions { tabs: Some(t), ..Default::default() }, ..Default::default() };
        apply(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: id, advanced }));
        ops.push(id);
    }
    (s, ops)
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([7; 16]), (*s.snapshot()).clone())
}

#[test]
fn the_tab_power_survives_save_and_reopen_with_the_same_hash() {
    let (s, ops) = session();
    let p = project(&s);
    let bytes = encode_container(&p).unwrap();
    let back = decode_container(&bytes).unwrap();
    assert_eq!(back.document.process(), p.document.process());
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    assert_eq!(encode_container(&back).unwrap(), bytes, "a second save writes the same bytes");
    let power = |op: OperationId| {
        back.document.process().operation(op).unwrap().advanced().line.tabs.unwrap().cut_power_percent()
    };
    assert_eq!((power(ops[0]), power(ops[1])), (Some(25.0), None));
}

#[test]
fn a_file_without_the_key_means_skipped_tabs_and_an_unused_power_writes_no_key() {
    let (s, ops) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    let json: serde_json::Value = {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut text = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("document.json").unwrap(), &mut text).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    assert!(json["operations"][1]["process"]["advanced"]["tabs"].get("cut_power_percent").is_none());
    // Hand-edit the first operation back to the form written before the key existed.
    let old = common::rewrite_json(&bytes, "document.json", |v| {
        v["operations"][0]["process"]["advanced"]["tabs"].as_object_mut().unwrap().remove("cut_power_percent");
    });
    let back = decode_container(&old).unwrap();
    let tabs = back.document.process().operation(ops[0]).unwrap().advanced().line.tabs.unwrap();
    assert_eq!((tabs.count(), tabs.cut_power_percent()), (3, None));
}

#[test]
fn a_power_outside_the_open_range_is_refused_with_its_path() {
    let (s, _) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    for bad in [0.0, 100.0, -3.0, 250.0] {
        let broken = common::rewrite_json(&bytes, "document.json", |v| {
            v["operations"][0]["process"]["advanced"]["tabs"]["cut_power_percent"] = bad.into();
        });
        match decode_container(&broken) {
            Err(LoadError::InvalidValue { path, .. }) => assert!(path.contains("advanced.tabs"), "{bad}: {path}"),
            other => panic!("{bad}: expected InvalidValue, got {other:?}"),
        }
    }
}
