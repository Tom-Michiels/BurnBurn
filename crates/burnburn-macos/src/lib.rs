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
pub mod l10n;
pub mod l10n_files;
pub mod l10n_g5;
pub mod l10n_g6;
pub mod l10n_g6_werk;
pub mod l10n_g7_trace;
pub mod l10n_machine;
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
