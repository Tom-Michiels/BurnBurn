//! W3-UI-schikken: Resize Slots in Selection. A dialog asks for the old and the new width; the
//! result is one undo entry, and a refusal keeps the dialog open with the reason.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::{EditLabel, UndoLabel};
use burnburn_geometry::{Path, Point};
use burnburn_presentation::{
    CommandKey, SchikkenIntent, SchikkenMessage, SlotChoice, SlotField, SlotsFailure, SlotsInput, UiIntent,
};
use burnburn_types::DocumentId;
use edit_rig::{enabled, opened_with, points_of, select, square, view};

type Harness = burnburn_presentation::HeadlessEditorHarness;

/// A 60 × 30 panel with notches of `depth` cut into the top edge at the given x ranges.
fn panel(features: &[(f64, f64)], depth: f64) -> Path {
    let top = 30.0;
    let mut pts = vec![(0.0, 0.0), (60.0, 0.0), (60.0, top)];
    let mut sorted = features.to_vec();
    sorted.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (l, r) in sorted {
        pts.extend([(r, top), (r, top - depth), (l, top - depth), (l, top)]);
    }
    pts.push((0.0, top));
    let pts: Vec<Point> = pts.iter().map(|&(x, y)| Point::new(x, y)).collect();
    Path::polyline(&pts, true).unwrap()
}

fn input(from: &str, to: &str) -> SlotsInput {
    SlotsInput { from: from.to_owned(), to: to.to_owned(), tolerance: "0,01".to_owned(), select: SlotChoice::Slots }
}

fn apply(h: &mut Harness, doc: DocumentId, input: SlotsInput) {
    h.send(UiIntent::Schikken(SchikkenIntent::ApplySlots { document: doc, input }));
}

fn has_vertex(h: &Harness, doc: DocumentId, id: burnburn_types::GeometryId, x: f64, y: f64) -> bool {
    points_of(h, doc, id).iter().any(|p| (p.0 - x).abs() < 1e-9 && (p.1 - y).abs() < 1e-9)
}

fn dialog_error(h: &Harness, doc: DocumentId) -> Option<SchikkenMessage> {
    view(h, doc).schikken.slots.expect("the dialog is open").error
}

#[test]
fn the_command_opens_a_dialog_and_changes_nothing_until_ok() {
    let (mut h, doc, ids) = opened_with(vec![panel(&[(10.0, 13.0), (25.0, 28.0)], 4.0)]);
    select(&mut h, doc, &ids);
    assert!(enabled(&h, CommandKey::ResizeSlots));
    let revision = view(&h, doc).revision;
    h.invoke(doc, CommandKey::ResizeSlots);
    let vm = view(&h, doc);
    assert!(vm.schikken.slots.is_some());
    assert_eq!(vm.revision, revision);
    h.send(UiIntent::Schikken(SchikkenIntent::CancelSlots { document: doc }));
    assert!(view(&h, doc).schikken.slots.is_none());
}

#[test]
fn ok_widens_every_matching_slot_in_one_undo_entry_and_closes_the_dialog() {
    let (mut h, doc, ids) = opened_with(vec![panel(&[(10.0, 13.0), (25.0, 28.0), (40.0, 43.0)], 4.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ResizeSlots);
    apply(&mut h, doc, input("3", "3,6"));
    let vm = view(&h, doc);
    assert!(vm.schikken.slots.is_none(), "the dialog closes after a valid OK");
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::ResizeSlots)));
    for (l, r) in [(10.0, 13.0), (25.0, 28.0), (40.0, 43.0)] {
        assert!(has_vertex(&h, doc, ids[0], l - 0.3, 26.0) && has_vertex(&h, doc, ids[0], r + 0.3, 26.0));
    }
    h.invoke(doc, CommandKey::Undo);
    assert!(has_vertex(&h, doc, ids[0], 10.0, 26.0) && has_vertex(&h, doc, ids[0], 13.0, 26.0));
}

#[test]
fn tabs_and_both_follow_the_choice() {
    // Raised tabs are not slots: with "slots" nothing matches, with "tabs" they are resized.
    let top = 30.0;
    let pts = [
        (0.0, 0.0),
        (60.0, 0.0),
        (60.0, top),
        (43.0, top),
        (43.0, top + 4.0),
        (40.0, top + 4.0),
        (40.0, top),
        (0.0, top),
    ];
    let tabbed = Path::polyline(&pts.map(|(x, y)| Point::new(x, y)), true).unwrap();
    let (mut h, doc, ids) = opened_with(vec![tabbed]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ResizeSlots);
    apply(&mut h, doc, input("3", "3,6"));
    assert_eq!(dialog_error(&h, doc), Some(SchikkenMessage::Slots(SlotsFailure::NoSlotsFound)));
    apply(&mut h, doc, SlotsInput { select: SlotChoice::Tabs, ..input("3", "3,6") });
    assert!(view(&h, doc).schikken.slots.is_none());
    assert!(has_vertex(&h, doc, ids[0], 39.7, 34.0) && has_vertex(&h, doc, ids[0], 43.3, 34.0));
}

#[test]
fn nothing_matching_keeps_the_dialog_with_the_reason_and_the_document_untouched() {
    let (mut h, doc, ids) = opened_with(vec![panel(&[(10.0, 13.0)], 4.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ResizeSlots);
    let revision = view(&h, doc).revision;
    apply(&mut h, doc, input("5", "6"));
    assert_eq!(dialog_error(&h, doc), Some(SchikkenMessage::Slots(SlotsFailure::NoSlotsFound)));
    assert_eq!(view(&h, doc).revision, revision);
    assert_eq!(view(&h, doc).schikken.slots.expect("open").input.from, "5", "what was typed stays");
}

#[test]
fn slots_that_would_collide_are_refused_with_that_reason() {
    let (mut h, doc, ids) = opened_with(vec![panel(&[(10.0, 13.0), (14.0, 17.0)], 4.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ResizeSlots);
    apply(&mut h, doc, input("3", "5"));
    assert_eq!(dialog_error(&h, doc), Some(SchikkenMessage::Slots(SlotsFailure::Collision)));
    // Narrower is fine.
    apply(&mut h, doc, input("3", "2"));
    assert!(view(&h, doc).schikken.slots.is_none());
}

#[test]
fn wrong_text_names_the_field() {
    let (mut h, doc, ids) = opened_with(vec![panel(&[(10.0, 13.0)], 4.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ResizeSlots);
    let revision = view(&h, doc).revision;
    apply(&mut h, doc, input("", "3,5"));
    assert_eq!(dialog_error(&h, doc), Some(SchikkenMessage::Slots(SlotsFailure::InvalidField(SlotField::From))));
    apply(&mut h, doc, input("3", "abc"));
    assert_eq!(dialog_error(&h, doc), Some(SchikkenMessage::Slots(SlotsFailure::InvalidField(SlotField::To))));
    apply(&mut h, doc, SlotsInput { tolerance: "-1".to_owned(), ..input("3", "3,5") });
    assert_eq!(dialog_error(&h, doc), Some(SchikkenMessage::Slots(SlotsFailure::InvalidField(SlotField::Tolerance))));
    apply(&mut h, doc, input("0", "3,5"));
    assert_eq!(dialog_error(&h, doc), Some(SchikkenMessage::Slots(SlotsFailure::InvalidField(SlotField::From))));
    assert_eq!(view(&h, doc).revision, revision);
}

#[test]
fn a_selection_without_geometry_cannot_open_the_dialog() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), Path::default()]);
    select(&mut h, doc, &ids[1..]);
    assert!(!enabled(&h, CommandKey::ResizeSlots));
    h.invoke(doc, CommandKey::ResizeSlots);
    assert!(view(&h, doc).schikken.slots.is_none());
    select(&mut h, doc, &[]);
    assert!(!enabled(&h, CommandKey::ResizeSlots));
}
