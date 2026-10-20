//! E04 (W3-UI-schikken): Arrange ▸ Dock and Move Together from the window. Each command is one
//! undo entry with its own label; a refusal says why and leaves the document alone.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::{EditLabel, UndoLabel};
use burnburn_presentation::{
    CommandKey, DockContactChoice, DockReference, PlaceFailure, PlaceOptionsInput, SchikkenIntent, SchikkenMessage,
    TogetherAnchorChoice, UiIntent,
};
use burnburn_types::GeometryId;
use edit_rig::{enabled, opened_with, points_of, select, selected, square, view};

fn min_x(h: &burnburn_presentation::HeadlessEditorHarness, doc: burnburn_types::DocumentId, id: GeometryId) -> f64 {
    points_of(h, doc, id).iter().map(|p| p.0).fold(f64::INFINITY, f64::min)
}

fn options(
    h: &mut burnburn_presentation::HeadlessEditorHarness,
    doc: burnburn_types::DocumentId,
    o: PlaceOptionsInput,
) {
    h.send(UiIntent::Schikken(SchikkenIntent::SetPlaceOptions { document: doc, options: o }));
}

fn refusal(h: &burnburn_presentation::HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> Vec<SchikkenMessage> {
    view(h, doc)
        .messages
        .iter()
        .filter_map(|m| match m.key {
            burnburn_presentation::MessageKey::Schikken(s) => Some(s),
            _ => None,
        })
        .collect()
}

#[test]
fn dock_left_slides_the_other_objects_against_the_first_selected_in_one_undo_entry() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    assert!(enabled(&h, CommandKey::DockLeft));
    h.invoke(doc, CommandKey::DockLeft);
    assert!((min_x(&h, doc, ids[1]) - 10.0).abs() < 1e-9, "the second square sits against the first");
    assert!(min_x(&h, doc, ids[0]).abs() < 1e-9, "the reference stays");
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::Dock)));
    assert_eq!(selected(&h, doc), ids, "the selection stays");
    h.invoke(doc, CommandKey::Undo);
    assert!((min_x(&h, doc, ids[1]) - 30.0).abs() < 1e-9);
}

#[test]
fn the_four_directions_dock_towards_their_side() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(40.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::DockLeft);
    assert!((min_x(&h, doc, ids[1]) - 10.0).abs() < 1e-9);
    // Dock right: the second square (now touching) goes back out to the right of nothing — so use a
    // fresh pair where the reference lies on the right.
    let (mut h, doc, ids) = opened_with(vec![square(40.0, 0.0, 10.0), square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::DockRight);
    assert!((min_x(&h, doc, ids[1]) - 30.0).abs() < 1e-9, "docked right: its right edge meets x = 40");
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(0.0, 40.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::DockDown);
    let ys: Vec<f64> = points_of(&h, doc, ids[1]).iter().map(|p| p.1).collect();
    assert!((ys.iter().cloned().fold(f64::INFINITY, f64::min) - 10.0).abs() < 1e-9);
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 40.0, 10.0), square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::DockUp);
    let ys: Vec<f64> = points_of(&h, doc, ids[1]).iter().map(|p| p.1).collect();
    assert!((ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - 40.0).abs() < 1e-9);
}

#[test]
fn the_gap_option_leaves_clearance_and_the_reference_choice_decides_who_stays() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    options(&mut h, doc, PlaceOptionsInput { gap: "2,5 mm".to_owned(), ..PlaceOptionsInput::default() });
    h.invoke(doc, CommandKey::DockLeft);
    assert!((min_x(&h, doc, ids[1]) - 12.5).abs() < 1e-9, "2,5 mm clearance");
    h.invoke(doc, CommandKey::Undo);
    select(&mut h, doc, &ids); // an undo selects what it restored
    options(
        &mut h,
        doc,
        PlaceOptionsInput {
            reference: DockReference::LastSelected,
            gap: "0".to_owned(),
            ..PlaceOptionsInput::default()
        },
    );
    h.invoke(doc, CommandKey::DockRight);
    assert!((min_x(&h, doc, ids[1]) - 30.0).abs() < 1e-9, "the last selected stays");
    assert!((min_x(&h, doc, ids[0]) - 20.0).abs() < 1e-9, "the first one moves against it");
    assert_eq!(view(&h, doc).schikken.place.input.reference, DockReference::LastSelected);
}

#[test]
fn a_dock_with_nothing_ahead_changes_nothing_and_says_why() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    let before = view(&h, doc).revision;
    h.invoke(doc, CommandKey::DockRight); // the reference lies to the left of the other square
    assert_eq!(view(&h, doc).revision, before);
    assert_eq!(refusal(&h, doc), vec![SchikkenMessage::Dock(PlaceFailure::NoContact)]);
    // The next successful command clears the report.
    h.invoke(doc, CommandKey::DockLeft);
    assert!(refusal(&h, doc).is_empty());
}

#[test]
fn overlapping_objects_cannot_dock_and_the_refusal_names_that() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(5.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::DockLeft);
    assert_eq!(refusal(&h, doc), vec![SchikkenMessage::Dock(PlaceFailure::Overlapping)]);
}

#[test]
fn a_wrong_gap_keeps_the_old_options_and_the_dock_still_works_with_them() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    options(&mut h, doc, PlaceOptionsInput { gap: "3".to_owned(), ..PlaceOptionsInput::default() });
    options(&mut h, doc, PlaceOptionsInput { gap: "-1".to_owned(), ..PlaceOptionsInput::default() });
    let vm = view(&h, doc);
    assert_eq!(vm.schikken.place.error, Some(SchikkenMessage::InvalidGap));
    assert_eq!(vm.schikken.place.input.gap, "3", "the options in force are still the last valid ones");
    h.invoke(doc, CommandKey::DockLeft);
    assert!((min_x(&h, doc, ids[1]) - 13.0).abs() < 1e-9);
    options(&mut h, doc, PlaceOptionsInput { gap: "abc".to_owned(), ..PlaceOptionsInput::default() });
    assert_eq!(view(&h, doc).schikken.place.error, Some(SchikkenMessage::InvalidGap));
}

#[test]
fn the_outline_contact_enters_a_notch_that_the_bounding_box_would_not() {
    // A "C" shape open to the right, reference; a small square docks into it from the right.
    let c = burnburn_geometry::Path::polyline(
        &[
            burnburn_geometry::Point::new(0.0, 0.0),
            burnburn_geometry::Point::new(10.0, 0.0),
            burnburn_geometry::Point::new(10.0, 3.0),
            burnburn_geometry::Point::new(3.0, 3.0),
            burnburn_geometry::Point::new(3.0, 7.0),
            burnburn_geometry::Point::new(10.0, 7.0),
            burnburn_geometry::Point::new(10.0, 10.0),
            burnburn_geometry::Point::new(0.0, 10.0),
        ],
        true,
    )
    .unwrap();
    let (mut h, doc, ids) = opened_with(vec![c, square(30.0, 4.0, 2.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::DockLeft);
    assert!((min_x(&h, doc, ids[1]) - 10.0).abs() < 1e-9, "bounding boxes: against the box edge");
    h.invoke(doc, CommandKey::Undo);
    select(&mut h, doc, &ids);
    options(&mut h, doc, PlaceOptionsInput { contact: DockContactChoice::Outline, ..PlaceOptionsInput::default() });
    h.invoke(doc, CommandKey::DockLeft);
    assert!((min_x(&h, doc, ids[1]) - 3.0).abs() < 1e-9, "outline contact: the square enters the notch");
}

#[test]
fn move_together_closes_the_gaps_towards_the_chosen_end_in_one_entry() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(70.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    assert!(enabled(&h, CommandKey::MoveTogetherHorizontal));
    h.invoke(doc, CommandKey::MoveTogetherHorizontal);
    let xs: Vec<f64> = ids.iter().map(|id| min_x(&h, doc, *id)).collect();
    assert!(xs[0].abs() < 1e-9 && (xs[1] - 10.0).abs() < 1e-9 && (xs[2] - 20.0).abs() < 1e-9, "{xs:?}");
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::MoveTogether)));
    h.invoke(doc, CommandKey::Undo);
    select(&mut h, doc, &ids);
    options(
        &mut h,
        doc,
        PlaceOptionsInput { gap: "5".to_owned(), anchor: TogetherAnchorChoice::End, ..PlaceOptionsInput::default() },
    );
    h.invoke(doc, CommandKey::MoveTogetherHorizontal);
    let xs: Vec<f64> = ids.iter().map(|id| min_x(&h, doc, *id)).collect();
    assert!(
        (xs[2] - 70.0).abs() < 1e-9 && (xs[1] - 55.0).abs() < 1e-9 && (xs[0] - 40.0).abs() < 1e-9,
        "closing towards the right end with 5 mm left between them: {xs:?}"
    );
}

#[test]
fn move_together_vertically_uses_the_vertical_axis() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(0.0, 30.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::MoveTogetherVertical);
    let ys: Vec<f64> = points_of(&h, doc, ids[1]).iter().map(|p| p.1).collect();
    assert!((ys.iter().cloned().fold(f64::INFINITY, f64::min) - 10.0).abs() < 1e-9);
}

#[test]
fn the_commands_need_two_objects_and_stay_silent_without() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..1]);
    for key in [
        CommandKey::DockLeft,
        CommandKey::DockRight,
        CommandKey::DockUp,
        CommandKey::DockDown,
        CommandKey::MoveTogetherHorizontal,
        CommandKey::MoveTogetherVertical,
    ] {
        assert!(!enabled(&h, key), "{key:?} with one object");
        let before = view(&h, doc).revision;
        let update = h.invoke(doc, key);
        assert!(update.effects.is_empty());
        assert_eq!(view(&h, doc).revision, before);
    }
    select(&mut h, doc, &ids);
    assert!(enabled(&h, CommandKey::DockUp) && enabled(&h, CommandKey::MoveTogetherVertical));
}

#[test]
fn grouped_objects_dock_as_one_body_when_asked() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(40.0, 0.0, 10.0), square(55.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[1..]);
    h.invoke(doc, CommandKey::Group);
    select(&mut h, doc, &[ids[0], ids[1], ids[2]]);
    options(&mut h, doc, PlaceOptionsInput { by_group: true, ..PlaceOptionsInput::default() });
    h.invoke(doc, CommandKey::DockLeft);
    // The group moves as one: the gap between its members survives (10 → 25 apart from 0).
    assert!((min_x(&h, doc, ids[1]) - 10.0).abs() < 1e-9);
    assert!((min_x(&h, doc, ids[2]) - 25.0).abs() < 1e-9, "the member keeps its distance within the group");
}
