//! The G5-cam dialogs of the Mac window, headless: Array, Feeder, Preflight and Cutting order are
//! menu items, say every row, note, label and button in both languages, turn their buttons into
//! intents and put the tool layers T1/T2 at the end of the colour palette.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{MenuItem, menu_items, object_labels};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{
    cam_action_intent, cam_action_title, cam_caption, cam_choice_label, cam_error, cam_label, cam_note, cam_title,
    finding_text, palette_buttons,
};
use burnburn_presentation::{
    CamDialog, CommandKey, DialogAction, DialogActionKind, DialogChoice, DialogError, DialogField, DialogNote,
    DialogViewModel, FindingKind, HeadlessEditorHarness, SelectMode, UiIntent,
};
use burnburn_types::DocumentId;

const LOCALES: [Locale; 2] = [Locale::Nl, Locale::En];

fn rig() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    for x in [10.0, 80.0] {
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (x, 10.0), (x + 30.0, 40.0));
    }
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids: ids[..1].to_vec(), mode: SelectMode::Replace });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, dialog: CamDialog) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog });
    h.controller().cam_dialog(doc, dialog).unwrap()
}

#[test]
fn the_new_dialogs_are_menu_items_with_titles_in_both_languages() {
    for dialog in [CamDialog::Array, CamDialog::Feeder, CamDialog::Preflight, CamDialog::Order] {
        assert!(menu_items().contains(&MenuItem::CamDialog(dialog)), "{dialog:?}");
        assert!(LOCALES.iter().all(|l| !cam_title(dialog, *l).is_empty()));
    }
}

#[test]
fn every_label_note_and_error_of_the_new_dialogs_has_words() {
    for locale in LOCALES {
        for kind in [
            FindingKind::OutsideBed,
            FindingKind::TravelOutsideBed,
            FindingKind::OpenFillContour,
            FindingKind::SpeedAboveMachineMax,
            FindingKind::PowerOutsideMachineRange,
            FindingKind::DuplicateCut,
            FindingKind::EmptyOperation,
            FindingKind::UnassignedGeometry,
            FindingKind::StalePlan,
            FindingKind::UnsupportedFeature,
            FindingKind::InvalidTabs,
            FindingKind::SharedBoundaryCutOnce,
        ] {
            assert!(!finding_text(kind, locale).is_empty(), "{kind:?}");
            assert!(!cam_note(DialogNote::Finding(kind), locale).is_empty());
        }
        for note in [
            DialogNote::ArrayNeedsOneObject,
            DialogNote::ArrayExtent { copies: 8, width_um: 85_000, height_um: 28_000 },
            DialogNote::FeederHasNoRdForm,
            DialogNote::PreflightNoPlan,
            DialogNote::PreflightStale,
            DialogNote::PreflightCounts { errors: 1, warnings: 2 },
            DialogNote::PreflightClean,
            DialogNote::OrderCycle,
            DialogNote::OrderSameLayer,
            DialogNote::OrderDuplicate,
        ] {
            assert!(!cam_note(note, locale).is_empty(), "{note:?} {locale:?}");
        }
        for e in [DialogError::TooManyCopies, DialogError::NoPath] {
            assert!(!cam_error(e, locale).is_empty());
        }
        for c in [DialogChoice::ArrayGrid, DialogChoice::ArrayCircular, DialogChoice::ArrayPath] {
            assert!(!cam_choice_label(c, locale).is_empty());
        }
        for kind in [DialogActionKind::ShowSource, DialogActionKind::AddRow, DialogActionKind::RemoveRow] {
            assert!(!cam_action_title(kind, CamDialog::Order, locale).is_empty());
        }
    }
    let extent = cam_note(DialogNote::ArrayExtent { copies: 8, width_um: 85_000, height_um: 28_000 }, Locale::En);
    assert!(extent.contains("8 copies") && extent.contains("85.000") && extent.contains("28.000"), "{extent}");
}

#[test]
fn a_label_is_its_phrase_and_its_data() {
    let (mut h, doc) = rig();
    h.invoke(doc, CommandKey::Compile);
    let vm = open(&mut h, doc, CamDialog::Order);
    let before = vm.row(DialogField::OrderBefore).unwrap();
    assert!(before.labels.is_empty() || before.labels.iter().all(|l| l.note.is_none()));
    let label = burnburn_presentation::DialogLabel {
        note: Some(DialogNote::Finding(FindingKind::OutsideBed)),
        text: "Rechthoek 2".into(),
    };
    assert_eq!(cam_label(&label, Locale::En), "Cuts outside the bed: Rechthoek 2");
    assert_eq!(cam_label(&burnburn_presentation::DialogLabel::plain("C01"), Locale::Nl), "C01");
}

#[test]
fn the_buttons_of_the_new_dialogs_become_intents() {
    let (mut h, doc) = rig();
    let vm = open(&mut h, doc, CamDialog::Order);
    // Two layers are needed for a constraint; with none the buttons are greyed and send nothing.
    assert_eq!(cam_action_intent(&vm, doc, CamDialog::Order, DialogActionKind::AddRow, None), None);
    h.send(UiIntent::CamDialogAction { document: doc, dialog: CamDialog::Order, action: DialogAction::Close });
    let vm = open(&mut h, doc, CamDialog::Preflight);
    assert_eq!(cam_action_intent(&vm, doc, CamDialog::Preflight, DialogActionKind::ShowSource, None), None);
    assert_eq!(
        cam_action_intent(&vm, doc, CamDialog::Preflight, DialogActionKind::Close, None),
        Some(UiIntent::CamDialogAction { document: doc, dialog: CamDialog::Preflight, action: DialogAction::Close })
    );
}

#[test]
fn the_palette_ends_with_the_tool_layers_and_the_object_list_says_how_many_layers_use_an_object() {
    let (mut h, doc) = rig();
    let buttons = palette_buttons(&h.view(), doc, Locale::En);
    assert_eq!(buttons.len(), 32);
    assert_eq!((buttons[30].label.as_str(), buttons[31].label.as_str()), ("T1", "T2"));

    // The selected first rectangle on two layers (C01 and a second treatment on C02).
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    assert_eq!(object_labels(&h.view(), doc)[0], "Rechthoek 1");
    h.send(UiIntent::AssignLayer { document: doc, slot: 2, add: true });
    assert_eq!(object_labels(&h.view(), doc)[0], "Rechthoek 1 (\u{d7}2)");
    assert_eq!(object_labels(&h.view(), doc)[1], "Rechthoek 2", "an unlinked object keeps its plain name");
    let _ = cam_caption;
}
