//! LF3 (Q7): default names and engine hints follow the UI language; names the user typed never do.

use burnburn_generators::Symbology;
use burnburn_host_models::l10n::Locale;
use burnburn_host_models::l10n_names::{display_name, payload_hint_text};

#[test]
fn default_names_follow_the_ui_language() {
    assert_eq!(display_name("Naamloos", Locale::En), "Untitled");
    assert_eq!(display_name("Naamloos 3", Locale::En), "Untitled 3");
    assert_eq!(display_name("Rechthoek 1", Locale::En), "Rectangle 1");
    assert_eq!(display_name("Afgeronde rechthoek 12", Locale::En), "Rounded Rectangle 12");
    assert_eq!(display_name("Schets 2", Locale::En), "Sketch 2");
    assert_eq!(display_name("Hersteld 2", Locale::En), "Recovered 2");
    assert_eq!(display_name("Rechthoek 1 (\u{d7}2)", Locale::En), "Rectangle 1 (\u{d7}2)");
}

#[test]
fn the_dutch_ui_shows_the_stored_names_unchanged() {
    for name in ["Naamloos", "Rechthoek 1", "Schets 2", "Hersteld 1"] {
        assert_eq!(display_name(name, Locale::Nl), name);
    }
}

#[test]
fn a_name_the_user_typed_is_never_translated() {
    for name in
        ["Offerte", "Rechthoek mijn deur", "Schets", "Naamloos concept", "Rechthoek 1 kopie", "Rechthoekje 4", ""]
    {
        assert_eq!(display_name(name, Locale::En), name, "{name:?}");
    }
}

#[test]
fn every_default_object_name_the_editor_creates_has_an_english_form() {
    for base in [
        "Rechthoek",
        "Driehoek",
        "Vijfhoek",
        "Achthoek",
        "Ellips",
        "Cirkel",
        "Lijn",
        "Polylijn",
        "Veelhoek",
        "Ster",
        "Dubbele ster",
        "Afgeronde rechthoek",
    ] {
        let shown = display_name(&format!("{base} 1"), Locale::En);
        assert!(shown.ends_with(" 1") && shown != format!("{base} 1"), "{base}: {shown}");
    }
}

#[test]
fn engine_hints_are_dutch_in_the_dutch_ui_and_untouched_in_english() {
    assert_eq!(payload_hint_text("text or bytes, up to version 10", Locale::Nl), "tekst of bytes, tot versie 10");
    assert_eq!(payload_hint_text("text or bytes, up to version 10", Locale::En), "text or bytes, up to version 10");
}

#[test]
fn every_symbology_hint_has_a_dutch_translation_without_english_words() {
    let english = [
        "text",
        "digits",
        "or",
        "and",
        "error",
        "correction",
        "not",
        "available",
        "check",
        "digit",
        "only",
        "with",
        "added",
        "no",
        "up",
        "to",
        "start",
        "stop",
        "data",
        "space",
        "characters",
        "version",
        "versions",
    ];
    for symbology in Symbology::ALL {
        let hint = symbology.capabilities().payload_hint;
        let nl = payload_hint_text(hint, Locale::Nl);
        // Hints that are only codes and numbers are the same in both languages.
        assert!(nl != hint || hint == "ASCII 32-127", "{symbology:?} has no Dutch hint: {hint}");
        let lower = nl.to_lowercase();
        for word in english {
            assert!(!lower.split(|c: char| !c.is_alphabetic()).any(|w| w == word), "{symbology:?}: {nl}");
        }
    }
}
