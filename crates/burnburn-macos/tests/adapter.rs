//! Headless adapter contract: AppKit events as plain data in, presentation intents out. The view
//! model comes from the real AppController (through the headless harness), not from a fake.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::PathBuf;

use burnburn_editor::{CanvasKey, Modifiers, PointerButton, PointerPhase};
use burnburn_macos::adapter::{
    CLOSE_ALERT_BUTTONS, CommitCause, MenuItem, MouseButton, MousePhase, NativeEvent, Reaction, close_choice, flags,
    key_code, menu_enabled, menu_item, menu_items, menu_tag, menu_title, message_text, modifiers, object_labels,
    response, selected_rows, translate, window_title,
};
use burnburn_macos::l10n::Locale;
use burnburn_presentation::{
    AppViewModel, CameraIntent, CloseChoice, CommandKey, DialogKind, FieldError, FieldKey, HeadlessEditorHarness,
    MessageKey, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, StableId};

/// A document with one selected 120 × 80 mm rectangle, drawn through the real controller.
fn one_rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    assert_eq!(h.document(doc).unwrap().objects.len(), 1);
    (h, doc)
}

fn intent(r: Reaction) -> UiIntent {
    match r {
        Reaction::Intent(i) => i,
        other => panic!("expected an intent, got {other:?}"),
    }
}

fn field(doc: DocumentId, text: &str, cause: CommitCause) -> NativeEvent {
    NativeEvent::FieldCommitted { document: doc, field: FieldKey::Width, text: text.into(), cause }
}

#[test]
fn width_field_commit_sends_raw_text_to_presentation() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let got = intent(translate(field(doc, " 150,5 mm ", CommitCause::Return), &vm));
    assert_eq!(got, UiIntent::CommitField { document: doc, field: FieldKey::Width, text: " 150,5 mm ".into() });
    // Invalid text is presentation's business too (DR-05): the host forwards it unparsed.
    let got = intent(translate(field(doc, "abc", CommitCause::EndEditing), &vm));
    assert_eq!(got, UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "abc".into() });
}

/// Return followed by Tab (end of editing) sends the action twice with the same text; the second
/// one must not become a second command. Unchanged text is never a commit.
#[test]
fn unchanged_field_text_sends_nothing_so_return_then_tab_commits_once() {
    let (mut h, doc) = one_rectangle();
    let first = translate(field(doc, "150", CommitCause::Return), &h.view());
    h.send(intent(first));
    let revision = h.document(doc).unwrap().revision;
    let second = translate(field(doc, "150", CommitCause::EndEditing), &h.view());
    assert_eq!(second, Reaction::Nothing);
    assert_eq!(translate(field(doc, "120", CommitCause::EndEditing), &one_rectangle().0.view()), Reaction::Nothing);
    assert_eq!(h.document(doc).unwrap().revision, revision);
}

#[test]
fn menu_items_become_invoke_for_the_active_document_and_open_asks_for_a_panel() {
    let (h, _) = one_rectangle();
    let vm = h.view();
    for key in [CommandKey::Undo, CommandKey::Redo, CommandKey::Save, CommandKey::New, CommandKey::Delete] {
        assert_eq!(
            translate(NativeEvent::Menu(MenuItem::Command(key)), &vm),
            Reaction::Intent(UiIntent::Invoke { document: None, command: key })
        );
    }
    assert_eq!(translate(NativeEvent::Menu(MenuItem::Open), &vm), Reaction::ShowOpenPanel);
}

#[test]
fn menu_tags_round_trip_and_unknown_tags_are_no_item() {
    let items = menu_items();
    assert!(items.contains(&MenuItem::Open));
    for key in CommandKey::ALL {
        assert!(items.contains(&MenuItem::Command(*key)), "{key:?} has a menu item");
    }
    let mut tags: Vec<isize> = items.iter().map(|i| menu_tag(*i)).collect();
    for item in &items {
        assert_eq!(menu_item(menu_tag(*item)), Some(*item));
    }
    tags.sort();
    tags.dedup();
    assert_eq!(tags.len(), items.len(), "tags are unique");
    assert!(tags.iter().all(|t| *t >= 1), "tag 0 is AppKit's default and means no item");
    assert_eq!(menu_item(0), None);
    assert_eq!(menu_item(-1), None);
}

#[test]
fn open_panel_ok_opens_the_path_and_cancel_changes_nothing() {
    let vm = HeadlessEditorHarness::new().view();
    let path = PathBuf::from("/tmp/x.burnburn");
    assert_eq!(
        translate(NativeEvent::OpenPanelFinished { path: Some(path.clone()) }, &vm),
        Reaction::Intent(UiIntent::Open { path })
    );
    assert_eq!(translate(NativeEvent::OpenPanelFinished { path: None }, &vm), Reaction::Nothing);
}

#[test]
fn save_panel_ok_saves_to_the_path_and_cancel_reports_the_cancelled_dialog() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let path = PathBuf::from("/tmp/plaat.burnburn");
    assert_eq!(
        translate(NativeEvent::SavePanelFinished { document: doc, path: Some(path.clone()) }, &vm),
        Reaction::Intent(UiIntent::Save { document: doc, path: Some(path) })
    );
    assert_eq!(
        translate(NativeEvent::SavePanelFinished { document: doc, path: None }, &vm),
        Reaction::Intent(UiIntent::DialogCancelled { document: doc, dialog: DialogKind::SavePanel })
    );
}

#[test]
fn close_request_and_alert_buttons_map_to_close_and_choices() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    assert_eq!(
        translate(NativeEvent::CloseRequested { document: doc }, &vm),
        Reaction::Intent(UiIntent::Close { document: doc })
    );
    assert_eq!(CLOSE_ALERT_BUTTONS, [CloseChoice::Save, CloseChoice::Cancel, CloseChoice::Discard]);
    assert_eq!(close_choice(response::FIRST_BUTTON), CloseChoice::Save);
    assert_eq!(close_choice(response::SECOND_BUTTON), CloseChoice::Cancel);
    assert_eq!(close_choice(response::THIRD_BUTTON), CloseChoice::Discard);
    for odd in [response::CANCEL, response::OK, -1000, 1003] {
        assert_eq!(close_choice(odd), CloseChoice::Cancel, "{odd}: unknown answers keep the document open");
    }
    assert_eq!(
        translate(NativeEvent::CloseAlertFinished { document: doc, response: response::SECOND_BUTTON }, &vm),
        Reaction::Intent(UiIntent::ConfirmClose { document: doc, choice: CloseChoice::Cancel })
    );
}

/// `validateMenuItem:` reads enablement from the view model: one undo stack, one source of truth.
#[test]
fn menu_validation_and_titles_follow_the_view_model() {
    let empty = HeadlessEditorHarness::new().view();
    assert!(menu_enabled(MenuItem::Command(CommandKey::New), &empty));
    assert!(menu_enabled(MenuItem::Open, &empty));
    assert!(!menu_enabled(MenuItem::Command(CommandKey::Undo), &empty));
    assert!(!menu_enabled(MenuItem::Command(CommandKey::Save), &empty), "no document, nothing to save");

    let (mut h, doc) = one_rectangle();
    let vm = h.view();
    assert!(menu_enabled(MenuItem::Command(CommandKey::Undo), &vm));
    assert!(!menu_enabled(MenuItem::Command(CommandKey::Redo), &vm));
    assert_eq!(menu_title(MenuItem::Command(CommandKey::Undo), &vm, Locale::Nl), "Herstel Rechthoek tekenen");
    assert_eq!(menu_title(MenuItem::Command(CommandKey::Redo), &vm, Locale::Nl), "Opnieuw");
    h.invoke(doc, CommandKey::Undo);
    let vm = h.view();
    assert!(!menu_enabled(MenuItem::Command(CommandKey::Undo), &vm));
    assert!(menu_enabled(MenuItem::Command(CommandKey::Redo), &vm));
    assert_eq!(menu_title(MenuItem::Command(CommandKey::Redo), &vm, Locale::Nl), "Opnieuw Rechthoek tekenen");
    assert_eq!(menu_title(MenuItem::Open, &vm, Locale::Nl), "Open…");
}

#[test]
fn table_selection_becomes_select_unless_it_already_matches_the_view_model() {
    let (mut h, doc) = one_rectangle();
    h.drag(doc, (200.0, 0.0), (250.0, 30.0));
    let vm = h.view();
    assert_eq!(object_labels(&vm, doc), vec!["Rechthoek 1".to_owned(), "Rechthoek 2".to_owned()]);
    assert_eq!(selected_rows(&vm, doc), vec![1], "the new rectangle is selected");
    let ids: Vec<_> = vm.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    // Programmatic sync from the view model echoes back as a no-op: no feedback loop.
    assert_eq!(translate(NativeEvent::TableSelectionChanged { document: doc, rows: vec![1] }, &vm), Reaction::Nothing);
    assert_eq!(
        translate(NativeEvent::TableSelectionChanged { document: doc, rows: vec![0, 1] }, &vm),
        Reaction::Intent(UiIntent::Select { document: doc, ids: ids.clone(), mode: SelectMode::Replace })
    );
    assert_eq!(
        translate(NativeEvent::TableSelectionChanged { document: doc, rows: vec![] }, &vm),
        Reaction::Intent(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace })
    );
    // Rows beyond the view model (a stale table) are ignored, never invented.
    assert_eq!(
        translate(NativeEvent::TableSelectionChanged { document: doc, rows: vec![0, 7] }, &vm),
        Reaction::Intent(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace })
    );
}

#[test]
fn modifier_flags_normalise_with_command_as_shortcut_modifier() {
    assert_eq!(modifiers(0), Modifiers::default());
    let all = modifiers(flags::SHIFT | flags::CONTROL | flags::OPTION | flags::COMMAND | 0xff);
    assert_eq!(all, Modifiers { shift: true, alt: true, command: true, control: true });
    assert_eq!(modifiers(flags::COMMAND), Modifiers { command: true, ..Modifiers::default() });
}

fn mouse(doc: DocumentId, phase: MousePhase, button: MouseButton, flags: usize) -> NativeEvent {
    NativeEvent::Mouse {
        document: doc,
        phase,
        button,
        location_pt: [120.5, 33.25],
        flags,
        click_count: 2,
        timestamp_s: 1.5,
    }
}

#[test]
fn mouse_events_become_canvas_pointer_events_in_points() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let UiIntent::Pointer { document, event: ev } =
        intent(translate(mouse(doc, MousePhase::Down, MouseButton::Left, flags::SHIFT), &vm))
    else {
        panic!("expected a pointer event")
    };
    assert_eq!(document, doc);
    assert_eq!(
        (ev.phase, ev.button, ev.position_pt, ev.click_count),
        (PointerPhase::Down, Some(PointerButton::Primary), [120.5, 33.25], 2)
    );
    assert!(ev.modifiers.shift && !ev.modifiers.command);
    assert_eq!(ev.time.offset(), std::time::Duration::from_secs_f64(1.5));
    let phase = |p, b, f| match intent(translate(mouse(doc, p, b, f), &vm)) {
        UiIntent::Pointer { event: ev, .. } => (ev.phase, ev.button),
        other => panic!("{other:?}"),
    };
    assert_eq!(phase(MousePhase::Dragged, MouseButton::Left, 0), (PointerPhase::Move, Some(PointerButton::Primary)));
    assert_eq!(phase(MousePhase::Up, MouseButton::Left, 0), (PointerPhase::Up, Some(PointerButton::Primary)));
    assert_eq!(phase(MousePhase::Moved, MouseButton::Left, 0), (PointerPhase::Move, None));
    assert_eq!(phase(MousePhase::Down, MouseButton::Right, 0), (PointerPhase::Down, Some(PointerButton::Secondary)));
    assert_eq!(phase(MousePhase::Down, MouseButton::Other, 0), (PointerPhase::Down, Some(PointerButton::Middle)));
    // Ctrl-click is the Mac secondary click (presentation-api.md).
    assert_eq!(
        phase(MousePhase::Down, MouseButton::Left, flags::CONTROL),
        (PointerPhase::Down, Some(PointerButton::Secondary))
    );
}

#[test]
fn trackpad_scroll_pans_wheel_lines_scale_and_pinch_zooms() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let scroll = |delta, precise| match intent(translate(
        NativeEvent::Scroll { document: doc, delta, precise, flags: 0, timestamp_s: 0.0 },
        &vm,
    )) {
        UiIntent::Camera { intent: CameraIntent::Scroll { delta_pt }, .. } => (delta_pt, precise),
        other => panic!("{other:?}"),
    };
    assert_eq!(scroll([3.0, -4.5], true), ([3.0, -4.5], true));
    assert_eq!(scroll([1.0, -2.0], false), ([10.0, -20.0], false), "wheel lines become points");
    // Cmd + wheel zooms around the pointer instead of panning (Windows: Ctrl + wheel).
    let ev = NativeEvent::Scroll {
        document: doc,
        delta: [0.0, 2.0],
        precise: false,
        flags: flags::COMMAND,
        timestamp_s: 0.0,
    };
    assert!(matches!(intent(translate(ev, &vm)), UiIntent::Camera { intent: CameraIntent::Magnify { .. }, .. }));
    match intent(translate(
        NativeEvent::Magnify { document: doc, magnification: 0.25, location_pt: [10.0, 20.0], timestamp_s: 0.0 },
        &vm,
    )) {
        UiIntent::Camera { intent: CameraIntent::Magnify { magnification, anchor_pt }, .. } => {
            assert_eq!((magnification, anchor_pt), (0.25, [10.0, 20.0]));
        }
        other => panic!("{other:?}"),
    }
    for bad in [f64::NAN, f64::INFINITY] {
        let ev = NativeEvent::Magnify { document: doc, magnification: bad, location_pt: [0.0, 0.0], timestamp_s: 0.0 };
        assert_eq!(translate(ev, &vm), Reaction::Nothing, "non-finite gesture values are dropped");
    }
}

#[test]
fn resize_becomes_viewport_and_unusable_sizes_are_dropped() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    match intent(translate(
        NativeEvent::Resized { document: doc, width_pt: 660.0, height_pt: 700.0, backing_scale: 2.0 },
        &vm,
    )) {
        UiIntent::Camera { intent: CameraIntent::Resized { viewport }, .. } => {
            assert_eq!((viewport.width_pt(), viewport.height_pt(), viewport.backing_scale()), (660.0, 700.0, 2.0));
        }
        other => panic!("{other:?}"),
    }
    let zero = NativeEvent::Resized { document: doc, width_pt: 0.0, height_pt: 700.0, backing_scale: 2.0 };
    assert_eq!(translate(zero, &vm), Reaction::Nothing);
}

#[test]
fn canvas_keys_become_key_intents_and_other_keys_pass_on() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    for (code, key) in [
        (key_code::ESCAPE, CanvasKey::Escape),
        (key_code::DELETE, CanvasKey::Backspace),
        (key_code::FORWARD_DELETE, CanvasKey::Delete),
        (key_code::RETURN, CanvasKey::Return),
    ] {
        assert_eq!(
            translate(NativeEvent::KeyDown { document: doc, key_code: code, flags: flags::SHIFT }, &vm),
            Reaction::Intent(UiIntent::Key {
                document: doc,
                key,
                modifiers: Modifiers { shift: true, ..Modifiers::default() }
            }),
            "{code}"
        );
    }
    assert_eq!(translate(NativeEvent::KeyDown { document: doc, key_code: 0, flags: 0 }, &vm), Reaction::Nothing);
}

#[test]
fn window_focus_becomes_focus_intent() {
    let (h, doc) = one_rectangle();
    assert_eq!(
        translate(NativeEvent::WindowBecameKey { document: doc }, &h.view()),
        Reaction::Intent(UiIntent::Focus { document: doc })
    );
}

/// After a window's session closed, its late callbacks reach nothing.
#[test]
fn events_for_a_closed_document_are_dropped() {
    let (mut h, doc) = one_rectangle();
    h.send(UiIntent::ConfirmClose { document: doc, choice: CloseChoice::Discard });
    let vm: std::sync::Arc<AppViewModel> = h.view();
    assert!(vm.document(doc).is_none());
    for ev in [
        field(doc, "200", CommitCause::Return),
        mouse(doc, MousePhase::Down, MouseButton::Left, 0),
        NativeEvent::CloseRequested { document: doc },
        NativeEvent::KeyDown { document: doc, key_code: key_code::ESCAPE, flags: 0 },
        NativeEvent::TableSelectionChanged { document: doc, rows: vec![0] },
        NativeEvent::SavePanelFinished { document: doc, path: Some(PathBuf::from("/tmp/a.burnburn")) },
    ] {
        assert_eq!(translate(ev.clone(), &vm), Reaction::Nothing, "{ev:?}");
    }
}

#[test]
fn texts_are_dutch_and_follow_the_view_model() {
    assert_eq!(message_text(MessageKey::InvalidLength(FieldError::NotANumber), Locale::Nl), "Geen geldig getal.");
    assert_eq!(message_text(MessageKey::InvalidLength(FieldError::Empty), Locale::Nl), "Vul een maat in.");
    assert_eq!(
        message_text(MessageKey::InvalidLength(FieldError::NotPositive), Locale::Nl),
        "De maat moet groter dan 0 zijn."
    );
    assert_eq!(message_text(MessageKey::SaveFailed, Locale::Nl), "Bewaren is mislukt.");
    assert_eq!(message_text(MessageKey::Field(FieldKey::Width), Locale::Nl), "Breedte");
    assert_eq!(message_text(MessageKey::Field(FieldKey::Height), Locale::Nl), "Hoogte");
    let (h, doc) = one_rectangle();
    assert_eq!(window_title(&h.view(), doc), "Naamloos");
    assert_eq!(window_title(&h.view(), DocumentId::from_raw(999_999)), "");
}

#[test]
fn export_sheet_ok_exports_rd_and_cancel_changes_nothing() {
    let (h, doc) = one_rectangle();
    let vm = h.view();
    let path = PathBuf::from("/tmp/plaat.rd");
    assert_eq!(
        translate(NativeEvent::ExportPanelFinished { document: doc, path: Some(path.clone()) }, &vm),
        Reaction::Intent(UiIntent::ExportRd { document: doc, path })
    );
    assert_eq!(translate(NativeEvent::ExportPanelFinished { document: doc, path: None }, &vm), Reaction::Nothing);
}

/// Final canvas text has no presentation intent yet: the host must not invent a document change.
#[test]
fn import_menu_asks_for_a_file_panel() {
    let (h, _doc) = one_rectangle();
    assert_eq!(translate(NativeEvent::Menu(MenuItem::Import), &h.view()), Reaction::ShowImportPanel);
    assert_eq!(menu_item(menu_tag(MenuItem::Import)), Some(MenuItem::Import));
    assert!(menu_items().contains(&MenuItem::Import));
}
