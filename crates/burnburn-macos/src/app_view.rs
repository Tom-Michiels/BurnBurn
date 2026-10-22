//! The app-level panel models are shared (`burnburn-host-models`, ADR 0009); only the way a key
//! binding is drawn is the macOS host's own.

pub use burnburn_host_models::app_view::*;

use burnburn_host_models::l10n::Locale;
use burnburn_presentation::ShortcutsViewModel;

/// One line per register entry, sorted by name, with bindings drawn by this host's `binding_glyphs`.
pub fn shortcut_lines(view: &ShortcutsViewModel, filter: &str, locale: Locale) -> Vec<ShortcutLine> {
    shortcut_lines_with(view, filter, crate::app_menu::binding_glyphs, locale)
}
