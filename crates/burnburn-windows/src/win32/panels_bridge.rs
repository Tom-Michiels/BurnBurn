//! The host side of the W4-L8b panels: what the [`Bridge`] needs from the controller. A child of
//! `win32`, so it can use the host state (`with_host`, `dispatch`) without widening any visibility.
//!
//! **Unconfirmed**: type-checked from macOS, never run on Windows.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use burnburn_presentation::{DiagnosticOptions, UiIntent};
use burnburn_types::DocumentId;

use super::{dispatch, system_locale, with_host};
use crate::l10n::Locale;
use crate::win32_panels::manager::{self, Bridge, Snapshot, rgba_of};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

struct PanelBridge;

impl Bridge for PanelBridge {
    fn snapshot(&self, document: Option<DocumentId>) -> Option<Snapshot> {
        with_host(|h| {
            let c = &h.controller;
            let vm = c.view();
            let doc = document.or(vm.active);
            Snapshot {
                laser: c.laser_view_for(doc),
                machine: c.machine_view(),
                camera: c.camera_view(),
                workspace: c.camera_workspace_view(),
                photo: c.alignment_photo().map(|f| (f.image.width(), f.image.height(), rgba_of(&f.image))),
                prefs: c.prefs_view(),
                shortcuts: c.shortcuts_view(),
                history: doc.and_then(|d| c.history_view(d)),
                preview: doc.and_then(|d| c.preview_view(d)),
                rd: doc.and_then(|d| c.rd_inspect_view(d)),
                debug: c.debug_log_view(),
                recent_projects: c.recent_projects(),
                recent_templates: c.recent_templates(),
                vm,
            }
        })
    }

    fn dispatch(&self, intent: UiIntent) {
        dispatch(intent);
    }

    fn support_report(&self, options: &DiagnosticOptions) -> String {
        with_host(|h| h.controller.support_report(options)).unwrap_or_default()
    }

    fn locale(&self) -> Locale {
        system_locale()
    }
}

/// `%LOCALAPPDATA%\BurnBurn`: the libraries (materials, art) live here, as on the Mac next to the
/// recent-projects file.
pub(super) fn library_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("BurnBurn");
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(e) => {
            eprintln!("burnburn-win: library folder {}: {e}", dir.display());
            None
        }
    }
}

/// The layer palette as 8-bit sRGB, slot by slot (the canvas colours, so exports match the screen).
fn layer_palette() -> Vec<[u8; 3]> {
    let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    burnburn_render::Theme::light().process_layers.iter().map(|c| [channel(c.r), channel(c.g), channel(c.b)]).collect()
}

/// Connects the panels: the library folder and palette, the camera port (Media Foundation, never
/// run on a real camera: see `win32::camera`) and the panel windows' events. Call once after the
/// host state exists.
pub(super) fn install() {
    let dir = library_dir();
    let message_window = with_host(|h| h.message_window.0 as isize);
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Some(target) = message_window {
            // SAFETY: PostMessageW is thread-safe; the handle is our message window (a plain value).
            let _ = unsafe { PostMessageW(Some(HWND(target as *mut _)), super::WM_CAMERA_FRAME, WPARAM(0), LPARAM(0)) };
        }
    });
    with_host(|h| {
        h.controller.set_library_dir(dir);
        h.controller.set_layer_palette(layer_palette());
        h.controller.attach_camera(Arc::new(super::camera::WindowsCameraPort::new(wake)));
    });
    manager::install(Rc::new(PanelBridge));
}
