//! Win32 message decoding and translation, headless (runs on macOS/Linux too). Message numbers
//! and bit layouts are the documented Win32 ABI; the view model comes from the real controller.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::PathBuf;

use burnburn_editor::{CanvasKey, Modifiers, PointerButton, PointerPhase};
use burnburn_presentation::{
    CameraIntent, CloseChoice, CommandKey, DialogKind, FieldKey, HeadlessEditorHarness, SelectMode, UiIntent,
};
use burnburn_types::DocumentId;
use burnburn_windows::adapter::{
    Control, MenuItem, MessageContext, Reaction, accelerators, button, command_id, field_text, menu_enabled, menu_item,
    menu_items, mk, msg, notify, object_labels, on_close_box, on_message, on_open_dialog, on_save_dialog, vk,
    window_title,
};

fn one_rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    (h, doc)
}

fn ctx(document: DocumentId, control: Control, dpi: u32) -> MessageContext<'static> {
    MessageContext {
        document,
        control,
        dpi,
        text: "",
        selected_rows: &[],
        alt: false,
        cursor_px: [0, 0],
        time_ms: 1500,
    }
}

/// `MAKELPARAM(x, y)` with 16-bit signed coordinates, as Win32 packs them.
fn lparam(x: i16, y: i16) -> isize {
    ((x as u16 as u32) | ((y as u16 as u32) << 16)) as i32 as isize
}

fn wm_command(id: u16, code: u16) -> usize {
    (id as usize) | ((code as usize) << 16)
}

fn intent(r: Reaction) -> UiIntent {
    match r {
        Reaction::Intent(i) => i,
        other => panic!("expected an intent, got {other:?}"),
    }
}

#[test]
fn menu_ids_round_trip_and_cover_every_command() {
    let items = menu_items();
    assert!(items.contains(&MenuItem::Open));
    for key in CommandKey::ALL {
        assert!(items.contains(&MenuItem::Command(*key)), "{key:?}");
    }
    let mut ids: Vec<u16> = items.iter().map(|i| command_id(*i)).collect();
    for item in &items {
        assert_eq!(menu_item(command_id(*item)), Some(*item));
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), items.len());
    assert!(ids.iter().all(|id| *id >= 100), "low ids are reserved for IDOK/IDCANCEL and controls");
    assert_eq!(menu_item(0), None);
}

/// Ctrl is the shortcut modifier; Ctrl+Y and Ctrl+Shift+Z both redo (presentation-api.md).
#[test]
fn accelerators_use_ctrl_and_match_the_platform_table() {
    let acc = accelerators();
    let find = |key: u8, ctrl: bool, shift: bool| {
        acc.iter().find(|a| a.key == u16::from(key) && a.ctrl == ctrl && a.shift == shift).and_then(|a| menu_item(a.id))
    };
    let c = MenuItem::Command;
    assert_eq!(find(b'N', true, false), Some(c(CommandKey::New)));
    assert_eq!(find(b'O', true, false), Some(MenuItem::Open));
    assert_eq!(find(b'S', true, false), Some(c(CommandKey::Save)));
    assert_eq!(find(b'S', true, true), Some(c(CommandKey::SaveAs)));
    assert_eq!(find(b'W', true, false), Some(c(CommandKey::Close)));
    assert_eq!(find(b'Z', true, false), Some(c(CommandKey::Undo)));
    assert_eq!(find(b'Y', true, false), Some(c(CommandKey::Redo)));
    assert_eq!(find(b'Z', true, true), Some(c(CommandKey::Redo)));
    assert!(acc.iter().all(|a| a.ctrl), "no plain-key accelerators: they would steal typing from the edit controls");
}

#[test]
fn wm_command_from_menu_or_accelerator_invokes_the_command() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let frame = ctx(doc, Control::Frame, 96);
    for code in [notify::MENU, notify::ACCELERATOR] {
        let r = on_message(
            &frame,
            msg::WM_COMMAND,
            wm_command(command_id(MenuItem::Command(CommandKey::Undo)), code),
            0,
            &vm,
        );
        assert_eq!(r, Reaction::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::Undo }));
    }
    let open = on_message(&frame, msg::WM_COMMAND, wm_command(command_id(MenuItem::Open), notify::MENU), 0, &vm);
    assert_eq!(open, Reaction::ShowOpenDialog);
    assert_eq!(on_message(&frame, msg::WM_COMMAND, wm_command(9, notify::MENU), 0, &vm), Reaction::PassOn);
}

/// Enter in the edit control (subclassed `WM_KEYDOWN VK_RETURN`) and focus loss (`EN_KILLFOCUS`)
/// both commit raw text; the second one with unchanged text sends nothing.
#[test]
fn edit_enter_then_focus_loss_commits_raw_text_once() {
    let (mut h, doc) = one_rectangle();
    let mut c = ctx(doc, Control::Field(FieldKey::Width), 96);
    c.text = " 150,0 mm";
    let first = on_message(&c, msg::WM_KEYDOWN, vk::RETURN, 0, &h.view());
    assert_eq!(
        first,
        Reaction::Intent(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: " 150,0 mm".into() })
    );
    h.send(intent(first));
    assert_eq!(field_text(&h.view(), doc, FieldKey::Width).as_deref(), Some("150"));
    c.text = "150";
    let lose_focus = on_message(&c, msg::WM_COMMAND, wm_command(1001, notify::EN_KILLFOCUS), 0x1234, &h.view());
    assert_eq!(lose_focus, Reaction::Nothing, "unchanged text is not a commit");
    // Other keys in the edit control are the control's own business.
    assert_eq!(on_message(&c, msg::WM_KEYDOWN, 0x41, 0, &h.view()), Reaction::PassOn);
}

#[test]
fn list_selection_change_selects_unless_it_matches_the_view_model() {
    let (mut h, doc) = one_rectangle();
    h.drag(doc, (200.0, 0.0), (250.0, 30.0));
    let vm = h.view();
    assert_eq!(object_labels(&vm, doc), vec!["Rechthoek 1".to_owned(), "Rechthoek 2".to_owned()]);
    let ids: Vec<_> = vm.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    let mut c = ctx(doc, Control::ObjectList, 96);
    let rows = [0usize];
    c.selected_rows = &rows;
    assert_eq!(
        on_message(&c, msg::WM_COMMAND, wm_command(1002, notify::LBN_SELCHANGE), 0, &vm),
        Reaction::Intent(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace })
    );
    let same = [1usize];
    c.selected_rows = &same;
    assert_eq!(on_message(&c, msg::WM_COMMAND, wm_command(1002, notify::LBN_SELCHANGE), 0, &vm), Reaction::Nothing);
}

/// Mouse `lParam` holds signed 16-bit client pixels; at 144 dpi (150 %) 300 px are 200 pt.
/// Captured drags outside the window give negative coordinates, which must stay negative.
#[test]
fn mouse_messages_become_pointer_events_in_points() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let c = ctx(doc, Control::Canvas, 144);
    let ev =
        match intent(on_message(&c, msg::WM_LBUTTONDOWN, mk::LBUTTON | mk::SHIFT | mk::CONTROL, lparam(300, 150), &vm))
        {
            UiIntent::Pointer { document, event } => {
                assert_eq!(document, doc);
                event
            }
            other => panic!("{other:?}"),
        };
    assert_eq!(
        (ev.phase, ev.button, ev.position_pt),
        (PointerPhase::Down, Some(PointerButton::Primary), [200.0, 100.0])
    );
    assert_eq!(ev.modifiers, Modifiers { shift: true, command: true, alt: false, control: false });
    assert_eq!(ev.time.offset(), std::time::Duration::from_millis(1500));
    let pos = |m, l| match intent(on_message(&c, m, 0, l, &vm)) {
        UiIntent::Pointer { event, .. } => (event.phase, event.button, event.position_pt),
        other => panic!("{other:?}"),
    };
    assert_eq!(pos(msg::WM_MOUSEMOVE, lparam(-30, -15)), (PointerPhase::Move, None, [-20.0, -10.0]));
    assert_eq!(pos(msg::WM_LBUTTONUP, lparam(3, 6)), (PointerPhase::Up, Some(PointerButton::Primary), [2.0, 4.0]));
    assert_eq!(pos(msg::WM_RBUTTONDOWN, lparam(0, 0)).1, Some(PointerButton::Secondary));
    assert_eq!(pos(msg::WM_MBUTTONUP, lparam(0, 0)).1, Some(PointerButton::Middle));
    // A move with the left button held is part of the drag, not a hover.
    match intent(on_message(&c, msg::WM_MOUSEMOVE, mk::LBUTTON, lparam(0, 0), &vm)) {
        UiIntent::Pointer { event, .. } => assert_eq!(event.button, Some(PointerButton::Primary)),
        other => panic!("{other:?}"),
    }
    match intent(on_message(&c, msg::WM_CAPTURECHANGED, 0, 0, &vm)) {
        UiIntent::Pointer { event, .. } => assert_eq!(event.phase, PointerPhase::Cancel),
        other => panic!("{other:?}"),
    }
}

/// Wheel: one notch (120) pans 40 pt; up moves content down. Ctrl+wheel zooms around the cursor.
#[test]
fn wheel_pans_and_ctrl_wheel_zooms_around_the_cursor() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let mut c = ctx(doc, Control::Canvas, 192);
    c.cursor_px = [400, 200];
    let up = (120i16 as u16 as usize) << 16;
    let down_ctrl = ((-240i16) as u16 as usize) << 16 | mk::CONTROL;
    assert_eq!(
        on_message(&c, msg::WM_MOUSEWHEEL, up, lparam(1000, 1000), &vm),
        Reaction::Intent(UiIntent::Camera { document: doc, intent: CameraIntent::Scroll { delta_pt: [0.0, 40.0] } })
    );
    assert_eq!(
        on_message(&c, msg::WM_MOUSEHWHEEL, up, 0, &vm),
        Reaction::Intent(UiIntent::Camera { document: doc, intent: CameraIntent::Scroll { delta_pt: [-40.0, 0.0] } })
    );
    assert_eq!(
        on_message(&c, msg::WM_MOUSEWHEEL, down_ctrl, 0, &vm),
        Reaction::Intent(UiIntent::Camera {
            document: doc,
            intent: CameraIntent::Magnify { magnification: -0.2, anchor_pt: [200.0, 100.0] }
        })
    );
}

/// `WM_SIZE` of the canvas: client pixels at the window's DPI become a viewport in points.
#[test]
fn canvas_size_and_dpi_become_the_viewport() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let c = ctx(doc, Control::Canvas, 144);
    match intent(on_message(&c, msg::WM_SIZE, 0, lparam(1200, 900), &vm)) {
        UiIntent::Camera { intent: CameraIntent::Resized { viewport }, .. } => {
            assert_eq!((viewport.width_pt(), viewport.height_pt(), viewport.backing_scale()), (800.0, 600.0, 1.5));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(on_message(&c, msg::WM_SIZE, 1, lparam(0, 0), &vm), Reaction::Nothing, "minimised: no usable size");
    assert_eq!(on_message(&ctx(doc, Control::Canvas, 0), msg::WM_SIZE, 0, lparam(10, 10), &vm), Reaction::Nothing);
}

#[test]
fn canvas_keys_and_frame_lifecycle() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let c = ctx(doc, Control::Canvas, 96);
    for (key, canvas_key) in [
        (vk::ESCAPE, CanvasKey::Escape),
        (vk::DELETE, CanvasKey::Delete),
        (vk::BACK, CanvasKey::Backspace),
        (vk::RETURN, CanvasKey::Return),
    ] {
        assert_eq!(
            on_message(&c, msg::WM_KEYDOWN, key, 0, &vm),
            Reaction::Intent(UiIntent::Key { document: doc, key: canvas_key, modifiers: Modifiers::default() })
        );
    }
    let frame = ctx(doc, Control::Frame, 96);
    assert_eq!(on_message(&frame, msg::WM_CLOSE, 0, 0, &vm), Reaction::Intent(UiIntent::Close { document: doc }));
    assert_eq!(on_message(&frame, msg::WM_ACTIVATE, 1, 0, &vm), Reaction::Intent(UiIntent::Focus { document: doc }));
    assert_eq!(on_message(&frame, msg::WM_ACTIVATE, 0, 0, &vm), Reaction::PassOn, "deactivation");
}

#[test]
fn dialogs_and_close_box_map_to_intents() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let p = PathBuf::from("plaat.burnburn");
    assert_eq!(on_open_dialog(Some(p.clone())), Reaction::Intent(UiIntent::Open { path: p.clone() }));
    assert_eq!(on_open_dialog(None), Reaction::Nothing);
    assert_eq!(
        on_save_dialog(doc, Some(p.clone()), &vm),
        Reaction::Intent(UiIntent::Save { document: doc, path: Some(p) })
    );
    assert_eq!(
        on_save_dialog(doc, None, &vm),
        Reaction::Intent(UiIntent::DialogCancelled { document: doc, dialog: DialogKind::SavePanel })
    );
    for (result, choice) in [
        (button::IDYES, CloseChoice::Save),
        (button::IDNO, CloseChoice::Discard),
        (button::IDCANCEL, CloseChoice::Cancel),
        (0, CloseChoice::Cancel),
    ] {
        assert_eq!(on_close_box(doc, result, &vm), Reaction::Intent(UiIntent::ConfirmClose { document: doc, choice }));
    }
}

#[test]
fn title_marks_dirty_documents_and_menus_follow_the_view_model() {
    let (mut h, doc) = one_rectangle();
    assert_eq!(window_title(&h.view(), doc), "*Naamloos — BurnBurn");
    assert!(menu_enabled(MenuItem::Command(CommandKey::Undo), &h.view()));
    assert!(menu_enabled(MenuItem::Open, &h.view()));
    h.send(UiIntent::Save { document: doc, path: Some(PathBuf::from("plaat.burnburn")) });
    h.run_tasks();
    assert_eq!(window_title(&h.view(), doc), "plaat.burnburn — BurnBurn", "saved: no dirty mark");
    h.invoke(doc, CommandKey::Undo);
    assert!(!menu_enabled(MenuItem::Command(CommandKey::Undo), &h.view()));
}

#[test]
fn messages_for_a_closed_document_are_dropped() {
    let (mut h, doc) = one_rectangle();
    h.send(UiIntent::ConfirmClose { document: doc, choice: CloseChoice::Discard });
    let vm = h.view();
    let mut c = ctx(doc, Control::Field(FieldKey::Width), 96);
    c.text = "200";
    assert_eq!(on_message(&c, msg::WM_KEYDOWN, vk::RETURN, 0, &vm), Reaction::Nothing);
    assert_eq!(on_message(&ctx(doc, Control::Canvas, 96), msg::WM_LBUTTONDOWN, mk::LBUTTON, 0, &vm), Reaction::Nothing);
    assert_eq!(on_save_dialog(doc, Some(PathBuf::from("x.burnburn")), &vm), Reaction::Nothing);
}

/// Status texts come from the shared NL/EN catalogue (the same strings as the macOS host).
#[test]
fn message_texts_come_from_the_shared_catalogue() {
    use burnburn_presentation::{FieldError, MessageKey};
    use burnburn_windows::adapter::message_text;
    use burnburn_windows::l10n::Locale;
    assert_eq!(message_text(MessageKey::SaveFailed, Locale::Nl), "Bewaren is mislukt.");
    assert_eq!(message_text(MessageKey::SaveFailed, Locale::En), "Saving failed.");
    assert_eq!(message_text(MessageKey::InvalidLength(FieldError::NotANumber), Locale::Nl), "Geen geldig getal.");
}
