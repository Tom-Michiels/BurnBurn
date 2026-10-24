//! A [`SpecLayout`] as Win32 controls (LF13): the dialog font measured with `GetTextExtentPoint32W`
//! for the DLU base and the text widths, and one child window per control at the rectangle the
//! headless builder gave it. Classes and styles come from [`crate::forms::spec_win32`].
//!
//! **Unconfirmed**: type-checked from macOS (`cargo check --target x86_64-pc-windows-msvc`), never
//! linked or run on Windows. Combo items, tab titles and the owner-drawn pictures are filled in by
//! the panel that owns the form (the same `CB_ADDSTRING` and `TCM_INSERTITEM` calls `forms.rs`
//! makes), not here.

use windows::Win32::Foundation::{HINSTANCE, HWND, SIZE};
use windows::Win32::Graphics::Gdi::{
    DEFAULT_GUI_FONT, GetDC, GetStockObject, GetTextExtentPoint32W, HDC, HFONT, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT, WS_CHILD, WS_VISIBLE,
};
use windows::core::PCWSTR;

use super::util::{send, wide};
use crate::forms::spec_layout::{DluBase, SpecLayout, TextMeter};
use crate::forms::spec_win32::{control_id, win_spec};

/// Measures text in the stock GUI font of one device context.
pub struct GdiMeter {
    hdc: HDC,
}

impl TextMeter for GdiMeter {
    fn width(&self, text: &str, _bold: bool) -> i32 {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        // SAFETY: `hdc` is a live device context with the dialog font selected (see `with_meter`);
        // `units` and `size` outlive the call.
        let ok = unsafe { GetTextExtentPoint32W(self.hdc, &units, &mut size) };
        if ok.as_bool() { size.cx } else { 0 }
    }
}

/// Runs `f` with a meter and the DLU base for the window `owner` at `dpi`. The base is the way
/// `GetDialogBaseUnits` finds it: the width of the 52 letters of the alphabet divided by 52 and
/// the font's line height.
pub fn with_meter<R>(owner: HWND, dpi: u32, f: impl FnOnce(DluBase, &GdiMeter) -> R) -> Option<R> {
    // SAFETY: a device context of our own window, released below on every path.
    let hdc = unsafe { GetDC(Some(owner)) };
    if hdc.is_invalid() {
        return None;
    }
    // SAFETY: the stock GUI font is a shared object; selecting it into our DC and restoring the old
    // object afterwards leaves nothing leaked.
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    // SAFETY: as above; `hdc` is valid.
    let old: HGDIOBJ = unsafe { SelectObject(hdc, font) };
    let meter = GdiMeter { hdc };
    let alphabet = meter.width("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ", false);
    let mut line = SIZE::default();
    // SAFETY: `hdc` is valid and `line` outlives the call.
    let _ = unsafe { GetTextExtentPoint32W(hdc, &[u16::from(b'X')], &mut line) };
    let fallback = DluBase::segoe_ui_9(dpi);
    let base = if alphabet > 0 && line.cy > 0 {
        DluBase { char_width: f64::from(alphabet) / 52.0, char_height: f64::from(line.cy), dpi }
    } else {
        fallback
    };
    let out = f(base, &meter);
    // SAFETY: restoring the object that was selected before, then releasing the DC we got.
    unsafe { SelectObject(hdc, old) };
    // SAFETY: releasing the DC got above, once.
    unsafe { ReleaseDC(Some(owner), hdc) };
    Some(out)
}

/// Creates the child windows of `layout` in `parent`, one per control, in layout order (the tab
/// order). A hidden control is created hidden.
pub fn create_controls(parent: HWND, instance: HINSTANCE, layout: &SpecLayout) -> Vec<HWND> {
    // SAFETY: the stock GUI font is a shared object; WM_SETFONT only references it.
    let font: HFONT = HFONT(unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0);
    let mut out = Vec::with_capacity(layout.controls.len());
    for (index, control) in layout.controls.iter().enumerate() {
        let spec = win_spec(control.class, control.enabled);
        let class = wide(spec.class);
        let title = wide(&control.text);
        let visible = if control.visible { WS_VISIBLE } else { WINDOW_STYLE(0) };
        // SAFETY: creating a child control of our form window; class and text buffers outlive the call.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_CHILD | visible | WINDOW_STYLE(spec.style),
                control.rect.x,
                control.rect.y,
                control.rect.w,
                control.rect.h,
                Some(parent),
                Some(HMENU(control_id(control.id, index) as *mut _)),
                Some(instance),
                None,
            )
        };
        if let Ok(hwnd) = hwnd {
            send(hwnd, WM_SETFONT, font.0 as usize, 1);
            out.push(hwnd);
        }
    }
    out
}
