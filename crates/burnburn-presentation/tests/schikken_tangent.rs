//! W3-UI-schikken: Tangent Circle with every solution shown. Targets are picked on the canvas,
//! all circles tangent to them are drawn, the pointer chooses one and a click creates it.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::{EditLabel, UndoLabel};
use burnburn_editor::{Modifiers, Overlay, ToolKind};
use burnburn_geometry::{Path, Point};
use burnburn_presentation::{TangentProblem, UiIntent};
use burnburn_types::DocumentId;
use edit_rig::{opened_with, view};

type Harness = burnburn_presentation::HeadlessEditorHarness;

fn line(a: (f64, f64), b: (f64, f64)) -> Path {
    Path::line(Point::new(a.0, a.1), Point::new(b.0, b.1)).unwrap()
}

fn start(h: &mut Harness, doc: DocumentId) {
    h.send(UiIntent::SelectTool { document: Some(doc), id: "shape.tangent_circle".to_owned() });
    assert_eq!(view(h, doc).tool, ToolKind::TangentCircle);
}

fn click(h: &mut Harness, doc: DocumentId, at: (f64, f64)) {
    h.click(doc, at, Modifiers::default());
}

fn overlays(h: &Harness, doc: DocumentId) -> (usize, usize, usize) {
    let scene = h.scene(doc).expect("scene");
    let count = |f: &dyn Fn(&Overlay) -> bool| scene.overlays.iter().filter(|o| f(o)).count();
    (
        count(&|o| matches!(o, Overlay::TangentCandidate { chosen: false, .. })),
        count(&|o| matches!(o, Overlay::TangentCandidate { chosen: true, .. })),
        count(&|o| matches!(o, Overlay::TangentContact { .. })),
    )
}

fn triangle() -> (Harness, DocumentId, Vec<burnburn_types::GeometryId>) {
    opened_with(vec![line((0.0, 0.0), (40.0, 0.0)), line((40.0, 0.0), (0.0, 30.0)), line((0.0, 30.0), (0.0, 0.0))])
}

#[test]
fn every_solution_is_shown_the_pointer_chooses_and_a_click_creates_one_circle() {
    let (mut h, doc, ids) = triangle();
    start(&mut h, doc);
    for at in [(20.0, 0.3), (20.0, 15.0), (0.3, 15.0)] {
        click(&mut h, doc, at);
    }
    h.hover(doc, (11.0, 9.0));
    let t = view(&h, doc).schikken.tangent.expect("tangent state");
    assert_eq!((t.picked, t.candidates, t.chosen, t.problem), (3, 4, true, None));
    assert_eq!(overlays(&h, doc), (3, 1, 3), "three other solutions, the chosen one and its three contacts");
    assert_eq!(view(&h, doc).undo, None, "nothing is created before the click");
    click(&mut h, doc, (11.0, 9.0));
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), ids.len() + 1);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::CreateShape)));
    assert!(vm.schikken.tangent.is_none() || vm.schikken.tangent.unwrap().picked == 0, "ready for the next circle");
    h.invoke(doc, burnburn_presentation::CommandKey::Undo);
    assert_eq!(view(&h, doc).objects.len(), ids.len());
}

#[test]
fn two_targets_take_the_typed_diameter_and_then_show_their_solutions() {
    let (mut h, doc, _) = opened_with(vec![line((0.0, 0.0), (100.0, 0.0)), line((0.0, 0.0), (0.0, 100.0))]);
    start(&mut h, doc);
    click(&mut h, doc, (50.0, 0.3));
    click(&mut h, doc, (0.3, 50.0));
    let t = view(&h, doc).schikken.tangent.expect("state");
    assert_eq!((t.picked, t.candidates, t.radius_mm), (2, 0, None), "two targets need a diameter first");
    h.send(UiIntent::CommitToolSize { document: doc, width: "8".to_owned(), height: String::new() });
    let t = view(&h, doc).schikken.tangent.expect("state");
    // The lines are infinite: one circle of that size in each of the four quadrants.
    assert_eq!((t.candidates, t.radius_mm), (4, Some(4.0)));
    h.hover(doc, (6.0, 6.0));
    click(&mut h, doc, (6.0, 6.0));
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::CreateShape)));
}

#[test]
fn an_undetermined_problem_is_named_and_creates_nothing() {
    let (mut h, doc, ids) = opened_with(vec![line((0.0, 0.0), (100.0, 0.0)), line((0.0, 10.0), (100.0, 10.0))]);
    start(&mut h, doc);
    click(&mut h, doc, (20.0, 0.2));
    click(&mut h, doc, (20.0, 10.2));
    h.send(UiIntent::CommitToolSize { document: doc, width: "10".to_owned(), height: String::new() });
    let t = view(&h, doc).schikken.tangent.expect("state");
    assert_eq!((t.candidates, t.problem), (0, Some(TangentProblem::Degenerate)));
    click(&mut h, doc, (40.0, 5.0));
    assert_eq!(view(&h, doc).objects.len(), ids.len());
}

#[test]
fn escape_leaves_the_tool_with_nothing_created() {
    let (mut h, doc, ids) = triangle();
    start(&mut h, doc);
    click(&mut h, doc, (20.0, 0.3));
    h.escape(doc);
    let t = view(&h, doc).schikken.tangent;
    assert!(t.is_none(), "{t:?}");
    assert_eq!(view(&h, doc).objects.len(), ids.len());
}
