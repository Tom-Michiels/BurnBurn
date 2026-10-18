//! IM08 end test, shared part (headless, runs on any OS): the Windows host's message route —
//! raw Win32 messages through `adapter::on_message` — drives the same AppController to the same
//! document as the headless harness's direct route. The native Win32/DX12 parts are not exercised
//! here and stay unconfirmed until a Windows machine runs them.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::{Path, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_presentation::{AppUpdate, CommandKey, FieldKey, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::adapter::{
    Control, MenuItem, MessageContext, Reaction, command_id, mk, msg, notify, on_message, on_save_dialog, vk,
};

const DPI: u32 = 144; // 150 %: logical points × 1.5 = pixels

fn lparam(x: i32, y: i32) -> isize {
    ((x as u16 as u32) | ((y as u16 as u32) << 16)) as i32 as isize
}

/// Sends what the Windows window procedure would send for one message; returns the update.
fn win(h: &mut HeadlessEditorHarness, ctx: &MessageContext<'_>, m: u32, wparam: usize, lp: isize) -> Option<AppUpdate> {
    match on_message(ctx, m, wparam, lp, &h.view()) {
        Reaction::Intent(i) => Some(h.send(i)),
        _ => None,
    }
}

fn ctx<'a>(document: DocumentId, control: Control, text: &'a str) -> MessageContext<'a> {
    MessageContext { document, control, dpi: DPI, text, selected_rows: &[], alt: false, cursor_px: [0, 0], time_ms: 0 }
}

fn menu(h: &mut HeadlessEditorHarness, document: DocumentId, item: MenuItem) -> Option<AppUpdate> {
    let c = ctx(document, Control::Frame, "");
    win(h, &c, msg::WM_COMMAND, usize::from(command_id(item)) | (usize::from(notify::ACCELERATOR) << 16), 0)
}

/// World mm → canvas client pixels through the window's own camera (what the user clicks).
fn px(h: &HeadlessEditorHarness, document: DocumentId, mm: (f64, f64)) -> isize {
    let vm = h.document(document).unwrap();
    let [x, y] = vm.camera.world_to_points(&vm.viewport, burnburn_geometry::Point::new(mm.0, mm.1));
    let scale = f64::from(DPI) / 96.0;
    let (xp, yp) = (x * scale, y * scale);
    assert_eq!((xp.fract(), yp.fract()), (0.0, 0.0), "the scenario uses whole pixels");
    lparam(xp as i32, yp as i32)
}

#[test]
fn windows_host_replays_dimension_scenario_with_same_document_result() {
    let path = PathBuf::from("plaat.burnburn");

    // Reference: the headless harness's direct route.
    let mut reference = HeadlessEditorHarness::new();
    let rdoc = reference.new_document().unwrap();
    reference.invoke(rdoc, CommandKey::ToolRectangle);
    reference.drag(rdoc, (0.0, 0.0), (120.0, 80.0));
    reference.send(UiIntent::CommitField { document: rdoc, field: FieldKey::Width, text: "150".into() });
    reference.send(UiIntent::Save { document: rdoc, path: Some(path.clone()) });
    reference.run_tasks();

    // Windows route: the launch document, tool menu, WM_SIZE at 150 %, a mouse drag, Enter + focus
    // loss in the width edit, Ctrl+S answered by IFileSaveDialog.
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    win(&mut h, &ctx(doc, Control::Canvas, ""), msg::WM_SIZE, 0, lparam(1200, 900)).unwrap();
    assert_eq!(h.document(doc).unwrap().viewport.backing_scale(), 1.5);
    menu(&mut h, doc, MenuItem::Command(CommandKey::ToolRectangle)).unwrap();
    let canvas = ctx(doc, Control::Canvas, "");
    let (from, mid, to) = (px(&h, doc, (0.0, 0.0)), px(&h, doc, (60.0, 40.0)), px(&h, doc, (120.0, 80.0)));
    win(&mut h, &canvas, msg::WM_LBUTTONDOWN, mk::LBUTTON, from).unwrap();
    win(&mut h, &canvas, msg::WM_MOUSEMOVE, mk::LBUTTON, mid).unwrap();
    win(&mut h, &canvas, msg::WM_MOUSEMOVE, mk::LBUTTON, to).unwrap();
    win(&mut h, &canvas, msg::WM_LBUTTONUP, 0, to).unwrap();
    let before = h.document(doc).unwrap().revision;
    win(&mut h, &ctx(doc, Control::Field(FieldKey::Width), "150"), msg::WM_KEYDOWN, vk::RETURN, 0).unwrap();
    let after_enter = h.document(doc).unwrap().revision;
    assert!(after_enter > before, "Enter committed");
    let kill_focus = usize::from(notify::EN_KILLFOCUS) << 16;
    assert!(win(&mut h, &ctx(doc, Control::Field(FieldKey::Width), "150"), msg::WM_COMMAND, kill_focus, 0).is_none());
    assert_eq!(h.document(doc).unwrap().revision, after_enter, "focus loss after Enter: no second command");
    let save = menu(&mut h, doc, MenuItem::Command(CommandKey::Save)).unwrap();
    assert!(save.effects.iter().any(|e| matches!(e, HostEffect::ShowSavePanel { .. })), "untitled: asks for a path");
    match on_save_dialog(doc, Some(path.clone()), &h.view()) {
        Reaction::Intent(i) => h.send(i),
        other => panic!("{other:?}"),
    };
    h.run_tasks();

    // Same document: objects, inspector values, undo label, saved state — and the same file.
    let (a, b) = (reference.document(rdoc).unwrap(), h.document(doc).unwrap());
    assert_eq!(
        a.objects.iter().map(|o| &o.label).collect::<Vec<_>>(),
        b.objects.iter().map(|o| &o.label).collect::<Vec<_>>()
    );
    let fields = |d: &burnburn_presentation::DocumentViewModel| {
        d.inspector.as_ref().map(|i| i.fields.iter().map(|f| (f.key, f.text.clone())).collect::<Vec<_>>())
    };
    assert_eq!(fields(&a), fields(&b));
    assert_eq!(b.inspector.as_ref().and_then(|i| i.field(FieldKey::Width)).map(|f| f.text.as_str()), Some("150"));
    assert_eq!(b.inspector.as_ref().and_then(|i| i.field(FieldKey::Height)).map(|f| f.text.as_str()), Some("80"));
    assert_eq!((a.undo, a.dirty, &a.title), (b.undo, b.dirty, &b.title));
    let file = |h: &HeadlessEditorHarness| h.fs().read(Path::new("plaat.burnburn"), 1 << 20).unwrap();
    assert_eq!(file(&reference), file(&h), "byte-identical project file");
}
