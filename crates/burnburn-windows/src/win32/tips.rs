//! Tooltips for the frame, the form containers and the panel windows (shared helper).
//!
//! Two kinds of tools on one tooltip control per window:
//! * a tool per enabled child control (`TTF_IDISHWND`, fixed text): the system shows it when the
//!   pointer rests on the control;
//! * one *area* tool over the whole client area with callback text: a disabled control gets no
//!   mouse input, so the window asks its headless layout (`disabled_reason_at` and friends) from
//!   its own `WM_MOUSEMOVE` and answers the `TTN_GETDISPINFOW` with the reason for the control
//!   under the pointer (empty text: no tooltip).
//!
//! **Unconfirmed**: type-checked from macOS, never run on Windows. The area tool relies on the
//! parent receiving `WM_MOUSEMOVE` while the pointer is over a disabled child.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, RECT};
use windows::Win32::UI::Controls::{
    NMTTDISPINFOW, TOOLTIP_FLAGS, TOOLTIPS_CLASSW, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_DELTOOLW,
    TTM_NEWTOOLRECTW, TTM_POP, TTM_UPDATETIPTEXTW, TTN_GETDISPINFOW, TTS_ALWAYSTIP, TTTOOLINFOW as TOOLINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, GetClientRect, WINDOW_STYLE, WS_EX_TOPMOST, WS_POPUP};
use windows::core::{PCWSTR, PWSTR};

use super::util::send;

/// `LPSTR_TEXTCALLBACKW`: the tooltip asks the owner for the text (`TTN_GETDISPINFOW`).
const TEXT_CALLBACK: PWSTR = PWSTR(usize::MAX as *mut u16);

/// The id of the area tool (the control tools use their window handle).
const AREA_ID: usize = 1;

thread_local! {
    /// The reason the area tool shows for each window (by handle), as UTF-16 with a NUL.
    static REASON: RefCell<BTreeMap<isize, Vec<u16>>> = const { RefCell::new(BTreeMap::new()) };
}

/// A tooltip control owned by `parent`.
pub(crate) fn create(parent: HWND, instance: HINSTANCE) -> Option<HWND> {
    // SAFETY: a tooltip control for our own window; a failure only loses tooltips.
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            TOOLTIPS_CLASSW,
            PCWSTR::null(),
            WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP),
            0,
            0,
            0,
            0,
            Some(parent),
            None,
            Some(instance),
            None,
        )
    }
    .ok()
}

fn info(parent: HWND, flags: TOOLTIP_FLAGS, id: usize, text: PWSTR) -> TOOLINFOW {
    TOOLINFOW {
        cbSize: std::mem::size_of::<TOOLINFOW>() as u32,
        uFlags: flags,
        hwnd: parent,
        uId: id,
        lpszText: text,
        ..Default::default()
    }
}

/// The tooltip `text` for the enabled child `control` of `parent` (empty text removes the tool).
pub(crate) fn set_control_tip(tip: HWND, parent: HWND, control: HWND, text: &str) {
    let mut wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let mut tool = info(parent, TTF_IDISHWND | TTF_SUBCLASS, control.0 as usize, PWSTR(wide.as_mut_ptr()));
    // The tool may or may not exist yet: remove (ignored when absent), then add when there is text.
    send(tip, TTM_DELTOOLW, 0, &mut tool as *mut TOOLINFOW as isize);
    if !text.is_empty() {
        send(tip, TTM_ADDTOOLW, 0, &mut tool as *mut TOOLINFOW as isize);
        send(tip, TTM_UPDATETIPTEXTW, 0, &mut tool as *mut TOOLINFOW as isize);
    }
}

/// The area tool over the whole client area of `parent` (call once, then after a resize).
pub(crate) fn set_area_tip(tip: HWND, parent: HWND, first: bool) {
    let mut rc = RECT::default();
    // SAFETY: our own window; `rc` is a valid out-parameter.
    let _ = unsafe { GetClientRect(parent, &mut rc) };
    let mut tool = info(parent, TTF_SUBCLASS, AREA_ID, TEXT_CALLBACK);
    tool.rect = rc;
    send(tip, if first { TTM_ADDTOOLW } else { TTM_NEWTOOLRECTW }, 0, &mut tool as *mut TOOLINFOW as isize);
}

/// The window saw the pointer move: `reason` is what the area tool should say now (`None`: nothing).
/// A tooltip that is up for another reason is hidden so that it asks again.
pub(crate) fn hover(tip: Option<HWND>, parent: HWND, reason: Option<&str>) {
    let wanted: Vec<u16> = reason.map(|r| r.encode_utf16().chain(std::iter::once(0)).collect()).unwrap_or_default();
    let changed = REASON
        .with(|r| {
            let mut r = r.try_borrow_mut().ok()?;
            let old = r.get(&(parent.0 as isize)).cloned().unwrap_or_default();
            r.insert(parent.0 as isize, wanted.clone());
            Some(old != wanted)
        })
        .unwrap_or(false);
    if changed && let Some(tip) = tip {
        send(tip, TTM_POP, 0, 0);
    }
}

/// `WM_NOTIFY` of `parent`: answers the area tool's request for text. `true` when it was one.
pub(crate) fn answer(parent: HWND, lparam: LPARAM) -> bool {
    // SAFETY: for WM_NOTIFY `lparam` is a pointer to the notification header (null: none).
    let Some(header) = (unsafe { (lparam.0 as *const windows::Win32::UI::Controls::NMHDR).as_ref() }) else {
        return false;
    };
    if header.code != TTN_GETDISPINFOW || header.idFrom != AREA_ID {
        return false;
    }
    // SAFETY: a TTN_GETDISPINFOW notification carries an NMTTDISPINFOW.
    let Some(request) = (unsafe { (lparam.0 as *mut NMTTDISPINFOW).as_mut() }) else { return false };
    REASON.with(|r| {
        if let Ok(mut r) = r.try_borrow_mut() {
            let text = r.entry(parent.0 as isize).or_default();
            if text.is_empty() {
                text.push(0);
            }
            // The buffer lives in the map until the next change for this window.
            request.lpszText = PWSTR(text.as_mut_ptr());
        }
    });
    true
}

/// A window is gone: forget its reason.
pub(crate) fn forget(parent: HWND) {
    REASON.with(|r| {
        if let Ok(mut r) = r.try_borrow_mut() {
            r.remove(&(parent.0 as isize));
        }
    });
}
