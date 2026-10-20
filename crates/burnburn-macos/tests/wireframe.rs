//! W2-L1 (bewerken): Beeld ▸ Draadmodel. The canvas draws the view model's wireframe flag with the
//! outline-only theme; toggling it changes what is drawn but not the document.

#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_macos::appkit::canvas_packet;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness};
use burnburn_render::{Primitive, RenderPacket};

fn fills(packet: &RenderPacket) -> usize {
    packet.primitives.iter().filter(|p| matches!(p, Primitive::Fill { .. })).count()
}

#[test]
fn the_wireframe_flag_of_the_view_model_removes_the_fills_from_what_the_canvas_draws() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (60.0, 40.0));
    let revision = h.document(doc).unwrap().revision;

    let solid = canvas_packet(&h.scene(doc).unwrap(), h.document(doc).unwrap().wireframe);
    assert!(fills(&solid) > 0, "the normal view fills the rectangle");

    let update = h.invoke(doc, CommandKey::ViewWireframe);
    assert!(update.scenes_changed.contains(&doc), "toggling asks for a redraw");
    let vm = h.document(doc).unwrap();
    assert!(vm.wireframe);
    let wire = canvas_packet(&h.scene(doc).unwrap(), vm.wireframe);
    assert_eq!(fills(&wire), 0, "wireframe draws outlines only");
    assert!(!wire.primitives.is_empty(), "the outlines are still drawn");
    assert_eq!(vm.revision, revision, "a view setting is not a document change");

    h.invoke(doc, CommandKey::ViewWireframe);
    assert!(fills(&canvas_packet(&h.scene(doc).unwrap(), h.document(doc).unwrap().wireframe)) > 0);
}
