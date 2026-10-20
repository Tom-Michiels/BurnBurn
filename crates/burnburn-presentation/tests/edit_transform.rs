//! ED02 (W2-L1-bewerken): the inspector's transform fields: percent scale, aspect lock, shear,
//! the 3×3 reference point and transforms of a multi-selection. Raw field text goes in, the
//! document gets one Transform (or SetDimension) per commit.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::UndoLabel;
use burnburn_presentation::{CommandKey, FieldKey, HeadlessEditorHarness, MessageKey, RefPoint, UiIntent};
use burnburn_types::DocumentId;
use edit_rig::{opened_with, points_of, select, square, view};

fn text(h: &HeadlessEditorHarness, doc: DocumentId, key: FieldKey) -> String {
    view(h, doc).inspector.expect("inspector").field(key).expect("field").text.clone()
}

fn commit(h: &mut HeadlessEditorHarness, doc: DocumentId, field: FieldKey, value: &str) {
    h.send(UiIntent::CommitField { document: doc, field, text: value.into() });
}

fn rect_path(w: f64, h: f64) -> burnburn_geometry::Path {
    burnburn_geometry::Path::polyline(
        &[
            burnburn_geometry::Point::new(0.0, 0.0),
            burnburn_geometry::Point::new(w, 0.0),
            burnburn_geometry::Point::new(w, h),
            burnburn_geometry::Point::new(0.0, h),
        ],
        true,
    )
    .unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_reference_point_defaults_to_the_bottom_left_corner_as_before() {
    let (mut h, doc, ids) = opened_with(vec![rect_path(40.0, 20.0)]);
    select(&mut h, doc, &ids);
    let vm = view(&h, doc);
    assert_eq!(vm.reference_point, RefPoint::BottomLeft);
    assert!(!vm.aspect_locked);
    assert_eq!((text(&h, doc, FieldKey::X), text(&h, doc, FieldKey::Y)), ("0".into(), "0".into()));
}

#[test]
fn x_and_y_show_and_move_the_chosen_reference_point() {
    let (mut h, doc, ids) = opened_with(vec![square(10.0, 20.0, 40.0)]);
    select(&mut h, doc, &ids);
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::Center });
    assert_eq!(view(&h, doc).reference_point, RefPoint::Center);
    assert_eq!((text(&h, doc, FieldKey::X), text(&h, doc, FieldKey::Y)), ("30".into(), "40".into()));
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::TopRight });
    assert_eq!((text(&h, doc, FieldKey::X), text(&h, doc, FieldKey::Y)), ("50".into(), "60".into()));
    commit(&mut h, doc, FieldKey::X, "100");
    commit(&mut h, doc, FieldKey::Y, "100");
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::BottomLeft });
    assert_eq!((text(&h, doc, FieldKey::X), text(&h, doc, FieldKey::Y)), ("60".into(), "60".into()));
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Transform));
}

#[test]
fn a_size_commit_keeps_the_reference_point_in_place() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::Center });
    commit(&mut h, doc, FieldKey::Width, "20");
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::BottomLeft });
    assert_eq!(
        (text(&h, doc, FieldKey::X), text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)),
        ("10".into(), "20".into(), "40".into())
    );
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::TopRight });
    commit(&mut h, doc, FieldKey::Height, "10");
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::BottomLeft });
    assert_eq!(text(&h, doc, FieldKey::Y), "30", "the top edge stayed at 40");
}

#[test]
fn the_default_reference_still_commits_set_dimension_with_its_label() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    commit(&mut h, doc, FieldKey::Width, "10");
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::SetDimension));
}

#[test]
fn aspect_lock_scales_both_axes_from_a_width_or_height_commit() {
    let (mut h, doc, ids) = opened_with(vec![rect_path(40.0, 20.0)]);
    select(&mut h, doc, &ids);
    h.send(UiIntent::SetAspectLock { document: doc, locked: true });
    assert!(view(&h, doc).aspect_locked);
    commit(&mut h, doc, FieldKey::Width, "20");
    assert_eq!((text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)), ("20".into(), "10".into()));
    commit(&mut h, doc, FieldKey::Height, "40");
    assert_eq!((text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)), ("80".into(), "40".into()));
    h.send(UiIntent::SetAspectLock { document: doc, locked: false });
    commit(&mut h, doc, FieldKey::Width, "10");
    assert_eq!((text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)), ("10".into(), "40".into()));
}

#[test]
fn percent_fields_scale_the_current_size_about_the_reference_point() {
    let (mut h, doc, ids) = opened_with(vec![rect_path(40.0, 20.0)]);
    select(&mut h, doc, &ids);
    assert_eq!((text(&h, doc, FieldKey::ScaleX), text(&h, doc, FieldKey::ScaleY)), ("100".into(), "100".into()));
    commit(&mut h, doc, FieldKey::ScaleX, "50");
    assert_eq!((text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)), ("20".into(), "20".into()));
    assert_eq!(text(&h, doc, FieldKey::ScaleX), "100", "the field returns to 100: it is an action, not a state");
    commit(&mut h, doc, FieldKey::ScaleY, "150");
    assert_eq!((text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)), ("20".into(), "30".into()));
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Transform));
    h.send(UiIntent::SetAspectLock { document: doc, locked: true });
    commit(&mut h, doc, FieldKey::ScaleX, "200");
    assert_eq!(
        (text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)),
        ("40".into(), "60".into()),
        "locked: both axes"
    );
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::TopRight });
    commit(&mut h, doc, FieldKey::ScaleY, "50");
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::BottomLeft });
    assert_eq!(
        (text(&h, doc, FieldKey::X), text(&h, doc, FieldKey::Y), text(&h, doc, FieldKey::Width)),
        ("20".into(), "30".into(), "20".into())
    );
}

#[test]
fn invalid_percent_text_is_a_field_error_and_sends_no_command() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    let before = view(&h, doc).revision;
    for bad in ["abc", "0", "-50", "", "1e999"] {
        commit(&mut h, doc, FieldKey::ScaleX, bad);
        let field = view(&h, doc).inspector.unwrap().field(FieldKey::ScaleX).unwrap().clone();
        assert!(matches!(field.error, Some(MessageKey::InvalidLength(_))), "{bad:?}: {field:?}");
        assert_eq!(view(&h, doc).revision, before, "{bad:?}");
    }
}

#[test]
fn shear_slants_about_the_reference_point_keeping_the_other_axis() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    commit(&mut h, doc, FieldKey::ShearX, "45");
    // x' = x + tan(45°)·(y − y_ref) with the reference at the bottom: the top edge moves 10 right.
    let pts = points_of(&h, doc, ids[0]);
    assert!(near(pts[0].0, 0.0) && near(pts[0].1, 0.0), "{pts:?}");
    assert!(near(pts[2].0, 20.0) && near(pts[2].1, 10.0), "{pts:?}");
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Transform));
    assert_eq!(text(&h, doc, FieldKey::ShearX), "0", "an action field returns to 0");
    h.invoke(doc, CommandKey::Undo);
    commit(&mut h, doc, FieldKey::ShearY, "45");
    let pts = points_of(&h, doc, ids[0]);
    assert!(near(pts[1].0, 10.0) && near(pts[1].1, 10.0), "y' = y + tan(45°)·(x − x_ref): {pts:?}");
}

#[test]
fn shear_about_the_centre_moves_the_halves_in_opposite_directions() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.send(UiIntent::SetReferencePoint { document: doc, point: RefPoint::Center });
    commit(&mut h, doc, FieldKey::ShearX, "45");
    let pts = points_of(&h, doc, ids[0]);
    assert!(near(pts[0].0, -5.0) && near(pts[2].0, 15.0), "{pts:?}");
    for bad in ["90", "-90", "abc"] {
        commit(&mut h, doc, FieldKey::ShearX, bad);
        assert!(view(&h, doc).inspector.unwrap().field(FieldKey::ShearX).unwrap().error.is_some(), "{bad}");
    }
}

#[test]
fn the_inspector_works_on_a_multi_selection_with_its_combined_bounds() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    assert!(view(&h, doc).inspector.is_some());
    assert_eq!((text(&h, doc, FieldKey::Width), text(&h, doc, FieldKey::Height)), ("40".into(), "10".into()));
    commit(&mut h, doc, FieldKey::Width, "80");
    assert_eq!(view(&h, doc).undo, Some(UndoLabel::Transform), "one entry for the whole selection");
    let a = points_of(&h, doc, ids[0]);
    let b = points_of(&h, doc, ids[1]);
    assert!(near(a[1].0, 20.0) && near(b[0].0, 60.0) && near(b[1].0, 80.0), "{a:?} {b:?}");
    assert!(near(a[2].1, 10.0), "the height is untouched: {a:?}");
    commit(&mut h, doc, FieldKey::X, "100");
    assert!(near(points_of(&h, doc, ids[0])[0].0, 100.0));
    h.invoke(doc, CommandKey::Undo);
    h.invoke(doc, CommandKey::Undo);
    assert!(near(points_of(&h, doc, ids[1])[1].0, 40.0));
}

#[test]
fn rotation_of_a_multi_selection_turns_about_the_combined_centre() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    commit(&mut h, doc, FieldKey::Rotation, "90");
    // Combined centre (20, 5): the right square swings up, the left square down.
    let b = points_of(&h, doc, ids[1]);
    assert!(b.iter().all(|p| p.1 > 5.0), "{b:?}");
    let a = points_of(&h, doc, ids[0]);
    assert!(a.iter().all(|p| p.1 < 5.0), "{a:?}");
}
