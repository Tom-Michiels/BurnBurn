//! BurnBurn's native Windows host (IM08, Win32 route via the `windows` crate; ADR 0006).
//!
//! [`adapter`] decodes Win32 messages (plain `msg`/`wParam`/`lParam` numbers) into intents for the
//! shared `burnburn-presentation` controller and has no Windows dependency: it is tested on every
//! platform. `win32` (Windows only) is the thin native layer: window, controls, file dialogs and the
//! DX12 canvas surface. Native code never mutates the document; it only produces intents.

pub mod adapter;
pub mod forms;
/// The hosts' one NL/EN catalogue, compiled from the macOS host's source file so both hosts show
/// the same strings (interim until it moves into `burnburn-presentation`, contracts-phase-c §IM20).
#[path = "../../burnburn-macos/src/l10n.rs"]
pub mod l10n;
pub mod menus;
pub mod tasks;

// --- Headless view-model modules of the macOS host, compiled from its source files ----------------
// (W4-L8a) Same interim route as `l10n`: these files have no AppKit and only use presentation view
// models and `crate::l10n`, so both hosts show identical panel texts, rows and enablement. They move
// into `burnburn-presentation` when the contracts phase gets to them (contracts-phase-c §IM20).
#[path = "../../burnburn-macos/src/app_text.rs"]
pub mod app_text;
#[path = "../../burnburn-macos/src/l10n_files.rs"]
pub mod l10n_files;
#[path = "../../burnburn-macos/src/panels/mod.rs"]
pub mod panels;
#[path = "../../burnburn-macos/src/panels_files.rs"]
pub mod panels_files;
#[path = "../../burnburn-macos/src/panels_image.rs"]
pub mod panels_image;
#[path = "../../burnburn-macos/src/toolbar.rs"]
pub mod toolbar;

#[cfg(windows)]
pub mod win32;
