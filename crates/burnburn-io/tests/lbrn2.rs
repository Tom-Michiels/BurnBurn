//! IM28 LightBurn (.lbrn2) subset import (P04): explicit supported list, everything else in the
//! report, cut settings as reviewable operation proposals.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::collections::BTreeSet;
use std::path::PathBuf;

use burnburn_io::import::{Lbrn2Draft, NoteKind, parse_lbrn2, supported_attributes};

fn research_fixture(name: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/fixtures").join(name))
        .unwrap()
}

/// Independent oracle: every (element, attribute) pair and every element name in the text,
/// found with plain string scanning (not the importer's tokenizer).
fn names_in(xml: &str) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
    let mut elements = BTreeSet::new();
    let mut attrs = BTreeSet::new();
    for tag in xml.split('<').skip(1) {
        if tag.starts_with('?') || tag.starts_with('/') || tag.starts_with('!') {
            continue;
        }
        let body = tag.split('>').next().unwrap();
        let mut words = body.split_whitespace();
        let name = words.next().unwrap().trim_end_matches('/').to_owned();
        let rest: String = body[name.len()..].to_owned();
        for part in rest.split('"').step_by(2) {
            if let Some(attr) = part.trim().strip_suffix('=') {
                attrs.insert((name.clone(), attr.trim().to_owned()));
            }
        }
        elements.insert(name);
    }
    (elements, attrs)
}

/// Every element and attribute is either in the published supported list or named in the
/// report: nothing is dropped silently.
fn assert_nothing_silently_dropped(xml: &str, draft: &Lbrn2Draft) {
    let (elements, attrs) = names_in(xml);
    let reported = |kind: NoteKind, name: &str| draft.report.notes.iter().any(|n| n.kind == kind && n.name == name);
    for e in &elements {
        assert!(
            supported_attributes(e).is_some()
                || reported(NoteKind::UnknownElement, e)
                || reported(NoteKind::UnmappedSetting, e),
            "element {e} neither supported nor reported"
        );
    }
    for (e, a) in &attrs {
        let supported = supported_attributes(e).is_some_and(|list| list.contains(&a.as_str()));
        let whole_element_reported = reported(NoteKind::UnknownElement, e) || reported(NoteKind::UnmappedSetting, e);
        assert!(
            supported || whole_element_reported || reported(NoteKind::UnknownAttribute, &format!("{e}@{a}")),
            "attribute {e}@{a} neither supported nor reported"
        );
    }
}

const WITH_UNKNOWNS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<LightBurnProject AppVersion="2.1.04" FormatVersion="1" MirrorX="True" Wobble="3">
    <Thumbnail Source="AAAA"/>
    <CutSetting type="Cut">
        <index Value="0"/>
        <name Value="Snijden"/>
        <maxPower Value="60"/>
        <speed Value="15"/>
        <dotTime Value="1"/>
        <tabCount Value="1"/>
    </CutSetting>
    <Shape Type="Rect" CutIndex="0" W="40" H="20" Cr="0" Glow="1">
        <XForm>1 0 0 1 50 30</XForm>
    </Shape>
    <Shape Type="Rect" CutIndex="0" W="10" H="10" Glow="2">
        <XForm>1 0 0 1 0 0</XForm>
    </Shape>
    <Mystery><Inner A="1"/></Mystery>
</LightBurnProject>
"#;

#[test]
fn import_reports_unknown_lbrn2_options_without_silent_drop() {
    let draft = parse_lbrn2(WITH_UNKNOWNS.as_bytes()).unwrap();
    let note = |kind: NoteKind, name: &str| {
        draft.report.notes.iter().find(|n| n.kind == kind && n.name == name).map(|n| n.count)
    };
    // Unknown attributes, elements and settings are named, with how often they occurred.
    assert_eq!(note(NoteKind::UnknownAttribute, "Shape@Glow"), Some(2));
    assert_eq!(note(NoteKind::UnknownAttribute, "LightBurnProject@MirrorX"), Some(1));
    assert_eq!(note(NoteKind::UnknownAttribute, "LightBurnProject@Wobble"), Some(1));
    assert_eq!(note(NoteKind::UnknownElement, "Thumbnail"), Some(1));
    assert_eq!(note(NoteKind::UnknownElement, "Mystery"), Some(1));
    assert_eq!(note(NoteKind::UnmappedSetting, "dotTime"), Some(1));
    assert_eq!(note(NoteKind::UnmappedSetting, "tabCount"), Some(1));
    assert_nothing_silently_dropped(WITH_UNKNOWNS, &draft);

    // The supported part still imports: two rectangles at mm scale, and one proposal.
    assert_eq!(draft.items.len(), 2);
    let b = draft.items[0].path.bounds().unwrap();
    assert_eq!((b.min().x, b.min().y, b.max().x, b.max().y), (30.0, 20.0, 70.0, 40.0));
    assert_eq!(draft.cut_settings.len(), 1);
    assert_eq!(draft.cut_settings[0].items, vec![0, 1]);
}

#[test]
fn research_fixtures_report_everything_they_do_not_import() {
    for name in ["rectangle-fill.lbrn2", "rectangle-fill_backup.lbrn2", "gray-ramp-grayscale.lbrn2"] {
        let bytes = research_fixture(name);
        let draft = parse_lbrn2(&bytes).unwrap();
        assert_nothing_silently_dropped(std::str::from_utf8(&bytes).unwrap(), &draft);
    }
}

use burnburn_document::process::OperationMode;
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::Point;
use burnburn_io::import::{AnalyzeError, CutKind, ImportError, ImportFormat, analyze};
use burnburn_types::{DocumentId, RequestId, StableId};

/// LightBurn's own file (research fixture, public XML of our own drawing): one closed path of
/// four vertices, moved by its XForm; one scan setting.
#[test]
fn rectangle_fixture_imports_path_and_a_disabled_fill_proposal() {
    let draft = parse_lbrn2(&research_fixture("rectangle-fill.lbrn2")).unwrap();
    assert_eq!(draft.report.app_version.as_deref(), Some("2.1.04"));
    assert_eq!(draft.items.len(), 1);
    let sp = &draft.items[0].path.subpaths()[0];
    assert!(sp.is_closed());
    let (dx, dy) = (811.68738, 428.02438);
    assert_eq!(sp.start(), Point::new(-10.0 + dx, 20.0 + dy));
    let ends: Vec<Point> = sp.segments().iter().map(|s| s.end()).collect();
    assert_eq!(
        ends,
        vec![
            Point::new(-40.0 + dx, 20.0 + dy),
            Point::new(-40.0 + dx, 35.0 + dy),
            Point::new(-10.0 + dx, 35.0 + dy),
            Point::new(-10.0 + dx, 20.0 + dy)
        ]
    );
    let p = &draft.cut_settings[0];
    assert_eq!((p.index, p.kind, p.items.clone()), (0, CutKind::Fill, vec![0]));
    let d = p.draft.as_ref().unwrap();
    assert!(!d.output_enabled, "a proposal never fires the laser before review");
    assert_eq!((d.name.as_str(), d.settings.speed.get_mm_per_s(), d.settings.power.max().get()), ("C00", 100.0, 20.0));
    assert!(matches!(d.mode, OperationMode::Fill(_)));
    // Missing values are assumed visibly, not silently.
    let fields: Vec<&str> = p.assumptions.iter().map(|a| a.field).collect();
    for f in ["minPower", "numPasses", "interval", "speed unit"] {
        assert!(fields.contains(&f), "assumption {f} listed: {fields:?}");
    }
    for unmapped in ["maxPower2", "dotTime", "priority", "tabCount", "tabCountMax"] {
        assert!(draft.report.notes.iter().any(|n| n.kind == NoteKind::UnmappedSetting && n.name == unmapped));
    }
}

const SHAPES: &str = r#"<LightBurnProject AppVersion="2.1.04" FormatVersion="1">
  <CutSetting type="Cut"><index Value="1"/><name Value="Lijn"/><minPower Value="40"/><maxPower Value="55"/>
    <speed Value="20"/><numPasses Value="2"/></CutSetting>
  <Shape Type="Group">
    <XForm>1 0 0 1 100 0</XForm>
    <Children>
      <Shape Type="Ellipse" CutIndex="1" Rx="10" Ry="5"><XForm>1 0 0 1 0 50</XForm></Shape>
      <Shape Type="Path" CutIndex="1" VertID="7" PrimID="7">
        <XForm>1 0 0 1 0 0</XForm>
        <VertList>V0 0c0x10c0y10c1x1V20 0c0x1c1x10c1y10</VertList>
        <PrimList>B0 1</PrimList>
      </Shape>
      <Shape Type="Path" CutIndex="1" VertID="7" PrimID="7"><XForm>1 0 0 1 0 100</XForm></Shape>
    </Children>
  </Shape>
  <Shape Type="Rect" CutIndex="1" W="20" H="10" Cr="2"><XForm>1 0 0 1 0 0</XForm></Shape>
  <Shape Type="Text" CutIndex="1" Font="Avenir Next,-1,100,5,50,0,0,0,0,0" Str="Hoi"><XForm>1 0 0 1 0 0</XForm></Shape>
  <Shape Type="Text" CutIndex="1" Font="Avenir Next,-1,100" Str="Met"><XForm>1 0 0 1 0 0</XForm>
    <BackupPath Type="Path"><VertList>V0 0c0x1c1x1V1 0c0x1c1x1V1 1c0x1c1x1</VertList><PrimList>LineClosed</PrimList></BackupPath>
  </Shape>
  <Shape Type="Bitmap" CutIndex="1" W="5" H="2" File="/elders/logo.png" Data="iVBORw0K"><XForm>1 0 0 1 0 0</XForm></Shape>
  <Shape Type="Polygon" CutIndex="1" N="6"/>
</LightBurnProject>"#;

#[test]
fn groups_shared_lists_text_and_bitmaps_are_imported_or_reported() {
    let draft = parse_lbrn2(SHAPES.as_bytes()).unwrap();
    assert_nothing_silently_dropped(SHAPES, &draft);
    let entities: Vec<&str> = draft.items.iter().map(|i| i.entity.as_str()).collect();
    assert_eq!(entities, ["Shape#1/Shape#1", "Shape#1/Shape#2", "Shape#1/Shape#3", "Shape#2", "Shape#4"]);
    // Group transform composes with the child's: ellipse centred at (100, 50).
    let b = draft.items[0].path.bounds().unwrap();
    assert!((b.center().x - 100.0).abs() < 1e-9 && (b.center().y - 50.0).abs() < 1e-9);
    // Bézier from the vertex control points; the shared list is reused 100 mm higher.
    let seg = draft.items[1].path.subpaths()[0].segments()[0];
    assert_eq!(
        seg,
        burnburn_geometry::Segment::Cubic {
            c1: Point::new(110.0, 10.0),
            c2: Point::new(110.0, 10.0),
            to: Point::new(120.0, 0.0)
        }
    );
    assert_eq!(draft.items[2].path.subpaths()[0].start(), Point::new(100.0, 100.0));
    // Rounded rectangle: four arcs.
    let arcs = draft.items[3].path.subpaths()[0]
        .segments()
        .iter()
        .filter(|s| matches!(s, burnburn_geometry::Segment::Arc(_)))
        .count();
    assert_eq!(arcs, 4);
    // Text: outlines only from the file's backup path; the font situation is reported.
    assert_eq!(draft.items[4].name, "Met");
    let font = &draft.report.fonts[0];
    assert_eq!((font.family.as_str(), font.texts, font.with_outlines), ("Avenir Next", 2, 1));
    let note = |k: NoteKind, n: &str| draft.report.notes.iter().find(|x| x.kind == k && x.name == n).map(|x| x.count);
    assert_eq!(note(NoteKind::NotConverted, "Text"), Some(1));
    assert_eq!(note(NoteKind::NotConverted, "Bitmap"), Some(1));
    assert_eq!(note(NoteKind::UnsupportedShape, "Polygon"), Some(1));
    assert_eq!(note(NoteKind::UnknownAttribute, "Shape@N"), Some(1));
    let asset = &draft.report.assets[0];
    assert_eq!((asset.linked_file.as_deref(), asset.embedded_chars), (Some("/elders/logo.png"), 8));
}

#[test]
fn proposals_commit_as_disabled_operations_through_the_process_api() {
    let draft = parse_lbrn2(SHAPES.as_bytes()).unwrap();
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut assets = burnburn_io::AssetStore::default();
    let asset = assets.add("application/x-lightburn", SHAPES.as_bytes());
    let run = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let created = run(&mut s, CommandPayload::Edit(draft.geometry_command(asset).unwrap())).changes.created;
    assert_eq!(created.len(), draft.items.len());
    for cmd in draft.operation_commands(&created).unwrap() {
        run(&mut s, CommandPayload::Process(cmd));
    }
    let snap = s.snapshot();
    let ops: Vec<_> = snap.process().operations().collect();
    assert_eq!(ops.len(), 1);
    assert_eq!((ops[0].name(), ops[0].targets(), ops[0].output_enabled()), ("Lijn", created.as_slice(), false));
    let st = ops[0].settings();
    assert_eq!((st.power.min().get(), st.power.max().get(), st.passes.get()), (40.0, 55.0, 2));
    assert_eq!(draft.operation_commands(&created[1..]), Err(ImportError::ItemCountMismatch));
}

#[test]
fn pdf_import_is_not_supported_yet() {
    assert_eq!(ImportFormat::from_extension("PDF"), Some(ImportFormat::Pdf));
    assert_eq!(ImportFormat::Pdf.check_supported(), Err(ImportError::NotSupportedYet(ImportFormat::Pdf)));
    assert_eq!(
        analyze(ImportFormat::Pdf, b"%PDF-1.7\n"),
        Err(AnalyzeError::Import(ImportError::NotSupportedYet(ImportFormat::Pdf)))
    );
    assert!(matches!(analyze(ImportFormat::Lbrn2, SHAPES.as_bytes()), Ok(burnburn_io::import::Analysis::Lbrn2(_))));
}

#[test]
fn lbrn2_limits_are_typed_errors() {
    use burnburn_io::import::{ParseError, ParseLimits, parse_lbrn2_with};
    let small = ParseLimits { max_bytes: 64 << 20, max_entities: 5, max_depth: 3, max_path_commands: 3 };
    assert_eq!(parse_lbrn2_with(SHAPES.as_bytes(), &small).unwrap_err(), ParseError::TooManyEntities { limit: 5 });
    let deep = "<LightBurnProject><a><b><c><d/></c></b></a></LightBurnProject>";
    let depth = ParseLimits { max_entities: 100, ..small };
    assert_eq!(parse_lbrn2_with(deep.as_bytes(), &depth).unwrap_err(), ParseError::TooDeep { limit: 3 });
    let verts = r#"<LightBurnProject><Shape Type="Path"><VertList>V0 0V1 0V1 1V0 1</VertList><PrimList>LineClosed</PrimList></Shape></LightBurnProject>"#;
    let cmds = ParseLimits { max_entities: 100, max_depth: 10, ..small };
    assert_eq!(parse_lbrn2_with(verts.as_bytes(), &cmds).unwrap_err(), ParseError::TooManyCommands { limit: 3 });
    assert!(matches!(parse_lbrn2(b"<svg/>"), Err(ParseError::Unsupported(_))));
    assert_eq!(parse_lbrn2(&[0xff, 0xfe]).unwrap_err(), ParseError::NotUtf8);
}
