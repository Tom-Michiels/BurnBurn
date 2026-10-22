//! BurnBurn's native Windows host (IM08, Win32 route via the `windows` crate; ADR 0006).
//!
//! [`adapter`] decodes Win32 messages (plain `msg`/`wParam`/`lParam` numbers) into intents for the
//! shared `burnburn-presentation` controller and has no Windows dependency: it is tested on every
//! platform. `win32` (Windows only) is the thin native layer: window, controls, file dialogs and the
//! DX12 canvas surface. Native code never mutates the document; it only produces intents.

// The hosts' one NL/EN catalogue lives in `burnburn-host-models` (ADR 0009).
pub use burnburn_host_models::{
    l10n, l10n_files, l10n_g5, l10n_g6, l10n_g6_werk, l10n_g7_editor, l10n_g7_trace, l10n_g7_werk, l10n_machine,
};

pub use burnburn_host_models::app_text;
pub use burnburn_host_models::panels_image;
pub use burnburn_host_models::panels_files;
pub use burnburn_host_models::toolbar;
pub use burnburn_host_models::recent;
pub use burnburn_host_models::quit_guard;
pub use burnburn_host_models::automation_host;

pub use burnburn_host_models::panels;
pub use burnburn_host_models::hub_panel;

pub mod adapter;
pub mod forms;
pub mod menus;
pub mod persist;
pub mod startup;
pub mod tasks;

// --- Headless view-model modules of the macOS host, compiled from its source files ----------------
// (W4-L8a) Same interim route as `l10n`: these files have no AppKit and only use presentation view
// models and `crate::l10n`, so both hosts show identical panel texts, rows and enablement. They move
// into `burnburn-presentation` when the contracts phase gets to them (contracts-phase-c §IM20).

#[cfg(windows)]
pub mod win32;

// W4-L8b: panel families (laser, image, files, camera, app) as plain data, plus the Windows
// `app_view` (shared Mac file) and the Win32 panel layer. The shared Mac files above are L8a's.
pub mod app_menu;
#[path = "../../burnburn-macos/src/app_view.rs"]
pub mod app_view;
pub mod host_panels;
#[cfg(windows)]
pub mod win32_panels;
