//! LFB-rest: a new or opened document frames the machine bed, and "Zoom naar bed" does it on demand.
//! The camera is window state: no document revision, no undo entry.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;
mod edit_rig;

use burnburn_editor::Viewport;
use burnburn_presentation::{CameraIntent, CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_types::DocumentId;
use common::{RUIDA, profile};
use edit_rig::{square, view};

fn visible_mm(h: &HeadlessEditorHarness, doc: DocumentId) -> (f64, f64, f64, f64) {
    let vm = view(h, doc);
    let a = vm.camera.points_to_world(&vm.viewport, [0.0, 0.0]);
    let b = vm.camera.points_to_world(&vm.viewport, [vm.viewport.width_pt(), vm.viewport.height_pt()]);
    (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
}

fn with_machine() -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::new();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h
}

fn assert_bed_framed(h: &HeadlessEditorHarness, doc: DocumentId) {
    let (x0, y0, x1, y1) = visible_mm(h, doc);
    assert!(
        x0 <= 0.0 && y0 <= 0.0 && x1 >= 1500.0 && y1 >= 900.0,
        "the 1500 x 900 bed is visible: {x0} {y0} {x1} {y1}"
    );
    // With a margin, but not zoomed far out: the bed fills most of the limiting axis.
    let fill = (1500.0 / (x1 - x0)).max(900.0 / (y1 - y0));
    assert!(fill > 0.7 && fill < 1.0, "bed fill {fill}");
}

#[test]
fn a_new_document_frames_the_bed_with_a_margin() {
    let mut h = with_machine();
    let doc = h.new_document().unwrap();
    assert_bed_framed(&h, doc);
}

#[test]
fn the_first_real_viewport_reframes_the_bed_but_a_later_resize_keeps_the_users_view() {
    let mut h = with_machine();
    let doc = h.new_document().unwrap();
    let vp = Viewport::new(1600.0, 700.0, 2.0).unwrap();
    h.send(UiIntent::Camera { document: doc, intent: CameraIntent::Resized { viewport: vp } });
    assert_bed_framed(&h, doc);
    // The user pans; a later resize no longer reframes.
    h.send(UiIntent::Camera { document: doc, intent: CameraIntent::Scroll { delta_pt: [50.0, 20.0] } });
    let panned = view(&h, doc).camera;
    let vp2 = Viewport::new(1200.0, 600.0, 2.0).unwrap();
    h.send(UiIntent::Camera { document: doc, intent: CameraIntent::Resized { viewport: vp2 } });
    assert_eq!(view(&h, doc).camera, panned);
}

#[test]
fn zoom_to_bed_command_reframes_the_bed_after_the_user_moved_away() {
    let mut h = with_machine();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Camera { document: doc, intent: CameraIntent::Scroll { delta_pt: [900.0, -500.0] } });
    h.invoke(doc, CommandKey::ZoomBed);
    assert_bed_framed(&h, doc);
    let vm = view(&h, doc);
    assert!(!vm.dirty, "view state is not a document change");
}

#[test]
fn zoom_to_bed_without_a_machine_changes_nothing() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let before = view(&h, doc).camera;
    h.invoke(doc, CommandKey::ZoomBed);
    assert_eq!(view(&h, doc).camera, before);
}

#[test]
fn an_opened_document_frames_the_bed_not_the_content() {
    let mut h = with_machine();
    let doc = edit_rig::open_in(&mut h, vec![square(0.0, 0.0, 10.0)]);
    assert_bed_framed(&h, doc);
}

#[test]
fn without_a_machine_an_opened_document_keeps_the_default_camera() {
    // Regression: framing the content here made pointer hit tolerances (mm per point) depend on the
    // drawing size and broke the fillet tool tests.
    let (mut h, doc, _) = edit_rig::opened_with(vec![square(0.0, 0.0, 20.0)]);
    let default = view(&h, doc).camera;
    assert!((default.points_per_mm() - 2.0).abs() < 1e-12, "{}", default.points_per_mm());
    assert_eq!((default.center_mm().x, default.center_mm().y), (0.0, 0.0));
    let vp = Viewport::new(1600.0, 700.0, 2.0).unwrap();
    h.send(UiIntent::Camera { document: doc, intent: CameraIntent::Resized { viewport: vp } });
    assert_eq!(view(&h, doc).camera, default);
}
