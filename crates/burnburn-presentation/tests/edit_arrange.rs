//! E03/E04/X02 (W2-L1-bewerken): draw order, groups, flips and rotation, lock/hide and the
//! selection commands. Each change is one undo entry with its own label.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::{ArrangeLabel, EditLabel, ObjectKind, UndoLabel};
use burnburn_editor::Modifiers;
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_presentation::{CommandKey, FieldKey, MessageKey, UiIntent};
use burnburn_types::GeometryId;
use edit_rig::{draw_order, enabled, opened_with, opened_with_kinds, points_of, select, selected, square, view};

#[test]
fn push_commands_change_the_draw_order_only_and_each_is_one_undo_entry() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(5.0, 5.0, 10.0), square(10.0, 10.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::PushToFront);
    assert_eq!(draw_order(&h, doc), vec![ids[1], ids[2], ids[0]]);
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Arrange(ArrangeLabel::DrawOrder)));
    assert_eq!(selected(&h, doc), vec![ids[0]], "the selection stays");
    h.invoke(doc, CommandKey::PushBackward);
    assert_eq!(draw_order(&h, doc), vec![ids[1], ids[0], ids[2]]);
    h.invoke(doc, CommandKey::PushToBack);
    assert_eq!(draw_order(&h, doc), ids);
    h.invoke(doc, CommandKey::PushForward);
    assert_eq!(draw_order(&h, doc), vec![ids[1], ids[0], ids[2]]);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(draw_order(&h, doc), ids);
    // The cut order (operations) is a different thing and stays untouched.
    assert!(view(&h, doc).operations.is_empty());
}

#[test]
fn a_push_that_changes_nothing_is_silent_and_makes_no_history() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(5.0, 5.0, 10.0)]);
    select(&mut h, doc, &[ids[1]]);
    let before = view(&h, doc).revision;
    let update = h.invoke(doc, CommandKey::PushToFront);
    assert!(update.effects.is_empty(), "no error dialog for a harmless no-op: {:?}", update.effects);
    assert_eq!(view(&h, doc).revision, before);
}

#[test]
fn the_click_picks_what_is_drawn_on_top_after_a_push() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(0.0, 0.0, 10.0)]);
    h.click(doc, (5.0, 5.0), Modifiers::default());
    assert_eq!(selected(&h, doc), vec![ids[1]], "the later object is on top");
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::PushToFront);
    select(&mut h, doc, &[]);
    h.click(doc, (5.0, 5.0), Modifiers::default());
    assert_eq!(selected(&h, doc), vec![ids[0]]);
}

#[test]
fn group_and_ungroup_are_single_entries_and_ungroup_takes_one_level_off() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(60.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::Group);
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Group)));
    assert_eq!(vm.objects[0].groups.len(), 1);
    assert_eq!(vm.objects[0].groups, vm.objects[1].groups);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::Group);
    assert_eq!(view(&h, doc).objects[0].groups.len(), 2, "grouping a group nests");
    h.invoke(doc, CommandKey::Ungroup);
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::Ungroup)));
    assert_eq!(vm.objects[0].groups.len(), 1, "one level comes off");
    assert!(vm.objects[2].groups.is_empty());
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).objects[0].groups.len(), 2);
}

#[test]
fn group_needs_two_objects_and_ungroup_a_grouped_one() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..1]);
    assert!(!enabled(&h, CommandKey::Group) && !enabled(&h, CommandKey::Ungroup));
    select(&mut h, doc, &ids);
    assert!(enabled(&h, CommandKey::Group) && !enabled(&h, CommandKey::Ungroup));
    h.invoke(doc, CommandKey::Group);
    assert!(enabled(&h, CommandKey::Ungroup));
}

#[test]
fn clicking_a_grouped_object_selects_its_whole_group() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(60.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::Group);
    h.escape(doc);
    select(&mut h, doc, &[]);
    h.click(doc, (5.0, 5.0), Modifiers::default());
    assert_eq!(selected(&h, doc), vec![ids[0], ids[1]]);
    select(&mut h, doc, &[]);
    h.click(doc, (65.0, 5.0), Modifiers::default());
    assert_eq!(selected(&h, doc), vec![ids[2]]);
}

fn compound() -> Path {
    let sq = |x: f64| square(x, 0.0, 4.0).subpaths()[0].clone();
    Path::from_subpaths(vec![sq(0.0), sq(10.0), sq(20.0)])
}

#[test]
fn break_apart_makes_one_object_per_subpath_in_one_entry() {
    let (mut h, doc, ids) = opened_with(vec![compound(), square(50.0, 0.0, 5.0)]);
    select(&mut h, doc, &[ids[0]]);
    assert!(enabled(&h, CommandKey::BreakApart));
    h.invoke(doc, CommandKey::BreakApart);
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 4);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::BreakApart)));
    assert_eq!(vm.selection.len(), 3, "the pieces are selected");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).objects.len(), 2);
    select(&mut h, doc, &[ids[1]]);
    assert!(!enabled(&h, CommandKey::BreakApart), "a single outline has nothing to break apart");
}

#[test]
fn auto_group_groups_holes_with_their_outline() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 20.0), square(5.0, 5.0, 4.0), square(50.0, 0.0, 5.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::AutoGroup);
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::AutoGroup)));
    assert_eq!(vm.objects[0].groups, vm.objects[1].groups);
    assert_eq!(vm.objects[0].groups.len(), 1);
    assert!(vm.objects[2].groups.is_empty());
}

fn triangle() -> Path {
    PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(20.0, 0.0))
        .line_to(Point::new(0.0, 10.0))
        .close()
        .build()
        .unwrap()
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn flips_mirror_about_the_selection_centre_and_keep_the_bounds() {
    let (mut h, doc, ids) = opened_with(vec![triangle()]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::FlipHorizontal);
    let pts = points_of(&h, doc, ids[0]);
    assert!(near(pts[0], (20.0, 0.0)) && near(pts[1], (0.0, 0.0)) && near(pts[2], (20.0, 10.0)), "{pts:?}");
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Transform));
    h.invoke(doc, CommandKey::FlipVertical);
    let pts = points_of(&h, doc, ids[0]);
    assert!(near(pts[0], (20.0, 10.0)) && near(pts[1], (0.0, 10.0)) && near(pts[2], (20.0, 0.0)), "{pts:?}");
    assert_eq!(view(&h, doc).inspector.unwrap().field(FieldKey::Width).unwrap().text, "20");
}

#[test]
fn rotate_90_turns_about_the_centre_and_swaps_width_and_height() {
    let (mut h, doc, ids) = opened_with(vec![triangle()]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::Rotate90Cw);
    let fields = |h: &burnburn_presentation::HeadlessEditorHarness| {
        let i = view(h, doc).inspector.unwrap();
        (i.field(FieldKey::X).unwrap().text.clone(), i.field(FieldKey::Y).unwrap().text.clone(), i.field(FieldKey::Width).unwrap().text.clone(), i.field(FieldKey::Height).unwrap().text.clone())
    };
    // Centre (10, 5); after a quarter turn the 20×10 box is 10×20 about the same centre.
    assert_eq!(fields(&h), ("5".into(), "-5".into(), "10".into(), "20".into()));
    // Clockwise: the corner at the origin (bottom-left, y up) goes to the top-left.
    let pts = points_of(&h, doc, ids[0]);
    assert!(near(pts[0], (5.0, 15.0)), "{pts:?}");
    h.invoke(doc, CommandKey::Rotate90Ccw);
    assert_eq!(fields(&h), ("0".into(), "0".into(), "20".into(), "10".into()));
    assert!(near(points_of(&h, doc, ids[0])[0], (0.0, 0.0)));
}

#[test]
fn locked_objects_cannot_be_picked_selected_all_or_banded_but_stay_in_the_tree() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::LockSelected);
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::Lock)));
    assert!(vm.objects[0].locked && !vm.objects[1].locked);
    select(&mut h, doc, &[]);
    h.click(doc, (5.0, 5.0), Modifiers::default());
    assert!(selected(&h, doc).is_empty(), "a click on a locked object picks nothing");
    h.invoke(doc, CommandKey::SelectAll);
    assert_eq!(selected(&h, doc), vec![ids[1]]);
    h.drag(doc, (-5.0, -5.0), (50.0, 20.0));
    assert_eq!(selected(&h, doc), vec![ids[1]], "a rubber band skips it too");
    // The object list can still select it, and Unlock works on that selection.
    select(&mut h, doc, &[ids[0]]);
    assert!(enabled(&h, CommandKey::UnlockSelected));
    h.invoke(doc, CommandKey::UnlockSelected);
    assert!(!view(&h, doc).objects[0].locked);
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Arrange(ArrangeLabel::Unlock)));
}

#[test]
fn unlock_all_unlocks_everything_even_without_a_selection() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::LockSelected);
    assert!(enabled(&h, CommandKey::UnlockAll));
    h.invoke(doc, CommandKey::UnlockAll);
    assert!(view(&h, doc).objects.iter().all(|o| !o.locked));
    assert!(!enabled(&h, CommandKey::UnlockAll));
}

#[test]
fn hidden_objects_leave_the_canvas_and_the_selection_but_stay_in_the_document() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::HideSelected);
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::Hide)));
    assert!(!vm.objects[0].visible && vm.objects[1].visible);
    assert!(vm.selection.is_empty(), "hidden objects are deselected");
    assert_eq!(draw_order(&h, doc), vec![ids[1]]);
    assert_eq!(vm.objects.len(), 2);
    assert!(enabled(&h, CommandKey::ShowAll));
    h.invoke(doc, CommandKey::ShowAll);
    assert_eq!(draw_order(&h, doc), ids);
    assert!(!enabled(&h, CommandKey::ShowAll));
}

#[test]
fn invert_selection_flips_between_selected_and_not_selected() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(60.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..1]);
    h.invoke(doc, CommandKey::InvertSelection);
    assert_eq!(selected(&h, doc), vec![ids[1], ids[2]]);
    let revision = view(&h, doc).revision;
    assert_eq!(revision, view(&h, doc).revision, "selection is window state, never a document change");
    assert_eq!(view(&h, doc).undo, None);
}

fn open_line() -> Path {
    Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap()
}

#[test]
fn select_open_shapes_picks_only_unclosed_paths() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), open_line(), open_line()]);
    h.invoke(doc, CommandKey::SelectOpenShapes);
    assert_eq!(selected(&h, doc), vec![ids[1], ids[2]]);
}

#[test]
fn select_circles_picks_circle_objects() {
    let circle = Path::circle(Point::new(50.0, 50.0), 5.0).unwrap();
    let (mut h, doc, ids) = opened_with_kinds(vec![(square(0.0, 0.0, 10.0), ObjectKind::Rectangle), (circle, ObjectKind::Circle)]);
    h.invoke(doc, CommandKey::SelectCircles);
    assert_eq!(selected(&h, doc), vec![ids[1]]);
}

#[test]
fn select_contained_picks_what_lies_inside_the_selection() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0), square(5.0, 5.0, 4.0), square(15.0, 15.0, 4.0), square(100.0, 0.0, 4.0)]);
    select(&mut h, doc, &ids[..1]);
    h.invoke(doc, CommandKey::SelectContained);
    assert_eq!(selected(&h, doc), vec![ids[1], ids[2]]);
}

#[test]
fn select_smaller_picks_shapes_smaller_than_the_first_selected() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 20.0), square(30.0, 0.0, 5.0), square(50.0, 0.0, 30.0), square(100.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..1]);
    h.invoke(doc, CommandKey::SelectSmaller);
    assert_eq!(selected(&h, doc), vec![ids[1], ids[3]]);
}

#[test]
fn select_in_layer_picks_every_shape_that_shares_a_layer_with_the_selection() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(60.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[0], ids[1]]);
    h.send(UiIntent::AssignLayer { document: doc, slot: 2, add: false });
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::SelectInLayer);
    assert_eq!(selected(&h, doc), vec![ids[0], ids[1]]);
}

#[test]
fn a_selection_command_that_matches_nothing_says_so() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::SelectContained);
    assert!(view(&h, doc).messages.iter().any(|m| m.key == MessageKey::NothingFound));
}

#[test]
fn edit_commands_follow_the_selection_in_the_enablement() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(60.0, 0.0, 10.0)]);
    let needs_selection = [
        CommandKey::Cut,
        CommandKey::Copy,
        CommandKey::Duplicate,
        CommandKey::PushForward,
        CommandKey::PushToBack,
        CommandKey::FlipHorizontal,
        CommandKey::Rotate90Cw,
        CommandKey::LockSelected,
        CommandKey::HideSelected,
        CommandKey::ZoomSelection,
    ];
    for key in needs_selection {
        assert!(!enabled(&h, key), "{key:?} without a selection");
    }
    select(&mut h, doc, &ids[..1]);
    for key in needs_selection {
        assert!(enabled(&h, key), "{key:?} with a selection");
    }
    assert!(enabled(&h, CommandKey::Paste) && enabled(&h, CommandKey::ZoomPage) && enabled(&h, CommandKey::InvertSelection));
    let _: Vec<GeometryId> = ids;
}
