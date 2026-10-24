//! LF15: colour is never the only carrier of a status. Every tone that has a colour comes with a
//! symbol and a word, a toned text starts with the tone's glyph, and the machine status and the
//! preflight counts say their state in words in both languages.

use burnburn_host_models::l10n::{Locale, machine_status_text};
use burnburn_host_models::panels::cam_note;
use burnburn_host_models::style::{ColorRole, StatusTone, toned_text};
use burnburn_presentation::{DialogNote, MachineStatusKey};

const TONES: [StatusTone; 3] = [StatusTone::Neutral, StatusTone::Warning, StatusTone::Error];

/// A symbol-font glyph or pictograph (not a letter, digit, space or ordinary punctuation).
fn has_glyph(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0x2190..=0x2BFF | 0x1F000..=0x1FAFF | 0xE000..=0xF8FF))
}

#[test]
fn every_coloured_tone_has_a_symbol_a_glyph_and_a_word_in_both_languages() {
    for tone in TONES {
        if tone.role() != ColorRole::TextSecondary {
            assert!(!tone.symbol().is_empty() && !tone.glyph().is_empty(), "{tone:?}: colour without a symbol");
            assert!(!tone.name(true).is_empty() && !tone.name(false).is_empty(), "{tone:?}: colour without a word");
        }
    }
    let words: Vec<&str> = TONES.iter().map(|t| t.name(true)).collect();
    assert!(words.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn a_warning_or_error_text_starts_with_its_glyph_and_a_neutral_or_empty_text_is_left_alone() {
    assert_eq!(toned_text(StatusTone::Error, "Ongeldig"), format!("{} Ongeldig", StatusTone::Error.glyph()));
    assert_eq!(toned_text(StatusTone::Warning, "Let op"), format!("{} Let op", StatusTone::Warning.glyph()));
    assert_eq!(toned_text(StatusTone::Neutral, "Klaar"), "Klaar");
    assert_eq!(toned_text(StatusTone::Error, ""), "");
    // Already marked: not marked twice.
    let once = toned_text(StatusTone::Error, "Ongeldig");
    assert_eq!(toned_text(StatusTone::Error, &once), once);
}

#[test]
fn the_machine_status_is_said_in_words_that_differ_per_state_in_both_languages() {
    use MachineStatusKey as S;
    let all = [
        S::Idle,
        S::VerifyingIdentity,
        S::Reverifying,
        S::Uploading,
        S::UploadedNotConfirmed,
        S::Stored,
        S::StartRequested,
        S::Running,
        S::Completed,
        S::Failed,
        S::Cancelled,
        S::Unknown,
    ];
    // A new state must be added to the list above before this compiles.
    let _exhaustive = |s: S| match s {
        S::Idle
        | S::VerifyingIdentity
        | S::Reverifying
        | S::Uploading
        | S::UploadedNotConfirmed
        | S::Stored
        | S::StartRequested
        | S::Running
        | S::Completed
        | S::Failed
        | S::Cancelled
        | S::Unknown => (),
    };
    for locale in [Locale::Nl, Locale::En] {
        let texts: Vec<&str> = all.iter().map(|s| machine_status_text(*s, locale)).collect();
        for (state, text) in all.iter().zip(&texts) {
            assert!(text.chars().any(char::is_alphabetic), "{state:?}/{locale:?}: no word");
            assert!(!has_glyph(text), "{state:?}/{locale:?}: a glyph in a status word: {text}");
        }
        for (i, a) in texts.iter().enumerate() {
            assert!(texts[i + 1..].iter().all(|b| a != b), "{locale:?}: two states say '{a}'");
        }
    }
}

#[test]
fn the_preflight_counts_name_errors_and_warnings_and_the_clean_state_says_so() {
    for locale in [Locale::Nl, Locale::En] {
        let counts = cam_note(DialogNote::PreflightCounts { errors: 2, warnings: 1 }, locale);
        let (error_word, warning_word) = match locale {
            Locale::Nl => ("fouten", "waarschuwing"),
            Locale::En => ("errors", "warning"),
        };
        assert!(counts.contains(error_word) && counts.contains(warning_word), "{counts}");
        assert!(!has_glyph(&counts));
        let clean = cam_note(DialogNote::PreflightClean, locale);
        assert!(clean.chars().any(char::is_alphabetic) && !has_glyph(&clean));
    }
}
