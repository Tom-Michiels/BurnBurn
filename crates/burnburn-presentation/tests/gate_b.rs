//! Integration gate B scenarios (docs/user-flows/gate-b.md): mounting plate, logo trace, and
//! "JobPlan for this revision → preview → RD export". Headless, through the real AppController.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::sketch::{Along, DimensionKind, DimensionRole, Relation, Sketch, SketchCommand, SketchLabel};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, GeometrySource, UndoLabel};
use burnburn_geometry::Point;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, SketchStatus, UiIntent};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, GeometryId, RequestId, StableId};

/// 120 × 80 plate, four Ø4 holes symmetric about both centre lines, 10 mm from the edges
/// (the plan's montageplaat; same construction as `burnburn-sketch/tests/common`).
fn plate() -> Sketch {
    let mut s = Sketch::new("Montageplaat");
    for (n, v) in [("width", "120 mm"), ("height", "80 mm"), ("offset", "10 mm"), ("hole", "4 mm")] {
        s.set_parameter(n, v).unwrap();
    }
    let p0 = s.point(0.5, -0.3).unwrap();
    let p1 = s.point(118.0, 1.0).unwrap();
    let p2 = s.point(121.0, 79.0).unwrap();
    let p3 = s.point(-1.0, 81.0).unwrap();
    let e = [s.line(p0, p1).unwrap(), s.line(p1, p2).unwrap(), s.line(p2, p3).unwrap(), s.line(p3, p0).unwrap()];
    s.relate(Relation::Fix { point: p0, at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Horizontal(e[0])).unwrap();
    s.relate(Relation::Vertical(e[1])).unwrap();
    s.relate(Relation::Horizontal(e[2])).unwrap();
    s.relate(Relation::Vertical(e[3])).unwrap();
    s.dimension(DimensionKind::Distance { a: p0, b: p1, along: Along::X }, "width", DimensionRole::Driving).unwrap();
    s.dimension(DimensionKind::Distance { a: p0, b: p3, along: Along::Y }, "height", DimensionRole::Driving).unwrap();
    let mb = s.point(61.0, 0.0).unwrap();
    let mt = s.point(59.0, 80.0).unwrap();
    let ml = s.point(0.0, 41.0).unwrap();
    let mr = s.point(120.0, 39.0).unwrap();
    s.relate(Relation::Midpoint { point: mb, line: e[0] }).unwrap();
    s.relate(Relation::Midpoint { point: mt, line: e[2] }).unwrap();
    s.relate(Relation::Midpoint { point: ml, line: e[3] }).unwrap();
    s.relate(Relation::Midpoint { point: mr, line: e[1] }).unwrap();
    let vaxis = s.construction_line(mb, mt).unwrap();
    let haxis = s.construction_line(ml, mr).unwrap();
    let c = [
        s.point(11.0, 9.0).unwrap(),
        s.point(108.0, 11.0).unwrap(),
        s.point(111.0, 71.0).unwrap(),
        s.point(9.0, 69.0).unwrap(),
    ];
    let holes = [
        s.circle(c[0], 2.2).unwrap(),
        s.circle(c[1], 1.9).unwrap(),
        s.circle(c[2], 2.0).unwrap(),
        s.circle(c[3], 2.1).unwrap(),
    ];
    s.dimension(DimensionKind::Distance { a: p0, b: c[0], along: Along::X }, "offset", DimensionRole::Driving).unwrap();
    s.dimension(DimensionKind::Distance { a: p0, b: c[0], along: Along::Y }, "offset", DimensionRole::Driving).unwrap();
    s.relate(Relation::Symmetric { a: c[0], b: c[1], about: vaxis }).unwrap();
    s.relate(Relation::Symmetric { a: c[1], b: c[2], about: haxis }).unwrap();
    s.relate(Relation::Symmetric { a: c[0], b: c[3], about: haxis }).unwrap();
    s.dimension(DimensionKind::Diameter { curve: holes[0] }, "hole", DimensionRole::Driving).unwrap();
    for h in &holes[1..] {
        s.relate(Relation::Equal(holes[0], *h)).unwrap();
    }
    s
}

fn opened(h: &mut HeadlessEditorHarness, path: PathBuf) -> DocumentId {
    h.send(UiIntent::Open { path });
    h.run_tasks()
        .iter()
        .flat_map(|u| &u.effects)
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(*document),
            _ => None,
        })
        .expect("opened")
}

/// The plate as a saved project (made with the domain API, as the sketch editor would leave it).
fn plate_project(h: &HeadlessEditorHarness) -> PathBuf {
    let mut s = DocumentSession::new(DocumentId::from_raw(99));
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(plate()) }),
    };
    s.apply_with(cmd, &[&SketchEvaluator::new(SketchSolverAdapter)]).expect("plate solves");
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from("/work/montageplaat.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone()))
        .expect("saved");
    path
}

fn centres(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(GeometryId, (f64, f64), (f64, f64))> {
    let scene = h.scene(doc).expect("scene");
    scene
        .items
        .iter()
        .map(|i| {
            let b = i.path.bounds().expect("bounds");
            (i.source, (b.center().x, b.center().y), (b.width(), b.height()))
        })
        .collect()
}

/// Gate B: the mounting plate stays correct when its width changes (S02, B8).
#[test]
fn headless_gate_b_mounting_plate_width_change() {
    let mut h = HeadlessEditorHarness::new();
    let path = plate_project(&h);
    let doc = opened(&mut h, path);
    let vm = h.document(doc).expect("view");
    let sketch = vm.sketches.first().expect("sketch listed").id;
    assert_eq!(vm.sketches[0].status, SketchStatus::Valid);
    let before = centres(&h, doc);
    assert_eq!(before.len(), 5, "outline + 4 holes");

    h.send(UiIntent::SetSketchParameter { document: doc, sketch, name: "width".into(), text: "150 mm".into() });
    let vm = h.document(doc).expect("view");
    assert_eq!(vm.undo, Some(UndoLabel::Sketch(SketchLabel::SetParameter)), "one entry for definition + geometry");
    assert_eq!(vm.sketches[0].status, SketchStatus::Valid);
    let after = centres(&h, doc);
    let ids: Vec<_> = after.iter().map(|(id, ..)| *id).collect();
    assert_eq!(ids, before.iter().map(|(id, ..)| *id).collect::<Vec<_>>(), "holes keep their GeometryIds");
    let holes: Vec<_> = after.iter().filter(|(_, _, size)| size.0 < 10.0).map(|(_, c, _)| *c).collect();
    for (c, (x, y)) in holes.iter().zip([(10.0, 10.0), (140.0, 10.0), (140.0, 70.0), (10.0, 70.0)]) {
        assert!((c.0 - x).abs() <= 1e-9 && (c.1 - y).abs() <= 1e-9, "{c:?} vs ({x}, {y})");
    }
    let outline = after.iter().find(|(_, _, size)| size.0 > 100.0).expect("outline").2;
    assert!((outline.0 - 150.0).abs() <= 1e-9 && (outline.1 - 80.0).abs() <= 1e-9);
    // Derived geometry is regenerated, never edited directly.
    assert!(h.scene(doc).expect("scene").items.iter().all(|i| i.path.bounds().is_some()));

    // An invalid expression keeps the last valid geometry and says so.
    h.send(UiIntent::SetSketchParameter { document: doc, sketch, name: "width".into(), text: "breedte +".into() });
    let vm = h.document(doc).expect("view");
    assert!(matches!(vm.sketches[0].status, SketchStatus::Failed), "{:?}", vm.sketches[0].status);
    assert_eq!(centres(&h, doc), after, "last valid geometry stays");

    h.invoke(doc, CommandKey::Undo);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(centres(&h, doc), before, "undo restores definition and geometry together");
    let _ = GeometrySource::Drawn;
}

mod cam_flow {
    use super::*;
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_presentation::{FieldKey, SelectMode};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile, decode_rd_file};

    const RUIDA: RuidaProfile =
        RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };

    fn profile(revision: u32) -> MachineProfile {
        MachineProfile::new(
            "research-1500x900",
            revision,
            MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
            1000.0,
            400.0,
            PowerRatio::new(0.0).unwrap(),
            PowerRatio::new(1.0).unwrap(),
        )
        .unwrap()
    }

    fn cut(speed: f64, power: f64) -> OperationDraft {
        let p = PowerPercent::new(power).unwrap();
        OperationDraft {
            name: "Snijden".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(speed).unwrap(),
                power: PowerRange::new(p, p).unwrap(),
                passes: Passes::new(1).unwrap(),
                air_assist: true,
            },
            output_enabled: true,
        }
    }

    /// Gate B: a JobPlan for this document revision → preview → RD export, all naming the same
    /// immutable plan. An edit afterwards marks the plan stale; the document undo never touches it.
    #[test]
    fn headless_gate_b_same_revision_preview_and_rd() {
        let mut h = HeadlessEditorHarness::new();
        h.fs().create_dir_all_for_test(FsPath::new("/work"));
        let doc = h.new_document().expect("window");
        h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (10.0, 10.0), (130.0, 90.0));
        let rect = h.document(doc).expect("view").objects[0].id;
        h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
        h.send(UiIntent::AddOperation { document: doc, draft: cut(100.0, 20.0) });
        assert!(h.view().commands[&CommandKey::Compile].enabled);

        h.invoke(doc, CommandKey::Compile);
        h.run_tasks();
        let vm = h.document(doc).expect("view");
        let plan = vm.plan.clone().expect("compiled plan");
        assert_eq!((plan.revision, plan.stale), (vm.revision, false), "plan for this revision");
        assert_eq!(plan.preview.plan, plan.id, "preview is derived from this plan");
        assert!(
            (plan.preview.cut_length_mm - 400.0).abs() < 1e-6,
            "120 × 80 perimeter: {}",
            plan.preview.cut_length_mm
        );
        assert_eq!(plan.errors, 0);

        let dirty_before = h.document(doc).expect("view").dirty;
        let ask = h.invoke(doc, CommandKey::ExportRd);
        assert!(
            ask.effects.iter().any(|e| matches!(e, HostEffect::ShowExportPanel { document, .. } if *document == doc))
        );
        let path = PathBuf::from("/work/plaat.rd");
        h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
        h.run_tasks();
        let vm = h.document(doc).expect("view");
        let export = vm.export.clone().expect("exported");
        assert_eq!((export.plan, export.plan_revision), (plan.id, plan.revision), "export names the previewed plan");
        let bytes = burnburn_io::fs::FileSystem::read(h.fs(), &path, 1 << 24).expect("file written");
        assert_eq!(export.bytes, bytes.len());
        let rd = decode_rd_file(&bytes, RUIDA.magic).expect("inspectable RD");
        assert!(rd.commands().count() > 0);
        assert_eq!(vm.dirty, dirty_before, "export does not change the document");
        assert_eq!(vm.revision, plan.revision, "export is not a document change");

        // An edit after compiling: the plan is stale but still the same immutable plan.
        h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "20".into() });
        let vm = h.document(doc).expect("view");
        let stale = vm.plan.clone().expect("plan kept");
        assert!(stale.stale && stale.id == plan.id);
        h.invoke(doc, CommandKey::Undo);
        let vm = h.document(doc).expect("view");
        assert_eq!(vm.plan.as_ref().map(|p| p.id), Some(plan.id), "undo never replays or replaces the plan");

        // A machine-profile change makes the plan stale as well (M01).
        h.invoke(doc, CommandKey::Compile);
        h.run_tasks();
        assert!(!h.document(doc).expect("view").plan.expect("plan").stale);
        h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(2)), ruida: RUIDA });
        assert!(h.document(doc).expect("view").plan.expect("plan").stale, "profile_change_marks_compiled_plan_stale");
    }

    /// A compile requested before an edit is never shown as the plan of the newer revision.
    #[test]
    fn compile_result_for_an_older_revision_is_dropped() {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().expect("window");
        h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (10.0, 10.0), (50.0, 30.0));
        h.send(UiIntent::AddOperation { document: doc, draft: cut(100.0, 20.0) });
        h.invoke(doc, CommandKey::Compile);
        let pending = h.take_task_requests();
        h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "60".into() });
        for r in pending {
            h.complete_despite_cancel(r);
        }
        assert_eq!(h.document(doc).expect("view").plan, None);
    }
}

mod trace_flow {
    use super::*;
    use burnburn_document::EditLabel;
    use burnburn_image::{ImageRecipe, PixelScale, Threshold};
    use burnburn_trace::TraceRecipe;

    const RING_PNG: &[u8] = include_bytes!("../../../test-data/trace/synthetic/ring.png");

    fn recipe(mm_per_px: Option<f64>) -> Box<TraceRecipe> {
        Box::new(TraceRecipe {
            image: ImageRecipe {
                scale: mm_per_px.map(|s| PixelScale::uniform_mm_per_pixel(s).unwrap()),
                threshold: Threshold::Otsu,
                ..ImageRecipe::default()
            },
            ..TraceRecipe::default()
        })
    }

    fn with_ring() -> (HeadlessEditorHarness, DocumentId) {
        let mut h = HeadlessEditorHarness::new();
        h.fs().create_dir_all_for_test(FsPath::new("/work"));
        burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/ring.png"), RING_PNG).unwrap();
        let doc = h.new_document().expect("window");
        h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/ring.png") });
        h.run_tasks();
        (h, doc)
    }

    /// Gate B (T01, T02, B9): a ring traced at its declared physical scale keeps its hole; applying
    /// is one undo entry; the source image travels with the project.
    #[test]
    fn headless_gate_b_ring_trace_keeps_hole() {
        let (mut h, doc) = with_ring();
        let t = h.document(doc).expect("view").tracer.expect("tracer open");
        assert_eq!((t.width_px, t.height_px), (64, 64));
        assert!(t.result.is_none() && !h.view().commands[&CommandKey::ApplyTrace].enabled);

        // Without a declared scale nothing is traced: the user must give one (T01).
        h.send(UiIntent::SetTraceRecipe { document: doc, recipe: recipe(None) });
        h.run_tasks();
        let t = h.document(doc).expect("view").tracer.expect("tracer");
        assert!(t.needs_scale && t.result.is_none());

        h.send(UiIntent::SetTraceRecipe { document: doc, recipe: recipe(Some(0.1)) });
        assert!(h.document(doc).expect("view").tracer.expect("tracer").computing, "computing… without blocking");
        h.run_tasks();
        let t = h.document(doc).expect("view").tracer.expect("tracer");
        let r = t.result.clone().expect("traced");
        assert_eq!((r.components, r.holes), (1, 1));
        assert!((r.width_mm - 4.8).abs() < 0.1, "outer Ø 4.8 mm at 0.1 mm/px: {}", r.width_mm);
        assert_eq!(h.document(doc).expect("view").revision, burnburn_types::RevisionId::INITIAL, "preview only");

        h.invoke(doc, CommandKey::ApplyTrace);
        let vm = h.document(doc).expect("view");
        assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)), "one undo entry");
        assert_eq!(vm.objects.len(), 1, "one ring object");
        let scene = h.scene(doc).expect("scene");
        let ring = &scene.items[0].path;
        assert_eq!(ring.subpaths().len(), 2, "outer contour and hole");
        assert!(ring.subpaths().iter().all(|sp| sp.is_closed()));
        let b = ring.bounds().expect("bounds");
        assert!((b.width() - 4.8).abs() < 0.1 && (b.height() - 4.8).abs() < 0.1, "{b:?}");

        h.invoke(doc, CommandKey::Undo);
        assert!(h.document(doc).expect("view").objects.is_empty());
    }

    /// A trace result for an older recipe is never shown after a newer request (check_fresh).
    #[test]
    fn trace_result_for_an_older_recipe_is_dropped() {
        let (mut h, doc) = with_ring();
        h.send(UiIntent::SetTraceRecipe { document: doc, recipe: recipe(Some(0.1)) });
        let older = h.take_task_requests();
        h.send(UiIntent::SetTraceRecipe { document: doc, recipe: recipe(Some(0.2)) });
        let newer = h.take_task_requests();
        for r in older {
            h.complete_despite_cancel(r);
        }
        assert!(h.document(doc).expect("view").tracer.expect("tracer").result.is_none(), "superseded");
        for r in newer {
            h.complete(r);
        }
        let r = h.document(doc).expect("view").tracer.expect("tracer").result.expect("newest");
        assert!((r.width_mm - 9.6).abs() < 0.2, "0.2 mm/px doubles the size: {}", r.width_mm);
    }

    /// Closing the tracer leaves the document as it was.
    #[test]
    fn closing_the_tracer_changes_nothing() {
        let (mut h, doc) = with_ring();
        let before = h.document(doc).expect("view");
        h.send(UiIntent::SetTraceRecipe { document: doc, recipe: recipe(Some(0.1)) });
        h.invoke(doc, CommandKey::CloseTracer);
        let pending = h.take_task_requests();
        assert!(pending.iter().all(|r| r.cancel.is_cancelled()), "closing cancels the trace");
        let after = h.document(doc).expect("view");
        assert_eq!((after.revision, after.objects.len(), after.tracer.is_none()), (before.revision, 0, true));
    }
}

mod logo_flow {
    use super::*;
    use burnburn_editor::IssueKind;
    use burnburn_geometry::Segment;
    use burnburn_io::fs::FileSystem;
    use burnburn_presentation::SelectMode;

    /// A free logo outline with a 0,05 mm seam at the start, in a 60 × 40 mm SVG.
    const LOGO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="60mm" height="40mm" viewBox="0 0 60 40">
<path id="logo" d="M 0.05 20 C 10 0 20 0 30 20 C 40 40 50 40 60 20 L 60 40 L 0 40 L 0 20"/></svg>"#;

    /// Gate B logo: import with physical size → node correction (one entry, other spans
    /// bit-identical) → geometry check names the seam → repair with own tolerance → clean.
    #[test]
    fn headless_gate_b_logo_node_repair() {
        let mut h = HeadlessEditorHarness::new();
        h.fs().create_dir_all_for_test(FsPath::new("/in"));
        let temp = h.fs().create_temp_in(FsPath::new("/in"), ".logo").unwrap();
        h.fs().write_all(&temp, LOGO.as_bytes()).unwrap();
        h.fs().replace(&temp, FsPath::new("/in/logo.svg")).unwrap();
        let doc = h.new_document().expect("window");

        h.send(UiIntent::Import { document: doc, path: PathBuf::from("/in/logo.svg") });
        h.run_tasks();
        let import = h.document(doc).unwrap().import.expect("preview");
        let (w, ht) = import.size_mm.expect("physical size");
        assert!((w - 60.0).abs() < 0.06 && ht > 20.0 && ht <= 40.0 + 1e-9, "mm from the SVG size: {w} × {ht}");
        h.invoke(doc, CommandKey::CommitImport);
        let vm = h.document(doc).unwrap();
        let logo = vm.objects[0].id;
        let seam = vm.issues.iter().position(|i| i.kind == IssueKind::OpenContour && i.objects == vec![logo]);
        let seam = seam.expect("geometry check names the seam");
        assert!((vm.issues[seam].size_mm.unwrap() - 0.05).abs() < 1e-9);

        // Node correction on the second span's first handle.
        let before = h.scene(doc).unwrap().items[0].path.subpaths()[0].clone();
        let Segment::Cubic { c1, .. } = before.segments()[1] else { panic!("second span is a curve") };
        h.send(UiIntent::Select { document: doc, ids: vec![logo], mode: SelectMode::Replace });
        h.invoke(doc, CommandKey::ToolNode);
        h.drag(doc, (c1.x, c1.y), (c1.x, c1.y - 5.0));
        assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Edit(burnburn_document::EditLabel::EditNodes)));
        let after = h.scene(doc).unwrap().items[0].path.subpaths()[0].clone();
        assert_eq!(after.segments()[0], before.segments()[0], "untouched span bit-identical");
        assert_ne!(after.segments()[1], before.segments()[1]);

        // Repair the seam with 0,1 mm: one entry, no open contour left; undo brings it back.
        h.invoke(doc, CommandKey::ToolSelect);
        let seam =
            h.document(doc).unwrap().issues.iter().position(|i| i.objects == vec![logo]).expect("seam still there");
        h.send(UiIntent::RepairIssue { document: doc, issue: seam, tolerance: "0,1".into() });
        let vm = h.document(doc).unwrap();
        assert_eq!(vm.undo, Some(UndoLabel::Edit(burnburn_document::EditLabel::CloseGaps)));
        assert!(vm.issues.iter().all(|i| i.kind != IssueKind::OpenContour), "{:?}", vm.issues);
        h.invoke(doc, CommandKey::Undo);
        assert!(h.document(doc).unwrap().issues.iter().any(|i| i.kind == IssueKind::OpenContour));
    }
}
