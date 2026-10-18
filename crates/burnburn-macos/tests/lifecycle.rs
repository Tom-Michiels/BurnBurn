//! IM20 app lifecycle without AppKit: quitting with dirty documents asks per document (the same
//! native close question as the close button) and one Annuleer keeps every remaining document open;
//! Finder/Dock open requests become `Open` intents. Driven against the real `AppController`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_macos::lifecycle::{QuitFlow, QuitStep, open_requests};
use burnburn_presentation::{CloseChoice, CommandKey, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_types::DocumentId;

fn dirty_document(h: &mut HeadlessEditorHarness) -> DocumentId {
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    assert!(h.document(doc).unwrap().dirty);
    doc
}

/// What the host does for one step: send the intent, let the flow see it and the effects.
fn send(h: &mut HeadlessEditorHarness, flow: &mut QuitFlow, intent: UiIntent) -> Vec<HostEffect> {
    flow.observe_intent(&intent);
    let update = h.send(intent);
    for e in &update.effects {
        flow.observe_effect(e);
    }
    update.effects
}

#[test]
fn quit_without_dirty_documents_terminates_right_away() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    assert_eq!(QuitFlow::begin(&h.view()), None);
}

#[test]
fn quit_asks_each_dirty_document_and_cancel_keeps_all_remaining_open() {
    let mut h = HeadlessEditorHarness::new();
    let first = dirty_document(&mut h);
    let clean = h.new_document().unwrap();
    let second = dirty_document(&mut h);

    let mut flow = QuitFlow::begin(&h.view()).expect("dirty documents need a quit flow");
    assert_eq!(flow.step(&h.view()), QuitStep::Close(first), "dirty documents first, in window order");
    let effects = send(&mut h, &mut flow, UiIntent::Close { document: first });
    assert!(effects.contains(&HostEffect::ConfirmClose { document: first }), "{effects:?}");
    // The sheet is up: nothing else happens until the user answers.
    assert_eq!(flow.step(&h.view()), QuitStep::Wait);

    let effects = send(&mut h, &mut flow, UiIntent::ConfirmClose { document: first, choice: CloseChoice::Discard });
    assert!(effects.contains(&HostEffect::CloseWindow { document: first }), "{effects:?}");
    assert_eq!(flow.step(&h.view()), QuitStep::Close(second));
    send(&mut h, &mut flow, UiIntent::Close { document: second });
    send(&mut h, &mut flow, UiIntent::ConfirmClose { document: second, choice: CloseChoice::Cancel });
    assert_eq!(flow.step(&h.view()), QuitStep::Abort);
    // Cancel is final for this quit: asking again would be a second question the user did not ask for.
    assert_eq!(flow.step(&h.view()), QuitStep::Abort);

    let vm = h.view();
    assert!(vm.document(first).is_none(), "the discarded document is closed");
    assert!(vm.document(second).is_some_and(|d| d.dirty && d.objects.len() == 1), "cancelled one stays as it was");
    assert!(vm.document(clean).is_some(), "the clean document is untouched by a cancelled quit");
}

#[test]
fn quit_terminates_after_every_dirty_document_was_answered() {
    let mut h = HeadlessEditorHarness::new();
    let a = dirty_document(&mut h);
    let b = dirty_document(&mut h);
    let mut flow = QuitFlow::begin(&h.view()).unwrap();
    for doc in [a, b] {
        assert_eq!(flow.step(&h.view()), QuitStep::Close(doc));
        send(&mut h, &mut flow, UiIntent::Close { document: doc });
        send(&mut h, &mut flow, UiIntent::ConfirmClose { document: doc, choice: CloseChoice::Discard });
    }
    assert_eq!(flow.step(&h.view()), QuitStep::Terminate);
}

/// Bewaar on an untitled document opens the save panel; cancelling that panel cancels the quit too.
#[test]
fn cancelling_the_save_panel_during_quit_aborts_the_quit() {
    let mut h = HeadlessEditorHarness::new();
    let doc = dirty_document(&mut h);
    let mut flow = QuitFlow::begin(&h.view()).unwrap();
    assert_eq!(flow.step(&h.view()), QuitStep::Close(doc));
    send(&mut h, &mut flow, UiIntent::Close { document: doc });
    let effects = send(&mut h, &mut flow, UiIntent::ConfirmClose { document: doc, choice: CloseChoice::Save });
    assert!(effects.iter().any(|e| matches!(e, HostEffect::ShowSavePanel { .. })), "{effects:?}");
    assert_eq!(flow.step(&h.view()), QuitStep::Wait);
    send(
        &mut h,
        &mut flow,
        UiIntent::DialogCancelled { document: doc, dialog: burnburn_presentation::DialogKind::SavePanel },
    );
    assert_eq!(flow.step(&h.view()), QuitStep::Abort);
    assert!(h.view().document(doc).is_some_and(|d| d.dirty));
}

/// A failed save while quitting must not hang the quit in `Wait` nor terminate: it aborts.
#[test]
fn an_error_for_the_asked_document_aborts_the_quit() {
    let mut h = HeadlessEditorHarness::new();
    let doc = dirty_document(&mut h);
    let mut flow = QuitFlow::begin(&h.view()).unwrap();
    assert_eq!(flow.step(&h.view()), QuitStep::Close(doc));
    flow.observe_effect(&HostEffect::ShowError {
        document: Some(doc),
        message: burnburn_presentation::MessageKey::SaveFailed,
    });
    assert_eq!(flow.step(&h.view()), QuitStep::Abort);
}

#[test]
fn finder_open_requests_become_open_intents_for_project_files_only() {
    let got = open_requests(vec![
        PathBuf::from("/tmp/a.burnburn"),
        PathBuf::from("/tmp/B.BurnBurn"),
        PathBuf::from("/tmp/photo.png"),
        PathBuf::from("/tmp/no-extension"),
    ]);
    assert_eq!(
        got,
        vec![
            UiIntent::Open { path: PathBuf::from("/tmp/a.burnburn") },
            UiIntent::Open { path: PathBuf::from("/tmp/B.BurnBurn") },
        ]
    );
}
