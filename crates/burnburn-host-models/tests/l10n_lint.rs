//! Lint over the NL/EN catalogues (LF3): both languages exist and say the same thing in the same
//! form. Catalogue macros already force a text in both languages for every key; this checks the
//! content: same placeholders, no English in Dutch, one ellipsis rule, consistent colons, one verb
//! form for the standard buttons, no developer text.

use std::collections::BTreeSet;

use burnburn_host_models::app_text::{AppText, app_text};
use burnburn_host_models::l10n::{HostText, Locale, host_text};
use burnburn_host_models::l10n_empty::{EmptyText, empty_text};
use burnburn_host_models::l10n_files::{FilesText, files_text};
use burnburn_host_models::l10n_g5::{G5Text, g5_text};
use burnburn_host_models::l10n_g6::{G6Text, g6_text};
use burnburn_host_models::l10n_g6_werk::{G6Text as G6WerkText, g6_text as g6_werk_text};
use burnburn_host_models::l10n_g7_editor::{G7Text as G7EditorText, g7_text as g7_editor_text};
use burnburn_host_models::l10n_g7_trace::{G7Text as G7TraceText, g7_text as g7_trace_text};
use burnburn_host_models::l10n_g7_werk::{G7Text as G7WerkText, g7_text as g7_werk_text};
use burnburn_host_models::l10n_machine::{MachineText, machine_text};

struct Entry {
    key: String,
    nl: &'static str,
    en: &'static str,
}

fn entries() -> Vec<Entry> {
    let mut all = Vec::new();
    macro_rules! gather {
        ($prefix:literal, $ty:ty, $f:ident) => {
            for k in <$ty>::ALL {
                all.push(Entry {
                    key: format!("{}::{:?}", $prefix, k),
                    nl: $f(*k, Locale::Nl),
                    en: $f(*k, Locale::En),
                });
            }
        };
    }
    gather!("AppText", AppText, app_text);
    gather!("HostText", HostText, host_text);
    gather!("EmptyText", EmptyText, empty_text);
    gather!("FilesText", FilesText, files_text);
    gather!("G5Text", G5Text, g5_text);
    gather!("G6Text", G6Text, g6_text);
    gather!("G6WerkText", G6WerkText, g6_werk_text);
    gather!("G7EditorText", G7EditorText, g7_editor_text);
    gather!("G7TraceText", G7TraceText, g7_trace_text);
    gather!("G7WerkText", G7WerkText, g7_werk_text);
    gather!("MachineText", MachineText, machine_text);
    all
}

fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_placeholder = false;
    for c in text.chars() {
        match c {
            '{' => in_placeholder = true,
            '}' => in_placeholder = false,
            c if in_placeholder => {
                let _ = c;
            }
            c if c.is_alphabetic() => current.extend(c.to_lowercase()),
            _ => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn placeholders(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else { break };
        found.insert(rest[start..start + len + 1].to_owned());
        rest = &rest[start + len + 1..];
    }
    found
}

fn report(problems: Vec<String>) {
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn the_catalogues_are_not_empty_and_have_both_languages() {
    let all = entries();
    assert!(all.len() > 800, "only {} texts found; a catalogue went missing from the lint", all.len());
    let mut problems = Vec::new();
    for e in &all {
        if e.nl.trim().is_empty() != e.en.trim().is_empty() {
            problems.push(format!("{}: one language is empty (nl {:?}, en {:?})", e.key, e.nl, e.en));
        }
    }
    report(problems);
}

#[test]
fn both_languages_use_the_same_placeholders() {
    let mut problems = Vec::new();
    for e in entries() {
        if placeholders(e.nl) != placeholders(e.en) {
            problems.push(format!("{}: nl {:?} vs en {:?}", e.key, e.nl, e.en));
        }
    }
    report(problems);
}

/// Texts that are the same in both languages on purpose: product and protocol names, units,
/// and words that are spelled alike. Everything else identical in NL and EN is a missed translation.
fn same_in_both_languages(text: &str) -> bool {
    let letters: Vec<String> = words(text);
    // No word of four letters or more that could be translated: units, codes, numbers, symbols.
    letters.iter().all(|w| w.chars().count() <= 3) || ALLOWED_IDENTICAL.contains(&text)
}

const ALLOWED_IDENTICAL: &[&str] = &[
    // Product and technical names, and words Dutch and English spell alike.
    "Plan",
    "Help",
    "Zoom",
    "Platform",
    "Offset",
    "Status",
    "Camera",
    "Bytes",
    "BurnBurn",
    "Open…",
    "Import",
    "Machine",
    "Machine:",
    "Later",
    "Parameters",
    "Items:",
    "Templates",
    "Templates:",
    "Orders",
    "Order {0}",
    "{0} — {1} (order {2})",
    "Upload",
    "Upload {0}",
    "Boolean",
    "object",
    "contour",
    "✗ conflict",
    "{0} × {1} pixels",
    "pixels (96 per inch)",
    "Contrast (0…10)",
    "Splines (exact)",
    "Lens k1",
    "Lens k2",
    "Lens p1",
    "Lens p2",
];

#[test]
fn no_text_is_left_untranslated() {
    let mut problems = Vec::new();
    for e in entries() {
        if e.nl == e.en && !same_in_both_languages(e.nl) {
            problems.push(format!("{}: {:?}", e.key, e.nl));
        }
    }
    report(problems);
}

const ENGLISH_ONLY_WORDS: &[&str] = &[
    "the",
    "and",
    "with",
    "please",
    "error",
    "failed",
    "cancel",
    "apply",
    "settings",
    "preferences",
    "your",
    "you",
    "will",
    "from",
    "are",
    "this",
    "that",
    "not",
    "has",
    "have",
    "could",
    "selection",
    "close",
    "delete",
    "unknown",
    "invalid",
    "unsupported",
    "available",
    "enabled",
    "disabled",
    "default",
    "export",
    "import",
];

#[test]
fn dutch_texts_contain_no_english_words() {
    let mut problems = Vec::new();
    for e in entries() {
        if e.nl == e.en {
            continue;
        }
        let found: Vec<String> = words(e.nl).into_iter().filter(|w| ENGLISH_ONLY_WORDS.contains(&w.as_str())).collect();
        if !found.is_empty() {
            problems.push(format!("{}: {:?} contains {:?}", e.key, e.nl, found));
        }
    }
    report(problems);
}

#[test]
fn dutch_addresses_the_user_as_je_not_u() {
    let mut problems = Vec::new();
    for e in entries() {
        let found: Vec<String> = words(e.nl).into_iter().filter(|w| w == "u" || w == "uw").collect();
        if !found.is_empty() {
            problems.push(format!("{}: {:?}", e.key, e.nl));
        }
    }
    report(problems);
}

#[test]
fn an_ellipsis_is_one_character_at_the_end_and_in_both_languages_or_neither() {
    let mut problems = Vec::new();
    for e in entries() {
        for (lang, text) in [("nl", e.nl), ("en", e.en)] {
            if text.contains("...") {
                problems.push(format!("{} {lang}: three dots instead of … in {text:?}", e.key));
            }
            if text.trim_end().trim_end_matches('…').contains('…')
                && !text.contains('\n')
                && text.matches('…').count() > 1
            {
                problems.push(format!("{} {lang}: more than one ellipsis in {text:?}", e.key));
            }
        }
        if e.nl.trim_end().ends_with('…') != e.en.trim_end().ends_with('…') {
            problems.push(format!("{}: ellipsis differs (nl {:?}, en {:?})", e.key, e.nl, e.en));
        }
    }
    report(problems);
}

#[test]
fn only_menu_items_that_open_a_dialog_end_in_an_ellipsis() {
    use AppText as A;
    let opens_a_dialog = [A::Preferences, A::Options, A::Shortcuts, A::InspectRd, A::MachineHub, A::SupportData];
    let does_not = [
        A::About,
        A::QuitApp,
        A::HideApp,
        A::HideOthers,
        A::ShowAll,
        A::Minimize,
        A::ZoomWindow,
        A::BringAllToFront,
        A::Manual,
        A::UndoHistory,
        A::Preview,
        A::DebugLog,
        A::MenuWindow,
        A::MenuHelp,
        A::Services,
    ];
    let mut problems = Vec::new();
    for item in opens_a_dialog {
        for locale in [Locale::Nl, Locale::En] {
            if !app_text(item, locale).ends_with('…') {
                problems.push(format!("{item:?} opens a dialog but has no …: {:?}", app_text(item, locale)));
            }
        }
    }
    for item in does_not {
        for locale in [Locale::Nl, Locale::En] {
            if app_text(item, locale).ends_with('…') {
                problems.push(format!("{item:?} opens no dialog but ends in …: {:?}", app_text(item, locale)));
            }
        }
    }
    report(problems);
}

#[test]
fn colons_are_consistent() {
    let mut problems = Vec::new();
    for e in entries() {
        for (lang, text) in [("nl", e.nl), ("en", e.en)] {
            if text.contains(" :") || text.contains("::") {
                problems.push(format!("{} {lang}: space before or double colon in {text:?}", e.key));
            }
        }
        // A label that ends in a colon does so in both languages.
        if e.nl.trim_end().ends_with(':') != e.en.trim_end().ends_with(':') {
            problems.push(format!("{}: colon differs (nl {:?}, en {:?})", e.key, e.nl, e.en));
        }
    }
    report(problems);
}

#[test]
fn standard_buttons_use_one_verb_form() {
    let nl: BTreeSet<&str> = entries().iter().map(|e| e.nl).collect();
    let mut problems = Vec::new();
    for (imperative, infinitive) in [("Annuleer", "Annuleren"), ("Sluit", "Sluiten"), ("Pas toe", "Toepassen")] {
        if nl.contains(imperative) && nl.contains(infinitive) {
            problems.push(format!("both {imperative:?} and {infinitive:?} are used as button text"));
        }
    }
    report(problems);
}

#[test]
fn no_developer_text_reaches_the_user() {
    let mut problems = Vec::new();
    for e in entries() {
        for (lang, text) in [("nl", e.nl), ("en", e.en)] {
            if text.contains(":?}")
                || text.contains("Some(")
                || text.contains("None") && lang == "nl"
                || text.contains("todo")
            {
                problems.push(format!("{} {lang}: {text:?}", e.key));
            }
        }
    }
    report(problems);
}
