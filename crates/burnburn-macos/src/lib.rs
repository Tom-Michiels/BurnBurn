//! BurnBurn's native macOS host.
//!
//! [`adapter`] translates native widget events (plain data) into `burnburn-presentation` intents
//! and view-model data into widget text; [`tasks`] runs presentation tasks on worker threads. Both
//! have no AppKit dependency and are tested headlessly on every platform. `appkit` (macOS only) is
//! the thin objc2 layer. Native callbacks never mutate the document; they only produce intents.

pub mod accessibility;
pub mod adapter;
pub mod app_menu;
pub mod app_text;
pub mod app_view;
#[cfg(target_os = "macos")]
pub mod appkit;
pub mod automation_host;
pub mod calib;
pub mod camera;
pub mod hub_panel;
// Shared with the Windows host through `burnburn-host-models` (ADR 0009).
pub use burnburn_host_models::{
    l10n, l10n_files, l10n_g5, l10n_g6, l10n_g6_werk, l10n_g7_editor, l10n_g7_trace, l10n_g7_werk, l10n_machine,
};
pub mod lifecycle;
pub mod panels;
pub mod panels_files;
pub mod panels_image;
pub mod pasteboard;
pub mod quit_guard;
pub mod recent;
pub mod tasks;
pub mod text_input;
pub mod toolbar;
