//! Golf-7 integration: the preflight error for a blocked outline (derived geometry whose source no
//! longer evaluates) has its own words in both languages, not the generic "Finding:" fallback.

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::finding_text;
use burnburn_presentation::FindingKind;

#[test]
fn a_blocked_outline_says_why_it_is_not_produced() {
    let nl = finding_text(FindingKind::BlockedGeometry, Locale::Nl);
    let en = finding_text(FindingKind::BlockedGeometry, Locale::En);
    assert_ne!(nl, "Melding:", "not the generic fallback");
    assert_ne!(en, "Finding:", "not the generic fallback");
    assert!(en.contains("not produced") && nl.contains("niet gemaakt"), "{nl} / {en}");
}
