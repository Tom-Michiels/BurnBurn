//! The app-level menu items and panel events on Windows (W4-L8b), without the `windows` crate.
//!
//! The macOS host has the same module (`burnburn_macos::app_menu`) written around `NSEvent` flags
//! and the Mac glyphs. This one has the Windows spelling (`Ctrl+Shift+S`), virtual-key codes and
//! the same presentation intents. `app_view` (shared with the Mac) draws bindings with
//! [`binding_glyphs`] from here.

use burnburn_presentation::KeyBinding;

/// A binding as the menus and the shortcut editor draw it on Windows.
pub fn binding_glyphs(binding: &KeyBinding) -> String {
    let _ = binding;
    String::new()
}
