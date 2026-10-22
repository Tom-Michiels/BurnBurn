//! Win32 messages to `burnburn-presentation` intents — without the `windows` crate.
//!
//! The Win32 layer passes each message as plain numbers (`msg`, `wParam`, `lParam`) plus a
//! [`MessageContext`] with what it read from the control (edit text, list selection, DPI). All
//! decoding — `GET_X_LPARAM` sign extension, wheel deltas, DPI → logical points, menu ids,
//! notification codes — and the meaning of each message live here and are tested on any OS.
//! Field text is never parsed here (DR-05): presentation owns units and decimal separators.

use std::path::PathBuf;

use burnburn_editor::{CanvasKey, Modifiers, PointerButton, PointerId, PointerPhase, Viewport};
use burnburn_presentation::{
    AppViewModel, CameraIntent, CloseChoice, DialogKind, FieldKey, MessageKey, SelectMode, UiIntent, ViewPointerEvent,
};
use burnburn_types::{DocumentId, MonotonicTime};

mod menu;
pub use menu::*;

/// Win32 message numbers (stable ABI values from WinUser.h).
pub mod msg {
    pub const WM_SIZE: u32 = 0x0005;
    pub const WM_ACTIVATE: u32 = 0x0006;
    pub const WM_CLOSE: u32 = 0x0010;
    pub const WM_KEYDOWN: u32 = 0x0100;
    pub const WM_CHAR: u32 = 0x0102;
    pub const WM_COMMAND: u32 = 0x0111;
    pub const WM_MOUSEMOVE: u32 = 0x0200;
    pub const WM_LBUTTONDOWN: u32 = 0x0201;
    pub const WM_LBUTTONUP: u32 = 0x0202;
    pub const WM_RBUTTONDOWN: u32 = 0x0204;
    pub const WM_RBUTTONUP: u32 = 0x0205;
    pub const WM_MBUTTONDOWN: u32 = 0x0207;
    pub const WM_MBUTTONUP: u32 = 0x0208;
    pub const WM_MOUSEWHEEL: u32 = 0x020A;
    pub const WM_MOUSEHWHEEL: u32 = 0x020E;
    pub const WM_CAPTURECHANGED: u32 = 0x0215;
}

/// Control notification codes (HIWORD of `wParam` in `WM_COMMAND`).
pub mod notify {
    pub const MENU: u16 = 0;
    pub const ACCELERATOR: u16 = 1;
    pub const LBN_SELCHANGE: u16 = 1;
    pub const EN_KILLFOCUS: u16 = 0x0200;
}

/// Virtual-key codes.
pub mod vk {
    pub const BACK: usize = 0x08;
    pub const RETURN: usize = 0x0D;
    pub const ESCAPE: usize = 0x1B;
    pub const DELETE: usize = 0x2E;
}

/// `MK_*` flags in `wParam` of mouse messages.
pub mod mk {
    pub const LBUTTON: usize = 0x0001;
    pub const RBUTTON: usize = 0x0002;
    pub const SHIFT: usize = 0x0004;
    pub const CONTROL: usize = 0x0008;
    pub const MBUTTON: usize = 0x0010;
}

/// `MessageBoxW` results of the close confirmation (MB_YESNOCANCEL: Bewaar / Niet bewaren / Annuleer).
pub mod button {
    pub const IDCANCEL: i32 = 2;
    pub const IDYES: i32 = 6;
    pub const IDNO: i32 = 7;
}

/// One wheel notch (`WHEEL_DELTA`).
pub const WHEEL_DELTA: f64 = 120.0;
/// Logical points panned per wheel notch.
pub const WHEEL_NOTCH_PT: f64 = 40.0;
/// Zoom per Ctrl+wheel notch (`magnification` as in AppKit's pinch).
pub const WHEEL_NOTCH_ZOOM: f64 = 0.1;
/// 100 % scaling.
pub const BASE_DPI: f64 = 96.0;

/// Inspector fields, top to bottom (edit controls in this order).
pub const INSPECTOR_FIELDS: [FieldKey; 5] =
    [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height, FieldKey::Rotation];

/// `WM_CHAR` UTF-16 units (IME results arrive the same way) to final text: surrogate pairs are
/// joined; control characters (Return, Backspace, Esc, Tab) are keys, not text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharDecoder {
    high: Option<u16>,
}

impl CharDecoder {
    pub fn push(&mut self, unit: u16) -> Option<String> {
        let c = match (self.high.take(), unit) {
            (None, 0xD800..=0xDBFF) => {
                self.high = Some(unit);
                return None;
            }
            (Some(high), 0xDC00..=0xDFFF) => char::decode_utf16([high, unit]).next()?.ok()?,
            // A high surrogate without its low half: dropped (and so is this unit if it is one).
            (Some(_), 0xD800..=0xDBFF) => {
                self.high = Some(unit);
                return None;
            }
            (_, unit) => char::from_u32(u32::from(unit))?,
        };
        (!c.is_control()).then(|| c.to_string())
    }
}

/// The recovery question's `MessageBoxW` result (Ja = herstel, Nee = verwerp, Annuleer = later).
pub fn on_recovery_box(path: &std::path::Path, result: i32) -> Reaction {
    let path = path.to_path_buf();
    match result {
        button::IDYES => Reaction::Intent(UiIntent::RestoreRecovery { path }),
        button::IDNO => Reaction::Intent(UiIntent::DiscardRecovery { path }),
        _ => Reaction::Nothing,
    }
}

/// The recovery question for file `index` (1-based) of `count`.
pub fn recovery_box_text(path: &std::path::Path, index: usize, count: usize, locale: crate::l10n::Locale) -> String {
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    match locale {
        crate::l10n::Locale::Nl => format!(
            "BurnBurn werd afgesloten met niet-bewaarde wijzigingen ({index} van {count}: {file}).\n\n\
             Ja: herstel in een nieuw, onbewaard venster. Nee: verwerp. Annuleer: later opnieuw vragen."
        ),
        crate::l10n::Locale::En => format!(
            "BurnBurn quit with unsaved changes ({index} of {count}: {file}).\n\n\
             Yes: restore in a new, unsaved window. No: discard. Cancel: ask again later."
        ),
    }
}

/// Where autosave writes recovery files: `%LOCALAPPDATA%\BurnBurn\Recovery`.
pub fn recovery_dir(local_app_data: &std::path::Path) -> PathBuf {
    local_app_data.join("BurnBurn").join("Recovery")
}

/// Which child window a message came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Frame,
    Canvas,
    Field(FieldKey),
    ObjectList,
}

/// What the Win32 layer read for one message.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageContext<'a> {
    pub document: DocumentId,
    pub control: Control,
    /// `GetDpiForWindow` of the window (96 = 100 %).
    pub dpi: u32,
    /// Edit control text (`GetWindowTextW`) for field messages.
    pub text: &'a str,
    /// Selected list rows (`LB_GETSELITEMS`) for list notifications.
    pub selected_rows: &'a [usize],
    /// Alt held (`GetKeyState(VK_MENU)`), which mouse `wParam` does not carry.
    pub alt: bool,
    /// Cursor in canvas client pixels for wheel messages (their `lParam` is in screen pixels).
    pub cursor_px: [i32; 2],
    /// `GetMessageTime` in milliseconds.
    pub time_ms: u32,
}

/// What the Win32 layer must do with one message.
#[derive(Debug, Clone, PartialEq)]
pub enum Reaction {
    /// Not ours: `DefWindowProcW`.
    PassOn,
    /// Handled, nothing to send.
    Nothing,
    Intent(UiIntent),
    ShowOpenDialog,
    /// Show or hide the Schikken options panel of the window.
    ToggleSchikkenPanel,
}

/// Decode and translate one message for the window showing `vm`.
fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}

fn hiword(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}

/// `GET_X_LPARAM` / `GET_Y_LPARAM`: signed 16-bit client coordinates.
fn point_px(lparam: isize) -> [i32; 2] {
    [i32::from((lparam & 0xFFFF) as u16 as i16), i32::from(((lparam >> 16) & 0xFFFF) as u16 as i16)]
}

fn scale(dpi: u32) -> Option<f64> {
    (dpi > 0).then(|| f64::from(dpi) / BASE_DPI)
}

fn to_points(px: [i32; 2], dpi: u32) -> Option<[f64; 2]> {
    let s = scale(dpi)?;
    Some([f64::from(px[0]) / s, f64::from(px[1]) / s])
}

fn mouse_modifiers(wparam: usize, alt: bool) -> Modifiers {
    // Ctrl is the shortcut modifier on Windows (`command`); there is no separate Mac Control key.
    Modifiers { shift: wparam & mk::SHIFT != 0, alt, command: wparam & mk::CONTROL != 0, control: false }
}

fn pointer(
    ctx: &MessageContext<'_>,
    phase: PointerPhase,
    button: Option<PointerButton>,
    px: [i32; 2],
    wparam: usize,
) -> Option<UiIntent> {
    let event = ViewPointerEvent {
        pointer: PointerId(0),
        phase,
        button,
        position_pt: to_points(px, ctx.dpi)?,
        modifiers: mouse_modifiers(wparam, ctx.alt),
        click_count: 1,
        time: MonotonicTime::from_offset(std::time::Duration::from_millis(u64::from(ctx.time_ms))),
    };
    Some(UiIntent::Pointer { document: ctx.document, event })
}

/// Decode and translate one message for the window showing `vm`.
pub fn on_message(ctx: &MessageContext<'_>, msg: u32, wparam: usize, lparam: isize, vm: &AppViewModel) -> Reaction {
    let document = ctx.document;
    let Some(doc) = vm.document(document) else {
        // A closed document's late messages are swallowed (the window is going away).
        return match msg {
            msg::WM_COMMAND | msg::WM_KEYDOWN | msg::WM_LBUTTONDOWN | msg::WM_LBUTTONUP | msg::WM_MOUSEMOVE => {
                Reaction::Nothing
            }
            _ => Reaction::PassOn,
        };
    };
    let intent = match (ctx.control, msg) {
        (Control::Frame, msg::WM_COMMAND) if matches!(hiword(wparam), notify::MENU | notify::ACCELERATOR) => {
            match menu_item(loword(wparam)) {
                Some(MenuItem::Open) => return Reaction::ShowOpenDialog,
                Some(MenuItem::CamDialog(dialog)) => UiIntent::OpenCamDialog { document, dialog },
                Some(MenuItem::SchikkenPanel) => return Reaction::ToggleSchikkenPanel,
                Some(MenuItem::Command(command)) => UiIntent::Invoke { document: Some(document), command },
                None => return Reaction::PassOn,
            }
        }
        (Control::Frame, msg::WM_CLOSE) => UiIntent::Close { document },
        (Control::Frame, msg::WM_ACTIVATE) if loword(wparam) != 0 => UiIntent::Focus { document },
        (Control::Field(field), msg::WM_KEYDOWN | msg::WM_COMMAND)
            if (msg == msg::WM_KEYDOWN && wparam == vk::RETURN)
                || (msg == msg::WM_COMMAND && hiword(wparam) == notify::EN_KILLFOCUS) =>
        {
            match commit(doc, document, field, ctx.text) {
                Some(intent) => intent,
                None => return Reaction::Nothing,
            }
        }
        (Control::ObjectList, msg::WM_COMMAND) if hiword(wparam) == notify::LBN_SELCHANGE => {
            let ids: Vec<_> = ctx.selected_rows.iter().filter_map(|r| doc.objects.get(*r).map(|o| o.id)).collect();
            if ids == doc.selection {
                return Reaction::Nothing;
            }
            UiIntent::Select { document, ids, mode: SelectMode::Replace }
        }
        // Final text (typed or from the IME), decoded by the Win32 layer into `ctx.text`.
        (Control::Canvas, msg::WM_CHAR) if !ctx.text.is_empty() => {
            UiIntent::InsertText { document, text: ctx.text.to_owned() }
        }
        (Control::Canvas, _) => match canvas(ctx, msg, wparam, lparam) {
            Some(intent) => intent,
            None if is_canvas_message(msg) => return Reaction::Nothing,
            None => return Reaction::PassOn,
        },
        _ => return Reaction::PassOn,
    };
    Reaction::Intent(intent)
}

/// A field commit, or `None` when the text equals what the view model shows (Enter then focus
/// loss sends the same text twice).
pub(crate) fn commit(
    doc: &burnburn_presentation::DocumentViewModel,
    document: DocumentId,
    field: FieldKey,
    text: &str,
) -> Option<UiIntent> {
    let shown = doc.inspector.as_ref().and_then(|i| i.field(field));
    if shown.is_some_and(|f| f.error.is_none() && f.text == text) {
        return None;
    }
    Some(UiIntent::CommitField { document, field, text: text.to_owned() })
}

fn is_canvas_message(msg: u32) -> bool {
    matches!(
        msg,
        msg::WM_SIZE
            | msg::WM_CHAR
            | msg::WM_MOUSEMOVE
            | msg::WM_LBUTTONDOWN
            | msg::WM_LBUTTONUP
            | msg::WM_RBUTTONDOWN
            | msg::WM_RBUTTONUP
            | msg::WM_MBUTTONDOWN
            | msg::WM_MBUTTONUP
            | msg::WM_MOUSEWHEEL
            | msg::WM_MOUSEHWHEEL
    )
}

fn canvas(ctx: &MessageContext<'_>, msg: u32, wparam: usize, lparam: isize) -> Option<UiIntent> {
    let document = ctx.document;
    let px = point_px(lparam);
    let held = if wparam & mk::LBUTTON != 0 {
        Some(PointerButton::Primary)
    } else if wparam & mk::RBUTTON != 0 {
        Some(PointerButton::Secondary)
    } else if wparam & mk::MBUTTON != 0 {
        Some(PointerButton::Middle)
    } else {
        None
    };
    match msg {
        msg::WM_LBUTTONDOWN => pointer(ctx, PointerPhase::Down, Some(PointerButton::Primary), px, wparam),
        msg::WM_LBUTTONUP => pointer(ctx, PointerPhase::Up, Some(PointerButton::Primary), px, wparam),
        msg::WM_RBUTTONDOWN => pointer(ctx, PointerPhase::Down, Some(PointerButton::Secondary), px, wparam),
        msg::WM_RBUTTONUP => pointer(ctx, PointerPhase::Up, Some(PointerButton::Secondary), px, wparam),
        msg::WM_MBUTTONDOWN => pointer(ctx, PointerPhase::Down, Some(PointerButton::Middle), px, wparam),
        msg::WM_MBUTTONUP => pointer(ctx, PointerPhase::Up, Some(PointerButton::Middle), px, wparam),
        msg::WM_MOUSEMOVE => pointer(ctx, PointerPhase::Move, held, px, wparam),
        // The OS took the mouse (Alt+Tab during a drag): abandon the gesture.
        msg::WM_CAPTURECHANGED => pointer(ctx, PointerPhase::Cancel, None, [0, 0], 0),
        msg::WM_MOUSEWHEEL | msg::WM_MOUSEHWHEEL => {
            let notches = f64::from(hiword(wparam) as i16) / WHEEL_DELTA;
            if msg == msg::WM_MOUSEWHEEL && wparam & mk::CONTROL != 0 {
                let anchor_pt = to_points(ctx.cursor_px, ctx.dpi)?;
                let magnification = (notches * WHEEL_NOTCH_ZOOM * 1e9).round() / 1e9;
                return Some(UiIntent::Camera { document, intent: CameraIntent::Magnify { magnification, anchor_pt } });
            }
            let delta_pt = if msg == msg::WM_MOUSEWHEEL {
                [0.0, notches * WHEEL_NOTCH_PT]
            } else {
                // Tilt right (positive) scrolls right: the content moves left.
                [-notches * WHEEL_NOTCH_PT, 0.0]
            };
            Some(UiIntent::Camera { document, intent: CameraIntent::Scroll { delta_pt } })
        }
        msg::WM_SIZE => {
            let s = scale(ctx.dpi)?;
            let (w, h) = (f64::from(loword(lparam as usize)), f64::from(hiword(lparam as usize)));
            let viewport = Viewport::new(w / s, h / s, s).ok()?;
            Some(UiIntent::Camera { document, intent: CameraIntent::Resized { viewport } })
        }
        msg::WM_KEYDOWN => {
            let key = match wparam {
                vk::ESCAPE => CanvasKey::Escape,
                vk::DELETE => CanvasKey::Delete,
                vk::BACK => CanvasKey::Backspace,
                vk::RETURN => CanvasKey::Return,
                _ => return None,
            };
            Some(UiIntent::Key { document, key, modifiers: Modifiers::default() })
        }
        _ => None,
    }
}

/// `IFileOpenDialog` closed (`None`: cancelled).
pub fn on_open_dialog(path: Option<PathBuf>) -> Reaction {
    match path {
        Some(path) => Reaction::Intent(UiIntent::Open { path }),
        None => Reaction::Nothing,
    }
}

/// `IFileSaveDialog` closed (`None`: cancelled).
pub fn on_save_dialog(document: DocumentId, path: Option<PathBuf>, vm: &AppViewModel) -> Reaction {
    if vm.document(document).is_none() {
        return Reaction::Nothing;
    }
    Reaction::Intent(match path {
        Some(path) => UiIntent::Save { document, path: Some(path) },
        None => UiIntent::DialogCancelled { document, dialog: DialogKind::SavePanel },
    })
}

/// The close confirmation's `MessageBoxW` result.
pub fn on_close_box(document: DocumentId, result: i32, vm: &AppViewModel) -> Reaction {
    if vm.document(document).is_none() {
        return Reaction::Nothing;
    }
    let choice = match result {
        button::IDYES => CloseChoice::Save,
        button::IDNO => CloseChoice::Discard,
        _ => CloseChoice::Cancel,
    };
    Reaction::Intent(UiIntent::ConfirmClose { document, choice })
}

/// Title bar text: file name, `*` when dirty, then the app name.
pub fn window_title(vm: &AppViewModel, document: DocumentId) -> String {
    vm.document(document)
        .map(|d| format!("{}{} — BurnBurn", if d.dirty { "*" } else { "" }, d.title))
        .unwrap_or_default()
}

pub fn object_labels(vm: &AppViewModel, document: DocumentId) -> Vec<String> {
    vm.document(document)
        .map(|d| {
            d.objects
                .iter()
                .map(|o| if o.linked > 1 { format!("{} (\u{d7}{})", o.label, o.linked) } else { o.label.clone() })
                .collect()
        })
        .unwrap_or_default()
}

pub fn selected_rows(vm: &AppViewModel, document: DocumentId) -> Vec<usize> {
    vm.document(document)
        .map(|d| d.objects.iter().enumerate().filter(|(_, o)| o.selected).map(|(i, _)| i).collect())
        .unwrap_or_default()
}

/// Inspector field text from the view model.
pub fn field_text(vm: &AppViewModel, document: DocumentId, field: FieldKey) -> Option<String> {
    vm.document(document)?.inspector.as_ref()?.field(field).map(|f| f.text.clone())
}

/// Status-line text from the shared NL/EN catalogue (never empty: unknown keys show their name).
pub fn message_text(key: MessageKey, locale: crate::l10n::Locale) -> String {
    crate::l10n::message_or_key(key, locale)
}
