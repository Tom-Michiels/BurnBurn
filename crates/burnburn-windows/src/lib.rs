//! BurnBurn's native Windows host (IM08, Win32 route via the `windows` crate; ADR 0006).
//!
//! [`adapter`] decodes Win32 messages (plain `msg`/`wParam`/`lParam` numbers) into intents for the
//! shared `burnburn-presentation` controller and has no Windows dependency: it is tested on every
//! platform. `win32` (Windows only) is the thin native layer: window, controls, file dialogs and the
//! DX12 canvas surface. Native code never mutates the document; it only produces intents.

// Platform-free models shared with the macOS host live in `burnburn-host-models` (ADR 0009; they were
// compiled from the macOS sources through `#[path]` before). Only `app_view` has a host gevel.
pub use burnburn_host_models::{
    app_text, automation_host, format, hub_panel, l10n, l10n_files, l10n_g5, l10n_g6, l10n_g6_werk, l10n_g7_editor,
    l10n_g7_trace, l10n_g7_werk, l10n_machine, l10n_names, panels, panels_files, panels_image, quit_guard, recent,
    style, toolbar,
};

pub mod adapter;
pub mod app_view;
pub mod appearance;
pub mod forms;
pub mod menus;
pub mod persist;
pub mod startup;
pub mod tasks;

#[cfg(windows)]
pub mod win32;

// W4-L8b: panel families (laser, image, files, camera, app) as plain data and the Win32 panel layer.
pub mod app_menu;
pub mod host_panels;
#[cfg(windows)]
pub mod win32_panels;
