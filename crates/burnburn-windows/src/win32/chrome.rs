//! The window chrome of the document frame (LF14): the dark title bar, the saved window, the
//! status bar control and the tool strip's bitmaps. What each of them should be is decided by the
//! headless modules ([`crate::dwm`], [`crate::window_state`], [`crate::status_bar`],
//! [`crate::icon_dib`]); this file only makes the Win32 calls.
//!
//! **Unconfirmed**: type-checked from macOS (`cargo check --target x86_64-pc-windows-msvc`), never
//! linked or run on Windows.

use std::path::PathBuf;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, EnumDisplayMonitors, GetDC,
    GetMonitorInfoW, HBITMAP, HDC, HMONITOR, InvalidateRect, MONITORINFO, ReleaseDC,
};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::Controls::{ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetWindowPlacement, GetWindowRect, HMENU, SW_SHOWMAXIMIZED, WINDOW_EX_STYLE, WINDOWPLACEMENT,
    WS_CHILD, WS_VISIBLE,
};
use windows::core::{BOOL, PCWSTR, w};

use super::util::{send, wide};
use crate::appearance::Appearance;
use crate::dwm::dark_title_bar;
use crate::icon_dib::Dib;
use crate::status_bar::StatusParts;
use crate::window_state::{DEFAULT_INSPECTOR, DEFAULT_SIDEBAR, SavedWindow, StoredWindow};

/// `SB_SETPARTS` and `SB_SETTEXTW` (`WM_USER + 4`, `WM_USER + 11`), and the flag that makes the
/// parent draw a part.
const SB_SETPARTS: u32 = 0x0404;
const SB_SETTEXTW: u32 = 0x040B;
const SBT_OWNERDRAW: usize = 0x1000;

/// The Windows build number (`CurrentBuildNumber`), 0 when it cannot be read.
pub fn os_build() -> u32 {
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let name = wide("CurrentBuildNumber");
    let mut buffer = [0u16; 16];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: NUL-terminated key and value names that outlive the call; `buffer` and `size` are a
    // writable buffer and its byte length, as RRF_RT_REG_SZ requires.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return 0;
    }
    let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len]).trim().parse().unwrap_or(0)
}

/// Makes the title bar of `frame` dark or light after `appearance`.
pub fn apply_dark_title_bar(frame: HWND, appearance: Appearance) {
    let Some(call) = dark_title_bar(appearance, os_build()) else { return };
    // SAFETY: our own frame; the attribute value is a 4-byte BOOL that outlives the call.
    let _ = unsafe {
        DwmSetWindowAttribute(
            frame,
            DWMWINDOWATTRIBUTE(call.attribute as i32),
            std::ptr::from_ref(&call.value).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

/// `%LOCALAPPDATA%\BurnBurn\window-state.txt`.
pub fn state_path() -> Option<PathBuf> {
    Some(crate::window_state::state_file(&PathBuf::from(std::env::var_os("LOCALAPPDATA")?)))
}

unsafe extern "system" fn collect_work_area(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the address of the Vec that `work_areas` passes and keeps alive.
    let out = unsafe { &mut *(data.0 as *mut Vec<(f64, f64, f64, f64)>) };
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..MONITORINFO::default() };
    // SAFETY: `info` is a valid MONITORINFO with its size set.
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        let r = info.rcWork;
        out.push((f64::from(r.left), f64::from(r.top), f64::from(r.right - r.left), f64::from(r.bottom - r.top)));
    }
    BOOL(1)
}

/// The work areas of all monitors (x, y, width, height in pixels).
pub fn work_areas() -> Vec<(f64, f64, f64, f64)> {
    let mut out: Vec<(f64, f64, f64, f64)> = Vec::new();
    // SAFETY: the callback only pushes into `out`, which outlives the synchronous call.
    let _ = unsafe {
        EnumDisplayMonitors(None, None, Some(collect_work_area), LPARAM(std::ptr::from_mut(&mut out) as isize))
    };
    out
}

/// The window to open the first frame in: the stored one where it can be reached, else a
/// default on the primary monitor. `None` when there is no monitor to place it on.
pub fn initial_window() -> Option<StoredWindow> {
    let screens = work_areas();
    let default = crate::window_state::default_window(*screens.first()?);
    let stored = state_path().and_then(|p| crate::window_state::load(&p));
    Some(crate::window_state::placement(stored, &screens, default))
}

/// Stores the position, size and maximised state of `frame`.
pub fn save_window_state(frame: HWND) {
    let mut placement = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
    // SAFETY: our own frame; `placement` is a valid out-parameter with its length set.
    if unsafe { GetWindowPlacement(frame, &mut placement) }.is_err() {
        return;
    }
    let r = placement.rcNormalPosition;
    let stored = StoredWindow {
        window: SavedWindow {
            x: f64::from(r.left),
            y: f64::from(r.top),
            width: f64::from((r.right - r.left).max(1)),
            height: f64::from((r.bottom - r.top).max(1)),
            sidebar: DEFAULT_SIDEBAR,
            inspector: DEFAULT_INSPECTOR,
        },
        maximized: placement.showCmd == SW_SHOWMAXIMIZED.0 as u32,
    };
    if let Some(path) = state_path()
        && let Err(e) = crate::window_state::save(&path, &stored)
    {
        eprintln!("burnburn-win: window state not saved: {e}");
    }
}

/// The status bar control, docked at the bottom of `parent` by the control itself.
pub fn create_status_bar(instance: HINSTANCE, parent: HWND, id: usize) -> windows::core::Result<HWND> {
    let init =
        INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES };
    // SAFETY: registers the common-control classes of this process; `init` is valid.
    let _ = unsafe { InitCommonControlsEx(&init) };
    // SAFETY: creating a child control of our frame; the class is a system class.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("msctls_statusbar32"),
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(instance),
            None,
        )
    }
}

/// Sets the part edges (`SB_SETPARTS`); the first part is owner-drawn by the frame in the colour of
/// its tone (`WM_DRAWITEM`), so it is marked here.
pub fn set_status_edges(bar: HWND, edges: &[i32]) {
    send(bar, SB_SETPARTS, edges.len(), edges.as_ptr() as isize);
    send(bar, SB_SETTEXTW, SBT_OWNERDRAW, 0);
}

/// Sets the texts of the coordinate, zoom and unit parts and asks for the first part to be drawn
/// again (its text is kept by the host, see `DocWin::status_text`).
pub fn set_status_texts(bar: HWND, parts: &StatusParts) {
    for (index, part) in parts.parts.iter().enumerate().skip(1) {
        let text = wide(&part.text);
        send(bar, SB_SETTEXTW, index, text.as_ptr() as isize);
    }
    // SAFETY: repaint request for our own status bar.
    let _ = unsafe { InvalidateRect(Some(bar), None, true) };
}

/// The height of the status bar in pixels (0 when it cannot be measured).
pub fn status_bar_height(bar: HWND) -> i32 {
    let mut rc = RECT::default();
    // SAFETY: our own status bar; `rc` is a valid out-parameter.
    if unsafe { GetWindowRect(bar, &mut rc) }.is_ok() { rc.bottom - rc.top } else { 0 }
}

/// A premultiplied BGRA bitmap as a `HBITMAP` for `BM_SETIMAGE`; the caller deletes it.
pub fn bitmap_of(dib: &Dib) -> Option<HBITMAP> {
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: i32::try_from(dib.width).ok()?,
        // a negative height: the rows run top down, as `Dib` has them
        biHeight: -i32::try_from(dib.height).ok()?,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let info = BITMAPINFO { bmiHeader: header, ..Default::default() };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: a screen DC for the call only; released below.
    let dc = unsafe { GetDC(None) };
    // SAFETY: `info` describes a 32-bit top-down bitmap; `bits` receives the pixel memory the
    // bitmap owns.
    let bitmap = unsafe { CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok();
    // SAFETY: releasing the DC got above.
    unsafe { ReleaseDC(None, dc) };
    let bitmap = bitmap?;
    if bits.is_null() {
        return None;
    }
    // SAFETY: the DIB section holds `width * height * 4` bytes at `bits`, exactly `dib.bgra.len()`.
    unsafe { std::ptr::copy_nonoverlapping(dib.bgra.as_ptr(), bits.cast::<u8>(), dib.bgra.len()) };
    Some(bitmap)
}
