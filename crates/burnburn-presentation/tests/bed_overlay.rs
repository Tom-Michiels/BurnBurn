//! LF2: the canvas scene carries the machine's work area as `Overlay::Bed`, beneath the grid, so the
//! bed follows the machine profile (size and origin corner).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_editor::{BedOrigin, Overlay};
use burnburn_presentation::HeadlessEditorHarness;
use burnburn_test_support::host::rig::rig;

#[test]
fn the_scene_has_the_bed_of_the_machine_profile_beneath_the_grid() {
    // The harness machine is the 1500 x 900 mm research bed with its origin bottom-left.
    let r = rig();
    let scene = r.h.scene(r.doc).expect("scene");
    let beds: Vec<_> = scene.overlays.iter().filter(|o| matches!(o, Overlay::Bed { .. })).collect();
    assert_eq!(
        beds,
        vec![&Overlay::Bed { width_mm: 1500.0, height_mm: 900.0, origin: BedOrigin::BottomLeft }],
        "exactly one bed, from the machine profile"
    );
    let bed_at = scene.overlays.iter().position(|o| matches!(o, Overlay::Bed { .. })).unwrap();
    let grid_at = scene.overlays.iter().position(|o| matches!(o, Overlay::Grid { .. })).unwrap();
    assert!(bed_at < grid_at, "the bed lies beneath the grid");
}

#[test]
fn without_a_machine_there_is_no_bed() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    let scene = h.scene(doc).expect("scene");
    assert!(!scene.overlays.iter().any(|o| matches!(o, Overlay::Bed { .. })), "no machine, no work area");
    assert!(scene.overlays.iter().any(|o| matches!(o, Overlay::Grid { .. })), "the grid still shows");
}
