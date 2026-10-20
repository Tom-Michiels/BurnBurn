//! Host side of the camera window (W3-UI-camera): opening it from the camera section, copying the
//! presentation's camera workspace into it, the photo in which alignment marks are clicked, and the
//! delivery of finished corner detections and lens fits. A child module of `app`, so it can use the
//! host's private state.
//!
//! Nothing here opens a camera or asks for permission: the access button of the main window's
//! camera section does that, and this window's "use camera" button only starts a camera the OS
//! already allowed.

use burnburn_image::SourceImage;
use burnburn_presentation::UiIntent;

use super::HostApp;
use crate::appkit::panels::camera_window::{CameraParts, camera_window};
use crate::panels::{
    CameraCommand, camera_command_intent, camera_window_panel, height_intent, margin_intent, next_unplaced_mark,
    photo_click_intent, photo_with_marks,
};

/// RGBA bytes of a frame, whatever its stored format.
fn rgba_of(image: &SourceImage) -> Vec<u8> {
    if image.format() == burnburn_image::PixelFormat::Rgba8 {
        return image.samples().to_vec();
    }
    let mut out = Vec::with_capacity(image.width() as usize * image.height() as usize * 4);
    for y in 0..image.height() {
        for x in 0..image.width() {
            out.extend(image.rgba_at(x, y).map(|v| v.round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

impl HostApp {
    /// The stored lens calibrations and workspace alignments for this session. (They live in memory
    /// until the project/IO layer has a form for them, W2-L5e: nothing is persisted yet.)
    pub fn attach_calibrations(&self, store: burnburn_image::calib::CalibrationStore) {
        if let Ok(mut controller) = self.controller.try_borrow_mut() {
            controller.attach_calibrations(store);
        }
        self.refresh();
    }

    /// The camera section's "Camerabeheer…" button: shows or hides the camera window.
    pub(crate) fn toggle_camera_window(&self) {
        if let Ok(mut slot) = self.camera_window.try_borrow_mut() {
            let window = slot.get_or_insert_with(|| camera_window(self.mtm, self.me.clone(), self.locale));
            window.shown = !window.shown;
            let panel = window.panel.clone();
            let result = if window.shown {
                crate::appkit::objc_try("showing the camera window", || panel.orderFront(None))
            } else {
                crate::appkit::objc_try("hiding the camera window", || panel.orderOut(None))
            };
            if let Err(e) = result {
                eprintln!("burnburn: {e}");
            }
        }
        self.refresh();
    }

    /// True while the camera window is on screen (it is only refreshed then).
    pub fn camera_window_shown(&self) -> bool {
        self.camera_window.try_borrow().ok().and_then(|w| w.as_ref().map(|w| w.shown)).unwrap_or(false)
    }

    /// Runs `f` on the widgets of the camera window (native tests drive them like a user).
    pub fn with_camera_window<R>(&self, f: impl FnOnce(&CameraParts<'_>) -> R) -> Option<R> {
        let slot = self.camera_window.try_borrow().ok()?;
        Some(f(&slot.as_ref()?.parts()))
    }

    /// The camera window's button `command`, with the choices and texts of the widgets.
    pub(crate) fn camera_command_pressed(&self, command: CameraCommand) {
        let inputs = match self.camera_window.try_borrow().ok().and_then(|w| w.as_ref().map(|w| w.inputs())) {
            Some(inputs) => inputs,
            None => return,
        };
        let (workspace, primary) = match self.controller.try_borrow() {
            Ok(c) => (c.camera_workspace_view(), c.camera_view()),
            Err(_) => return,
        };
        if let Some(intent) = camera_command_intent(command, workspace.as_ref(), primary.as_ref(), &inputs) {
            self.dispatch(intent);
            self.refresh();
        }
    }

    /// Material height or edge margin committed (Return, Tab). Text that is empty or equals what the
    /// window shows sends nothing, so Return then Tab is one commit.
    pub(crate) fn camera_text_committed(&self, tag: isize, text: &str) {
        let shown = {
            let (ws, primary) = match self.controller.try_borrow() {
                Ok(c) => (c.camera_workspace_view(), c.camera_view()),
                Err(_) => return,
            };
            camera_window_panel(ws.as_ref(), primary.as_ref(), self.locale)
        };
        let (intent, current): (fn(&str) -> UiIntent, &str) = match tag {
            1 => (height_intent, &shown.height_text),
            2 => (margin_intent, &shown.margin_text),
            _ => return,
        };
        if text.trim().is_empty() || text == current {
            return;
        }
        self.dispatch(intent(text));
        self.refresh();
    }

    /// A click in the photo (view size and point in the view's own coordinates, origin bottom-left):
    /// places the chosen mark, then moves on to the next mark that has no position yet.
    pub(crate) fn camera_photo_clicked(&self, view: (f64, f64), click: (f64, f64)) {
        let mark = self.camera_window.try_borrow().ok().and_then(|w| w.as_ref().map(|w| w.inputs().mark));
        let Some(mark) = mark else { return };
        let workspace = self.controller.try_borrow().ok().and_then(|c| c.camera_workspace_view());
        let Some(intent) = photo_click_intent(workspace.as_ref(), mark, view, click) else { return };
        self.dispatch(intent);
        let alignment =
            self.controller.try_borrow().ok().and_then(|c| c.camera_workspace_view()).and_then(|v| v.alignment);
        if let (Some(alignment), Ok(i)) = (alignment, usize::try_from(mark))
            && let Some(next) = next_unplaced_mark(&alignment, i)
            && let Ok(slot) = self.camera_window.try_borrow()
            && let Some(window) = slot.as_ref()
        {
            window.select_mark(next);
        }
        self.refresh();
    }

    /// Copies the camera workspace into the window, and the photo with the clicked marks into its
    /// image view. A window that is not on screen is not touched.
    pub(crate) fn refresh_camera_window(&self) {
        if !self.camera_window_shown() {
            return;
        }
        let (workspace, primary, photo) = match self.controller.try_borrow() {
            Ok(c) => (c.camera_workspace_view(), c.camera_view(), c.alignment_photo()),
            Err(_) => return,
        };
        let model = camera_window_panel(workspace.as_ref(), primary.as_ref(), self.locale);
        let Ok(slot) = self.camera_window.try_borrow() else { return };
        let Some(window) = slot.as_ref() else { return };
        window.refresh(&model);
        let marks: Vec<Option<[i64; 2]>> = workspace
            .as_ref()
            .and_then(|w| w.alignment.as_ref())
            .map(|a| a.marks.iter().map(|m| m.pixel.map(|p| [p[0].round() as i64, p[1].round() as i64])).collect())
            .unwrap_or_default();
        let key = photo.as_ref().map(|p| (p.seq, marks));
        window.show_photo(key, || {
            let frame = photo?;
            let alignment = workspace.as_ref()?.alignment.as_ref()?;
            let clicked: Vec<([f64; 2], burnburn_image::calib::MarkRole)> =
                alignment.marks.iter().filter_map(|m| Some((m.pixel?, m.role))).collect();
            let (w, h) = (frame.image.width(), frame.image.height());
            Some((w, h, photo_with_marks(&rgba_of(&frame.image), w, h, &clicked)))
        });
    }
}
