//! Beeld ▸ Replace to Fit on Windows (G7-trace, T09), headless: the button of the Beeld panel and
//! the menu item sit next to Replace Image, the button asks for a bitmap and a chosen file becomes
//! `UiIntent::ReplaceImageToFit`. The Win32 realisation is only type-checked from macOS.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;

use burnburn_presentation::{AppViewModel, CommandKey, CommandState, HeadlessEditorHarness, MessageKey, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::host_panels::image::{ImageInput, id, image_spec, on_image_event, on_image_file_chosen};
use burnburn_windows::host_panels::{DialogRequest, FilePurpose, PanelReaction, PanelValues};
use burnburn_windows::l10n::Locale;
use common::click;

fn vm(enabled: bool) -> (AppViewModel, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let mut vm = (*h.view()).clone();
    vm.commands.insert(
        CommandKey::ReplaceImageToFit,
        CommandState { enabled, checked: false, label: MessageKey::Command(CommandKey::ReplaceImageToFit) },
    );
    (vm, doc)
}

#[test]
fn the_button_follows_the_command_and_asks_for_a_bitmap() {
    let (on, doc) = vm(true);
    let input = ImageInput { document: doc, vm: &on, locale: Locale::Nl };
    let spec = image_spec(&input);
    let button = spec.control(id::REPLACE_FIT).expect("the Replace to Fit button");
    assert!(button.enabled);
    assert_eq!(button.text, "Beeld vervangen en passen…");
    match on_image_event(&input, &click(id::REPLACE_FIT, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::ReplaceToFit { document: doc });
            assert_eq!(filters[0].extensions, vec!["png", "jpg", "jpeg", "bmp", "tif", "tiff"]);
        }
        other => panic!("{other:?}"),
    }
    let (off, doc) = vm(false);
    let input = ImageInput { document: doc, vm: &off, locale: Locale::En };
    assert!(!image_spec(&input).control(id::REPLACE_FIT).unwrap().enabled);
    assert_eq!(on_image_event(&input, &click(id::REPLACE_FIT, PanelValues::default())), PanelReaction::Nothing);
}

#[test]
fn a_chosen_file_becomes_the_intent_and_cancel_or_a_closed_window_sends_nothing() {
    let (vm, doc) = vm(true);
    let path = PathBuf::from("C:/foto.png");
    assert_eq!(
        on_image_file_chosen(&vm, &FilePurpose::ReplaceToFit { document: doc }, Some(path.clone())),
        PanelReaction::Intent(UiIntent::ReplaceImageToFit { document: doc, path: path.clone() })
    );
    assert_eq!(on_image_file_chosen(&vm, &FilePurpose::ReplaceToFit { document: doc }, None), PanelReaction::Nothing);
    let gone = <DocumentId as burnburn_types::StableId>::from_raw(4242);
    assert_eq!(
        on_image_file_chosen(&vm, &FilePurpose::ReplaceToFit { document: gone }, Some(path)),
        PanelReaction::Nothing
    );
}
