//! G8-editor E08: text of several lines and variable text on a path, and moving a bound text. The
//! outline of a bound text is derived, so moving it changes where it starts on the guide: the
//! displacement projected on the guide's direction there, kept within the guide. Moving it
//! together with its guide moves the guide and the text follows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use burnburn_document::UndoLabel;
use burnburn_document::text::TextLabel;
use burnburn_generators::{Distort, PathTextParams, text_on_path};
use burnburn_geometry::{Path, Point, ProductionTolerance};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, FieldKey, HeadlessEditorHarness, SelectMode, TextIssue, TextPathEdit, UiIntent,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::{DocumentId, GeometryId, Length};

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, data.clone());
    src.insert("Liberation Sans", FontStyle::Bold, data);
    (Arc::new(src), regular)
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn tol() -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap()
}

struct World {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    circle: GeometryId,
}

/// A text and a circle of radius 40 centred on (100, 100), both selected.
fn world(content: &str) -> World {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.send(UiIntent::InsertText { document: doc, text: content.into() });
    let outlines: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::ToolCircle);
    h.drag(doc, (100.0, 100.0), (140.0, 100.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let circle = vm(&h, doc).objects.iter().map(|o| o.id).find(|id| !outlines.contains(id)).expect("the circle");
    let mut both = outlines.clone();
    both.push(circle);
    h.send(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
    World { h, doc, circle }
}

impl World {
    fn bind(&mut self) {
        self.h.invoke(self.doc, CommandKey::TextOnPath);
        self.h.send(UiIntent::TextPath { document: self.doc, edit: TextPathEdit::Apply });
    }
    fn text_outlines(&self) -> Vec<GeometryId> {
        vm(&self.h, self.doc).objects.iter().map(|o| o.id).filter(|id| *id != self.circle).collect()
    }
    fn select_text(&mut self) {
        let ids = self.text_outlines();
        self.h.send(UiIntent::Select { document: self.doc, ids, mode: SelectMode::Replace });
    }
    fn source(&mut self) -> burnburn_text::TextObject {
        self.h.controller().text_sources(self.doc).remove(0)
    }
    fn circle_path(&self) -> Path {
        let scene = self.h.scene(self.doc).unwrap();
        (*scene.items.iter().find(|i| i.source == self.circle).unwrap().path).clone()
    }
    fn bounds_of(&self, ids: &[GeometryId]) -> (Point, Point) {
        let scene = self.h.scene(self.doc).unwrap();
        let mut all: Option<burnburn_geometry::Bounds> = None;
        for item in scene.items.iter().filter(|i| ids.contains(&i.source)) {
            let b = item.path.bounds().unwrap();
            all = Some(all.map_or(b, |a| a.union(b)));
        }
        let b = all.unwrap();
        (b.min(), b.max())
    }
    fn field(&self, key: FieldKey) -> f64 {
        let doc = vm(&self.h, self.doc);
        doc.inspector.as_ref().unwrap().field(key).unwrap().text.replace(',', ".").parse().unwrap()
    }
    /// Unit direction of the circle at its start (where a text with start 0 begins).
    fn guide_direction(&self) -> (f64, f64) {
        let path = self.circle_path();
        let sp = &path.subpaths()[0];
        let d = sp.segments()[0].derivative(sp.start(), 0.0);
        (d.x / d.length(), d.y / d.length())
    }
}

#[test]
fn a_text_of_two_lines_binds_to_the_guide_and_follows_it() {
    let mut w = world("HI\nIH");
    w.bind();
    assert_eq!(vm(&w.h, w.doc).text_on_path, None, "the dialog closed: nothing was refused");
    let binding = w.source().path.expect("the text is bound");
    assert_eq!(binding.guide, w.circle);
    assert_eq!(vm(&w.h, w.doc).undo, Some(UndoLabel::Text(TextLabel::Edit)));
    // The outline is the generator's: both lines bent along the circle.
    let (src, _) = fonts();
    let params = PathTextParams { distort: Distort::Rigid, start: 0.0, offset: 0.0, reverse: false, tolerance: tol() };
    let expected = text_on_path(&w.source(), &*src, &w.circle_path(), &params).unwrap().path;
    let got = w.bounds_of(&w.text_outlines());
    let want = expected.bounds().unwrap();
    assert!(got.0.distance(want.min()) < 1e-9 && got.1.distance(want.max()) < 1e-9);
}

#[test]
fn moving_a_bound_text_along_the_guide_changes_where_it_starts() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let (tx, ty) = w.guide_direction();
    assert!(tx.abs() < 1e-6 && (ty.abs() - 1.0).abs() < 1e-6, "the circle starts heading up or down: {tx} {ty}");
    let y = w.field(FieldKey::Y);
    // Five millimetres in the direction of travel at the start.
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::Y, text: format!("{}", y + 5.0 * ty.signum()) });
    assert_eq!(vm(&w.h, w.doc).problem, None);
    let binding = w.source().path.unwrap();
    // The direction at the start is that of the first chord of the flattened circle (0.01 mm
    // tolerance): within 0.02 rad of the exact tangent, so within 0.001 mm of 5 here.
    assert!((binding.start_mm - 5.0).abs() < 2e-3, "start {}", binding.start_mm);
    assert_eq!((binding.offset_mm, binding.reverse, binding.guide), (0.0, false, w.circle));
    // The outline is the text at that start, and the move is one undo step.
    let (src, _) = fonts();
    let params = PathTextParams {
        distort: Distort::Rigid,
        start: binding.start_mm,
        offset: 0.0,
        reverse: false,
        tolerance: tol(),
    };
    let expected = text_on_path(&w.source(), &*src, &w.circle_path(), &params).unwrap().path.bounds().unwrap();
    let got = w.bounds_of(&w.text_outlines());
    assert!(got.0.distance(expected.min()) < 1e-9 && got.1.distance(expected.max()) < 1e-9);
    w.h.invoke(w.doc, CommandKey::Undo);
    assert!((w.source().path.unwrap().start_mm).abs() < 1e-9, "back at the start");
}

#[test]
fn a_move_across_the_guide_leaves_the_start_alone_and_one_before_its_beginning_stops_at_zero() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let (_, ty) = w.guide_direction();
    let revision = vm(&w.h, w.doc).revision;
    // Across the guide's direction (x at the start of the circle): the start does not change.
    let x = w.field(FieldKey::X);
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::X, text: format!("{}", x + 7.0) });
    let after = w.source().path.unwrap().start_mm;
    assert!(after.abs() < 1e-6, "no component along the guide: {after}");
    // Against the direction of travel at start 0: nothing to give, so nothing moves.
    let y = w.field(FieldKey::Y);
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::Y, text: format!("{}", y - 9.0 * ty.signum()) });
    assert!(w.source().path.unwrap().start_mm.abs() < 1e-9);
    let _ = revision;
}

#[test]
fn a_move_past_the_end_of_the_guide_stops_where_the_text_still_fits() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let (_, ty) = w.guide_direction();
    let y = w.field(FieldKey::Y);
    w.h.send(UiIntent::CommitField {
        document: w.doc,
        field: FieldKey::Y,
        text: format!("{}", y + 5000.0 * ty.signum()),
    });
    let start = w.source().path.unwrap().start_mm;
    let (src, _) = fonts();
    let at =
        |start: f64| PathTextParams { distort: Distort::Rigid, start, offset: 0.0, reverse: false, tolerance: tol() };
    assert!(text_on_path(&w.source(), &*src, &w.circle_path(), &at(start)).is_ok(), "it fits at {start}");
    assert!(text_on_path(&w.source(), &*src, &w.circle_path(), &at(start + 0.05)).is_err(), "and not a hair further");
    assert!(start > 200.0, "the circle is 251 mm round: {start}");
    assert_eq!(vm(&w.h, w.doc).problem, None, "a move that is too long is cut short, not refused");
    assert_eq!(vm(&w.h, w.doc).text_toolbar.and_then(|t| t.issue), None::<TextIssue>);
}

#[test]
fn moving_the_text_together_with_its_guide_moves_both_and_keeps_the_start() {
    let mut w = world("HIT");
    w.bind();
    let mut both = w.text_outlines();
    both.push(w.circle);
    w.h.send(UiIntent::Select { document: w.doc, ids: both, mode: SelectMode::Replace });
    let before = (w.bounds_of(&w.text_outlines()), w.bounds_of(&[w.circle]));
    let x = w.field(FieldKey::X);
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::X, text: format!("{}", x + 12.0) });
    assert_eq!(vm(&w.h, w.doc).problem, None);
    let after = (w.bounds_of(&w.text_outlines()), w.bounds_of(&[w.circle]));
    for (b, a) in [(before.0, after.0), (before.1, after.1)] {
        assert!((a.0.x - b.0.x - 12.0).abs() < 1e-6 && (a.0.y - b.0.y).abs() < 1e-6, "{b:?} -> {a:?}");
        assert!((a.1.x - b.1.x - 12.0).abs() < 1e-6);
    }
    assert!(w.source().path.unwrap().start_mm.abs() < 1e-9, "the text still starts where it did");
    w.h.invoke(w.doc, CommandKey::Undo);
    assert_eq!(w.bounds_of(&w.text_outlines()), before.0, "one undo moves both back");
    assert_eq!(w.bounds_of(&[w.circle]), before.1);
}

#[test]
fn a_text_that_is_not_bound_is_not_moved_by_this() {
    // The outlines of a free text are still derived: the move is refused as before.
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.send(UiIntent::InsertText { document: doc, text: "HIT".into() });
    let x: f64 =
        vm(&h, doc).inspector.as_ref().unwrap().field(FieldKey::X).unwrap().text.replace(',', ".").parse().unwrap();
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: format!("{}", x + 3.0) });
    let moved = vm(&h, doc).revision != revision;
    let after: f64 =
        vm(&h, doc).inspector.as_ref().unwrap().field(FieldKey::X).unwrap().text.replace(',', ".").parse().unwrap();
    assert!(moved || (after - x).abs() < 1e-9, "either it moved as a whole or nothing changed");
}

// --- G9-editor E08: across the guide, the true tangent, turning and scaling ---------------------------------------

fn expected_bounds(w: &mut World, start: f64, offset: f64) -> (Point, Point) {
    let (src, _) = fonts();
    let params = PathTextParams { distort: Distort::Rigid, start, offset, reverse: false, tolerance: tol() };
    let b = text_on_path(&w.source(), &*src, &w.circle_path(), &params).unwrap().path.bounds().unwrap();
    (b.min(), b.max())
}

#[test]
fn g9_moving_a_bound_text_across_the_guide_changes_its_distance_to_the_guide() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    // The circle starts at (140, 100) heading up: its left normal points to the centre, so a move
    // of 7 mm in +x (away from the centre) puts the baseline 7 mm to the right of the guide.
    let x = w.field(FieldKey::X);
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::X, text: format!("{}", x + 7.0) });
    assert_eq!(vm(&w.h, w.doc).problem, None);
    let binding = w.source().path.unwrap();
    // The inspector shows the position with a few decimals: the typed move is 7 mm to within 1e-3.
    assert!((binding.offset_mm + 7.0).abs() < 1e-3, "offset {}", binding.offset_mm);
    assert!(binding.start_mm.abs() < 1e-3, "start {}", binding.start_mm);
    let want = expected_bounds(&mut w, binding.start_mm, binding.offset_mm);
    let got = w.bounds_of(&w.text_outlines());
    assert!(got.0.distance(want.0) < 1e-9 && got.1.distance(want.1) < 1e-9, "the outline is the text at that offset");
    // One undo brings the offset back.
    w.h.invoke(w.doc, CommandKey::Undo);
    assert!(w.source().path.unwrap().offset_mm.abs() < 1e-9);
}

#[test]
fn g9_a_diagonal_move_splits_into_along_and_across_in_one_undo_step() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let (x, y) = (w.field(FieldKey::X), w.field(FieldKey::Y));
    let revision = vm(&w.h, w.doc).revision;
    // 3 mm to the left (towards the centre: +3 across) and 4 mm up (along the direction of travel).
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::X, text: format!("{}", x - 3.0) });
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::Y, text: format!("{}", y + 4.0) });
    let binding = w.source().path.unwrap();
    // The inspector shows the bounds of the bent text, so the second move starts from the outline
    // the first one made: the oracle is the text at the binding the window ended with.
    assert!(binding.offset_mm > 0.0, "towards the centre is positive: {}", binding.offset_mm);
    assert!(binding.start_mm > 0.0, "{}", binding.start_mm);
    let want = expected_bounds(&mut w, binding.start_mm, binding.offset_mm);
    let got = w.bounds_of(&w.text_outlines());
    assert!(got.0.distance(want.0) < 1e-9 && got.1.distance(want.1) < 1e-9);
    assert!(vm(&w.h, w.doc).revision > revision);
}

#[test]
fn g9_the_start_follows_the_exact_tangent_not_the_first_chord() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let y = w.field(FieldKey::Y);
    w.h.send(UiIntent::CommitField { document: w.doc, field: FieldKey::Y, text: format!("{}", y + 5.0) });
    // The circle starts heading straight up: five millimetres up is exactly five along the guide.
    let start = w.source().path.unwrap().start_mm;
    // (The inspector rounds the shown position: 2e-4 is its resolution; the first chord is 1.2e-3 off.)
    assert!((start - 5.0).abs() < 2e-4, "start {start}");
}

#[test]
fn g9_turning_or_scaling_a_bound_text_alone_is_refused_in_words_and_changes_nothing() {
    use burnburn_presentation::{HostEffect, MessageKey};
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let before = (vm(&w.h, w.doc).revision, w.source().path.unwrap(), w.source().size);
    for field in [FieldKey::Rotation, FieldKey::ScaleX, FieldKey::ShearX] {
        let text = if field == FieldKey::Rotation {
            "30"
        } else if field == FieldKey::ShearX {
            "10"
        } else {
            "150"
        };
        let update = w.h.send(UiIntent::CommitField { document: w.doc, field, text: text.into() });
        assert!(
            update
                .effects
                .iter()
                .any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::BoundTextFollowsGuide, .. })),
            "{field:?}: {:?}",
            update.effects
        );
    }
    let update = w.h.invoke(w.doc, CommandKey::Rotate90Cw);
    assert!(
        update
            .effects
            .iter()
            .any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::BoundTextFollowsGuide, .. }))
    );
    assert_eq!(before, (vm(&w.h, w.doc).revision, w.source().path.unwrap(), w.source().size), "nothing changed");
}

#[test]
fn g9_turning_the_text_together_with_its_guide_turns_both_and_the_text_follows() {
    let mut w = world("HIT");
    w.bind();
    let mut both = w.text_outlines();
    both.push(w.circle);
    w.h.send(UiIntent::Select { document: w.doc, ids: both, mode: SelectMode::Replace });
    let before = w.source().path.unwrap();
    w.h.invoke(w.doc, CommandKey::Rotate90Cw);
    assert_eq!(vm(&w.h, w.doc).problem, None);
    let after = w.source().path.unwrap();
    assert_eq!((after.start_mm, after.offset_mm), (before.start_mm, before.offset_mm), "the binding is unchanged");
    let want = expected_bounds(&mut w, after.start_mm, after.offset_mm);
    let got = w.bounds_of(&w.text_outlines());
    assert!(got.0.distance(want.0) < 1e-9 && got.1.distance(want.1) < 1e-9, "the text follows the turned guide");
}

// --- Variable text on a path ----------------------------------------------------------------------

mod variable {
    use super::*;
    use burnburn_generators::JobClock;
    use burnburn_presentation::{SerialField, VariableEdit};
    use burnburn_types::ContentHash;
    use common::*;

    fn clock() -> JobClock {
        JobClock::new(1_781_751_845, -300).unwrap()
    }

    /// The rig's rectangle job plus a text with a cut, bound to a circle that is not cut.
    fn bound_rig(content: &str, variable: bool) -> Rig {
        let mut r = rig();
        let (src, font) = fonts();
        r.h.controller().attach_fonts(src, font);
        r.h.controller().attach_clock(Arc::new(|| Some(clock())));
        let doc = r.doc;
        r.h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
        r.h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.text".into() });
        r.h.click(doc, (20.0, 50.0), Default::default());
        r.h.send(UiIntent::InsertText { document: doc, text: content.into() });
        r.h.invoke(doc, CommandKey::ToolSelect);
        if variable {
            r.h.send(UiIntent::Variable { document: doc, edit: VariableEdit::SetVariable(true) });
            for (field, text) in [
                (SerialField::Start, "10"),
                (SerialField::End, "95"),
                (SerialField::AdvanceBy, "5"),
                (SerialField::Digits, "4"),
            ] {
                r.h.send(UiIntent::Variable { document: doc, edit: VariableEdit::Serial(field, text.into()) });
            }
        }
        r.h.send(UiIntent::AddOperation { document: doc, draft: cut() });
        // The guide: a circle of radius 40 well inside the bed; bind the text to it.
        let text_ids: Vec<_> = vm(&r.h, doc).selection.clone();
        r.h.invoke(doc, CommandKey::ToolCircle);
        r.h.drag(doc, (300.0, 300.0), (340.0, 300.0));
        r.h.invoke(doc, CommandKey::ToolSelect);
        let known: Vec<_> = vm(&r.h, doc).objects.iter().map(|o| o.id).collect();
        let circle = known
            .iter()
            .copied()
            .find(|id| !text_ids.contains(id) && vm(&r.h, doc).objects.last().map(|o| o.id) == Some(*id))
            .unwrap();
        let mut both = text_ids;
        both.push(circle);
        r.h.send(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
        r.h.invoke(doc, CommandKey::TextOnPath);
        r.h.send(UiIntent::TextPath { document: doc, edit: TextPathEdit::Apply });
        r
    }

    fn compile_hash(r: &mut Rig) -> ContentHash {
        r.h.invoke(r.doc, CommandKey::Compile);
        r.h.run_tasks();
        r.h.invoke(r.doc, CommandKey::PrepareJob);
        r.h.run_tasks();
        r.h.document(r.doc).unwrap().job.expect("prepared job").rd_hash
    }

    #[test]
    fn a_variable_text_on_a_path_cuts_what_its_record_says_bent_along_the_guide() {
        let mut variable = bound_rig("SN-{serial}", true);
        let mut literal = bound_rig("SN-0010", false);
        assert!(variable.h.controller().text_sources(variable.doc)[0].path.is_some(), "bound");
        assert!(variable.h.controller().text_sources(variable.doc)[0].variable.is_some(), "and variable");
        let (a, b) = (compile_hash(&mut variable), compile_hash(&mut literal));
        assert_eq!(a, b, "the job of the bound variable text is byte-identical to the bound literal text");
        // Another serial prints another text, along the same guide; the document keeps its template.
        variable.h.send(UiIntent::Variable { document: variable.doc, edit: VariableEdit::Next });
        let c = compile_hash(&mut variable);
        assert_ne!(c, a);
        assert_eq!(variable.h.text_contents(variable.doc).last().map(String::as_str), Some("SN-{serial}"));
        let mut next_literal = bound_rig("SN-0015", false);
        assert_eq!(c, compile_hash(&mut next_literal));
    }
}
