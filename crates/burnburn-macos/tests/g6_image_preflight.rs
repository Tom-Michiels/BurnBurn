//! G6-machine, C05 on the Mac: the preflight finding for an unevenly scaled pass-through image has
//! its own words in both languages (not the generic "Finding:" fallback).

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::finding_text;
use burnburn_presentation::FindingKind;

#[test]
fn the_pass_through_finding_has_its_own_words() {
    let nl = finding_text(FindingKind::PassThroughResampled, Locale::Nl);
    let en = finding_text(FindingKind::PassThroughResampled, Locale::En);
    assert_ne!(nl, finding_text(FindingKind::StalePlan, Locale::Nl));
    assert_ne!(nl, "Melding:", "not the generic fallback");
    assert_ne!(en, "Finding:", "not the generic fallback");
    assert!(nl.contains("pixel") && en.contains("pixel"));
}
