//! W3-UI-bestanden, export dialog (M026, P05): SVG/DXF options, and the list of what the file
//! cannot carry before anything is written. Constraints and modifiers need an acknowledgement.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::sketch::{Along, DimensionKind, DimensionRole, Relation, Sketch, SketchCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::Point;
use burnburn_io::export::ExportLoss;
use burnburn_io::fs::FileSystem;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, ExportCurves, ExportDialogViewModel, ExportField, ExportFormat, ExportLayers, ExportOptionsInput,
    ExportUnit, FilesIntent, FilesMessage, HeadlessEditorHarness, HostEffect, MessageKey, SelectMode, UiIntent,
};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, RequestId, StableId};

fn harness() -> HeadlessEditorHarness {
    let h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    h
}

fn dialog(h: &HeadlessEditorHarness, doc: DocumentId) -> ExportDialogViewModel {
    h.document(doc).unwrap().files.export.expect("the export dialog is open")
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, format: ExportFormat) -> ExportDialogViewModel {
    h.send(UiIntent::Files(FilesIntent::OpenExport { document: doc, format }));
    h.run_tasks();
    dialog(h, doc)
}

fn options(h: &mut HeadlessEditorHarness, doc: DocumentId, o: ExportOptionsInput) -> ExportDialogViewModel {
    h.send(UiIntent::Files(FilesIntent::SetExportOptions { document: doc, options: Box::new(o) }));
    h.run_tasks();
    dialog(h, doc)
}

fn write(h: &mut HeadlessEditorHarness, doc: DocumentId, name: &str) -> (PathBuf, ExportDialogViewModel) {
    let path = PathBuf::from(format!("/out/{name}"));
    h.send(UiIntent::Files(FilesIntent::ExportDrawing { document: doc, path: path.clone() }));
    h.run_tasks();
    (path, dialog(h, doc))
}

fn text(h: &HeadlessEditorHarness, path: &FsPath) -> String {
    String::from_utf8(h.fs().read(path, 1 << 24).unwrap()).unwrap()
}

/// A rectangle (40 × 20 mm) and a circle (diameter 10 mm), nothing selected.
fn shapes(h: &mut HeadlessEditorHarness) -> DocumentId {
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolCircle);
    h.drag(doc, (60.0, 10.0), (65.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });
    doc
}

#[test]
fn the_commands_open_the_dialog_and_report_it_as_open() {
    let mut h = harness();
    let doc = shapes(&mut h);
    for (command, format) in [(CommandKey::ExportSvg, ExportFormat::Svg), (CommandKey::ExportDxf, ExportFormat::Dxf)] {
        h.invoke(doc, command);
        h.run_tasks();
        assert_eq!(dialog(&h, doc).format, format);
        h.send(UiIntent::Files(FilesIntent::CloseExport { document: doc }));
        assert!(h.document(doc).unwrap().files.export.is_none());
    }
    let update = h.invoke(doc, CommandKey::ImportBundle);
    assert!(update.effects.contains(&HostEffect::ShowBundleOpenPanel { document: doc }));
    h.invoke(doc, CommandKey::ShowMaterialLibrary);
    assert!(h.view().commands[&CommandKey::ShowMaterialLibrary].checked);
    h.invoke(doc, CommandKey::ShowMaterialLibrary);
    assert!(!h.view().commands[&CommandKey::ShowMaterialLibrary].checked);
    h.invoke(doc, CommandKey::ShowArtLibrary);
    assert!(h.document(doc).unwrap().files.art.is_some());
}

/// Selecting limits the file to the selection unless the dialog says otherwise.
#[test]
fn the_dialog_counts_the_objects_and_the_selection_decides_the_default_scope() {
    let mut h = harness();
    let doc = shapes(&mut h);
    let d = open(&mut h, doc, ExportFormat::Svg);
    assert_eq!((d.objects, d.options.selection_only), (2, false));
    h.send(UiIntent::Files(FilesIntent::CloseExport { document: doc }));
    let first = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![first], mode: SelectMode::Replace });
    let d = open(&mut h, doc, ExportFormat::Svg);
    assert_eq!((d.objects, d.options.selection_only), (1, true), "a selection is exported by default");
    let all = ExportOptionsInput { selection_only: false, ..d.options.clone() };
    assert_eq!(options(&mut h, doc, all).objects, 2);
}

#[test]
fn nothing_to_export_is_refused_without_writing() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let d = open(&mut h, doc, ExportFormat::Svg);
    assert_eq!(d.objects, 0);
    let (path, d) = write(&mut h, doc, "leeg.svg");
    assert_eq!(d.issue, Some(MessageKey::Files(FilesMessage::ExportEmpty)));
    assert!(!h.fs().exists(&path).unwrap());
}

/// SVG options reach the file: unit and decimals decide the size text (40 × 20 mm = 1.57 × 0.79 in).
#[test]
fn svg_options_decide_the_file() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let mut o = open(&mut h, doc, ExportFormat::Svg).options;
    let (path, _) = write(&mut h, doc, "mm.svg");
    assert!(text(&h, &path).contains(r#"width="40mm" height="20mm""#), "{}", text(&h, &path));

    o.unit = ExportUnit::Inch;
    o.decimals = "2".into();
    options(&mut h, doc, o.clone());
    let (path, d) = write(&mut h, doc, "inch.svg");
    assert_eq!(d.written, Some(path.clone()));
    assert!(text(&h, &path).contains(r#"width="1.57in" height="0.79in""#), "{}", text(&h, &path));

    o.unit = ExportUnit::Pixel;
    o.decimals = "1".into();
    options(&mut h, doc, o);
    let (path, _) = write(&mut h, doc, "px.svg");
    assert!(text(&h, &path).contains(r#"width="151.2px" height="75.6px""#), "{}", text(&h, &path));
}

/// The layer of an object is its operation: grouped in SVG, a layer in DXF when asked for.
#[test]
fn layers_follow_the_operations_when_asked() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let layer = h.document(doc).unwrap().operations[0].name.clone();
    let mut o = open(&mut h, doc, ExportFormat::Svg).options;
    o.layers = ExportLayers::Layer;
    options(&mut h, doc, o);
    let (path, _) = write(&mut h, doc, "lagen.svg");
    assert!(text(&h, &path).contains(&format!(r#"<g id="{layer}">"#)), "{}", text(&h, &path));

    h.send(UiIntent::Files(FilesIntent::CloseExport { document: doc }));
    let mut o = open(&mut h, doc, ExportFormat::Dxf).options;
    o.layers = ExportLayers::Layer;
    options(&mut h, doc, o);
    let (path, _) = write(&mut h, doc, "lagen.dxf");
    assert!(text(&h, &path).contains(&layer), "{}", text(&h, &path));
}

#[test]
fn a_wrong_option_names_its_field_and_the_last_good_options_stay() {
    let mut h = harness();
    let doc = shapes(&mut h);
    let good = open(&mut h, doc, ExportFormat::Svg).options;
    for (field, bad) in [
        (ExportField::Stroke, ExportOptionsInput { stroke_mm: "dik".into(), ..good.clone() }),
        (ExportField::Stroke, ExportOptionsInput { stroke_mm: "0".into(), ..good.clone() }),
        (ExportField::Decimals, ExportOptionsInput { decimals: "13".into(), ..good.clone() }),
        (ExportField::Decimals, ExportOptionsInput { decimals: "-1".into(), ..good.clone() }),
        (ExportField::Tolerance, ExportOptionsInput { tolerance_mm: "0".into(), ..good.clone() }),
    ] {
        let d = options(&mut h, doc, bad);
        assert_eq!(d.field_error, Some(field));
        assert_eq!(d.options, good, "the last good options are kept");
        assert_eq!(d.issue, Some(MessageKey::Files(FilesMessage::ExportInvalidOption)));
    }
    let d = options(&mut h, doc, ExportOptionsInput { decimals: "3".into(), ..good });
    assert!(d.field_error.is_none() && d.issue.is_none());
}

/// DXF as polylines names the curves it flattened; an ellipse is one of them (a circle is written
/// exactly as bulges).
#[test]
fn dxf_curve_options_change_what_is_reported() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolEllipse);
    h.drag(doc, (60.0, 0.0), (90.0, 15.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });
    let d = open(&mut h, doc, ExportFormat::Dxf);
    assert!(!d.needs_acknowledgement);
    let mut o = d.options;
    o.curves = ExportCurves::Polylines;
    o.tolerance_mm = "0.01".into();
    let d = options(&mut h, doc, o);
    let flattened: Vec<_> = d.losses.iter().filter(|l| matches!(l.loss, ExportLoss::CurvesFlattened { .. })).collect();
    assert_eq!(flattened.len(), 1, "only the ellipse has curves: {:?}", d.losses);
    assert!(!flattened[0].object.is_empty());
    let ExportLoss::CurvesFlattened { max_deviation_mm, .. } = flattened[0].loss else { panic!() };
    assert!(max_deviation_mm <= 0.01 + 1e-12, "{max_deviation_mm}");
    assert!(!d.needs_acknowledgement, "approximations are information, not a blocker");
    let (path, d) = write(&mut h, doc, "krom.dxf");
    assert_eq!(d.written, Some(path.clone()));
    assert!(text(&h, &path).contains("LWPOLYLINE") || text(&h, &path).contains("POLYLINE"));
}

/// A project with a sketch that has six constraints: 4 relations and 2 dimensions.
fn sketch_project() -> (HeadlessEditorHarness, DocumentId) {
    let mut s = Sketch::new("Plaat");
    let p = [
        s.point(0.0, 0.0).unwrap(),
        s.point(98.0, 1.0).unwrap(),
        s.point(99.0, 41.0).unwrap(),
        s.point(1.0, 39.0).unwrap(),
    ];
    let e = [
        s.line(p[0], p[1]).unwrap(),
        s.line(p[1], p[2]).unwrap(),
        s.line(p[2], p[3]).unwrap(),
        s.line(p[3], p[0]).unwrap(),
    ];
    s.relate(Relation::Fix { point: p[0], at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Horizontal(e[0])).unwrap();
    s.relate(Relation::Vertical(e[1])).unwrap();
    s.relate(Relation::Horizontal(e[2])).unwrap();
    s.dimension(DimensionKind::Distance { a: p[0], b: p[1], along: Along::X }, "100 mm", DimensionRole::Driving)
        .unwrap();
    s.dimension(DimensionKind::Distance { a: p[1], b: p[2], along: Along::Y }, "40 mm", DimensionRole::Driving)
        .unwrap();
    let mut session = DocumentSession::new(DocumentId::from_raw(77));
    let cmd = DocumentCommand {
        document: session.id(),
        expected: session.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(s) }),
    };
    session.apply_with(cmd, &[&SketchEvaluator::new(SketchSolverAdapter)]).expect("solves");
    let mut h = harness();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from("/work/plaat.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([5; 16]), (*session.snapshot()).clone()))
        .unwrap();
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    (h, doc)
}

/// P05: the dropped constraints are listed with the file's first object before anything is
/// written, and the file is only written after the acknowledgement.
#[test]
fn export_lists_lost_constraints_before_writing() {
    let (mut h, doc) = sketch_project();
    let d = open(&mut h, doc, ExportFormat::Svg);
    assert!(d.objects >= 1);
    let dropped: Vec<_> = d.losses.iter().filter(|l| matches!(l.loss, ExportLoss::ConstraintsDropped { .. })).collect();
    assert_eq!(dropped.len(), 1, "reported once for the sketch: {:?}", d.losses);
    let ExportLoss::ConstraintsDropped { count, .. } = dropped[0].loss else { panic!() };
    assert_eq!(count, 6, "four relations and two dimensions");
    assert!(d.needs_acknowledgement && !d.acknowledged);

    let (path, d) = write(&mut h, doc, "plaat.svg");
    assert_eq!(d.issue, Some(MessageKey::Files(FilesMessage::ExportNeedsAcknowledgement)));
    assert!(!h.fs().exists(&path).unwrap(), "nothing is written before the acknowledgement");

    h.send(UiIntent::Files(FilesIntent::SetExportAcknowledged { document: doc, acknowledged: true }));
    let (path, d) = write(&mut h, doc, "plaat.svg");
    assert_eq!(d.written, Some(path.clone()));
    assert!(text(&h, &path).starts_with("<?xml"), "{}", text(&h, &path));
}

#[test]
fn a_result_for_older_options_is_dropped() {
    let mut h = harness();
    let doc = shapes(&mut h);
    open(&mut h, doc, ExportFormat::Dxf);
    let base = dialog(&h, doc).options;
    h.send(UiIntent::Files(FilesIntent::SetExportOptions {
        document: doc,
        options: Box::new(ExportOptionsInput { curves: ExportCurves::Polylines, ..base.clone() }),
    }));
    let older = h.take_task_requests();
    h.send(UiIntent::Files(FilesIntent::SetExportOptions { document: doc, options: Box::new(base) }));
    let newer = h.take_task_requests();
    for request in older.into_iter().chain(newer) {
        h.complete_despite_cancel(request);
    }
    let d = dialog(&h, doc);
    assert!(!d.checking);
    assert!(
        !d.losses.iter().any(|l| matches!(l.loss, ExportLoss::CurvesFlattened { .. })),
        "the report of the older (polyline) options must not show: {:?}",
        d.losses
    );
}

/// A pattern is a modifier: its copies are written as plain objects, which must be acknowledged.
#[test]
fn a_pattern_is_reported_as_modifiers_applied_and_needs_the_acknowledgement() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let source = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![source], mode: SelectMode::Replace });
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.pattern.rectangular".into() });
    h.send(UiIntent::SetToolOption { document: doc, key: burnburn_presentation::ParamKey::Columns, text: "2".into() });
    h.send(UiIntent::SetToolOption { document: doc, key: burnburn_presentation::ParamKey::Rows, text: "2".into() });
    h.drag(doc, (0.0, 0.0), (30.0, 40.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });

    let d = open(&mut h, doc, ExportFormat::Dxf);
    assert!(d.objects >= 2, "source and copies: {}", d.objects);
    let applied: Vec<_> = d.losses.iter().filter(|l| matches!(l.loss, ExportLoss::ModifiersApplied { .. })).collect();
    assert_eq!(applied.len(), 1, "{:?}", d.losses);
    let ExportLoss::ModifiersApplied { count, .. } = applied[0].loss else { panic!() };
    assert_eq!(count, 1, "one array step");
    assert!(d.needs_acknowledgement);
    let (path, d) = write(&mut h, doc, "reeks.dxf");
    assert_eq!(d.issue, Some(MessageKey::Files(FilesMessage::ExportNeedsAcknowledgement)));
    assert!(!h.fs().exists(&path).unwrap());
}

/// Layer colours come from the host's palette (slot by slot) and are left out when asked.
#[test]
fn layer_colours_come_from_the_host_palette() {
    let mut h = harness();
    h.controller().set_layer_palette(vec![[0, 0, 0], [255, 0, 0], [0, 0, 255]]);
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let mut o = open(&mut h, doc, ExportFormat::Svg).options;
    let (path, _) = write(&mut h, doc, "kleur.svg");
    assert!(text(&h, &path).contains(r##"stroke="#ff0000""##), "{}", text(&h, &path));
    o.use_colors = false;
    options(&mut h, doc, o);
    let (path, _) = write(&mut h, doc, "zwart.svg");
    assert!(text(&h, &path).contains(r##"stroke="#000000""##), "{}", text(&h, &path));
}
