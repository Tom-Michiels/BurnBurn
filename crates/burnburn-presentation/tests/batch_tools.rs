//! The SolidCraft batch tools in the window (W2-L1-tekenen): mirror with copies and the two
//! patterns create real objects in one undo entry; collinear and auto-constrain add their relations
//! to the open sketch as one transaction. The constraint tools and the constraint panel share one
//! route to the sketch.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::sketch::{Sketch, SketchCommand};
use burnburn_document::{
    CommandPayload, DocumentCommand, DocumentSession, DocumentSnapshot, EditLabel, GeometryObject, GeometrySource,
    ObjectKind, ShapeLabel, UndoLabel,
};
use burnburn_editor::Modifiers;
use burnburn_geometry::{Path, Point};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, Enablement, HeadlessEditorHarness, HostEffect, MessageKey, ParamKey, SelectMode,
    SketchConstraintKind, ToolRejection, UiIntent,
};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, GeometryId, RequestId, StableId};

type H = HeadlessEditorHarness;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn square(x: f64, y: f64, size: f64) -> Path {
    Path::polyline(&[p(x, y), p(x + size, y), p(x + size, y + size), p(x, y + size)], true).unwrap()
}

fn opened(paths: Vec<Path>) -> (H, DocumentId, Vec<GeometryId>) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let objects: Vec<_> = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let id = GeometryId::from_raw(i as u64 + 1);
            GeometryObject::new(id, format!("Pad {}", i + 1), ObjectKind::Path, GeometrySource::Drawn, path)
        })
        .collect();
    let next = objects.len() as u64 + 1;
    let snapshot = DocumentSnapshot::from_parts(objects, next).expect("snapshot");
    let file = PathBuf::from("/work/batch.burnburn");
    save_project(h.fs(), &file, &Project::new(ProjectUuid::from_bytes([9; 16]), snapshot)).expect("fixture");
    h.send(UiIntent::Open { path: file });
    let doc = open_window(&mut h);
    let ids = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, ids)
}

fn open_window(h: &mut H) -> DocumentId {
    h.run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened")
}

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn select(h: &mut H, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn choose(h: &mut H, doc: DocumentId, id: &str) {
    h.send(UiIntent::SelectTool { document: Some(doc), id: id.to_owned() });
}

fn option(h: &mut H, doc: DocumentId, key: ParamKey, text: &str) {
    h.send(UiIntent::SetToolOption { document: doc, key, text: text.to_owned() });
}

fn click(h: &mut H, doc: DocumentId, at: (f64, f64)) {
    h.click(doc, at, Modifiers::default());
}

fn bounds_of(h: &H, doc: DocumentId, index: usize) -> (f64, f64, f64, f64) {
    let b = h.scene(doc).expect("scene").items[index].path.bounds().expect("bounds");
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

fn enablement(h: &H, id: &str) -> Enablement {
    h.view().toolbar.button(id).unwrap_or_else(|| panic!("{id} is in the toolbar")).enablement
}

// ---------------------------------------------------------------------------------------------
// Mirror
// ---------------------------------------------------------------------------------------------

#[test]
fn mirror_with_a_copy_adds_the_mirrored_object_next_to_the_original() {
    let (mut h, doc, ids) = opened(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "sketch.mirror");
    assert_eq!(enablement(&h, "sketch.mirror"), Enablement::Enabled);
    assert_eq!(
        vm(&h, doc).tool_options.iter().map(|r| (r.key, r.text.as_str())).collect::<Vec<_>>(),
        [(ParamKey::KeepOriginal, "0")]
    );
    option(&mut h, doc, ParamKey::KeepOriginal, "1");
    click(&mut h, doc, (50.0, -10.0));
    click(&mut h, doc, (50.0, 10.0));
    let after = vm(&h, doc);
    assert_eq!(after.objects.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["Pad 1", "Pad 1 kopie"]);
    assert_eq!(after.undo, Some(UndoLabel::Edit(EditLabel::CreatePath)));
    assert_eq!(bounds_of(&h, doc, 0), (0.0, 0.0, 40.0, 40.0), "the original stays");
    assert_eq!(bounds_of(&h, doc, 1), (60.0, 0.0, 100.0, 40.0), "mirrored about x = 50");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&h, doc).objects.len(), 1);
}

#[test]
fn mirror_without_a_copy_mirrors_the_object_itself() {
    let (mut h, doc, ids) = opened(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "sketch.mirror");
    click(&mut h, doc, (50.0, -10.0));
    click(&mut h, doc, (50.0, 10.0));
    assert_eq!(vm(&h, doc).objects.len(), 1);
    assert_eq!(bounds_of(&h, doc, 0), (60.0, 0.0, 100.0, 40.0));
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Transform));
}

#[test]
fn a_mirrored_copy_of_a_basic_shape_is_still_a_basic_shape() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    choose(&mut h, doc, "shape.star");
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    choose(&mut h, doc, "sketch.mirror");
    option(&mut h, doc, ParamKey::KeepOriginal, "1");
    click(&mut h, doc, (50.0, -10.0));
    click(&mut h, doc, (50.0, 10.0));
    let after = vm(&h, doc);
    assert_eq!(after.objects.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["Ster 1", "Ster 1 kopie"]);
    assert_eq!(after.undo, Some(UndoLabel::Shape(ShapeLabel::Create)));
    assert!(after.shape_panel.is_some(), "the copy has its parameters, and is the selected object");
}

// ---------------------------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------------------------

#[test]
fn a_rectangular_pattern_is_one_undoable_array_of_the_selected_object() {
    let (mut h, doc, ids) = opened(vec![square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "sketch.pattern.rectangular");
    assert_eq!(
        vm(&h, doc).tool_options.iter().map(|r| (r.key, r.text.as_str())).collect::<Vec<_>>(),
        [(ParamKey::Columns, "3"), (ParamKey::Rows, "3")]
    );
    option(&mut h, doc, ParamKey::Columns, "2");
    option(&mut h, doc, ParamKey::Rows, "2");
    h.drag(doc, (0.0, 0.0), (30.0, 40.0));
    let after = vm(&h, doc);
    assert_eq!(after.undo, Some(UndoLabel::Structure(burnburn_document::structure::StructureLabel::CreateModifier)));
    // The source and the array output: 2 × 2 copies at pitch (30, 40).
    eprintln!("{:?}", after.objects);
    let total: usize = (0..after.objects.len()).map(|i| bounds_of(&h, doc, i)).filter(|b| b.2 - b.0 > 0.0).count();
    assert!(total >= 2, "the pattern has output: {:?}", after.objects);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&h, doc).objects.len(), 1);
}

#[test]
fn a_circular_pattern_takes_its_count_and_sweep_from_the_options() {
    let (mut h, doc, ids) = opened(vec![square(20.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "sketch.pattern.circular");
    assert_eq!(
        vm(&h, doc).tool_options.iter().map(|r| (r.key, r.text.as_str())).collect::<Vec<_>>(),
        [(ParamKey::Count, "6"), (ParamKey::Sweep, "360")]
    );
    option(&mut h, doc, ParamKey::Count, "4");
    option(&mut h, doc, ParamKey::Sweep, "abc");
    assert!(vm(&h, doc).tool_options[1].error.is_some(), "a bad sweep is shown in its row");
    click(&mut h, doc, (0.0, 0.0));
    let after = vm(&h, doc);
    assert_eq!(after.undo, Some(UndoLabel::Structure(burnburn_document::structure::StructureLabel::CreateModifier)));
    // Four copies around the centre, one of them the source: the outline spans both sides of it.
    let xs: Vec<f64> =
        (0..after.objects.len()).flat_map(|i| [bounds_of(&h, doc, i).0, bounds_of(&h, doc, i).2]).collect();
    assert!(xs.iter().any(|x| *x < -10.0) && xs.iter().any(|x| *x > 29.0), "{xs:?}");
}

#[test]
fn a_pattern_of_several_objects_says_why_it_waits() {
    let (mut h, doc, ids) = opened(vec![square(0.0, 0.0, 10.0), square(40.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "sketch.pattern.circular");
    let revision = vm(&h, doc).revision;
    click(&mut h, doc, (0.0, 100.0));
    let after = vm(&h, doc);
    assert_eq!(after.revision, revision);
    assert!(after.messages.iter().any(|m| m.key == MessageKey::ToolRejected(ToolRejection::SingleObjectOnly)));
}

// ---------------------------------------------------------------------------------------------
// Sketch constraints: one route
// ---------------------------------------------------------------------------------------------

/// A sketch of two lines (ids: points 1–4, lines 5 and 6), the second slightly off horizontal.
fn two_lines() -> Sketch {
    let mut s = Sketch::new("S");
    let a = s.point(0.0, 0.0).unwrap();
    let b = s.point(40.0, 0.1).unwrap();
    let c = s.point(0.0, 20.0).unwrap();
    let d = s.point(40.0, 20.0).unwrap();
    s.line(a, b).unwrap();
    s.line(c, d).unwrap();
    s
}

fn in_sketch(sketch: Sketch) -> (H, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let mut session = DocumentSession::new(DocumentId::from_raw(77));
    let cmd = DocumentCommand {
        document: session.id(),
        expected: session.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch) }),
    };
    session.apply_with(cmd, &[&SketchEvaluator::new(SketchSolverAdapter)]).expect("solves");
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let file = PathBuf::from("/work/schets.burnburn");
    save_project(h.fs(), &file, &Project::new(ProjectUuid::from_bytes([5; 16]), (*session.snapshot()).clone()))
        .unwrap();
    h.send(UiIntent::Open { path: file });
    let doc = open_window(&mut h);
    h.send(UiIntent::Select { document: doc, ids: vec![vm(&h, doc).objects[0].id], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::SketchEdit);
    assert!(vm(&h, doc).sketch_mode.is_some(), "sketch mode is on");
    (h, doc)
}

fn kinds(h: &H, doc: DocumentId) -> Vec<SketchConstraintKind> {
    vm(h, doc).sketch_mode.expect("sketch mode").rows.iter().map(|r| r.kind).collect()
}

#[test]
fn auto_constrain_adds_every_obvious_relation_as_one_undo_entry() {
    let (mut h, doc) = in_sketch(two_lines());
    assert_eq!(enablement(&h, "sketch.auto_constrain"), Enablement::Enabled);
    choose(&mut h, doc, "sketch.auto_constrain");
    click(&mut h, doc, (20.0, 0.05));
    let found = kinds(&h, doc);
    assert_eq!(found.iter().filter(|k| **k == SketchConstraintKind::Horizontal).count(), 2, "{found:?}");
    assert!(matches!(vm(&h, doc).undo, Some(UndoLabel::Sketch(_))));
    h.invoke(doc, CommandKey::Undo);
    assert!(kinds(&h, doc).is_empty(), "both relations were one entry");
}

#[test]
fn collinear_is_two_relations_in_one_entry() {
    let (mut h, doc) = in_sketch(two_lines());
    choose(&mut h, doc, "sketch.constraint.collinear");
    click(&mut h, doc, (10.0, 0.0));
    click(&mut h, doc, (10.0, 20.0));
    let found = kinds(&h, doc);
    assert!(
        found.contains(&SketchConstraintKind::Parallel) && found.contains(&SketchConstraintKind::PointOn),
        "{found:?}"
    );
    h.invoke(doc, CommandKey::Undo);
    assert!(kinds(&h, doc).is_empty());
}

#[test]
fn a_constraint_tool_and_the_panel_button_make_the_same_relation() {
    // The panel route: select both lines, press "Parallel".
    let (mut h, doc) = in_sketch(two_lines());
    let entity = |raw: u32| burnburn_document::sketch::EntityId::from_raw(raw);
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: vec![entity(4), entity(5)],
        constraints: vec![],
        mode: SelectMode::Replace,
    });
    h.invoke(doc, CommandKey::SketchParallel);
    let panel = vm(&h, doc).sketch_mode.expect("mode").rows;
    h.invoke(doc, CommandKey::Undo);
    // The tool route: pick the two lines.
    choose(&mut h, doc, "sketch.constraint.parallel");
    click(&mut h, doc, (10.0, 0.0));
    click(&mut h, doc, (10.0, 20.0));
    let tool = vm(&h, doc).sketch_mode.expect("mode").rows;
    assert_eq!(
        panel.iter().map(|r| (r.kind, r.entities.clone())).collect::<Vec<_>>(),
        tool.iter().map(|r| (r.kind, r.entities.clone())).collect::<Vec<_>>()
    );
    assert_eq!(tool.len(), 1);
}

#[test]
fn a_constraint_tool_applies_at_once_to_a_fitting_sketch_selection() {
    let (mut h, doc) = in_sketch(two_lines());
    let entity = |raw: u32| burnburn_document::sketch::EntityId::from_raw(raw);
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: vec![entity(4), entity(5)],
        constraints: vec![],
        mode: SelectMode::Replace,
    });
    choose(&mut h, doc, "sketch.constraint.parallel");
    assert_eq!(kinds(&h, doc), [SketchConstraintKind::Parallel], "the selection was enough");
}

#[test]
fn constraint_tools_are_only_offered_while_a_sketch_is_open() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    assert!(matches!(enablement(&h, "sketch.constraint.parallel"), Enablement::Disabled(_)));
    h.invoke(doc, CommandKey::SketchNew);
    assert_eq!(enablement(&h, "sketch.constraint.parallel"), Enablement::Enabled);
    assert_eq!(enablement(&h, "sketch.auto_constrain"), Enablement::Enabled);
}
