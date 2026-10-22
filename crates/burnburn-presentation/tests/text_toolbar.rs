//! The text toolbar, Convert to Path and text on a path (W3-UI-tekst), headless: the toolbar
//! follows the one selected text object, every accepted edit is one `TextCommand::Set` (one undo
//! step), typed numbers are parsed here and a bad one changes nothing. Outline oracles are
//! relations between independent renderings of the same font (bounds of another text, a known
//! spacing step), not the implementation's own formulas.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::UndoLabel;
use burnburn_document::text::{TextLabel, TextMode};
use burnburn_geometry::Bounds;
use burnburn_presentation::{
    Align, CommandKey, DocumentViewModel, HeadlessEditorHarness, SelectMode, TextEdit, TextField, TextIssue, UiIntent,
    VAlign,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;

fn face(name: &str) -> Arc<FontData> {
    Arc::new(FontData::from_bytes(read_fixture("fonts", name).unwrap()).unwrap())
}

/// "Liberation Sans" with a regular and a (stand-in) bold face, "BIZ UDPGothic" regular only.
/// There is no real bold fixture; the bold face reuses the regular bytes, which is enough to
/// test that the toolbar offers and selects the face — never its typography.
fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let liberation = face("LiberationSans-Regular.ttf");
    let regular = src.insert("Liberation Sans", FontStyle::Regular, liberation.clone());
    src.insert("Liberation Sans", FontStyle::Bold, liberation);
    src.insert("BIZ UDPGothic", FontStyle::Regular, face("BIZUDPGothic-Regular.ttf"));
    (Arc::new(src), regular)
}

fn text_doc(content: &str) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.send(UiIntent::InsertText { document: doc, text: content.into() });
    (h, doc)
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: TextEdit) {
    h.send(UiIntent::Text { document: doc, edit });
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn source(h: &mut HeadlessEditorHarness, doc: DocumentId) -> burnburn_text::TextObject {
    h.controller().text_sources(doc).remove(0)
}

/// Extent of everything drawn for the document's objects, in document mm.
#[derive(Debug, Clone, Copy)]
struct Ink {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl Ink {
    fn width(&self) -> f64 {
        self.max_x - self.min_x
    }
    fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

fn ink(h: &HeadlessEditorHarness, doc: DocumentId) -> Ink {
    let scene = h.scene(doc).unwrap();
    let ids: Vec<_> = vm(h, doc).objects.iter().map(|o| o.id).collect();
    let mut all: Option<Bounds> = None;
    for item in scene.items.iter().filter(|i| ids.contains(&i.source)) {
        let b = item.path.bounds().expect("a drawn outline");
        all = Some(all.map_or(b, |a| a.union(b)));
    }
    let b = all.expect("the text has outlines");
    Ink { min_x: b.min().x, max_x: b.max().x, min_y: b.min().y, max_y: b.max().y }
}

#[test]
fn the_toolbar_follows_the_selected_text_and_only_that() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    assert!(vm(&h, doc).text_toolbar.is_none(), "no text, no toolbar");

    h.send(UiIntent::InsertText { document: doc, text: "HIT".into() });
    let bar = vm(&h, doc).text_toolbar.expect("the new text is selected");
    assert_eq!(
        &*bar.families,
        ["BIZ UDPGothic".to_owned(), "Liberation Sans".to_owned()],
        "the source's families, sorted"
    );
    assert_eq!(bar.family, "Liberation Sans");
    assert_eq!((bar.height.as_str(), bar.h_space.as_str(), bar.v_space.as_str()), ("10", "0", "0"));
    assert_eq!((bar.align, bar.valign, bar.uppercase, bar.welded.on), (Align::Left, VAlign::Baseline, false, false));

    // Selecting something that is not text takes the toolbar away.
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (50.0, 50.0), (80.0, 70.0));
    assert!(vm(&h, doc).text_toolbar.is_none(), "a rectangle is selected");
}

#[test]
fn two_texts_selected_share_one_toolbar_and_an_edit_changes_both() {
    let (mut h, doc) = text_doc("one");
    h.send(UiIntent::InsertText { document: doc, text: "two".into() }); // extends the selected text
    h.invoke(doc, CommandKey::ToolSelect);
    // A second text object far away.
    h.controller().handle(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    h.send(UiIntent::InsertText { document: doc, text: "other".into() });
    assert_eq!(h.text_contents(doc).len(), 2);
    let all: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids: all, mode: SelectMode::Replace });
    // Changed in G7-editor (E07): several texts share one toolbar and an edit reaches all of them.
    assert_eq!(vm(&h, doc).text_toolbar.map(|b| b.count), Some(2));
    edit(&mut h, doc, TextEdit::Uppercase(true));
    assert!(h.controller().text_sources(doc).iter().all(|t| t.layout.uppercase));
}

#[test]
fn bold_and_italic_are_offered_only_when_the_family_has_the_face() {
    let (mut h, doc) = text_doc("HIT");
    let bar = vm(&h, doc).text_toolbar.unwrap();
    assert!(bar.bold.enabled && !bar.bold.on, "Liberation Sans has a bold face");
    assert!(
        !bar.italic.enabled && bar.italic.why_off == Some(TextIssue::StyleMissing),
        "no italic face: off with a reason"
    );

    edit(&mut h, doc, TextEdit::Bold(true));
    let font = source(&mut h, doc).font;
    assert_eq!((font.family.as_str(), font.style), ("Liberation Sans", FontStyle::Bold));
    assert!(vm(&h, doc).text_toolbar.unwrap().bold.on);
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Text(TextLabel::Edit)));

    // Italic is refused: no command, the reason is in the toolbar, shaping never swaps the face.
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, TextEdit::Italic(true));
    assert_eq!(vm(&h, doc).revision, rev);
    assert_eq!(vm(&h, doc).text_toolbar.unwrap().issue, Some(TextIssue::StyleMissing));
    assert_eq!(source(&mut h, doc).font.style, FontStyle::Bold);

    h.invoke(doc, CommandKey::Undo);
    assert_eq!(source(&mut h, doc).font.style, FontStyle::Regular, "one undo step");
}

#[test]
fn a_family_change_keeps_the_style_when_the_family_has_it_and_never_invents_a_family() {
    let (mut h, doc) = text_doc("HIT");
    edit(&mut h, doc, TextEdit::Bold(true));
    // BIZ UDPGothic has no bold face: the regular face is chosen, and the toolbar says so.
    edit(&mut h, doc, TextEdit::Family("BIZ UDPGothic".into()));
    let font = source(&mut h, doc).font;
    assert_eq!((font.family.as_str(), font.style), ("BIZ UDPGothic", FontStyle::Regular));
    let bar = vm(&h, doc).text_toolbar.unwrap();
    assert!(!bar.bold.on && !bar.bold.enabled);

    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, TextEdit::Family("Comic Sans".into()));
    assert_eq!(vm(&h, doc).revision, rev, "a family the source does not list changes nothing");
    assert_eq!(vm(&h, doc).text_toolbar.unwrap().issue, Some(TextIssue::FamilyUnavailable));
    assert_eq!(source(&mut h, doc).font.family, "BIZ UDPGothic");
}

#[test]
fn height_scales_the_outlines_by_the_typed_ratio() {
    let (mut h, doc) = text_doc("HIT");
    let before = ink(&h, doc);
    edit(&mut h, doc, TextEdit::Height("12,5 mm".into()));
    assert_eq!(source(&mut h, doc).size.em_mm(), 12.5);
    let after = ink(&h, doc);
    let ratio = after.height() / before.height();
    assert!((ratio - 1.25).abs() < 1e-6, "ratio {ratio}");
    assert_eq!(vm(&h, doc).text_toolbar.unwrap().height, "12.5");
}

#[test]
fn uppercase_draws_the_capital_letters() {
    let (mut h, doc) = text_doc("hit");
    edit(&mut h, doc, TextEdit::Uppercase(true));
    assert_eq!(h.text_contents(doc), ["hit"], "the source text stays as typed");
    let drawn = ink(&h, doc);
    let (reference, ref_doc) = text_doc("HIT");
    let expected = ink(&reference, ref_doc);
    assert!((drawn.width() - expected.width()).abs() < 1e-9 && (drawn.height() - expected.height()).abs() < 1e-9);
}

#[test]
fn horizontal_alignment_moves_the_text_around_its_anchor() {
    let (mut h, doc) = text_doc("HIT");
    let left = ink(&h, doc);
    assert!(left.min_x > -0.5 && left.min_x < 1.0, "left aligned: the ink starts at the anchor");
    edit(&mut h, doc, TextEdit::Align(Align::Center));
    let centre = ink(&h, doc);
    assert!(centre.min_x < -1.0 && centre.max_x > 1.0, "centred: the anchor is inside the ink");
    edit(&mut h, doc, TextEdit::Align(Align::Right));
    let right = ink(&h, doc);
    assert!(right.max_x < 1.0 && right.max_x > -0.5, "right aligned: the ink ends at the anchor");
    assert!((left.width() - right.width()).abs() < 1e-9, "alignment never changes the shapes");
}

#[test]
fn vertical_alignment_hangs_the_block_from_the_anchor() {
    // "HIT" has no descender: the baseline is the bottom of the ink.
    let (mut h, doc) = text_doc("HIT");
    assert!(ink(&h, doc).min_y.abs() < 1e-6, "default: the first baseline sits on the anchor");
    edit(&mut h, doc, TextEdit::VAlign(VAlign::Top));
    assert!(ink(&h, doc).max_y <= 1e-6, "top: the ink hangs below the anchor");
    edit(&mut h, doc, TextEdit::VAlign(VAlign::Bottom));
    assert!(ink(&h, doc).min_y >= -1e-6, "bottom: the block (with its descent) stands on the anchor");
    edit(&mut h, doc, TextEdit::VAlign(VAlign::Middle));
    let middle = ink(&h, doc);
    assert!(middle.min_y < 0.0 && middle.max_y > 0.0, "middle: the anchor is inside the block");
}

#[test]
fn hspace_and_vspace_add_exactly_the_typed_distance() {
    let (mut h, doc) = text_doc("HIT");
    let width = ink(&h, doc).width();
    edit(&mut h, doc, TextEdit::HSpace("2".into()));
    // Three letters have two gaps; the spacing is not added after the last letter.
    assert!((ink(&h, doc).width() - (width + 4.0)).abs() < 1e-9);
    edit(&mut h, doc, TextEdit::HSpace("-0,5".into()));
    assert!((ink(&h, doc).width() - (width - 1.0)).abs() < 1e-9, "negative spacing is allowed");

    let (mut h2, doc2) = text_doc("A\nB");
    let height = ink(&h2, doc2).height();
    edit(&mut h2, doc2, TextEdit::VSpace("5 mm".into()));
    assert!((ink(&h2, doc2).height() - (height + 5.0)).abs() < 1e-9);
    assert_eq!(vm(&h2, doc2).text_toolbar.unwrap().v_space, "5");
}

#[test]
fn every_accepted_edit_is_one_undo_step_and_a_repeat_is_none() {
    let (mut h, doc) = text_doc("HIT");
    for e in [
        TextEdit::Uppercase(true),
        TextEdit::Align(Align::Center),
        TextEdit::VAlign(VAlign::Middle),
        TextEdit::HSpace("1".into()),
        TextEdit::VSpace("1".into()),
        TextEdit::Height("20".into()),
        TextEdit::Welded(true),
    ] {
        let rev = vm(&h, doc).revision;
        edit(&mut h, doc, e.clone());
        let after = vm(&h, doc);
        assert_eq!(after.revision.raw(), rev.raw() + 1, "{e:?} is one command");
        assert_eq!(after.undo, Some(UndoLabel::Text(TextLabel::Edit)));
        edit(&mut h, doc, e.clone());
        assert_eq!(vm(&h, doc).revision, after.revision, "{e:?} again changes nothing");
    }
    let t = source(&mut h, doc);
    assert!(t.layout.uppercase && t.layout.welded);
    assert_eq!((t.layout.align, t.layout.valign), (Align::Center, VAlign::Middle));
    assert_eq!((t.layout.letter_spacing_mm, t.layout.v_space_mm, t.size.em_mm()), (1.0, 1.0, 20.0));
    for _ in 0..7 {
        h.invoke(doc, CommandKey::Undo);
    }
    let t = source(&mut h, doc);
    assert_eq!((t.layout, t.size.em_mm()), (Default::default(), 10.0), "seven undos restore the original");
}

#[test]
fn typed_numbers_are_parsed_here_and_a_bad_one_changes_nothing() {
    let (mut h, doc) = text_doc("HIT");
    let rev = vm(&h, doc).revision;
    for (e, field) in [
        (TextEdit::HSpace("veel".into()), TextField::HSpace),
        (TextEdit::VSpace("1,000.5".into()), TextField::VSpace),
        (TextEdit::Height("0".into()), TextField::Height),
        (TextEdit::Height("-3".into()), TextField::Height),
        (TextEdit::Height("".into()), TextField::Height),
    ] {
        edit(&mut h, doc, e.clone());
        let after = vm(&h, doc);
        assert_eq!(after.revision, rev, "{e:?}");
        assert_eq!(after.text_toolbar.unwrap().issue, Some(TextIssue::InvalidNumber(field)), "{e:?}");
    }
    // The next valid edit clears the reason.
    edit(&mut h, doc, TextEdit::HSpace("1".into()));
    assert_eq!(vm(&h, doc).text_toolbar.unwrap().issue, None);
}

#[test]
fn welded_needs_outline_text() {
    let (mut h, doc) = text_doc("HIT");
    assert!(vm(&h, doc).text_toolbar.unwrap().welded.enabled);
    edit(&mut h, doc, TextEdit::SingleLine(true));
    assert_eq!(source(&mut h, doc).mode, TextMode::SingleLine);
    let bar = vm(&h, doc).text_toolbar.unwrap();
    assert!(bar.single_line && !bar.welded.enabled);
    assert_eq!(bar.welded.why_off, Some(TextIssue::WeldedNeedsOutline));
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, TextEdit::Welded(true));
    assert_eq!(vm(&h, doc).revision, rev);
    assert_eq!(vm(&h, doc).text_toolbar.unwrap().issue, Some(TextIssue::WeldedNeedsOutline));
}

#[test]
fn convert_to_path_turns_the_text_into_plain_objects_and_undo_brings_it_back() {
    let (mut h, doc) = text_doc("OK");
    let drawn: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    assert!(h.view().commands[&CommandKey::ShapesConvertToPath].enabled, "text outlines can be converted");
    let before = ink(&h, doc);

    h.invoke(doc, CommandKey::ShapesConvertToPath);
    let after = vm(&h, doc);
    assert_eq!(after.undo, Some(UndoLabel::Text(TextLabel::ConvertToPath)));
    assert!(h.text_contents(doc).is_empty(), "the text object is gone");
    assert!(after.text_toolbar.is_none());
    assert_eq!(after.objects.iter().map(|o| o.id).collect::<Vec<_>>(), drawn, "same objects, now plain paths");
    let converted = ink(&h, doc);
    assert!((converted.width() - before.width()).abs() < 1e-12 && (converted.height() - before.height()).abs() < 1e-12);
    assert!(!h.view().commands[&CommandKey::ShapesConvertToPath].enabled, "a plain path has nothing to convert");

    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.text_contents(doc), ["OK"], "undo restores the text with its outlines");
    assert!(vm(&h, doc).text_toolbar.is_some());
}

// ---------------------------------------------------------------------------------------------
// Text on a path
// ---------------------------------------------------------------------------------------------

use burnburn_geometry::{Path, Point};
use burnburn_presentation::{Distort, TextPathEdit};
use burnburn_types::GeometryId;

/// A text and a circle of radius 40 centred on (100, 100), both selected. Returns the ids of the
/// text outlines and of the circle.
fn text_and_circle(content: &str) -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>, GeometryId) {
    let (mut h, doc) = text_doc(content);
    let outlines: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::ToolCircle);
    h.drag(doc, (100.0, 100.0), (140.0, 100.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let circle = vm(&h, doc).objects.iter().map(|o| o.id).find(|id| !outlines.contains(id)).expect("the circle");
    let mut both = outlines.clone();
    both.push(circle);
    h.send(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
    (h, doc, outlines, circle)
}

fn guide_of(h: &HeadlessEditorHarness, doc: DocumentId, circle: GeometryId) -> (Path, Point, f64) {
    let scene = h.scene(doc).unwrap();
    let item = scene.items.iter().find(|i| i.source == circle).unwrap();
    let b = item.path.bounds().unwrap();
    ((*item.path).clone(), b.center(), b.width() / 2.0)
}

fn radii_of(path: &Path, centre: Point) -> Vec<f64> {
    let tol = burnburn_geometry::ProductionTolerance::new(burnburn_types::Length::from_mm(0.001).unwrap()).unwrap();
    path.flatten(&tol).into_iter().flat_map(|pl| pl.points).map(|p| (p - centre).length()).collect()
}

fn path_edit(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: TextPathEdit) {
    h.send(UiIntent::TextPath { document: doc, edit });
}

/// The new object of the last Apply: not one of `known`.
fn newest(h: &HeadlessEditorHarness, doc: DocumentId, known: &[GeometryId]) -> GeometryId {
    vm(h, doc).objects.iter().map(|o| o.id).find(|id| !known.contains(id)).expect("a new object")
}

#[test]
fn text_on_a_path_needs_one_text_and_one_guide() {
    let (mut h, doc) = text_doc("HIT");
    assert!(!h.view().commands[&CommandKey::TextOnPath].enabled, "a text alone has no guide");
    h.invoke(doc, CommandKey::TextOnPath);
    assert!(vm(&h, doc).text_on_path.is_none(), "no dialog without a guide");

    let (mut h, doc, _, _) = text_and_circle("HIT");
    assert!(h.view().commands[&CommandKey::TextOnPath].enabled);
    h.invoke(doc, CommandKey::TextOnPath);
    let dialog = vm(&h, doc).text_on_path.expect("the dialog opens");
    assert!(dialog.ready && dialog.distort == Distort::Rigid && dialog.start == "0" && !dialog.reverse);

    // The dialog follows the selection: with the circle deselected there is no guide.
    let first: Vec<_> = vm(&h, doc).objects.iter().take(1).map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids: first, mode: SelectMode::Replace });
    assert!(!vm(&h, doc).text_on_path.unwrap().ready);
    let rev = vm(&h, doc).revision;
    path_edit(&mut h, doc, TextPathEdit::Apply);
    assert_eq!(vm(&h, doc).revision, rev, "nothing to bend along");
    assert_eq!(vm(&h, doc).text_on_path.unwrap().issue, Some(TextIssue::PathNeedsTextAndGuide));
    path_edit(&mut h, doc, TextPathEdit::Cancel);
    assert!(vm(&h, doc).text_on_path.is_none());
}

#[test]
fn rigid_text_on_a_circle_equals_the_generator_and_crosses_the_guide_warped_text_does_not() {
    let (mut h, doc, outlines, circle) = text_and_circle("HIT");
    let (guide, centre, radius) = guide_of(&h, doc, circle);
    let known: Vec<_> = outlines.iter().copied().chain([circle]).collect();
    let (src, _) = fonts();
    let cap = 10.0 * 1409.0 / 2048.0; // cap height of Liberation Sans at 10 mm
    let eps = 0.02;
    let text = source(&mut h, doc);
    let tol = burnburn_geometry::ProductionTolerance::new(burnburn_types::Length::from_mm(0.01).unwrap()).unwrap();
    let direct = |distort| {
        let params =
            burnburn_generators::PathTextParams { distort, start: 0.0, offset: 0.0, reverse: false, tolerance: tol };
        burnburn_generators::text_on_path(&text, &*src, &guide, &params).unwrap().path
    };

    h.invoke(doc, CommandKey::TextOnPath);
    // Since G7-editor the dialog binds by default; these tests are about the plain paths.
    path_edit(&mut h, doc, TextPathEdit::Live(false));
    path_edit(&mut h, doc, TextPathEdit::Apply);
    assert!(vm(&h, doc).text_on_path.is_none(), "applying closes the dialog");
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(burnburn_document::EditLabel::CreatePath)));
    let rigid_id = newest(&h, doc, &known);
    let scene = h.scene(doc).unwrap();
    let rigid = scene.items.iter().find(|i| i.source == rigid_id).unwrap().path.clone();
    assert_eq!(rigid.bounds(), direct(Distort::Rigid).bounds(), "the dialog's Rigid is the generator's Rigid");
    let r = radii_of(&rigid, centre);
    assert!(
        r.iter().any(|d| *d > radius + eps) && r.iter().any(|d| *d < radius - eps),
        "straight glyphs on a tangent cross the circle"
    );
    assert_eq!(h.text_contents(doc), ["HIT"], "the text stays");

    // Warp: the same dialog with the other choice. The outlines bend: all of the text lies on one side.
    let known: Vec<_> = known.into_iter().chain([rigid_id]).collect();
    let mut both = outlines.clone();
    both.push(circle);
    h.send(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::TextOnPath);
    path_edit(&mut h, doc, TextPathEdit::Live(false));
    path_edit(&mut h, doc, TextPathEdit::Distort(Distort::Warp));
    assert_eq!(vm(&h, doc).text_on_path.unwrap().distort, Distort::Warp);
    path_edit(&mut h, doc, TextPathEdit::Apply);
    let warp_id = newest(&h, doc, &known);
    let scene = h.scene(doc).unwrap();
    let warped = scene.items.iter().find(|i| i.source == warp_id).unwrap().path.clone();
    assert_eq!(warped.bounds(), direct(Distort::Warp).bounds(), "the dialog's Warp is the generator's Warp");
    assert_ne!(warped.bounds(), rigid.bounds());
    let r = radii_of(&warped, centre);
    let (lo, hi) = (r.iter().copied().fold(f64::INFINITY, f64::min), r.iter().copied().fold(0.0, f64::max));
    let inside = hi <= radius + eps && lo >= radius - cap - eps;
    let outside = lo >= radius - eps && hi <= radius + cap + eps;
    assert!(inside || outside, "warped text hugs one side of the circle: {lo}..{hi} around {radius}");

    // One undo step per Apply.
    h.invoke(doc, CommandKey::Undo);
    assert!(
        vm(&h, doc).objects.iter().all(|o| o.id != warp_id) && vm(&h, doc).objects.iter().any(|o| o.id == rigid_id)
    );
}

#[test]
fn text_on_a_path_start_offset_and_direction_reach_the_generator() {
    let (mut h, doc, outlines, circle) = text_and_circle("HIT");
    let (guide, _, _) = guide_of(&h, doc, circle);
    let known: Vec<_> = outlines.iter().copied().chain([circle]).collect();
    let (src, _) = fonts();
    let text = source(&mut h, doc);
    let tol = burnburn_geometry::ProductionTolerance::new(burnburn_types::Length::from_mm(0.01).unwrap()).unwrap();
    let params = burnburn_generators::PathTextParams {
        distort: Distort::Warp,
        start: 12.5,
        offset: -1.5,
        reverse: true,
        tolerance: tol,
    };
    let expected = burnburn_generators::text_on_path(&text, &*src, &guide, &params).unwrap().path;

    h.invoke(doc, CommandKey::TextOnPath);
    path_edit(&mut h, doc, TextPathEdit::Live(false));
    path_edit(&mut h, doc, TextPathEdit::Distort(Distort::Warp));
    path_edit(&mut h, doc, TextPathEdit::Start("12,5 mm".into()));
    path_edit(&mut h, doc, TextPathEdit::Offset("-1,5".into()));
    path_edit(&mut h, doc, TextPathEdit::Reverse(true));
    path_edit(&mut h, doc, TextPathEdit::Apply);
    let id = newest(&h, doc, &known);
    let scene = h.scene(doc).unwrap();
    let got = scene.items.iter().find(|i| i.source == id).unwrap().path.bounds();
    assert_eq!(got, expected.bounds());
}

#[test]
fn text_on_a_path_says_why_it_cannot_and_keeps_the_dialog() {
    // Longer than the circle (2π·40 ≈ 251 mm) after the start.
    let (mut h, doc, _, _) = text_and_circle("HIT");
    h.invoke(doc, CommandKey::TextOnPath);
    let rev = vm(&h, doc).revision;
    for (edit, issue) in [
        (TextPathEdit::Start("abc".into()), TextIssue::InvalidNumber(TextField::PathStart)),
        (TextPathEdit::Start("-1".into()), TextIssue::InvalidNumber(TextField::PathStart)),
        (TextPathEdit::Start("0".into()), TextIssue::InvalidNumber(TextField::PathOffset)),
        (TextPathEdit::Start("1000".into()), TextIssue::PathTextTooLong),
    ] {
        let offset_bad = matches!(issue, TextIssue::InvalidNumber(TextField::PathOffset));
        if offset_bad {
            path_edit(&mut h, doc, TextPathEdit::Offset("x".into()));
        }
        path_edit(&mut h, doc, edit);
        path_edit(&mut h, doc, TextPathEdit::Apply);
        let dialog = vm(&h, doc).text_on_path.expect("still open");
        assert_eq!(dialog.issue, Some(issue));
        assert_eq!(vm(&h, doc).revision, rev, "{issue:?}: no command");
        if offset_bad {
            path_edit(&mut h, doc, TextPathEdit::Offset("0".into()));
        }
    }

    // Text on a path is one line.
    let (mut h, doc, _, _) = text_and_circle("A\nB");
    h.invoke(doc, CommandKey::TextOnPath);
    path_edit(&mut h, doc, TextPathEdit::Apply);
    assert_eq!(vm(&h, doc).text_on_path.unwrap().issue, Some(TextIssue::PathMultiline));
}
