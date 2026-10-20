//! The CAM dialogs of the Mac window, headless (W3-UI-cam): menu items and their intents, the words
//! for every field, choice, note, error and refusal in both languages, and the intents a changed
//! widget produces. No AppKit here; `tests/native/macos/cam_dialogs.rs` drives the real widgets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::OrderKey;
use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_items, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::{Locale, known_message_keys, message};
use burnburn_macos::panels::{
    cam_action_intent, cam_action_title, cam_caption, cam_choice_label, cam_error, cam_field_intent, cam_menu_title,
    cam_move_intent, cam_note, cam_result_text, cam_support_line, cam_title, order_key_caption, rd_refusal_text,
};
use burnburn_presentation::{
    CamDialog, CapabilityRow, DialogAction, DialogActionKind, DialogChoice, DialogError, DialogField, DialogNote,
    DialogValue, DialogViewModel, FieldKey, HeadlessEditorHarness, MessageKey, RdRefusal, SupportView, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::DocumentId;

const LOCALES: [Locale; 2] = [Locale::Nl, Locale::En];

fn rig(z: Support) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let profile = MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(MachineCapabilities::conservative().with(Feature::ZStep, z))
    .unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, dialog: CamDialog) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog });
    h.controller().cam_dialog(doc, dialog).unwrap()
}

#[test]
fn every_dialog_has_a_menu_item_that_opens_it_for_the_window_in_front() {
    let (h, doc) = rig(Support::Unsupported);
    for dialog in CamDialog::ALL {
        let item = MenuItem::CamDialog(dialog);
        assert!(menu_items().contains(&item), "{dialog:?} is a menu item");
        assert_eq!(menu_item(menu_tag(item)), Some(item), "{dialog:?} tag round-trips");
        assert!(menu_tag(item) >= 1);
        assert_eq!(
            translate(NativeEvent::Menu(item), &h.view()),
            Reaction::Intent(UiIntent::OpenCamDialog { document: doc, dialog })
        );
        assert!(menu_enabled(item, &h.view()));
        for locale in LOCALES {
            assert_eq!(menu_title(item, &h.view(), locale), cam_menu_title(dialog, locale));
            assert!(menu_title(item, &h.view(), locale).ends_with('…'), "it asks for input first");
        }
    }
    // Without a window there is nothing to open it for.
    let empty = HeadlessEditorHarness::new();
    let item = MenuItem::CamDialog(CamDialog::Device);
    assert_eq!(translate(NativeEvent::Menu(item), &empty.view()), Reaction::Nothing);
    assert!(!menu_enabled(item, &empty.view()));
    // All tags stay distinct.
    let mut tags: Vec<isize> = menu_items().into_iter().map(menu_tag).collect();
    tags.sort_unstable();
    let count = tags.len();
    tags.dedup();
    assert_eq!(tags.len(), count);
}

#[test]
fn every_row_choice_and_note_of_every_dialog_is_said_in_both_languages() {
    let (mut h, doc) = rig(Support::Supported);
    // A window with a rotary on, so every Device row and note is there.
    for dialog in CamDialog::ALL {
        let vm = open(&mut h, doc, dialog);
        for locale in LOCALES {
            assert!(!cam_title(dialog, locale).is_empty());
            for row in &vm.rows {
                assert!(!cam_caption(row.field, &vm, locale).trim().is_empty(), "{:?} {locale:?}", row.field);
                for choice in row.field.choices() {
                    assert!(!cam_choice_label(*choice, locale).is_empty(), "{choice:?}");
                }
            }
            for action in &vm.actions {
                assert!(!cam_action_title(action.action, dialog, locale).is_empty());
            }
            for note in &vm.notes {
                assert!(!cam_note(*note, locale).is_empty(), "{note:?}");
            }
        }
        h.send(UiIntent::CamDialogAction { document: doc, dialog, action: DialogAction::Close });
    }
    for locale in LOCALES {
        for key in [OrderKey::Layer, OrderKey::Group, OrderKey::Priority] {
            assert!(!order_key_caption(key, locale).is_empty());
        }
        for note in [
            DialogNote::PriorityNeedsOrderLevel,
            DialogNote::NoMachineProfile,
            DialogNote::FocusNeedsZ,
            DialogNote::TestIsSeparate,
            DialogNote::NewProfile,
            DialogNote::DialectUnverified,
            DialogNote::RotaryUnconfirmed,
            DialogNote::RotaryNotInCliProfile,
            DialogNote::ProfileChangeMakesPlansStale,
            DialogNote::TestCompileFailed,
            DialogNote::TestSaved,
            DialogNote::ExportFailed,
            DialogNote::RdRefused(RdRefusal::CrossHatch),
        ] {
            assert!(!cam_note(note, locale).is_empty(), "{note:?} {locale:?}");
        }
        for error in [
            DialogError::Invalid,
            DialogError::OutOfRange,
            DialogError::NotWhole,
            DialogError::Count,
            DialogError::ParameterNotForMode,
            DialogError::SameParameter,
            DialogError::Size,
            DialogError::TooManyCells,
            DialogError::Unsupported(Feature::ZStep),
        ] {
            assert!(!cam_error(error, locale).is_empty(), "{error:?} {locale:?}");
        }
    }
    assert_ne!(cam_title(CamDialog::Device, Locale::Nl), cam_title(CamDialog::Device, Locale::En));
}

#[test]
fn the_diameter_caption_follows_the_kind_of_rotary() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, CamDialog::Device);
    let roller = DialogField::RotaryKind.choices().iter().position(|c| *c == DialogChoice::RotaryRoller).unwrap();
    h.send(UiIntent::SetCamField {
        document: doc,
        dialog: CamDialog::Device,
        field: DialogField::RotaryKind,
        value: DialogValue::Choice(roller),
    });
    let vm = h.controller().cam_dialog(doc, CamDialog::Device).unwrap();
    assert!(cam_caption(DialogField::RotaryDiameter, &vm, Locale::En).to_lowercase().contains("roller"));
    assert!(cam_caption(DialogField::RotaryDiameter, &open_chuck(), Locale::En).to_lowercase().contains("object"));
}

fn open_chuck() -> DialogViewModel {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, CamDialog::Device)
}

#[test]
fn every_refusal_of_rd_output_is_said_in_words_and_reaches_the_message_catalogue() {
    for reason in [
        RdRefusal::CrossHatch,
        RdRefusal::DotMode,
        RdRefusal::RampLength,
        RdRefusal::FocusOffset,
        RdRefusal::ScanAngle,
        RdRefusal::GrayscaleModulation,
        RdRefusal::Other,
        RdRefusal::TooManyParts,
        RdRefusal::NegativeCoordinate,
    ] {
        for locale in LOCALES {
            let text = rd_refusal_text(reason, locale);
            assert!(text.contains("RD"), "{reason:?} names what is refused: {text}");
            assert_eq!(message(MessageKey::RdRefused(reason), locale).as_deref(), Some(text));
            assert!(known_message_keys().contains(&MessageKey::RdRefused(reason)));
        }
    }
    let nl = rd_refusal_text(RdRefusal::CrossHatch, Locale::Nl);
    assert!(nl.to_lowercase().contains("kruisarcering") && nl.contains("geweigerd"));
    assert!(rd_refusal_text(RdRefusal::DotMode, Locale::En).contains("refused"));
    assert!(rd_refusal_text(RdRefusal::FocusOffset, Locale::En).contains("evidence-cam.md"));
}

#[test]
fn the_priority_field_and_the_new_undo_labels_have_words() {
    for locale in LOCALES {
        assert!(message(MessageKey::Field(FieldKey::Priority), locale).is_some_and(|t| !t.is_empty()));
    }
    assert!(known_message_keys().contains(&MessageKey::Field(FieldKey::Priority)));
    let label = |l| message(MessageKey::UndoAction(burnburn_document::UndoLabel::Process(l)), Locale::En).unwrap();
    assert!(label(burnburn_document::process::ProcessLabel::SetOptimization).contains("Optimization"));
    assert!(label(burnburn_document::process::ProcessLabel::SetPriority).contains("Priority"));
    let nl = message(
        MessageKey::UndoAction(burnburn_document::UndoLabel::Process(
            burnburn_document::process::ProcessLabel::SetPriority,
        )),
        Locale::Nl,
    )
    .unwrap();
    assert!(nl.contains("Prioriteit wijzigen"), "{nl}");
}

#[test]
fn the_capability_list_says_why_a_feature_is_not_available() {
    for locale in LOCALES {
        for feature in Feature::ALL {
            for support in
                [SupportView::Supported, SupportView::Unsupported, SupportView::Unverified("a reason".into())]
            {
                let line = cam_support_line(&CapabilityRow { feature, support }, locale);
                assert!(line.contains(':'), "{feature:?}: {line}");
                assert!(!line.starts_with(':'), "the feature has a name: {line}");
            }
        }
    }
    let z = cam_support_line(&CapabilityRow { feature: Feature::ZStep, support: SupportView::Unsupported }, Locale::En);
    assert!(z.contains("not supported") && z.contains("Z axis"), "{z}");
    let angle = cam_support_line(
        &CapabilityRow { feature: Feature::ScanAngle, support: SupportView::Unverified("x".into()) },
        Locale::Nl,
    );
    assert!(angle.contains("niet bewezen") && angle.contains("schuine"), "{angle}");
    let own = cam_support_line(
        &CapabilityRow { feature: Feature::Tabs, support: SupportView::Unverified("not proven here".into()) },
        Locale::En,
    );
    assert!(own.contains("not proven here"), "a reason the profile gave stays as written: {own}");
}

#[test]
fn a_changed_widget_becomes_one_intent_and_a_repeat_becomes_none() {
    let (mut h, doc) = rig(Support::Unsupported);
    let vm = open(&mut h, doc, CamDialog::Device);
    let same = DialogValue::Text("1500".into());
    assert_eq!(cam_field_intent(Some(&vm), doc, CamDialog::Device, DialogField::BedWidth, same), None);
    let other = DialogValue::Text("900".into());
    assert_eq!(
        cam_field_intent(Some(&vm), doc, CamDialog::Device, DialogField::BedWidth, other.clone()),
        Some(UiIntent::SetCamField {
            document: doc,
            dialog: CamDialog::Device,
            field: DialogField::BedWidth,
            value: other
        })
    );
    assert_eq!(
        cam_field_intent(None, doc, CamDialog::Device, DialogField::BedWidth, DialogValue::Text("1".into())),
        None
    );

    // A field with an error sends its text again, so a retry of the same text goes through.
    h.send(UiIntent::SetCamField {
        document: doc,
        dialog: CamDialog::Device,
        field: DialogField::BedWidth,
        value: DialogValue::Text("abc".into()),
    });
    h.send(UiIntent::CamDialogAction { document: doc, dialog: CamDialog::Device, action: DialogAction::Apply });
    let vm = h.controller().cam_dialog(doc, CamDialog::Device).unwrap();
    assert!(vm.row(DialogField::BedWidth).unwrap().error.is_some());
    assert!(
        cam_field_intent(Some(&vm), doc, CamDialog::Device, DialogField::BedWidth, DialogValue::Text("abc".into()))
            .is_some()
    );
}

#[test]
fn buttons_send_their_action_only_when_enabled_and_save_waits_for_a_path() {
    let (mut h, doc) = rig(Support::Unsupported);
    let vm = open(&mut h, doc, CamDialog::Device);
    assert_eq!(
        cam_action_intent(&vm, doc, CamDialog::Device, DialogActionKind::Apply, None),
        Some(UiIntent::CamDialogAction { document: doc, dialog: CamDialog::Device, action: DialogAction::Apply })
    );
    assert_eq!(
        cam_action_intent(&vm, doc, CamDialog::Device, DialogActionKind::ReturnToFlatbed, None),
        None,
        "there is no rotary to leave"
    );
    assert_eq!(
        cam_action_intent(&vm, doc, CamDialog::Device, DialogActionKind::Generate, None),
        None,
        "not a Device button"
    );

    let vm = open(&mut h, doc, CamDialog::FocusTest);
    assert_eq!(
        cam_action_intent(&vm, doc, CamDialog::FocusTest, DialogActionKind::Generate, None),
        None,
        "a focus test without Z does not start"
    );

    let vm = open(&mut h, doc, CamDialog::MaterialTest);
    assert_eq!(
        cam_action_intent(&vm, doc, CamDialog::MaterialTest, DialogActionKind::ExportRd, Some("/x.rd".into())),
        None,
        "nothing to save yet"
    );
    h.send(UiIntent::CamDialogAction {
        document: doc,
        dialog: CamDialog::MaterialTest,
        action: DialogAction::Generate,
    });
    h.run_tasks();
    let vm = h.controller().cam_dialog(doc, CamDialog::MaterialTest).unwrap();
    assert_eq!(cam_action_intent(&vm, doc, CamDialog::MaterialTest, DialogActionKind::ExportRd, None), None);
    let intent = cam_action_intent(&vm, doc, CamDialog::MaterialTest, DialogActionKind::ExportRd, Some("/x.rd".into()));
    assert_eq!(
        intent,
        Some(UiIntent::CamDialogAction {
            document: doc,
            dialog: CamDialog::MaterialTest,
            action: DialogAction::ExportRd("/x.rd".into())
        })
    );
    for locale in LOCALES {
        let text = cam_result_text(vm.result.as_ref().unwrap(), locale);
        assert!(text.contains("25") && text.contains("plan-"), "{text}");
        assert!(text.contains(if locale == Locale::Nl { "schatting" } else { "estimate" }), "{text}");
    }
}

#[test]
fn the_order_buttons_follow_the_row_and_move_the_level() {
    let (mut h, doc) = rig(Support::Unsupported);
    let vm = open(&mut h, doc, CamDialog::Optimization);
    assert_eq!(cam_move_intent(&vm, doc, OrderKey::Layer, true), None, "the first level cannot go up");
    assert_eq!(cam_move_intent(&vm, doc, OrderKey::Priority, false), None, "the last cannot go down");
    let down = cam_move_intent(&vm, doc, OrderKey::Layer, false).expect("down");
    assert_eq!(
        down,
        UiIntent::CamDialogAction {
            document: doc,
            dialog: CamDialog::Optimization,
            action: DialogAction::MoveDown(OrderKey::Layer)
        }
    );
    h.send(down);
    let vm = h.controller().cam_dialog(doc, CamDialog::Optimization).unwrap();
    assert_eq!(vm.order()[1].0, OrderKey::Layer);
}
