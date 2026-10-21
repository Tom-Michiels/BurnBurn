//! BurnBurn's native Windows host (IM08, Win32 route via the `windows` crate; ADR 0006).
//!
//! [`adapter`] decodes Win32 messages (plain `msg`/`wParam`/`lParam` numbers) into intents for the
//! shared `burnburn-presentation` controller and has no Windows dependency: it is tested on every
//! platform. `win32` (Windows only) is the thin native layer: window, controls, file dialogs and the
//! DX12 canvas surface. Native code never mutates the document; it only produces intents.

pub mod adapter;
/// The hosts' one NL/EN catalogue, compiled from the macOS host's source file so both hosts show
/// the same strings (interim until it moves into `burnburn-presentation`, contracts-phase-c §IM20).
#[path = "../../burnburn-macos/src/l10n.rs"]
pub mod l10n;
pub mod tasks;
#[cfg(windows)]
pub mod win32;

// W4-L8b: the Mac host's plain-data panel models (no AppKit in them) compiled for the Windows host
// too, so both hosts show the same texts, enablement and intents. Windows adds only control ids,
// message decoding and control layout (`panel_*`, `host_panels`). The `crate::` paths inside these
// files resolve to the module names below.
pub mod app_menu;
#[path = "../../burnburn-macos/src/app_text.rs"]
pub mod app_text;
#[path = "../../burnburn-macos/src/app_view.rs"]
pub mod app_view;
pub mod host_panels;
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
pub mod win32_panels;
