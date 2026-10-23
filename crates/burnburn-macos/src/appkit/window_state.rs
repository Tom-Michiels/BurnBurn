//! Where the document window's frame and pane widths live between runs (LF7). The app keeps them
//! in the user's defaults; tests name a temporary suite instead, and a host that was told neither
//! (every other native test) keeps nothing. The text format and its validation are
//! `burnburn_host_models::window_state`.

use burnburn_host_models::window_state::SavedWindow;
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSString, NSUserDefaults};

const KEY: &str = "burnburn.window.frame";

pub(crate) enum WindowState {
    /// Nothing is read or written.
    Off,
    /// The user's own defaults (the running app).
    Standard,
    /// A temporary defaults domain (tests).
    Suite(Retained<NSUserDefaults>),
}

impl WindowState {
    /// A temporary suite; `None` when the name is not usable as a domain.
    pub(crate) fn suite(name: &str) -> Option<WindowState> {
        NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&NSString::from_str(name)))
            .map(WindowState::Suite)
    }

    fn defaults(&self) -> Option<Retained<NSUserDefaults>> {
        match self {
            WindowState::Off => None,
            WindowState::Standard => Some(NSUserDefaults::standardUserDefaults()),
            WindowState::Suite(defaults) => Some(defaults.clone()),
        }
    }

    /// Anything that is stored in the user's own defaults: the toolbar then keeps its
    /// customisation there too.
    pub(crate) fn is_standard(&self) -> bool {
        matches!(self, WindowState::Standard)
    }

    /// The saved window, when it exists and some of it is still on a screen.
    pub(crate) fn load(&self, mtm: MainThreadMarker) -> Option<SavedWindow> {
        let text = self.defaults()?.stringForKey(&NSString::from_str(KEY))?.to_string();
        let saved = SavedWindow::parse(&text)?;
        let screens: Vec<(f64, f64, f64, f64)> = NSScreen::screens(mtm)
            .iter()
            .map(|s| {
                let f = s.visibleFrame();
                (f.origin.x, f.origin.y, f.size.width, f.size.height)
            })
            .collect();
        saved.is_visible_on(&screens).then_some(saved)
    }

    #[allow(unsafe_code)]
    pub(crate) fn save(&self, saved: &SavedWindow) {
        if let Some(defaults) = self.defaults() {
            let text = NSString::from_str(&saved.to_text());
            // SAFETY: an NSString is a property-list value and the key is an NSString.
            unsafe { defaults.setObject_forKey(Some(&text), &NSString::from_str(KEY)) };
        }
    }
}
