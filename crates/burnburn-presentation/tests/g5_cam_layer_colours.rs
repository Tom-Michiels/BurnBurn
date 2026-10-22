//! C01 (G5-cam): the colour of a layer is kept in the project file. Opened again, every layer has
//! the palette button it had; the colours are not production data (no dirty flag, no undo).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, SelectMode, TOOL_SLOT_T2, UiIntent};
use burnburn_types::DocumentId;

fn draw_on(h: &mut HeadlessEditorHarness, doc: DocumentId, x: f64, slot: u8) {
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (x, 10.0), (x + 20.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects.last().unwrap().id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot, add: false });
}

fn slots(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(String, u8)> {
    h.document(doc).unwrap().operations.iter().map(|o| (o.name.clone(), o.slot)).collect()
}

#[test]
fn layer_colours_come_back_after_save_and_open() {
    let mut first = HeadlessEditorHarness::new();
    let doc = first.new_document().unwrap();
    // Colours that are not the lowest free ones, in an order that differs from the list order.
    draw_on(&mut first, doc, 10.0, 17);
    draw_on(&mut first, doc, 50.0, 4);
    draw_on(&mut first, doc, 90.0, TOOL_SLOT_T2);
    let before = slots(&first, doc);
    assert_eq!(before.iter().map(|(_, s)| *s).collect::<Vec<_>>(), vec![17, 4, TOOL_SLOT_T2]);
    first.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/kleuren.burnburn");
    first.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    first.run_tasks();

    let mut second = HeadlessEditorHarness::with_fs(first.shared_fs());
    second.send(UiIntent::Open { path });
    let doc = second
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    assert_eq!(slots(&second, doc), before, "every layer has the colour it had");
    assert!(!second.document(doc).unwrap().dirty, "reading the colours is not an edit");
}
