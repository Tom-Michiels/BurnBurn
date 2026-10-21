//! The catalogue tool strip as push buttons at the top of the frame. The places come from
//! [`crate::forms::toolstrip::strip_layout`] (tested headlessly); this file creates the buttons,
//! shows the selected tool as pressed and opens a group's drop-down as a popup menu.

use windows::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, BM_GETCHECK, BM_SETCHECK, CreatePopupMenu, DestroyMenu, GetWindowRect, MF_GRAYED, MF_STRING,
    MoveWindow, TPM_NONOTIFY, TPM_RETURNCMD, TrackPopupMenu,
};
use windows::core::{HSTRING, w};

use super::forms::child;
use super::util::{send, set_text};
use crate::forms::layout::Rect;
use crate::forms::toolstrip::{StripAction, StripButton, StripLayout, StripPanel, strip_layout};

/// First child id of the strip's buttons; control `n` has id `STRIP_BASE + n`.
pub(super) const STRIP_BASE: usize = 3000;
const BS_PUSHBUTTON: u32 = 0x0000;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_PUSHLIKE: u32 = 0x1000;
const BST_CHECKED: usize = 1;

pub(super) struct StripWin {
    ctls: Vec<(HWND, StripAction)>,
    signature: Vec<String>,
}

impl StripWin {
    pub(super) fn new() -> StripWin {
        StripWin { ctls: Vec::new(), signature: Vec::new() }
    }

    /// Bring the buttons in step with the strip; returns the height the strip needs at `width`.
    pub(super) fn sync(
        &mut self,
        instance: HINSTANCE,
        frame: HWND,
        panels: &[StripPanel],
        width: i32,
        scale: f64,
    ) -> i32 {
        let lay: StripLayout = strip_layout(panels, width, scale);
        let signature: Vec<String> = lay.controls.iter().map(|c| format!("{:?}{}", c.action, c.label)).collect();
        if signature != self.signature {
            for (hwnd, _) in self.ctls.drain(..) {
                // SAFETY: destroying our own button; no message is in flight for it.
                let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd) };
            }
            for (i, c) in lay.controls.iter().enumerate() {
                let bits = match c.action {
                    StripAction::Tool(_) => BS_AUTOCHECKBOX | BS_PUSHLIKE,
                    StripAction::Dropdown { .. } => BS_PUSHBUTTON,
                };
                let r = Rect { ..c.rect };
                if let Some(hwnd) = child(instance, frame, w!("BUTTON"), &c.label, bits, r, STRIP_BASE + i) {
                    self.ctls.push((hwnd, c.action.clone()));
                }
            }
            self.signature = signature;
        }
        for ((hwnd, _), c) in self.ctls.iter().zip(&lay.controls) {
            // SAFETY: moving our own button.
            let _ = unsafe { MoveWindow(*hwnd, c.rect.x, c.rect.y, c.rect.w, c.rect.h, true) };
            let want = if c.selected { BST_CHECKED } else { 0 };
            if matches!(c.action, StripAction::Tool(_)) && send(*hwnd, BM_GETCHECK, 0, 0).0 as usize != want {
                send(*hwnd, BM_SETCHECK, want, 0);
            }
            // SAFETY: plain state change on our own button.
            let _ = unsafe { EnableWindow(*hwnd, c.enabled) };
            // The tooltip is the button's own accessible name: the window text carries the label.
            set_text(*hwnd, &c.label);
        }
        lay.height
    }

    /// What the button with child id `id` does.
    pub(super) fn action(&self, id: usize) -> Option<&StripAction> {
        self.ctls.get(id.checked_sub(STRIP_BASE)?).map(|(_, a)| a)
    }

    /// The button with child id `id`.
    pub(super) fn button(&self, id: usize) -> Option<HWND> {
        self.ctls.get(id.checked_sub(STRIP_BASE)?).map(|(h, _)| *h)
    }
}

/// A group's drop-down below `anchor`: the id of the variant the user chose.
pub(super) fn choose_variant(frame: HWND, anchor: HWND, variants: &[StripButton]) -> Option<String> {
    // SAFETY: a popup menu this thread owns, destroyed below.
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    for (i, v) in variants.iter().enumerate() {
        let flags = if v.enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
        let text = HSTRING::from(if v.selected { format!("\u{2713} {}", v.label) } else { v.label.clone() });
        // SAFETY: our menu; the string outlives the call.
        let _ = unsafe { AppendMenuW(menu, flags, i + 1, &text) };
    }
    let mut rc = RECT::default();
    // SAFETY: our own button; `rc` is a valid out-parameter.
    let _ = unsafe { GetWindowRect(anchor, &mut rc) };
    // SAFETY: modal popup on the UI thread; TPM_RETURNCMD returns the chosen id instead of posting it.
    let chosen =
        unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, rc.left, rc.bottom, Some(0), frame, None) };
    // SAFETY: destroying the menu created above.
    let _ = unsafe { DestroyMenu(menu) };
    let index = usize::try_from(chosen.0).ok()?.checked_sub(1)?;
    variants.get(index).map(|v| v.id.clone())
}
