//! The dark title bar (LF14): which `DwmSetWindowAttribute` call makes the frame's title bar follow
//! the system appearance, as plain data. The call itself is in `win32`.

use crate::appearance::Appearance;

/// `DWMWA_USE_IMMERSIVE_DARK_MODE` since Windows 10 build 19041.
pub const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
/// The same switch before that build (undocumented value, from build 17763).
pub const DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1: u32 = 19;
/// First build with the documented attribute (Windows 10 2004).
pub const BUILD_DOCUMENTED_ATTRIBUTE: u32 = 19041;
/// First build with a dark title bar at all (Windows 10 1809).
pub const BUILD_FIRST_DARK_TITLE_BAR: u32 = 17763;

/// One `DwmSetWindowAttribute(hwnd, attribute, &value, 4)` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DarkTitleBar {
    pub attribute: u32,
    /// The `BOOL`: 1 for a dark title bar, 0 for a light one.
    pub value: i32,
}

/// The call for `appearance` on Windows build `os_build`; `None` on a build without dark title bars
/// (or when the build is unknown, 0).
pub fn dark_title_bar(appearance: Appearance, os_build: u32) -> Option<DarkTitleBar> {
    let attribute = match os_build {
        b if b >= BUILD_DOCUMENTED_ATTRIBUTE => DWMWA_USE_IMMERSIVE_DARK_MODE,
        b if b >= BUILD_FIRST_DARK_TITLE_BAR => DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1,
        _ => return None,
    };
    Some(DarkTitleBar { attribute, value: i32::from(appearance == Appearance::Dark) })
}
