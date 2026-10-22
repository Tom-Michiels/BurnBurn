//! G7-editor E08 in the window: Text on a Path keeps the text bound to its guide. The dialog
//! binds (and still makes plain paths when asked), the toolbar of a bound text shows and edits
//! the binding, and when the guide moves, changes shape or is deleted the text follows or says why
//! it cannot, each in one undo step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::UndoLabel;
use burnburn_document::text::PathBinding;
use burnburn_geometry::{Path, Point, ProductionTolerance};
use burnburn_presentation::{
    CommandKey, Distort, DocumentViewModel, HeadlessEditorHarness, PathBindingStatus, SelectMode, TextEdit, TextIssue,
    TextPathEdit, UiIntent,
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
    fn path_edit(&mut self, edit: TextPathEdit) {
        self.h.send(UiIntent::TextPath { document: self.doc, edit });
    }
    fn text_edit(&mut self, edit: TextEdit) {
        self.h.send(UiIntent::Text { document: self.doc, edit });
    }
    fn bind(&mut self) {
        self.h.invoke(self.doc, CommandKey::TextOnPath);
        self.path_edit(TextPathEdit::Apply);
    }
    fn select_text(&mut self) {
        let ids = self.text_outlines();
        self.h.send(UiIntent::Select { document: self.doc, ids, mode: SelectMode::Replace });
    }
    /// The objects that are outlines of the text now (everything but the circle).
    fn text_outlines(&self) -> Vec<GeometryId> {
        vm(&self.h, self.doc).objects.iter().map(|o| o.id).filter(|id| *id != self.circle).collect()
    }
    fn source(&mut self) -> burnburn_text::TextObject {
        self.h.controller().text_sources(self.doc).remove(0)
    }
    fn bounds_of(&self, ids: &[GeometryId]) -> Option<(Point, Point)> {
        let scene = self.h.scene(self.doc).unwrap();
        let mut all: Option<burnburn_geometry::Bounds> = None;
        for item in scene.items.iter().filter(|i| ids.contains(&i.source)) {
            let b = item.path.bounds()?;
            all = Some(all.map_or(b, |a| a.union(b)));
        }
        all.map(|b| (b.min(), b.max()))
    }
    fn text_bounds(&self) -> (Point, Point) {
        self.bounds_of(&self.text_outlines()).expect("the text has outlines")
    }
    fn circle_centre(&self) -> Point {
        let b = self.bounds_of(&[self.circle]).unwrap();
        Point::new((b.0.x + b.1.x) / 2.0, (b.0.y + b.1.y) / 2.0)
    }
    fn circle_path(&self) -> Path {
        let scene = self.h.scene(self.doc).unwrap();
        (*scene.items.iter().find(|i| i.source == self.circle).unwrap().path).clone()
    }
}

fn tol() -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap()
}

#[test]
fn the_dialog_keeps_the_text_bound_by_default_and_applying_is_one_undo_step() {
    let mut w = world("HIT");
    w.h.invoke(w.doc, CommandKey::TextOnPath);
    assert!(vm(&w.h, w.doc).text_on_path.unwrap().live, "following the guide is the default");
    let straight = w.text_bounds();
    let objects = vm(&w.h, w.doc).objects.len();
    w.path_edit(TextPathEdit::Apply);

    assert!(vm(&w.h, w.doc).text_on_path.is_none());
    assert_eq!(vm(&w.h, w.doc).undo, Some(UndoLabel::Text(burnburn_document::text::TextLabel::Edit)));
    assert_eq!(vm(&w.h, w.doc).objects.len(), objects, "the text bent in place: no new object");
    let binding = w.source().path.expect("the text is bound");
    assert_eq!(binding, PathBinding { guide: w.circle, start_mm: 0.0, offset_mm: 0.0, reverse: false, warp: false });
    assert_ne!(w.text_bounds(), straight, "and it lies on the circle now");

    // The outline equals what the generator makes of the same text and guide.
    let (src, _) = fonts();
    let text = w.source();
    let params = burnburn_generators::PathTextParams {
        distort: Distort::Rigid,
        start: 0.0,
        offset: 0.0,
        reverse: false,
        tolerance: tol(),
    };
    let expected = burnburn_generators::text_on_path(&text, &*src, &w.circle_path(), &params).unwrap().path;
    let got = w.text_bounds();
    let want = expected.bounds().unwrap();
    assert!(got.0.distance(want.min()) < 1e-9 && got.1.distance(want.max()) < 1e-9);

    w.h.invoke(w.doc, CommandKey::Undo);
    assert_eq!(w.source().path, None);
    assert_eq!(w.text_bounds(), straight);
}

#[test]
fn live_off_makes_ordinary_paths_as_before() {
    let mut w = world("HIT");
    let known = vm(&w.h, w.doc).objects.len();
    w.h.invoke(w.doc, CommandKey::TextOnPath);
    w.path_edit(TextPathEdit::Live(false));
    assert!(!vm(&w.h, w.doc).text_on_path.unwrap().live);
    w.path_edit(TextPathEdit::Apply);
    assert_eq!(w.source().path, None, "the text itself is untouched");
    assert_eq!(vm(&w.h, w.doc).objects.len(), known + 1, "one new plain path");
    assert_eq!(vm(&w.h, w.doc).undo, Some(UndoLabel::Edit(burnburn_document::EditLabel::CreatePath)));
}

#[test]
fn start_offset_direction_and_bending_reach_the_binding() {
    let mut w = world("HIT");
    w.h.invoke(w.doc, CommandKey::TextOnPath);
    w.path_edit(TextPathEdit::Distort(Distort::Warp));
    w.path_edit(TextPathEdit::Start("12,5 mm".into()));
    w.path_edit(TextPathEdit::Offset("-1,5".into()));
    w.path_edit(TextPathEdit::Reverse(true));
    w.path_edit(TextPathEdit::Apply);
    let b = w.source().path.unwrap();
    assert_eq!((b.start_mm, b.offset_mm, b.reverse, b.warp), (12.5, -1.5, true, true));
}

#[test]
fn a_guide_that_cannot_carry_the_text_keeps_the_dialog_and_binds_nothing() {
    let mut w = world("HIT");
    w.h.invoke(w.doc, CommandKey::TextOnPath);
    let rev = vm(&w.h, w.doc).revision;
    w.path_edit(TextPathEdit::Start("1000".into()));
    w.path_edit(TextPathEdit::Apply);
    assert_eq!(vm(&w.h, w.doc).text_on_path.unwrap().issue, Some(TextIssue::PathTextTooLong));
    assert_eq!(vm(&w.h, w.doc).revision, rev);
    assert_eq!(w.source().path, None);
}

#[test]
fn moving_the_guide_moves_the_bound_text_with_it_in_one_undo_step() {
    let mut w = world("HIT");
    w.bind();
    let (text_before, centre_before) = (w.text_bounds(), w.circle_centre());
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![w.circle], mode: SelectMode::Replace });
    w.h.drag(w.doc, (140.0, 100.0), (152.0, 109.0));
    let moved = w.circle_centre();
    let (dx, dy) = (moved.x - centre_before.x, moved.y - centre_before.y);
    assert!(dx.abs() > 1.0 || dy.abs() > 1.0, "the circle moved");
    let after = w.text_bounds();
    assert!((after.0.x - text_before.0.x - dx).abs() < 1e-6 && (after.0.y - text_before.0.y - dy).abs() < 1e-6);
    assert!((after.1.x - text_before.1.x - dx).abs() < 1e-6 && (after.1.y - text_before.1.y - dy).abs() < 1e-6);

    w.h.invoke(w.doc, CommandKey::Undo);
    assert_eq!(w.circle_centre(), centre_before);
    assert_eq!(w.text_bounds(), text_before, "the guide and the text go back together");
}

#[test]
fn the_toolbar_of_a_bound_text_shows_the_binding() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let bar = vm(&w.h, w.doc).text_toolbar.expect("a toolbar");
    let path = bar.path.expect("the text follows a path");
    assert_eq!(path.status, PathBindingStatus::Following);
    assert_eq!(
        (path.start.as_str(), path.offset.as_str(), path.reverse, path.distort),
        ("0", "0", false, Distort::Rigid)
    );
    assert!(path.guide.is_some());
    // A text that follows nothing shows no path section.
    w.text_edit(TextEdit::UnbindPath);
    assert!(vm(&w.h, w.doc).text_toolbar.unwrap().path.is_none());
}

#[test]
fn the_binding_is_edited_from_the_toolbar_and_each_edit_is_one_undo_step() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let before = w.text_bounds();
    w.text_edit(TextEdit::PathStart("30".into()));
    assert_eq!(w.source().path.unwrap().start_mm, 30.0);
    assert_eq!(vm(&w.h, w.doc).undo, Some(UndoLabel::Text(burnburn_document::text::TextLabel::Edit)));
    assert_ne!(w.text_bounds(), before, "the outline moved along the guide");
    w.text_edit(TextEdit::PathOffset("-2".into()));
    w.text_edit(TextEdit::PathReverse(true));
    w.text_edit(TextEdit::PathDistort(Distort::Warp));
    let b = w.source().path.unwrap();
    assert_eq!((b.start_mm, b.offset_mm, b.reverse, b.warp), (30.0, -2.0, true, true));

    w.h.invoke(w.doc, CommandKey::Undo);
    assert!(!w.source().path.unwrap().warp, "one edit, one step");
    w.h.invoke(w.doc, CommandKey::Undo);
    w.h.invoke(w.doc, CommandKey::Undo);
    w.h.invoke(w.doc, CommandKey::Undo);
    assert_eq!(w.text_bounds(), before);
}

#[test]
fn a_bad_binding_value_is_refused_and_changes_nothing() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    let rev = vm(&w.h, w.doc).revision;
    for (edit, issue) in [
        (TextEdit::PathStart("abc".into()), TextIssue::InvalidNumber(burnburn_presentation::TextField::PathStart)),
        (TextEdit::PathStart("-3".into()), TextIssue::InvalidNumber(burnburn_presentation::TextField::PathStart)),
        (TextEdit::PathOffset("x".into()), TextIssue::InvalidNumber(burnburn_presentation::TextField::PathOffset)),
        (TextEdit::PathStart("1000".into()), TextIssue::PathTextTooLong),
    ] {
        w.text_edit(edit);
        assert_eq!(vm(&w.h, w.doc).text_toolbar.unwrap().issue, Some(issue));
        assert_eq!(vm(&w.h, w.doc).revision, rev);
    }
}

#[test]
fn path_settings_on_a_text_that_follows_nothing_are_refused() {
    let mut w = world("HIT");
    w.select_text();
    let rev = vm(&w.h, w.doc).revision;
    w.text_edit(TextEdit::PathStart("5".into()));
    assert_eq!(vm(&w.h, w.doc).text_toolbar.unwrap().issue, Some(TextIssue::NotOnPath));
    assert_eq!(vm(&w.h, w.doc).revision, rev);
}

#[test]
fn unbinding_returns_the_text_to_its_own_place_and_it_stops_following() {
    let mut w = world("HIT");
    let straight = w.text_bounds();
    w.bind();
    w.select_text();
    w.text_edit(TextEdit::UnbindPath);
    assert_eq!(w.source().path, None);
    assert_eq!(w.text_bounds(), straight);
    // The circle moves; the free text stays.
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![w.circle], mode: SelectMode::Replace });
    w.h.drag(w.doc, (140.0, 100.0), (152.0, 109.0));
    assert_eq!(w.text_bounds(), straight);
}

#[test]
fn a_deleted_guide_is_said_and_undo_brings_the_text_back_to_following() {
    let mut w = world("HIT");
    w.bind();
    let on_circle = w.text_bounds();
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![w.circle], mode: SelectMode::Replace });
    w.h.invoke(w.doc, CommandKey::Delete);
    w.select_text();
    let path = vm(&w.h, w.doc).text_toolbar.unwrap().path.unwrap();
    assert_eq!((path.status, path.guide), (PathBindingStatus::GuideMissing, None));
    assert_eq!(w.text_bounds(), on_circle, "the last outline stays where it was");
    w.h.invoke(w.doc, CommandKey::Undo);
    w.select_text();
    assert_eq!(vm(&w.h, w.doc).text_toolbar.unwrap().path.unwrap().status, PathBindingStatus::Following);
}

#[test]
fn a_guide_that_shrinks_below_the_text_is_shown_as_not_fitting() {
    let mut w = world("HIT");
    w.bind();
    w.select_text();
    // Scale the circle to a few millimetres: the text no longer fits on it.
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![w.circle], mode: SelectMode::Replace });
    w.h.send(UiIntent::CommitField {
        document: w.doc,
        field: burnburn_presentation::FieldKey::Width,
        text: "3".into(),
    });
    // The text followed the narrower guide, so the selection moved with the change: pick the circle again.
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![w.circle], mode: SelectMode::Replace });
    w.h.send(UiIntent::CommitField {
        document: w.doc,
        field: burnburn_presentation::FieldKey::Height,
        text: "3".into(),
    });
    w.select_text();
    let path = vm(&w.h, w.doc).text_toolbar.unwrap().path.unwrap();
    assert_eq!(path.status, PathBindingStatus::DoesNotFit);
}
