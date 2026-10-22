//! The generic Win32 panel window (W4-L8b): renders a [`PanelSpec`] with standard controls, updates
//! them in place when the next spec only differs in values, and reports every control notification
//! as a [`PanelEvent`] with the values of all controls.
//!
//! **Unconfirmed**: type-checked from macOS (`cargo check --target x86_64-pc-windows-msvc`), never
//! linked or run. Tooltips on disabled controls (the reason a control is off) may not appear on
//! Windows, because disabled windows receive no mouse input; that is to be checked by hand.
//!
//! Invariants (why the `unsafe` here is sound):
//! - Single UI thread: all panel windows and the state live on the thread that runs the message
//!   loop; the state is a thread-local `RefCell`, borrowed briefly and never across a call that can
//!   re-enter (the handler is cloned out of the state first).
//! - Window procedures never unwind: each body runs under `catch_unwind` and falls back to the
//!   default procedure.
//! - Strings passed to Win32 are NUL-terminated UTF-16 buffers (`HSTRING`) that outlive the call.

#![allow(unsafe_code)]

pub mod dialogs;
pub mod manager;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, COLOR_WINDOW, DEFAULT_GUI_FONT, DIB_RGB_COLORS, EndPaint,
    FillRect, GetStockObject, HALFTONE, HBRUSH, InvalidateRect, PAINTSTRUCT, SRCCOPY, SetStretchBltMode, StretchDIBits,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, SetScrollInfo, TBM_SETPOS, TBM_SETRANGE,
    TOOLTIPS_CLASSW, TRACKBAR_CLASSW, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    TTS_ALWAYSTIP, TTTOOLINFOW as TOOLINFOW,
};
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, GetKeyState, SetFocus, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_PUSHBUTTON, CB_ADDSTRING, CB_GETCURSEL,
    CB_RESETCONTENT, CB_SETCURSEL, CBS_DROPDOWNLIST, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
    DestroyWindow, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, GetClientRect, GetDlgCtrlID,
    GetScrollInfo, GetSystemMetrics, GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, LB_ADDSTRING,
    LB_GETCURSEL, LB_RESETCONTENT, LB_SETCURSEL, LBS_NOTIFY, LoadCursorW, MoveWindow, RegisterClassExW, SB_VERT,
    SCROLLINFO, SIF_ALL, SIF_PAGE, SIF_POS, SIF_RANGE, SM_CYSCREEN, SW_HIDE, SW_SHOW, SW_SHOWNORMAL, SendMessageW,
    SetForegroundWindow, SetWindowTextW, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_CAPTURECHANGED,
    WM_CHAR, WM_CLOSE, WM_COMMAND, WM_HSCROLL, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEWHEEL, WM_PAINT, WM_SETFONT, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_VSCROLL, WNDCLASSEXW, WS_BORDER,
    WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOPMOST, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP,
    WS_SYSMENU, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

use crate::host_panels::{
    ControlKind, ControlPatch, ControlSpec, Family, Metrics, PanelEvent, PanelSpec, PanelValues, SpecDiff, diff_specs,
    layout, notify,
};
use crate::panels_image::BitmapImage;

/// Which panel window: a family, one of its sheets, and the document it is about (0: the app).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PanelKey {
    pub family: Family,
    pub sheet: u8,
    pub document: u64,
}

/// `TBM_GETPOS` (`WM_USER`).
const TBM_GETPOS: u32 = 0x0400;

/// Width of a panel window's client area at 96 dpi.
pub const PANEL_WIDTH: i32 = 520;

type Handler = Rc<dyn Fn(PanelKey, PanelEvent)>;

struct Panel {
    hwnd: HWND,
    spec: PanelSpec,
    controls: BTreeMap<u16, HWND>,
    tooltip: Option<HWND>,
    /// Content height in pixels and the current scroll offset.
    content_h: i32,
    scroll: i32,
}

#[derive(Default)]
struct State {
    panels: BTreeMap<PanelKey, Panel>,
    by_hwnd: BTreeMap<isize, PanelKey>,
    bitmaps: BTreeMap<isize, BitmapImage>,
    handler: Option<Handler>,
    instance: Option<HINSTANCE>,
    registered: bool,
    bold: Option<isize>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    /// While set, key presses in panel controls are reported as `notify::KEY` and swallowed
    /// (the shortcut editor waits for a key).
    static CAPTURE_KEYS: Cell<bool> = const { Cell::new(false) };
}

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
    STATE.with(|s| s.try_borrow_mut().ok().map(|mut s| f(&mut s)))
}

/// The function that receives every panel event (set once by the host).
pub fn set_handler(handler: Rc<dyn Fn(PanelKey, PanelEvent)>) {
    with_state(|s| s.handler = Some(handler));
}

/// The shortcut editor waits for a key (or stops waiting).
pub fn set_capture_keys(on: bool) {
    CAPTURE_KEYS.with(|c| c.set(on));
}

pub fn is_open(key: PanelKey) -> bool {
    with_state(|s| s.panels.get(&key).is_some_and(|p| p.hwnd.0 as usize != 0)).unwrap_or(false)
}

pub fn open_keys() -> Vec<PanelKey> {
    with_state(|s| s.panels.keys().copied().collect()).unwrap_or_default()
}

/// The window handle of a panel (as an owner for dialogs).
pub fn hwnd_of(key: PanelKey) -> Option<HWND> {
    with_state(|s| s.panels.get(&key).map(|p| p.hwnd)).flatten()
}

fn wide_style(bits: u32) -> WINDOW_STYLE {
    WINDOW_STYLE(bits)
}

fn scale() -> f64 {
    // SAFETY: plain system query.
    f64::from(unsafe { GetDpiForSystem() }.max(96)) / 96.0
}

fn metrics_for(width_px: i32) -> Metrics {
    let s = scale();
    let px = |v: i32| (f64::from(v) * s).round() as i32;
    Metrics { width: width_px, margin: px(10), gap: px(6), line: px(24), text_line: px(16) }
}

// ---------------------------------------------------------------------------------------------
// Registration and window creation.
// ---------------------------------------------------------------------------------------------

fn ensure_registered() -> Option<HINSTANCE> {
    if let Some(Some(instance)) = with_state(|s| s.registered.then_some(s.instance).flatten()) {
        return Some(instance);
    }
    // SAFETY: the module handle of this executable (no ownership).
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }.ok()?.0);
    let controls =
        INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES };
    // SAFETY: a fully initialised description; registers the trackbar and tooltip classes.
    let _ = unsafe { InitCommonControlsEx(&controls) };
    // SAFETY: loads the shared system arrow cursor (no ownership).
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
    let register = |name: PCWSTR, proc_: windows::Win32::UI::WindowsAndMessaging::WNDPROC, background: HBRUSH| {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: proc_,
            hInstance: instance,
            hCursor: cursor,
            hbrBackground: background,
            lpszClassName: name,
            ..Default::default()
        };
        // SAFETY: a fully initialised class description with a static name and our window procedure.
        unsafe { RegisterClassExW(&class) != 0 }
    };
    let background = HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut _);
    let ok = register(w!("BurnBurnPanel"), Some(panel_proc), background)
        && register(w!("BurnBurnBitmap"), Some(bitmap_proc), HBRUSH::default());
    if !ok {
        return None;
    }
    with_state(|s| {
        s.instance = Some(instance);
        s.registered = true;
    });
    Some(instance)
}

fn set_text(hwnd: HWND, text: &str) {
    // SAFETY: one of our windows on this (UI) thread; the HSTRING outlives the call.
    let _ = unsafe { SetWindowTextW(hwnd, &HSTRING::from(text.replace('\n', "\r\n"))) };
}

fn window_text(hwnd: HWND) -> String {
    // SAFETY: the length query and the copy use a buffer of the reported length + NUL.
    let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` is a valid, writable buffer for the duration of the call.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(len)])
}

fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> LRESULT {
    // SAFETY: our own control on the UI thread; pointer-carrying `lparam`s point to buffers that
    // live for the (synchronous) call.
    unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
}

fn add_string(hwnd: HWND, message: u32, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    send(hwnd, message, 0, wide.as_ptr() as isize);
}

fn fill_items(control: HWND, combo: bool, items: &[String], selected: Option<usize>) {
    let (reset, add, select) = if combo {
        (CB_RESETCONTENT, CB_ADDSTRING, CB_SETCURSEL)
    } else {
        (LB_RESETCONTENT, LB_ADDSTRING, LB_SETCURSEL)
    };
    send(control, reset, 0, 0);
    for item in items {
        add_string(control, add, item);
    }
    send(control, select, selected.unwrap_or(usize::MAX), 0);
}

fn bold_font() -> Option<isize> {
    if let Some(Some(f)) = with_state(|s| s.bold) {
        return Some(f);
    }
    use windows::Win32::Graphics::Gdi::{CreateFontIndirectW, FW_BOLD, GetObjectW, LOGFONTW};
    // SAFETY: stock objects are shared and need no release.
    let stock = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    let mut font = LOGFONTW::default();
    // SAFETY: `font` is a valid out-buffer of the size passed.
    let got =
        unsafe { GetObjectW(stock, std::mem::size_of::<LOGFONTW>() as i32, Some((&mut font as *mut LOGFONTW).cast())) };
    if got == 0 {
        return None;
    }
    font.lfWeight = FW_BOLD.0 as i32;
    // SAFETY: creates a font owned by this process for its lifetime.
    let handle = unsafe { CreateFontIndirectW(&font) };
    let raw = handle.0 as isize;
    with_state(|s| s.bold = Some(raw));
    Some(raw)
}

/// Creates the child window of one control (not yet placed).
fn create_control(parent: HWND, instance: HINSTANCE, c: &ControlSpec, tooltip: Option<HWND>) -> Option<HWND> {
    let id = HMENU(usize::from(c.id) as *mut _);
    let base = WS_CHILD | WS_VISIBLE;
    let (class, style, text): (PCWSTR, WINDOW_STYLE, String) = match &c.kind {
        ControlKind::Label | ControlKind::Heading => (w!("STATIC"), base | wide_style(0), c.text.clone()),
        ControlKind::Report { .. } => (
            w!("EDIT"),
            base | WS_BORDER | WS_VSCROLL | wide_style((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
            c.text.replace('\n', "\r\n"),
        ),
        ControlKind::Button | ControlKind::HoldButton => {
            (w!("BUTTON"), base | WS_TABSTOP | wide_style(BS_PUSHBUTTON as u32), c.text.clone())
        }
        ControlKind::Check { .. } => {
            (w!("BUTTON"), base | WS_TABSTOP | wide_style(BS_AUTOCHECKBOX as u32), c.text.clone())
        }
        ControlKind::Edit => {
            (w!("EDIT"), base | WS_BORDER | WS_TABSTOP | wide_style(ES_AUTOHSCROLL as u32), c.text.clone())
        }
        ControlKind::Combo { .. } => {
            (w!("COMBOBOX"), base | WS_TABSTOP | WS_VSCROLL | wide_style(CBS_DROPDOWNLIST as u32), String::new())
        }
        ControlKind::List { .. } => {
            (w!("LISTBOX"), base | WS_TABSTOP | WS_BORDER | WS_VSCROLL | wide_style(LBS_NOTIFY as u32), String::new())
        }
        ControlKind::Slider { .. } => (TRACKBAR_CLASSW, base | WS_TABSTOP, String::new()),
        ControlKind::Bitmap { .. } => (w!("BurnBurnBitmap"), base | WS_BORDER, String::new()),
    };
    let title = HSTRING::from(text);
    // SAFETY: creating a child control of our panel window; class names are system or ours.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            &title,
            style,
            0,
            0,
            10,
            10,
            Some(parent),
            Some(id),
            Some(instance),
            None,
        )
    }
    .ok()?;
    // SAFETY: the stock GUI font is shared and needs no release.
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    let font =
        if matches!(c.kind, ControlKind::Heading) { bold_font().unwrap_or(font.0 as isize) } else { font.0 as isize };
    send(hwnd, WM_SETFONT, font as usize, 1);
    apply_values(hwnd, c);
    // SAFETY: subclassing our own control; the reference data is the panel window.
    let _ = unsafe { SetWindowSubclass(hwnd, Some(control_proc), 1, parent.0 as usize) };
    if let Some(tip) = tooltip {
        set_tooltip(tip, hwnd, &c.tooltip);
    }
    Some(hwnd)
}

/// Sets the state a spec holds in a freshly created control.
fn apply_values(hwnd: HWND, c: &ControlSpec) {
    // SAFETY: our own control on the UI thread.
    let _ = unsafe { EnableWindow(hwnd, c.enabled) };
    // SAFETY: as above.
    let _ = unsafe { ShowWindow(hwnd, if c.visible { SW_SHOW } else { SW_HIDE }) };
    match &c.kind {
        ControlKind::Check { checked } => {
            send(hwnd, BM_SETCHECK, usize::from(*checked), 0);
        }
        ControlKind::Combo { items, selected } => fill_items(hwnd, true, items, *selected),
        ControlKind::List { items, selected, .. } => fill_items(hwnd, false, items, *selected),
        ControlKind::Slider { min, max, pos } => {
            send(hwnd, TBM_SETRANGE, 1, ((*max as isize) << 16) | (*min as isize & 0xFFFF));
            send(hwnd, TBM_SETPOS, 1, *pos as isize);
        }
        ControlKind::Bitmap { image, .. } => {
            set_bitmap(hwnd, image.clone());
        }
        _ => {}
    }
}

fn set_bitmap(hwnd: HWND, image: Option<BitmapImage>) {
    with_state(|s| match image {
        Some(i) => {
            s.bitmaps.insert(hwnd.0 as isize, i);
        }
        None => {
            s.bitmaps.remove(&(hwnd.0 as isize));
        }
    });
    // SAFETY: repaint request for our own window.
    let _ = unsafe { InvalidateRect(Some(hwnd), None, true) };
}

fn tool_info(panel_tip: HWND, control: HWND, text: &mut Vec<u16>) -> TOOLINFOW {
    let _ = panel_tip;
    TOOLINFOW {
        cbSize: std::mem::size_of::<TOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND | TTF_SUBCLASS,
        hwnd: HWND(std::ptr::null_mut()),
        uId: control.0 as usize,
        lpszText: PWSTR(text.as_mut_ptr()),
        ..Default::default()
    }
}

fn set_tooltip(tip: HWND, control: HWND, text: &str) {
    let mut wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = tool_info(tip, control, &mut wide);
    // The tool may or may not exist yet: remove (ignored when absent), then add when there is text.
    send(tip, TTM_DELTOOLW, 0, &mut info as *mut TOOLINFOW as isize);
    if !text.is_empty() {
        send(tip, TTM_ADDTOOLW, 0, &mut info as *mut TOOLINFOW as isize);
        send(tip, TTM_UPDATETIPTEXTW, 0, &mut info as *mut TOOLINFOW as isize);
    }
}

// ---------------------------------------------------------------------------------------------
// Layout and scrolling.
// ---------------------------------------------------------------------------------------------

fn client_width(hwnd: HWND) -> i32 {
    let mut rc = RECT::default();
    // SAFETY: our window; `rc` is a valid out-parameter.
    let _ = unsafe { GetClientRect(hwnd, &mut rc) };
    rc.right - rc.left
}

fn client_height(hwnd: HWND) -> i32 {
    let mut rc = RECT::default();
    // SAFETY: our window; `rc` is a valid out-parameter.
    let _ = unsafe { GetClientRect(hwnd, &mut rc) };
    rc.bottom - rc.top
}

/// Places every control for the window's current width; returns the content height.
fn place_controls(panel: &mut Panel) -> i32 {
    let width = client_width(panel.hwnd);
    let (places, height) = layout(&panel.spec, &metrics_for(width));
    for p in places {
        if let Some(hwnd) = panel.controls.get(&p.id) {
            // SAFETY: one of our child windows on the UI thread; repaint requested.
            let _ = unsafe { MoveWindow(*hwnd, p.x, p.y - panel.scroll, p.w, p.h, true) };
        }
    }
    height
}

fn update_scrollbar(panel: &mut Panel) {
    let page = client_height(panel.hwnd);
    let max_scroll = (panel.content_h - page).max(0);
    panel.scroll = panel.scroll.clamp(0, max_scroll);
    let info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0,
        nMax: panel.content_h.max(0),
        nPage: page.max(0) as u32,
        nPos: panel.scroll,
        nTrackPos: 0,
    };
    // SAFETY: our window; `info` is fully initialised.
    unsafe { SetScrollInfo(panel.hwnd, SB_VERT, &info, true) };
}

fn relayout(panel: &mut Panel) {
    panel.content_h = place_controls(panel);
    update_scrollbar(panel);
}

fn scroll_to(key: PanelKey, target: i32) {
    with_state(|s| {
        let Some(panel) = s.panels.get_mut(&key) else { return };
        let before = panel.scroll;
        let max_scroll = (panel.content_h - client_height(panel.hwnd)).max(0);
        panel.scroll = target.clamp(0, max_scroll);
        if panel.scroll != before {
            // SAFETY: scrolls the children of our own window.
            let _ = unsafe {
                windows::Win32::UI::WindowsAndMessaging::ScrollWindow(panel.hwnd, 0, before - panel.scroll, None, None)
            };
        }
        update_scrollbar(panel);
    });
}

// ---------------------------------------------------------------------------------------------
// Public operations.
// ---------------------------------------------------------------------------------------------

/// Shows the panel `key` with `spec`: creates the window when it does not exist, otherwise updates
/// it (in place when only values changed).
pub fn show(key: PanelKey, spec: &PanelSpec) -> bool {
    if is_open(key) {
        update(key, spec);
        if let Some(hwnd) = hwnd_of(key) {
            // SAFETY: our own window on the UI thread.
            let _ = unsafe { ShowWindow(hwnd, SW_SHOWNORMAL) };
            // SAFETY: as above.
            let _ = unsafe { SetForegroundWindow(hwnd) };
        }
        return true;
    }
    create(key, spec)
}

fn create(key: PanelKey, spec: &PanelSpec) -> bool {
    let Some(instance) = ensure_registered() else { return false };
    let s = scale();
    let width = (f64::from(PANEL_WIDTH) * s).round() as i32;
    let (_, content) = layout(spec, &metrics_for(width));
    // SAFETY: plain system query.
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let height = content.min((f64::from(screen_h) * 0.85) as i32).max(1);
    let style = WS_OVERLAPPED
        | WS_CAPTION
        | WS_SYSMENU
        | WS_THICKFRAME
        | WS_MINIMIZEBOX
        | WS_MAXIMIZEBOX
        | WS_CLIPCHILDREN
        | WS_VSCROLL;
    let mut outer = RECT { left: 0, top: 0, right: width, bottom: height };
    // SAFETY: computes the window rectangle for the client size; `outer` is a valid in/out value.
    let _ = unsafe { AdjustWindowRectEx(&mut outer, style, false, WINDOW_EX_STYLE(0)) };
    let title = HSTRING::from(spec.title.as_str());
    // SAFETY: our registered panel class; no menu.
    let Ok(hwnd) = (unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BurnBurnPanel"),
            &title,
            style,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            outer.right - outer.left,
            outer.bottom - outer.top,
            None,
            None,
            Some(instance),
            None,
        )
    }) else {
        return false;
    };
    // SAFETY: the tooltip control for this panel; a failure only loses tooltips.
    let tooltip = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            TOOLTIPS_CLASSW,
            PCWSTR::null(),
            WS_POPUP | wide_style(TTS_ALWAYSTIP),
            0,
            0,
            0,
            0,
            Some(hwnd),
            None,
            Some(instance),
            None,
        )
    }
    .ok();
    let mut controls = BTreeMap::new();
    for c in spec.controls() {
        if let Some(child) = create_control(hwnd, instance, c, tooltip) {
            controls.insert(c.id, child);
        }
    }
    let mut panel = Panel { hwnd, spec: spec.clone(), controls, tooltip, content_h: content, scroll: 0 };
    relayout(&mut panel);
    with_state(|st| {
        st.by_hwnd.insert(hwnd.0 as isize, key);
        st.panels.insert(key, panel);
    });
    // SAFETY: showing our own window.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
    }
    true
}

/// Brings the window in line with `spec`: nothing, a few controls, or all of them.
pub fn update(key: PanelKey, spec: &PanelSpec) {
    let Some(old) = with_state(|s| s.panels.get(&key).map(|p| p.spec.clone())).flatten() else { return };
    match diff_specs(&old, spec) {
        SpecDiff::Same => {}
        SpecDiff::Patch(patches) => {
            with_state(|s| {
                if let Some(panel) = s.panels.get_mut(&key) {
                    for patch in &patches {
                        apply_patch(panel, patch);
                    }
                    panel.spec = spec.clone();
                }
            });
        }
        SpecDiff::Rebuild => rebuild(key, spec),
    }
}

fn apply_patch(panel: &Panel, patch: &ControlPatch) {
    let Some(hwnd) = panel.controls.get(&patch.id).copied() else { return };
    let kind = panel.spec.control(patch.id).map(|c| c.kind.clone());
    if let Some(text) = &patch.text {
        // A field the user is typing in keeps its text.
        // SAFETY: plain query.
        let typing = matches!(kind, Some(ControlKind::Edit)) && unsafe { GetFocus() } == hwnd;
        if !typing {
            set_text(hwnd, text);
        }
    }
    if let Some(enabled) = patch.enabled {
        // SAFETY: our own control on the UI thread.
        let _ = unsafe { EnableWindow(hwnd, enabled) };
    }
    if let Some(visible) = patch.visible {
        // SAFETY: our own control on the UI thread.
        let _ = unsafe { ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE }) };
    }
    if let Some(tip) = patch.tooltip.as_deref().zip(panel.tooltip) {
        set_tooltip(tip.1, hwnd, tip.0);
    }
    if let Some(checked) = patch.checked {
        send(hwnd, BM_SETCHECK, usize::from(checked), 0);
    }
    let combo = matches!(kind, Some(ControlKind::Combo { .. }));
    if let Some((items, selected)) = &patch.items {
        fill_items(hwnd, combo, items, *selected);
    }
    if let Some(selected) = patch.selected {
        send(hwnd, if combo { CB_SETCURSEL } else { LB_SETCURSEL }, selected.unwrap_or(usize::MAX), 0);
    }
    if let Some(pos) = patch.slider {
        send(hwnd, TBM_SETPOS, 1, pos as isize);
    }
    if let Some(image) = &patch.bitmap {
        set_bitmap(hwnd, image.clone());
    }
}

/// Destroys the controls and builds them again for `spec` (the window itself stays).
fn rebuild(key: PanelKey, spec: &PanelSpec) {
    let Some((hwnd, old_controls, tooltip)) = with_state(|s| {
        s.panels.get(&key).map(|p| (p.hwnd, p.controls.values().copied().collect::<Vec<_>>(), p.tooltip))
    })
    .flatten() else {
        return;
    };
    let Some(instance) = ensure_registered() else { return };
    for control in old_controls {
        with_state(|s| s.bitmaps.remove(&(control.0 as isize)));
        // SAFETY: destroying one of our own child windows.
        let _ = unsafe { DestroyWindow(control) };
    }
    let mut controls = BTreeMap::new();
    for c in spec.controls() {
        if let Some(child) = create_control(hwnd, instance, c, tooltip) {
            controls.insert(c.id, child);
        }
    }
    with_state(|s| {
        if let Some(panel) = s.panels.get_mut(&key) {
            panel.controls = controls;
            panel.spec = spec.clone();
            relayout(panel);
        }
    });
    // SAFETY: repaint request for our own window.
    let _ = unsafe { InvalidateRect(Some(hwnd), None, true) };
}

/// Hides and forgets the panel `key`.
pub fn close(key: PanelKey) {
    let Some(panel) = with_state(|s| {
        let panel = s.panels.remove(&key)?;
        s.by_hwnd.remove(&(panel.hwnd.0 as isize));
        for control in panel.controls.values() {
            s.bitmaps.remove(&(control.0 as isize));
        }
        Some(panel)
    })
    .flatten() else {
        return;
    };
    // SAFETY: destroying our own window (its children go with it).
    let _ = unsafe { DestroyWindow(panel.hwnd) };
}

/// The values of every control of the panel, as a `PanelEvent` carries them.
pub fn values(key: PanelKey) -> Option<PanelValues> {
    with_state(|s| s.panels.get(&key).map(read_values)).flatten()
}

fn read_values(panel: &Panel) -> PanelValues {
    let mut v = PanelValues::default();
    for c in panel.spec.controls() {
        let Some(hwnd) = panel.controls.get(&c.id).copied() else { continue };
        match &c.kind {
            ControlKind::Edit => {
                v.texts.insert(c.id, window_text(hwnd));
            }
            ControlKind::Check { .. } => {
                v.checks.insert(c.id, send(hwnd, BM_GETCHECK, 0, 0).0 == 1);
            }
            ControlKind::Combo { .. } => {
                let i = send(hwnd, CB_GETCURSEL, 0, 0).0;
                v.selection.insert(c.id, usize::try_from(i).ok());
            }
            ControlKind::List { .. } => {
                let i = send(hwnd, LB_GETCURSEL, 0, 0).0;
                v.selection.insert(c.id, usize::try_from(i).ok());
            }
            ControlKind::Slider { .. } => {
                v.sliders.insert(c.id, send(hwnd, TBM_GETPOS, 0, 0).0 as i32);
            }
            _ => {}
        }
    }
    v
}

// ---------------------------------------------------------------------------------------------
// Events.
// ---------------------------------------------------------------------------------------------

fn key_of(hwnd: HWND) -> Option<PanelKey> {
    with_state(|s| s.by_hwnd.get(&(hwnd.0 as isize)).copied()).flatten()
}

/// Builds the event for `id`/`code` and hands it to the host's handler (outside any borrow).
fn emit(key: PanelKey, id: u16, code: u16, extra: impl FnOnce(&mut PanelEvent)) {
    let Some(values) = values(key) else { return };
    let mut event = PanelEvent { id, code, values, ..PanelEvent::default() };
    extra(&mut event);
    let handler = with_state(|s| s.handler.clone()).flatten();
    if let Some(handler) = handler {
        handler(key, event);
    }
}

fn guard(what: &str, default: LRESULT, body: impl FnOnce() -> LRESULT) -> LRESULT {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| {
        eprintln!("burnburn-win: internal error in {what}; the message was dropped");
        default
    })
}

fn modifiers() -> (bool, bool, bool) {
    let down = |vk: i32| {
        // SAFETY: a plain keyboard-state query on the UI thread.
        let state = unsafe { GetKeyState(vk) };
        state < 0
    };
    (down(i32::from(VK_CONTROL.0)), down(i32::from(VK_MENU.0)), down(i32::from(VK_SHIFT.0)))
}

extern "system" fn panel_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: default handling of a message for our own window.
    let default = || unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    guard("panel_proc", LRESULT(0), || {
        let Some(key) = key_of(hwnd) else { return default() };
        match message {
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as u16;
                let code = ((wparam.0 >> 16) & 0xFFFF) as u16;
                // Notifications of the controls: BN_CLICKED = 0, xBN_SELCHANGE = 1, LBN_DBLCLK = 2,
                // EN_KILLFOCUS = 0x200 are the numbers `notify` uses as well.
                if lparam.0 != 0 {
                    emit(key, id, code, |_| {});
                }
                LRESULT(0)
            }
            WM_HSCROLL => {
                let slider = HWND(lparam.0 as *mut _);
                // SAFETY: reading the control id of our own trackbar.
                let id = unsafe { GetDlgCtrlID(slider) } as u16;
                emit(key, id, notify::SLIDER, |_| {});
                LRESULT(0)
            }
            WM_VSCROLL => {
                let mut info = SCROLLINFO {
                    cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_ALL,
                    ..Default::default()
                };
                // SAFETY: our window; `info` is a valid in/out value.
                let _ = unsafe { GetScrollInfo(hwnd, SB_VERT, &mut info) };
                let current = info.nPos;
                let target = match (wparam.0 & 0xFFFF) as i32 {
                    0 => current - 24,                // SB_LINEUP
                    1 => current + 24,                // SB_LINEDOWN
                    2 => current - info.nPage as i32, // SB_PAGEUP
                    3 => current + info.nPage as i32, // SB_PAGEDOWN
                    5 => info.nTrackPos,              // SB_THUMBTRACK
                    _ => current,
                };
                scroll_to(key, target);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16;
                let current = with_state(|s| s.panels.get(&key).map_or(0, |p| p.scroll)).unwrap_or(0);
                scroll_to(key, current - i32::from(delta) / 3);
                LRESULT(0)
            }
            WM_SIZE => {
                with_state(|s| {
                    if let Some(panel) = s.panels.get_mut(&key) {
                        relayout(panel);
                    }
                });
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if (wparam.0 & 0xFFFF) == 0 {
                    emit(key, 0, notify::DEACTIVATED, |_| {});
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                // The family decides what closing means (jog release, cancel a preview, ...).
                emit(key, 0, notify::CLOSED, |_| {});
                close(key);
                LRESULT(0)
            }
            _ => default(),
        }
    })
}

/// Subclass of every control: Return in edits, jog buttons (press/release), arrow keys of the laser
/// window, captured keys of the shortcut editor.
extern "system" fn control_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    parent: usize,
) -> LRESULT {
    // SAFETY: the subclass chain's default handling.
    let default = || unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    guard("control_proc", LRESULT(0), || {
        let panel = HWND(parent as *mut _);
        let Some(key) = key_of(panel) else { return default() };
        // SAFETY: reading the control id of our own control.
        let id = unsafe { GetDlgCtrlID(hwnd) } as u16;
        let kind =
            with_state(|s| s.panels.get(&key).and_then(|p| p.spec.control(id).map(|c| c.kind.clone()))).flatten();
        let repeat = (lparam.0 >> 30) & 1 == 1;
        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = (wparam.0 & 0xFFFF) as u16;
                if CAPTURE_KEYS.with(Cell::get) {
                    let (ctrl, alt, shift) = modifiers();
                    emit(key, id, notify::KEY, |e| {
                        e.key = Some(crate::host_panels::KeyStroke { vk, ctrl, alt, shift })
                    });
                    return LRESULT(0);
                }
                if key.family == Family::Laser
                    && let Some(jog) = crate::host_panels::laser::jog_control_for_vk(vk)
                {
                    if !repeat {
                        emit(key, jog, notify::PRESS, |_| {});
                    }
                    return LRESULT(0);
                }
                if vk == 0x0D && matches!(kind, Some(ControlKind::Edit)) {
                    emit(key, id, notify::ENTER, |_| {});
                    return LRESULT(0);
                }
                if matches!(kind, Some(ControlKind::HoldButton)) && vk == 0x20 && !repeat {
                    emit(key, id, notify::PRESS, |_| {});
                }
                default()
            }
            WM_KEYUP | WM_SYSKEYUP => {
                let vk = (wparam.0 & 0xFFFF) as u16;
                if key.family == Family::Laser
                    && let Some(jog) = crate::host_panels::laser::jog_control_for_vk(vk)
                {
                    emit(key, jog, notify::RELEASE, |_| {});
                    return LRESULT(0);
                }
                if matches!(kind, Some(ControlKind::HoldButton)) && vk == 0x20 {
                    emit(key, id, notify::RELEASE, |_| {});
                }
                default()
            }
            // Return in a single-line edit also produces a WM_CHAR that would beep.
            WM_CHAR if wparam.0 == 0x0D && matches!(kind, Some(ControlKind::Edit)) => LRESULT(0),
            WM_LBUTTONDOWN if matches!(kind, Some(ControlKind::HoldButton)) => {
                let result = default();
                emit(key, id, notify::PRESS, |_| {});
                result
            }
            WM_LBUTTONUP | WM_CAPTURECHANGED | WM_KILLFOCUS if matches!(kind, Some(ControlKind::HoldButton)) => {
                let result = default();
                emit(key, id, notify::RELEASE, |_| {});
                result
            }
            _ => default(),
        }
    })
}

extern "system" fn bitmap_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: default handling of a message for our own window.
    let default = || unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    guard("bitmap_proc", LRESULT(0), || match message {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: standard paint cycle on our own window.
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            let mut rc = RECT::default();
            // SAFETY: our window; `rc` is a valid out-parameter.
            let _ = unsafe { GetClientRect(hwnd, &mut rc) };
            // SAFETY: fills the client rectangle with the window background.
            unsafe { FillRect(hdc, &rc, HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut _)) };
            let image = with_state(|s| s.bitmaps.get(&(hwnd.0 as isize)).cloned()).flatten();
            if let Some(img) = image
                && img.width > 0
                && img.height > 0
                && img.rgba.len() == img.width as usize * img.height as usize * 4
            {
                // BGRA, top-down (negative height).
                let mut bgra = img.rgba.clone();
                for px in bgra.as_chunks_mut::<4>().0 {
                    px.swap(0, 2);
                }
                let header = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: img.width as i32,
                    biHeight: -(img.height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                };
                let info = BITMAPINFO { bmiHeader: header, ..Default::default() };
                // Fit proportionally, centred (the click mapping assumes the control is the picture).
                let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
                // SAFETY: sets the stretch mode of our window's DC.
                unsafe { SetStretchBltMode(hdc, HALFTONE) };
                // SAFETY: draws a pixel buffer that outlives the call into our window's DC.
                unsafe {
                    StretchDIBits(
                        hdc,
                        0,
                        0,
                        cw,
                        ch,
                        0,
                        0,
                        img.width as i32,
                        img.height as i32,
                        Some(bgra.as_ptr().cast()),
                        &info,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                }
            }
            // SAFETY: ends the paint cycle begun above.
            let _ = unsafe { EndPaint(hwnd, &ps) };
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // SAFETY: the parent of our bitmap control is the panel window.
            let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) }.unwrap_or_default();
            let Some(key) = key_of(parent) else { return default() };
            // SAFETY: reading the control id of our own control.
            let id = unsafe { GetDlgCtrlID(hwnd) } as u16;
            let x = f64::from((lparam.0 & 0xFFFF) as u16 as i16);
            let y = f64::from(((lparam.0 >> 16) & 0xFFFF) as u16 as i16);
            let (w, h) = (f64::from(client_width(hwnd)), f64::from(client_height(hwnd)));
            // SAFETY: focus to our own control.
            let _ = unsafe { SetFocus(Some(hwnd)) };
            emit(key, id, notify::BITMAP_CLICK, |e| {
                e.click_px = Some([x, y]);
                e.view_px = Some([w, h]);
            });
            LRESULT(0)
        }
        _ => default(),
    })
}
