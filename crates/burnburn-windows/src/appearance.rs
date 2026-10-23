//! Light or dark: how the Windows host reads the system appearance (LFB-rest, W4/LF14). Plain data
//! and translation only; the registry read and the canvas theme (`Theme::dark()`) are in `win32`.

/// The appearance the apps should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

/// Registry key (below `HKEY_CURRENT_USER`) that holds the app theme choice.
pub const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
/// DWORD value in [`PERSONALIZE_KEY`]: 0 = apps use the dark theme, 1 = light.
pub const APPS_USE_LIGHT_THEME: &str = "AppsUseLightTheme";
/// The `lParam` string of `WM_SETTINGCHANGE` that announces a theme switch.
pub const THEME_CHANGED_SETTING: &str = "ImmersiveColorSet";

/// The appearance for the value of `AppsUseLightTheme` (`None` when it is absent or unreadable:
/// older Windows versions have no dark mode, so the answer is light).
pub fn appearance_from_apps_use_light_theme(value: Option<u32>) -> Appearance {
    match value {
        Some(0) => Appearance::Dark,
        _ => Appearance::Light,
    }
}

/// Whether a `WM_SETTINGCHANGE` with this `lParam` string means the theme changed.
pub fn is_theme_change(setting: Option<&str>) -> bool {
    setting == Some(THEME_CHANGED_SETTING)
}
