//! LFB-rest: the Windows canvas follows the system's dark mode. The registry value
//! `AppsUseLightTheme` translates to an appearance, and a `WM_SETTINGCHANGE` announces a change.
//! (The registry read and `Theme::dark()` on the canvas are in `win32`: Windows runner.)

use burnburn_windows::appearance::{
    APPS_USE_LIGHT_THEME, Appearance, PERSONALIZE_KEY, THEME_CHANGED_SETTING, appearance_from_apps_use_light_theme,
    is_theme_change,
};

#[test]
fn zero_means_the_apps_use_the_dark_theme_and_one_the_light_theme() {
    assert_eq!(appearance_from_apps_use_light_theme(Some(0)), Appearance::Dark);
    assert_eq!(appearance_from_apps_use_light_theme(Some(1)), Appearance::Light);
}

#[test]
fn a_missing_or_odd_value_is_light_never_a_panic() {
    assert_eq!(appearance_from_apps_use_light_theme(None), Appearance::Light, "no value: an older Windows");
    for odd in [2, 0xFFFF_FFFF] {
        assert_eq!(appearance_from_apps_use_light_theme(Some(odd)), Appearance::Light, "{odd}");
    }
    assert_eq!(Appearance::default(), Appearance::Light);
}

#[test]
fn only_the_immersive_colour_set_broadcast_is_a_theme_change() {
    assert!(is_theme_change(Some(THEME_CHANGED_SETTING)));
    assert!(!is_theme_change(Some("intl")));
    assert!(!is_theme_change(Some("")));
    assert!(!is_theme_change(None));
}

#[test]
fn the_registry_names_are_the_documented_ones() {
    assert_eq!(PERSONALIZE_KEY, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    assert_eq!(APPS_USE_LIGHT_THEME, "AppsUseLightTheme");
}
