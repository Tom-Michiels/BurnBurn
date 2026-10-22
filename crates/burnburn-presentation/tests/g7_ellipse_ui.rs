//! G7-editor S01 in sketch mode: the minor axis of an ellipse is a dimension (from the panel and
//! from the dimension tool with its option), a line can be made tangent to an ellipse and a point
//! can be put on it, each in one history entry, with the outline following the solver.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::sketch::EntityId;
use burnburn_editor::Modifiers;
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, ParamKey, SelectMode, SketchConstraintKind,
    SketchDimensionKind, SketchStatus, UiIntent,
};
use burnburn_types::DocumentId;

type H = HeadlessEditorHarness;

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

/// Sketch mode with an ellipse around (10, 10): major end (14, 10), minor point (10, 12).
fn ellipse_sketch() -> (H, DocumentId, EntityId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::SketchNew);
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.ellipse".to_owned() });
    for at in [(10.0, 10.0), (14.0, 10.0), (10.0, 12.0)] {
        h.click(doc, at, Modifiers::default());
    }
    assert_eq!(vm(&h, doc).sketch_mode.as_ref().unwrap().refused, None);
    (h, doc, EntityId::from_raw(2))
}

fn select(h: &mut H, doc: DocumentId, entities: Vec<EntityId>) {
    h.send(UiIntent::SketchSelect { document: doc, entities, constraints: vec![], mode: SelectMode::Replace });
}

fn kinds(h: &H, doc: DocumentId) -> Vec<SketchConstraintKind> {
    vm(h, doc).sketch_mode.expect("sketch mode").rows.iter().map(|r| r.kind).collect()
}

fn bounds(h: &H, doc: DocumentId, index: usize) -> (f64, f64, f64, f64) {
    let b = h.scene(doc).unwrap().items[index].path.bounds().unwrap();
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

#[test]
fn the_minor_axis_is_added_from_the_panel_and_drives_the_outline() {
    let (mut h, doc, ellipse) = ellipse_sketch();
    select(&mut h, doc, vec![ellipse]);
    h.send(UiIntent::AddSketchDimension {
        document: doc,
        kind: SketchDimensionKind::MinorDiameter,
        text: "3 mm".into(),
    });
    assert!(kinds(&h, doc).contains(&SketchConstraintKind::MinorDiameter), "{:?}", kinds(&h, doc));
    assert_eq!(vm(&h, doc).sketch_mode.as_ref().unwrap().refused, None);
    let (_, y0, _, y1) = bounds(&h, doc, 0);
    assert!(near(y1 - y0, 3.0), "the minor axis follows the dimension: {}", y1 - y0);

    // The row's text is editable like any dimension; one history entry each.
    let row = vm(&h, doc)
        .sketch_mode
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.kind == SketchConstraintKind::MinorDiameter)
        .unwrap();
    let sketch = vm(&h, doc).sketch_mode.unwrap().sketch;
    h.send(UiIntent::SetSketchDimension { document: doc, sketch, constraint: row.id, text: "5 mm".into() });
    let (_, y0, _, y1) = bounds(&h, doc, 0);
    assert!(near(y1 - y0, 5.0), "{}", y1 - y0);
    h.invoke(doc, CommandKey::Undo);
    let (_, y0, _, y1) = bounds(&h, doc, 0);
    assert!(near(y1 - y0, 3.0));
    h.invoke(doc, CommandKey::Undo);
    assert!(!kinds(&h, doc).contains(&SketchConstraintKind::MinorDiameter));
    let (_, y0, _, y1) = bounds(&h, doc, 0);
    assert!(near(y1 - y0, 4.0), "the drawn minor axis again: {}", y1 - y0);
}

#[test]
fn a_minor_axis_that_is_not_possible_is_refused_in_the_panel_without_a_change() {
    let (mut h, doc, ellipse) = ellipse_sketch();
    select(&mut h, doc, vec![ellipse]);
    let before = vm(&h, doc).revision;
    h.send(UiIntent::AddSketchDimension {
        document: doc,
        kind: SketchDimensionKind::MinorDiameter,
        text: "0 mm".into(),
    });
    let sketch = vm(&h, doc).sketch_mode.unwrap();
    // A zero axis is a dimension error: the sketch says so and keeps the last valid outline.
    assert_eq!(sketch.status, SketchStatus::Failed, "the zero axis is a dimension error");
    assert!(sketch.problem.is_some(), "and the panel says why");
    assert_ne!(vm(&h, doc).revision, before);
    // A selection that is no ellipse offers no minor axis.
    h.invoke(doc, CommandKey::Undo);
    select(&mut h, doc, vec![EntityId::from_raw(0)]);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::AddSketchDimension {
        document: doc,
        kind: SketchDimensionKind::MinorDiameter,
        text: "3 mm".into(),
    });
    assert_eq!(vm(&h, doc).revision, revision);
}

#[test]
fn the_dimension_tool_gives_the_minor_axis_when_its_option_is_on() {
    let (mut h, doc, _) = ellipse_sketch();
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.dimension".to_owned() });
    let row = vm(&h, doc).tool_options.iter().find(|r| r.key == ParamKey::MinorAxis).cloned();
    assert_eq!(row.map(|r| (r.text, r.toggle)), Some(("0".to_owned(), true)), "the option is offered, off");

    // On the outline of the ellipse, off the two points.
    h.click(doc, (12.0, 11.7321), Modifiers::default());
    assert!(
        kinds(&h, doc).contains(&SketchConstraintKind::Diameter),
        "the major axis by default: {:?}",
        kinds(&h, doc)
    );
    h.invoke(doc, CommandKey::Undo);

    h.send(UiIntent::SetToolOption { document: doc, key: ParamKey::MinorAxis, text: "1".into() });
    h.click(doc, (12.0, 11.7321), Modifiers::default());
    assert!(kinds(&h, doc).contains(&SketchConstraintKind::MinorDiameter), "{:?}", kinds(&h, doc));
    assert!(!kinds(&h, doc).contains(&SketchConstraintKind::Diameter));
    // The option is kept when the tool is chosen again.
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.line".to_owned() });
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.dimension".to_owned() });
    let row = vm(&h, doc).tool_options.iter().find(|r| r.key == ParamKey::MinorAxis).cloned();
    assert_eq!(row.map(|r| r.text), Some("1".to_owned()));
}

#[test]
fn a_line_made_tangent_to_the_ellipse_touches_its_top() {
    let (mut h, doc, ellipse) = ellipse_sketch();
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.line".to_owned() });
    h.drag(doc, (2.0, 16.0), (18.0, 16.0)); // a level line: the tool suggests its Horizontal
    h.send(UiIntent::SelectTool { document: Some(doc), id: "select".to_owned() });
    let line = EntityId::from_raw(5); // after the centre, the major end, the ellipse and the line's two points
    select(&mut h, doc, vec![line, ellipse]);
    h.invoke(doc, CommandKey::SketchTangent);
    assert!(kinds(&h, doc).contains(&SketchConstraintKind::Tangent), "{:?}", kinds(&h, doc));
    assert_eq!(vm(&h, doc).sketch_mode.as_ref().unwrap().refused, None);
    // The ellipse is the first output, the line the second: they touch.
    let ellipse_box = bounds(&h, doc, 0);
    let line_box = bounds(&h, doc, 1);
    assert!(near(line_box.1, line_box.3), "level");
    assert!(
        near(ellipse_box.3, line_box.1) || near(ellipse_box.1, line_box.1),
        "the line touches the ellipse: {ellipse_box:?} {line_box:?}"
    );
    h.invoke(doc, CommandKey::Undo);
    assert!(!kinds(&h, doc).contains(&SketchConstraintKind::Tangent), "one history entry");
}

#[test]
fn an_end_point_put_on_the_ellipse_lies_on_its_outline() {
    let (mut h, doc, ellipse) = ellipse_sketch();
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.line".to_owned() });
    h.drag(doc, (16.0, 14.0), (20.0, 14.0));
    h.send(UiIntent::SelectTool { document: Some(doc), id: "select".to_owned() });
    let start = EntityId::from_raw(3);
    select(&mut h, doc, vec![start, ellipse]);
    h.invoke(doc, CommandKey::SketchCoincident);
    assert!(kinds(&h, doc).contains(&SketchConstraintKind::PointOn), "{:?}", kinds(&h, doc));
    let e = bounds(&h, doc, 0);
    let (cx, cy, a, b) = ((e.0 + e.2) / 2.0, (e.1 + e.3) / 2.0, (e.2 - e.0) / 2.0, (e.3 - e.1) / 2.0);
    let line = h.scene(doc).unwrap().items[1].path.clone();
    let p = line.subpaths()[0].start();
    let on = ((p.x - cx) / a).powi(2) + ((p.y - cy) / b).powi(2);
    assert!((on - 1.0).abs() < 1e-3, "(x/a)^2 + (y/b)^2 = {on} at {p:?}");
}
