//! Common-line cutting (C12, G7-cam) in the project file: the option survives save and reopen with
//! the same hash, a file written before it loads without it, an unused option writes no key and
//! values outside the range are refused with their path.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use burnburn_document::process::{
    AdvancedOptions, CommonLine, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, Length, OperationId, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

/// Two line operations: [0] with common-line cutting, [1] without.
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
    let mut ops = Vec::new();
    for (i, common) in [Some(CommonLine::new(mm(0.05), mm(2.5)).unwrap()), None].into_iter().enumerate() {
        let draft = OperationDraft {
            name: format!("op {i}"),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings,
            output_enabled: true,
        };
        let id = apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }))
            .changes
            .operations[0];
        if common.is_some() {
            let advanced = AdvancedOptions { common_line: common, ..Default::default() };
            apply(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: id, advanced }));
        }
        ops.push(id);
    }
    (s, ops)
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([7; 16]), (*s.snapshot()).clone())
}

#[test]
fn the_option_survives_save_and_reopen_with_the_same_hash() {
    let (s, ops) = session();
    let p = project(&s);
    let bytes = encode_container(&p).unwrap();
    let back = decode_container(&bytes).unwrap();
    assert_eq!(back.document.process(), p.document.process());
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    assert_eq!(encode_container(&back).unwrap(), bytes, "a second save writes the same bytes");
    let common = |op: OperationId| {
        back.document
            .process()
            .operation(op)
            .unwrap()
            .advanced()
            .common_line
            .map(|c| (c.tolerance_mm(), c.min_overlap_mm()))
    };
    assert_eq!((common(ops[0]), common(ops[1])), (Some((0.05, 2.5)), None));
}

#[test]
fn a_file_without_the_key_means_no_common_line_and_an_unused_option_writes_no_key() {
    let (s, ops) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    let json: serde_json::Value = {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut text = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("document.json").unwrap(), &mut text).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    assert!(json["operations"][1]["process"].get("advanced").is_none_or(|a| a.get("common_line").is_none()));
    let old = common::rewrite_json(&bytes, "document.json", |v| {
        v["operations"][0]["process"]["advanced"].as_object_mut().unwrap().remove("common_line");
    });
    let back = decode_container(&old);
    // Without its only option the advanced block is empty; a file may still carry it.
    let back = back.unwrap();
    assert_eq!(back.document.process().operation(ops[0]).unwrap().advanced().common_line, None);
}

#[test]
fn values_outside_the_range_are_refused_with_their_path() {
    let (s, _) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    for (field, bad) in [("tolerance_mm", 0.0), ("tolerance_mm", 6.0), ("min_overlap_mm", -1.0)] {
        let broken = common::rewrite_json(&bytes, "document.json", |v| {
            v["operations"][0]["process"]["advanced"]["common_line"][field] = bad.into();
        });
        match decode_container(&broken) {
            Err(LoadError::InvalidValue { path, .. }) => assert!(path.contains("common_line"), "{field} {bad}: {path}"),
            other => panic!("{field} {bad}: expected InvalidValue, got {other:?}"),
        }
    }
}
