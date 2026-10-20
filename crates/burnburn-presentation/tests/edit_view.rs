//! W2-L1-bewerken: fit to page / selection, zoom steps, wireframe, and the lock guard of edits.
//! View commands are window state: no document revision, no undo entry.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;
mod edit_rig;

use burnburn_presentation::{CommandKey, FieldKey, HostEffect, MessageKey, UiIntent};
use common::{RUIDA, profile};
use edit_rig::{opened_with, select, square, view};

fn visible_mm(
    h: &burnburn_presentation::HeadlessEditorHarness,
    doc: burnburn_types::DocumentId,
) -> (f64, f64, f64, f64) {
    let vm = view(h, doc);
    let a = vm.camera.points_to_world(&vm.viewport, [0.0, 0.0]);
    let b = vm.camera.points_to_world(&vm.viewport, [vm.viewport.width_pt(), vm.viewport.height_pt()]);
    (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
}

#[test]
fn zoom_to_selection_frames_the_selection_with_a_margin_and_changes_no_document() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(500.0, 300.0, 40.0)]);
    select(&mut h, doc, &ids[1..]);
    let before = view(&h, doc);
    h.invoke(doc, CommandKey::ZoomSelection);
    let (x0, y0, x1, y1) = visible_mm(&h, doc);
    assert!(x0 < 500.0 && x1 > 540.0 && y0 < 300.0 && y1 > 340.0, "the whole square is visible: {x0} {y0} {x1} {y1}");
    assert!(x1 - x0 < 540.0 - 500.0 + 400.0, "and the view is much closer than before");
    let after = view(&h, doc);
    assert_eq!((after.revision, after.undo, after.dirty), (before.revision, before.undo, before.dirty));
    let centre = after.camera.center_mm();
    assert!((centre.x - 520.0).abs() < 1e-9 && (centre.y - 320.0).abs() < 1e-9);
}

#[test]
fn zoom_to_page_shows_everything_without_a_machine_and_the_bed_with_one() {
    let (mut h, doc, _) = opened_with(vec![square(0.0, 0.0, 10.0), square(100.0, 50.0, 10.0)]);
    h.invoke(doc, CommandKey::ZoomPage);
    let (x0, y0, x1, y1) = visible_mm(&h, doc);
    assert!(x0 <= 0.0 && y0 <= 0.0 && x1 >= 110.0 && y1 >= 60.0, "{x0} {y0} {x1} {y1}");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.invoke(doc, CommandKey::ZoomPage);
    let (x0, y0, x1, y1) = visible_mm(&h, doc);
    assert!(x0 <= 0.0 && y0 <= 0.0 && x1 >= 1500.0 && y1 >= 900.0, "the 1500 × 900 bed fits: {x0} {y0} {x1} {y1}");
}

#[test]
fn zoom_to_page_of_an_empty_document_without_a_machine_does_nothing() {
    let (mut h, doc, _) = opened_with(vec![]);
    let before = view(&h, doc).camera;
    h.invoke(doc, CommandKey::ZoomPage);
    assert_eq!(view(&h, doc).camera, before);
}

#[test]
fn zoom_in_and_out_are_inverse_steps_about_the_middle_of_the_view() {
    let (mut h, doc, _) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    let start = view(&h, doc).camera;
    h.invoke(doc, CommandKey::ZoomIn);
    let zoomed = view(&h, doc).camera;
    assert!(zoomed.points_per_mm() > start.points_per_mm());
    h.invoke(doc, CommandKey::ZoomOut);
    let back = view(&h, doc).camera;
    assert!((back.points_per_mm() - start.points_per_mm()).abs() < 1e-9);
    assert!((back.center_mm().x - start.center_mm().x).abs() < 1e-9);
}

#[test]
fn wireframe_is_a_checked_window_toggle() {
    let (mut h, doc, _) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    assert!(!view(&h, doc).wireframe);
    assert!(!h.view().commands[&CommandKey::ViewWireframe].checked);
    h.invoke(doc, CommandKey::ViewWireframe);
    assert!(view(&h, doc).wireframe && h.view().commands[&CommandKey::ViewWireframe].checked);
    assert!(!view(&h, doc).dirty, "a view setting is not a document change");
    h.invoke(doc, CommandKey::ViewWireframe);
    assert!(!view(&h, doc).wireframe);
}

#[test]
fn locked_objects_selected_from_the_list_refuse_edits_until_they_are_unlocked() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..1]);
    h.invoke(doc, CommandKey::LockSelected);
    let locked_at = view(&h, doc).revision;
    // Still selected: every edit that would move, resize or delete it is refused with a reason.
    let refused = |update: &burnburn_presentation::AppUpdate| {
        update.effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::ObjectsLocked, .. }))
    };
    assert!(refused(&h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "50".into() })));
    assert!(refused(&h.invoke(doc, CommandKey::Delete)));
    assert!(refused(&h.invoke(doc, CommandKey::Cut)), "a cut deletes");
    assert!(refused(&h.invoke(doc, CommandKey::FlipHorizontal)));
    assert_eq!(view(&h, doc).revision, locked_at, "nothing changed");
    // Copy and duplicate are reading the object, not editing it.
    assert!(!refused(&h.invoke(doc, CommandKey::Copy)));
    h.invoke(doc, CommandKey::UnlockSelected);
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "50".into() });
    assert_eq!(view(&h, doc).inspector.unwrap().field(FieldKey::X).unwrap().text, "50");
}
