//! IM20 localisation contract for the hosts: one Rust catalogue (NL/EN) for every view-model
//! message key and every host-only text (menus, sheets, panels). Missing text is a test failure,
//! never an empty label on screen.

use burnburn_macos::l10n::{
    HostText, Locale, close_alert_title, host_text, known_message_keys, locale_from_preferred, message,
};
use burnburn_presentation::{CommandKey, FieldKey, MachineAction, MessageKey, OperationField, Outcome};

#[test]
fn all_message_keys_have_nl_and_en() {
    let keys = known_message_keys();
    // Every command and every field must be covered, not only a hand-picked few.
    for command in CommandKey::ALL {
        assert!(keys.contains(&MessageKey::Command(*command)), "catalogue misses command {command:?}");
    }
    for field in [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height] {
        assert!(keys.contains(&MessageKey::Field(field)), "catalogue misses field {field:?}");
    }
    let mut missing = Vec::new();
    for key in &keys {
        for locale in [Locale::Nl, Locale::En] {
            if message(*key, locale).is_none_or(|t| t.trim().is_empty()) {
                missing.push((*key, locale));
            }
        }
    }
    assert!(missing.is_empty(), "missing translations: {missing:?}");
}

/// Keys of the integration round (rotation, operations, text input, camera, outcome recording)
/// are in the catalogue, so the hosts never show a raw key for them.
#[test]
fn integration_round_keys_have_nl_and_en() {
    let mut keys = vec![
        MessageKey::Field(FieldKey::Rotation),
        MessageKey::NoFontAvailable,
        MessageKey::CameraNotAuthorized,
        MessageKey::VisualPlacementOnly,
    ];
    keys.extend([OperationField::Name, OperationField::Speed, OperationField::Power].map(MessageKey::InvalidNumber));
    keys.extend(Outcome::ALL.map(|o| MessageKey::MachineAction(MachineAction::RecordOutcome(o))));
    let known = known_message_keys();
    for key in keys {
        assert!(known.contains(&key), "catalogue does not list {key:?}");
        for locale in [Locale::Nl, Locale::En] {
            assert!(message(key, locale).is_some_and(|t| !t.trim().is_empty()), "{key:?} {locale:?}");
        }
    }
}

#[test]
fn all_host_texts_have_nl_and_en() {
    for text in HostText::ALL {
        for locale in [Locale::Nl, Locale::En] {
            assert!(!host_text(*text, locale).trim().is_empty(), "{text:?} has no {locale:?} text");
        }
    }
}

/// Dutch and English really differ (a catalogue that copies one language passes the test above).
#[test]
fn nl_and_en_are_different_languages() {
    let pairs = [
        (MessageKey::Command(CommandKey::Save), "Bewaar", "Save"),
        (MessageKey::Command(CommandKey::Undo), "Herstel", "Undo"),
        (MessageKey::Field(FieldKey::Width), "Breedte", "Width"),
        (MessageKey::SaveFailed, "Bewaren is mislukt.", "Saving failed."),
    ];
    for (key, nl, en) in pairs {
        assert_eq!(message(key, Locale::Nl).as_deref(), Some(nl));
        assert_eq!(message(key, Locale::En).as_deref(), Some(en));
    }
    assert_eq!(host_text(HostText::CloseDiscard, Locale::Nl), "Niet bewaren");
    assert_eq!(host_text(HostText::CloseDiscard, Locale::En), "Don’t Save");
    assert_eq!(close_alert_title("Plaat.burnburn", Locale::Nl), "Wil je de wijzigingen in “Plaat.burnburn” bewaren?");
    assert_eq!(
        close_alert_title("Plaat.burnburn", Locale::En),
        "Do you want to save the changes made to “Plaat.burnburn”?"
    );
}

/// Menus and sheets follow the system language (`NSLocale.preferredLanguages`, in order); a
/// language we do not ship falls through to the next preference, then to English.
#[test]
fn locale_follows_the_first_supported_preferred_language() {
    assert_eq!(locale_from_preferred(["nl-BE", "en-GB"]), Locale::Nl);
    assert_eq!(locale_from_preferred(["nl"]), Locale::Nl);
    assert_eq!(locale_from_preferred(["en-US", "nl-NL"]), Locale::En);
    assert_eq!(locale_from_preferred(["fr-BE", "nl-BE"]), Locale::Nl);
    assert_eq!(locale_from_preferred(["de-DE"]), Locale::En);
    assert_eq!(locale_from_preferred(Vec::<&str>::new()), Locale::En);
    // ISO 639-2 codes and odd casing / separators from the OS.
    assert_eq!(locale_from_preferred(["NL_be"]), Locale::Nl);
    assert_eq!(locale_from_preferred(["nld"]), Locale::Nl);
    // A language that merely starts with the letters "nl…" but is not Dutch does not count.
    assert_eq!(locale_from_preferred(["nlx"]), Locale::En);
}
