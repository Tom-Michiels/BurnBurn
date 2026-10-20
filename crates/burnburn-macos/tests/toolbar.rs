//! Tool strip texts, headless: labels, tooltips with shortcut, panel titles and the reasons of
//! disabled tools, from `assets/l10n/tools.toml`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::l10n::Locale;
use burnburn_macos::toolbar::{panel_title, reason_text, spec_for_tag, tag_for_id, tool_label, tool_tip, tooltip};
use burnburn_presentation::{DisabledReason, Enablement, PanelId, ToolCatalog};

const LOCALES: [Locale; 2] = [Locale::Nl, Locale::En];

#[test]
fn every_catalog_id_has_a_label_in_both_languages() {
    for spec in ToolCatalog::standard().specs() {
        for locale in LOCALES {
            let label = tool_label(spec.id, locale);
            assert!(!label.trim().is_empty(), "{} has no {locale:?} label", spec.id);
            assert_ne!(label, spec.id, "{} shows its id as label in {locale:?}", spec.id);
        }
    }
}

#[test]
fn nl_and_en_labels_differ_for_a_translated_tool() {
    assert_eq!(tool_label("sketch.line", Locale::Nl), "Lijn");
    assert_eq!(tool_label("sketch.line", Locale::En), "Line");
}

#[test]
fn tools_from_the_text_file_carry_an_explanation_in_both_languages() {
    for spec in ToolCatalog::standard().specs() {
        for locale in LOCALES {
            let tip = tool_tip(spec.id, locale);
            // Not in the text file yet (reported to H-assets): these show their command's name.
            if matches!(spec.id, "sketch.pen" | "sketch.polyline" | "sketch.ellipse.box") {
                continue;
            }
            assert!(tip.is_some_and(|t| !t.trim().is_empty()), "{} has no {locale:?} tip", spec.id);
        }
    }
}

#[test]
fn panel_titles_follow_the_language() {
    assert_eq!(panel_title(PanelId::Make, Locale::Nl), "MAKEN");
    assert_eq!(panel_title(PanelId::Make, Locale::En), "CREATE");
    for panel in PanelId::ALL {
        for locale in LOCALES {
            assert!(!panel_title(panel, locale).is_empty(), "{panel:?} {locale:?}");
        }
    }
}

#[test]
fn an_enabled_tooltip_is_label_shortcut_and_explanation() {
    let tip = tooltip("sketch.rectangle.two_point", Some("R"), Enablement::Enabled, Locale::En);
    assert!(tip.starts_with("Rectangle: two corners (R) — "), "{tip}");
    assert!(tip.contains("click one corner"), "{tip}");
}

#[test]
fn a_tooltip_without_shortcut_has_no_empty_parentheses() {
    let tip = tooltip("sketch.rectangle.center", None, Enablement::Enabled, Locale::En);
    assert!(!tip.contains("()"), "{tip}");
    assert!(tip.starts_with("Center rectangle — "), "{tip}");
}

#[test]
fn a_disabled_tooltip_names_the_reason_instead_of_the_explanation() {
    let tip = tooltip("sketch.fillet", Some("F"), Enablement::Disabled(DisabledReason::NotAvailable), Locale::Nl);
    assert!(tip.starts_with("Hoekafronding — "), "{tip}");
    assert!(tip.ends_with(reason_text(DisabledReason::NotAvailable, Locale::Nl)), "{tip}");
    assert!(tip.contains("Nog niet beschikbaar"), "{tip}");
}

#[test]
fn every_reason_has_text_in_both_languages_and_they_differ() {
    for reason in [
        DisabledReason::NotAvailable,
        DisabledReason::Follows,
        DisabledReason::NeedsSketch,
        DisabledReason::NoDocument,
        DisabledReason::OperatorRole,
        DisabledReason::NeedsSelection,
        DisabledReason::NeedsNodes,
    ] {
        let (nl, en) = (reason_text(reason, Locale::Nl), reason_text(reason, Locale::En));
        assert!(!nl.is_empty() && !en.is_empty());
        assert_ne!(nl, en, "{reason:?}");
    }
}

#[test]
fn native_tags_map_back_to_the_same_catalog_entry() {
    for spec in ToolCatalog::standard().specs() {
        let tag = tag_for_id(spec.id).unwrap();
        assert_eq!(spec_for_tag(tag).unwrap().id, spec.id);
    }
    assert!(spec_for_tag(-1).is_none());
    assert!(spec_for_tag(10_000).is_none());
}

#[test]
fn a_measurement_is_read_out_in_the_window_language() {
    let m = burnburn_presentation::ToolMeasurement {
        distance_text: "50".into(),
        dx_text: "30".into(),
        dy_text: "40".into(),
    };
    assert_eq!(burnburn_macos::toolbar::measurement_text(&m, Locale::Nl), "Afstand 50 mm (dx 30 mm, dy 40 mm)");
    assert_eq!(burnburn_macos::toolbar::measurement_text(&m, Locale::En), "Distance 50 mm (dx 30 mm, dy 40 mm)");
}
