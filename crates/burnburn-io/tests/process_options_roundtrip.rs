//! IM25 process options persist additively in schema 2: offset fill, scan angle, image mode,
//! tabs, lead-in/out, overcut, perforation, Z step, sub-operations and the shared-edge policy.
//! Operations without them keep their earlier file form and hash.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::process::{
    AdvancedOptions, DitherMode, FillSettings, ImageRender, ImageSettings, Kerf, Lead, LineOptions, OffsetFillOrder,
    OffsetFillSettings, OperationDraft, OperationMode, Overcut, Passes, Perforation, PowerPercent, PowerRange,
    ProcessCommand, ProcessSettings, RecipeDraft, ScanAxis, ScanDirection, SharedEdgePolicy, Speed, SubOperation,
    TabSpec, ToneSettings, ZStep,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{ContentHash, DocumentId, Length, OperationId, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn settings(speed: f64, power: f64, passes: u32) -> ProcessSettings {
    let p = PowerPercent::new(power).unwrap();
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(passes).unwrap(),
        air_assist: false,
    }
}

fn every_option() -> AdvancedOptions {
    let angled = FillSettings::new(mm(0.1), ScanAxis::Y, ScanDirection::Unidirectional, mm(1.5))
        .unwrap()
        .with_angle(-22.5)
        .unwrap();
    AdvancedOptions {
        line: LineOptions {
            tabs: Some(TabSpec::new(3, mm(0.4)).unwrap()),
            lead_in: Some(Lead::new(mm(1.0 / 3.0)).unwrap()),
            lead_out: Some(Lead::new(mm(0.75)).unwrap()),
            overcut: Some(Overcut::new(mm(0.5)).unwrap()),
            perforation: Some(Perforation::new(mm(2.0), mm(0.5)).unwrap()),
            ..Default::default()
        },
        z_step: Some(ZStep::new(mm(-0.2)).unwrap()),
        z_offset: Some(burnburn_document::process::ZOffset::new(mm(0.75)).unwrap()),
        sub_operations: vec![
            SubOperation { mode: OperationMode::Fill(angled), settings: settings(300.0, 12.5, 1) },
            SubOperation {
                mode: OperationMode::OffsetFill(OffsetFillSettings::new(mm(0.2), OffsetFillOrder::InsideOut).unwrap()),
                settings: settings(50.0, 40.0, 2),
            },
        ],
        shared_edges: SharedEdgePolicy::CutPerRegion,
    }
}

fn image(render: ImageRender) -> OperationMode {
    let s = ImageSettings::new(ContentHash::from_bytes([4; 32]), mm(0.1), mm(0.08), render)
        .unwrap()
        .with_tone(ToneSettings::new(1.8, -0.1, 1.25).unwrap())
        .with_direction(ScanDirection::Unidirectional, true)
        .with_overscan(mm(2.5))
        .unwrap();
    OperationMode::Image(s)
}

/// Every new mode once, plus a Line operation with every advanced option and a recipe with a
/// new mode.
fn session() -> (DocumentSession, Vec<OperationId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(40.0), mm(30.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }).changes.created[0];
    let angled = FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0))
        .unwrap()
        .with_angle(30.0)
        .unwrap();
    let modes = vec![
        OperationMode::Line { kerf: Kerf::width(mm(0.15)).unwrap() },
        OperationMode::Fill(angled),
        OperationMode::OffsetFill(OffsetFillSettings::new(mm(0.3), OffsetFillOrder::OutsideIn).unwrap()),
        image(ImageRender::Grayscale),
        image(ImageRender::Dither(DitherMode::Threshold { level: 140 })),
        image(ImageRender::Dither(DitherMode::FloydSteinberg)),
        image(ImageRender::Dither(DitherMode::Jarvis)),
        image(ImageRender::Dither(DitherMode::Stucki)),
        image(ImageRender::Dither(DitherMode::Atkinson)),
        image(ImageRender::Dither(DitherMode::Ordered { size: 4 })),
    ];
    let mut ops = Vec::new();
    for (i, mode) in modes.into_iter().enumerate() {
        let draft =
            OperationDraft { name: format!("op {i}"), mode, settings: settings(20.0, 30.0, 1), output_enabled: true };
        let p = CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] });
        ops.push(apply(&mut s, p).changes.operations[0]);
    }
    let p = CommandPayload::Process(ProcessCommand::SetAdvanced { operation: ops[0], advanced: every_option() });
    apply(&mut s, p);
    let draft = RecipeDraft {
        name: "Ringen".into(),
        material: "mdf".into(),
        thickness: mm(3.0),
        machine: "ruida".into(),
        notes: String::new(),
        mode: OperationMode::OffsetFill(OffsetFillSettings::new(mm(0.25), OffsetFillOrder::InsideOut).unwrap()),
        settings: settings(80.0, 25.0, 1),
    };
    apply(&mut s, CommandPayload::Process(ProcessCommand::AddRecipe { draft }));
    (s, ops)
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())
}

#[test]
fn every_process_option_survives_save_and_reopen_with_the_same_hash() {
    let (s, ops) = session();
    let p = project(&s);
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.process(), p.document.process());
    let op = back.document.process().operation(ops[0]).unwrap();
    assert_eq!(op.advanced(), &every_option());
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    // And a second roundtrip writes the same bytes (canonical, deterministic file form).
    assert_eq!(encode_container(&back).unwrap(), encode_container(&p).unwrap());
}

#[test]
fn project_hash_changes_with_each_advanced_option() {
    let (mut s, ops) = session();
    let mut seen = vec![project_hash(&project(&s)).unwrap()];
    let base = every_option();
    let variants: Vec<AdvancedOptions> = vec![
        AdvancedOptions {
            line: LineOptions { tabs: Some(TabSpec::new(4, mm(0.4)).unwrap()), ..base.line },
            ..base.clone()
        },
        AdvancedOptions { line: LineOptions { lead_in: None, ..base.line }, ..base.clone() },
        AdvancedOptions {
            line: LineOptions { lead_out: Some(Lead::new(mm(0.7)).unwrap()), ..base.line },
            ..base.clone()
        },
        AdvancedOptions { line: LineOptions { overcut: None, ..base.line }, ..base.clone() },
        AdvancedOptions {
            line: LineOptions { perforation: Some(Perforation::new(mm(2.0), mm(0.6)).unwrap()), ..base.line },
            ..base.clone()
        },
        AdvancedOptions { z_step: Some(ZStep::new(mm(0.2)).unwrap()), ..base.clone() },
        AdvancedOptions { sub_operations: base.sub_operations[..1].to_vec(), ..base.clone() },
        AdvancedOptions { shared_edges: SharedEdgePolicy::CutOnce, ..base.clone() },
        AdvancedOptions::default(),
    ];
    for advanced in variants {
        let cmd = DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(1),
            payload: CommandPayload::Process(ProcessCommand::SetAdvanced { operation: ops[0], advanced }),
        };
        s.apply(cmd).unwrap();
        let p = project(&s);
        let h = project_hash(&p).unwrap();
        assert!(!seen.contains(&h), "every option takes part in the project hash");
        seen.push(h);
        let back = decode_container(&encode_container(&p).unwrap()).unwrap();
        assert_eq!(project_hash(&back).unwrap(), h);
    }
}

#[test]
fn invalid_option_values_in_a_file_are_refused_with_their_path() {
    let (s, _) = session();
    let bytes = encode_container(&project(&s)).unwrap();
    let cases: [(&str, fn(&mut serde_json::Value)); 3] = [
        ("advanced.tabs", |v| v["operations"][0]["process"]["advanced"]["tabs"]["count"] = 0.into()),
        ("advanced.z_step_mm", |v| v["operations"][0]["process"]["advanced"]["z_step_mm"] = 80.0.into()),
        ("angle_deg", |v| v["operations"][1]["process"]["mode"]["angle_deg"] = serde_json::Value::Null),
    ];
    for (needle, edit) in cases {
        let broken = common::rewrite_json(&bytes, "document.json", edit);
        match decode_container(&broken) {
            Err(LoadError::InvalidValue { path, .. }) => assert!(path.contains(needle), "{needle}: {path}"),
            other => panic!("{needle}: expected InvalidValue, got {other:?}"),
        }
    }
}

#[test]
fn operations_without_new_options_keep_their_file_form() {
    let p = common::sample_project();
    let bytes = encode_container(&p).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut json = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("document.json").unwrap(), &mut json).unwrap();
    for absent in ["advanced", "angle_deg", "offset_fill", "image", "sub_operations", "shared_edges"] {
        assert!(!json.contains(absent), "{absent} must be omitted when unused");
    }
}
