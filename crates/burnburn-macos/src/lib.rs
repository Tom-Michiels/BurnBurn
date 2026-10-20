//! BurnBurn's native macOS host.
//!
//! [`adapter`] translates native widget events (plain data) into `burnburn-presentation` intents
//! and view-model data into widget text; [`tasks`] runs presentation tasks on worker threads. Both
//! have no AppKit dependency and are tested headlessly on every platform. `appkit` (macOS only) is
//! the thin objc2 layer. Native callbacks never mutate the document; they only produce intents.

pub mod accessibility;
pub mod adapter;
#[cfg(target_os = "macos")]
pub mod appkit;
pub mod calib;
pub mod camera;
pub mod l10n;
pub mod lifecycle;
pub mod panels;
pub mod pasteboard;
pub mod recent;
pub mod tasks;
pub mod text_input;
pub mod toolbar;
