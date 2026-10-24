//! The saved document window on Windows (LF14): the shared [`SavedWindow`] (frame and pane
//! widths) plus the maximised flag, in `%LOCALAPPDATA%\BurnBurn\window-state.txt`. Pixels, origin
//! top left; the file is plain text so a damaged one is simply ignored.

use std::io;
use std::path::{Path, PathBuf};

pub use burnburn_host_models::window_state::SavedWindow;

/// The width of the object list and of the inspector on a first start, in pixels at 96 dpi.
pub const DEFAULT_SIDEBAR: f64 = 220.0;
pub const DEFAULT_INSPECTOR: f64 = 280.0;

/// The window as stored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoredWindow {
    pub window: SavedWindow,
    pub maximized: bool,
}

/// `<local app data>\BurnBurn\window-state.txt`.
pub fn state_file(local_app_data: &Path) -> PathBuf {
    local_app_data.join("BurnBurn").join("window-state.txt")
}

impl StoredWindow {
    /// The file text: the shared window line, then `maximized=0|1`.
    pub fn to_text(&self) -> String {
        format!("{}\nmaximized={}\n", self.window.to_text(), u8::from(self.maximized))
    }

    /// The window of a file text; `None` for anything that is not a window.
    pub fn parse(text: &str) -> Option<StoredWindow> {
        let mut lines = text.lines();
        let window = SavedWindow::parse(lines.next()?)?;
        let maximized = match lines.next().map(str::trim) {
            None | Some("maximized=0") => false,
            Some("maximized=1") => true,
            Some(_) => return None,
        };
        Some(StoredWindow { window, maximized })
    }
}

/// Reads the stored window; a missing or damaged file is `None`.
pub fn load(path: &Path) -> Option<StoredWindow> {
    StoredWindow::parse(&std::fs::read_to_string(path).ok()?)
}

/// Writes the stored window, creating the folder. The text goes to a neighbour file first and is
/// renamed over the old one, so a crash never leaves half a file.
pub fn save(path: &Path, window: &StoredWindow) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, window.to_text())?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// The window to open: the stored one when enough of it lies on one of the `screens` (x, y, width,
/// height of the work areas), else `default`.
pub fn placement(
    stored: Option<StoredWindow>,
    screens: &[(f64, f64, f64, f64)],
    default: StoredWindow,
) -> StoredWindow {
    stored.filter(|s| s.window.is_visible_on(screens)).unwrap_or(default)
}

/// The window of a first start: four fifths of the work area, centred, panes at their usual width.
pub fn default_window(work_area: (f64, f64, f64, f64)) -> StoredWindow {
    let (x, y, w, h) = work_area;
    StoredWindow {
        window: SavedWindow {
            x: x + 0.1 * w,
            y: y + 0.1 * h,
            width: 0.8 * w,
            height: 0.8 * h,
            sidebar: DEFAULT_SIDEBAR,
            inspector: DEFAULT_INSPECTOR,
        },
        maximized: false,
    }
}
