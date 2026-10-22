//! LF3 (Q10): the settings item and window per host.

use burnburn_host_models::app_text::{AppText, app_text};
use burnburn_host_models::l10n::Locale;

#[test]
fn the_mac_settings_item_is_called_settings_and_the_window_title_has_no_ellipsis() {
    assert_eq!(app_text(AppText::Preferences, Locale::Nl), "Instellingen…");
    assert_eq!(app_text(AppText::Preferences, Locale::En), "Settings…");
    assert_eq!(app_text(AppText::PrefsTitle, Locale::Nl), "Instellingen");
    assert_eq!(app_text(AppText::PrefsTitle, Locale::En), "Settings");
}

#[test]
fn windows_keeps_its_own_word_for_the_same_window() {
    assert_eq!(app_text(AppText::Options, Locale::Nl), "Opties…");
    assert_eq!(app_text(AppText::Options, Locale::En), "Options…");
    assert_eq!(app_text(AppText::OptionsTitle, Locale::Nl), "Opties");
    assert_eq!(app_text(AppText::OptionsTitle, Locale::En), "Options");
}
