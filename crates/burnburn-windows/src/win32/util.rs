//! Small safe wrappers over the Win32 calls every panel needs (text, messages, list selection).

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowTextLengthW, GetWindowTextW, LB_GETSELCOUNT, LB_GETSELITEMS, SendMessageW, SetWindowTextW,
};
use windows::core::HSTRING;

pub(super) fn set_text(hwnd: HWND, text: &str) {
    // SAFETY: `hwnd` is one of our windows on this (UI) thread; the HSTRING outlives the call.
    let _ = unsafe { SetWindowTextW(hwnd, &HSTRING::from(text)) };
}

/// `SendMessageW` to one of our own controls. Pointer-carrying `lparam`s must point to buffers
/// that live for the (synchronous) call.
pub(super) fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> LRESULT {
    // SAFETY: our own control on the UI thread; see the function contract for pointer arguments.
    unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
}

pub(super) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(super) fn window_text(hwnd: HWND) -> String {
    // SAFETY: the length query and the copy use a buffer of the reported length + NUL.
    let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` is a valid, writable buffer for the duration of the call.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(len)])
}

pub(super) fn list_selection(list: HWND) -> Vec<usize> {
    // SAFETY: LB_GETSELCOUNT/LB_GETSELITEMS on our list box; the buffer holds `count` ints.
    let count = unsafe { SendMessageW(list, LB_GETSELCOUNT, None, None) }.0.max(0) as usize;
    let mut rows = vec![0i32; count];
    // SAFETY: as above; the pointer stays valid during the synchronous call.
    let n =
        unsafe { SendMessageW(list, LB_GETSELITEMS, Some(WPARAM(count)), Some(LPARAM(rows.as_mut_ptr() as isize))) }.0;
    rows.truncate(n.max(0) as usize);
    rows.into_iter().filter_map(|r| usize::try_from(r).ok()).collect()
}
