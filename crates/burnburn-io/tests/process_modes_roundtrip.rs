//! W1-L3 options in the file: tab spacing, lead style/angle, dot mode, cross-hatch, flood fill,
//! fill grouping and ramp length persist additively in schema 2. Files written before them load
//! unchanged, and operations that do not use them keep their earlier file form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::process::{
    AdvancedOptions, DotMode, FillGrouping, FillSettings, Kerf, Lead, LeadStyle, LineOptions, OperationDraft,
    OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, ScanAxis, ScanDirection, Speed,
    TabPlacement, TabSpec,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, Length, OperationId, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn settings() -> ProcessSettings {
    let p = PowerPercent::new(30.0).unwrap();
    ProcessSettings {
        speed: Speed::mm_per_s(20.0).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: false,
    }
}

fn extended_fill() -> FillSettings {
    FillSettings::new(mm(0.1), ScanAxis::Y, ScanDirection::Unidirectional, mm(1.5))
        .unwrap()
        .with_angle(22.5)
        .unwrap()
        .with_cross_hatch(true)
        .with_grouping(FillGrouping::Groups)
        .with_flood_fill(true)
        .with_ramp(mm(0.75))
        .unwrap()
}

/// Operation ids 2..: spaced tabs with a tilted line lead and an arc lead, dot mode, an
/// extended fill, and a plain fill/line with none of the options.
fn session() -> (DocumentSession, Vec<OperationId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(40.0), mm(30.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }).changes.created[0];
    let plain_fill = FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    let modes = [
        OperationMode::Line { kerf: Kerf::NONE },
        OperationMode::Line { kerf: Kerf::NONE },
        OperationMode::Fill(extended_fill()),
        OperationMode::Fill(plain_fill),
    ];
    let mut ops = Vec::new();
    for (i, mode) in modes.into_iter().enumerate() {
        let draft = OperationDraft { name: format!("op {i}"), mode, settings: settings(), output_enabled: true };
        let p = CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] });
        ops.push(apply(&mut s, p).changes.operations[0]);
    }
    let tilted = Lead::new(mm(2.0)).unwrap().with_angle(30.0).unwrap();
    let arc = Lead::new(mm(1.5)).unwrap().with_style(LeadStyle::Arc);
    let line = LineOptions {
        tabs: Some(TabSpec::spaced(mm(25.0), mm(0.75)).unwrap()),
        lead_in: Some(tilted),
        lead_out: Some(arc),
        ..Default::default()
    };
    apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::SetAdvanced {
            operation: ops[0],
            advanced: AdvancedOptions { line, ..Default::default() },
        }),
    );
    let dot = LineOptions { dot: Some(DotMode::new(mm(0.4), 1.25).unwrap()), ..Default::default() };
    apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::SetAdvanced {
            operation: ops[1],
            advanced: AdvancedOptions { line: dot, ..Default::default() },
        }),
    );
    (s, ops)
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([5; 16]), (*s.snapshot()).clone())
}

fn document_json(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut json = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("document.json").unwrap(), &mut json).unwrap();
    json
}

#[test]
fn line_and_fill_extras_survive_save_and_reopen_with_the_same_hash() {
    let (s, ops) = session();
    let p = project(&s);
    let bytes = encode_container(&p).unwrap();
    let back = decode_container(&bytes).unwrap();
    assert_eq!(back.document.process(), p.document.process(), "nothing dropped, nothing invented");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    assert_eq!(encode_container(&back).unwrap(), bytes, "a second save writes the same bytes");

    let process = back.document.process();
    let line = process.operation(ops[0]).unwrap().advanced().line;
    assert_eq!(line.tabs.unwrap().placement(), TabPlacement::Spacing(25.0));
    assert_eq!((line.lead_in.unwrap().style(), line.lead_in.unwrap().angle_deg()), (LeadStyle::Line, 30.0));
    assert_eq!(line.lead_out.unwrap().style(), LeadStyle::Arc);
    assert_eq!(process.operation(ops[1]).unwrap().advanced().line.dot, Some(DotMode::new(mm(0.4), 1.25).unwrap()));
    let OperationMode::Fill(f) = process.operation(ops[2]).unwrap().mode() else { panic!("fill") };
    assert_eq!(f, extended_fill());
}

#[test]
fn operations_without_the_new_options_keep_their_earlier_file_form() {
    // A project with only the earlier options must not mention any new key.
    let p = common::sample_project();
    let json = document_json(&encode_container(&p).unwrap());
    for absent in ["cross_hatch", "flood_fill", "grouping", "ramp_mm", "dot", "spacing_mm", "style", "angle_deg"] {
        assert!(!json.contains(absent), "{absent} must be omitted when unused");
    }
    // And inside the new project the plain fill (op 5) and the counted-tab form are untouched:
    // an operation list entry for a plain fill has exactly the earlier keys.
    let (s, _) = session();
    let json: serde_json::Value =
        serde_json::from_str(&document_json(&encode_container(&project(&s)).unwrap())).unwrap();
    let plain = &json["operations"][3]["process"]["mode"];
    let mut keys: Vec<&str> = plain.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["axis", "direction", "interval_mm", "overscan_mm", "type"]);
}

#[test]
fn counted_tabs_in_an_earlier_file_load_as_counted_tabs() {
    // Earlier files wrote `tabs: { count, width_mm }`; hand-edit a saved spaced-tab file back to it.
    let (s, ops) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    let old = common::rewrite_json(&bytes, "document.json", |v| {
        v["operations"][0]["process"]["advanced"]["tabs"] = serde_json::json!({ "count": 3, "width_mm": 0.4 });
    });
    let back = decode_container(&old).unwrap();
    let tabs = back.document.process().operation(ops[0]).unwrap().advanced().line.tabs.unwrap();
    assert_eq!((tabs.placement(), tabs.count(), tabs.width_mm()), (TabPlacement::Count(3), 3, 0.4));
}

#[test]
fn invalid_extras_in_a_file_are_refused_with_their_path() {
    let (s, _) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    type Edit = fn(&mut serde_json::Value);
    let cases: [(&str, Edit); 6] = [
        ("advanced.tabs", |v| {
            v["operations"][0]["process"]["advanced"]["tabs"]["count"] = 2.into(); // count and spacing together
        }),
        ("advanced.tabs", |v| {
            v["operations"][0]["process"]["advanced"]["tabs"] = serde_json::json!({ "width_mm": 1.0 }); // neither
        }),
        ("advanced.lead_in_angle_deg", |v| {
            v["operations"][0]["process"]["advanced"]["lead_in_angle_deg"] = 120.0.into()
        }),
        ("advanced.dot", |v| v["operations"][1]["process"]["advanced"]["dot"]["dwell_ms"] = 0.0.into()),
        ("process.advanced", |v| {
            v["operations"][1]["process"]["advanced"]["tabs"] = serde_json::json!({ "count": 2, "width_mm": 1.0 })
        }),
        ("ramp_mm", |v| v["operations"][2]["process"]["mode"]["ramp_mm"] = (-1.0).into()),
    ];
    for (needle, edit) in cases {
        let broken = common::rewrite_json(&bytes, "document.json", edit);
        match decode_container(&broken) {
            Err(LoadError::InvalidValue { path, .. }) => assert!(path.contains(needle), "{needle}: {path}"),
            other => panic!("{needle}: expected InvalidValue, got {other:?}"),
        }
    }
}
