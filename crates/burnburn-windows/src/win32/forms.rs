//! A [`Form`] as Win32 controls: static texts, edit fields, check boxes, drop-down lists, push
//! buttons and list boxes in a scrolling container window. The geometry comes from
//! [`crate::forms::layout`] (tested headlessly); this file creates the child windows at those
//! rectangles, keeps them in step with the form (only changed values are written, so a field the
//! user is typing in keeps its text), reads the controls back and turns a notification into a
//! [`ControlValue`]. What a control means is decided by [`crate::forms::react`], never here.
//!
//! **Unconfirmed**: type-checked from macOS, never linked or run on Windows.

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{DEFAULT_GUI_FONT, GetStockObject};
use windows::Win32::UI::Controls::SetScrollInfo;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetKeyState, IsWindowEnabled, VK_RETURN, VK_SHIFT};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BM_SETCHECK, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CW_USEDEFAULT,
    CreateWindowExW, DLGC_WANTMESSAGE, DestroyWindow, GetClientRect, GetDlgCtrlID, GetParent, GetScrollInfo, HMENU,
    LB_ADDSTRING, LB_GETCURSEL, LB_GETSELCOUNT, LB_GETSELITEMS, LB_RESETCONTENT, LB_SETCURSEL, LB_SETSEL, MSG,
    MoveWindow, PostMessageW, SB_VERT, SCROLLINFO, SIF_ALL, SIF_PAGE, SIF_POS, SIF_RANGE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOZORDER, ScrollWindow, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_GETDLGCODE, WM_KEYDOWN,
    WM_SETFONT, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{PCWSTR, w};

use burnburn_types::{DocumentId, StableId};

use super::util::{send, set_text, wide, window_text};
use crate::forms::choice::ChoiceList;
use crate::forms::layout::{Item, Layout, Rect, disabled_tip_at, layout};
use crate::forms::{ControlId, ControlValue, Form, FormKind, Placement};

/// The window class of every form container.
pub(super) const FORM_CLASS: PCWSTR = w!("BurnBurnForm");
/// First child id; control `n` has id `CTL_BASE + n`.
const CTL_BASE: usize = 2000;
/// Notification the edit subclass posts when Return is pressed (no system code uses it).
const EN_RETURN: u16 = 0x7001;

// Notification codes (HIWORD of wParam) of the standard controls.
const BN_CLICKED: u16 = 0;
const EN_KILLFOCUS: u16 = 0x0200;
const CBN_SELCHANGE: u16 = 1;
const LBN_SELCHANGE: u16 = 1;
const LBN_DBLCLK: u16 = 2;

// Control styles that the `windows` crate types as plain integers.
const SS_LEFT: u32 = 0x0000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_WANTRETURN: u32 = 0x1000;
const BS_PUSHBUTTON: u32 = 0x0000;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_PUSHLIKE: u32 = 0x1000;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const LBS_NOTIFY: u32 = 0x0001;
const LBS_EXTENDEDSEL: u32 = 0x0800;
const BST_CHECKED: usize = 1;

pub(super) fn style(bits: u32) -> WINDOW_STYLE {
    WINDOW_STYLE(bits)
}

/// One control (and its caption) of a form window.
struct Ctl {
    hwnd: HWND,
    caption: Option<HWND>,
    id: Option<ControlId>,
    item: Item,
    /// The text last written to an edit field, so a typed text is not overwritten.
    shown: String,
}

/// One panel's container window with its controls.
pub(super) struct FormWin {
    pub hwnd: HWND,
    pub kind: FormKind,
    pub placement: Placement,
    instance: HINSTANCE,
    ctls: Vec<Ctl>,
    /// What the controls are, so that a form with the same shape is updated and another rebuilt.
    signature: Vec<String>,
    content_h: i32,
    scroll: i32,
    scale: f64,
    /// The form as last synced (re-laid out when the container is resized).
    last: Form,
    /// The container's tooltip control: hints of the fields, the blocked items of the pop-ups, and
    /// (area tool) the hint of a disabled field.
    tip: Option<HWND>,
}

/// The shape of an item: kind and identity, not its values.
fn signature_of(item: &Item) -> String {
    match item {
        Item::Heading(_) => "H".to_owned(),
        Item::Text(t) => format!("T{}", t.len() / 40),
        Item::Problem(t) => format!("P{}", t.len() / 40),
        Item::Field { id, .. } => format!("F{id:?}"),
        Item::Check { id, .. } => format!("C{id:?}"),
        Item::Choice { id, items, .. } => format!("S{id:?}{}", items.len()),
        Item::Button(b) => format!("B{:?}", b.id),
        Item::List { id, items, .. } => format!("L{id:?}{}", items.len()),
        Item::Memo { id, .. } => format!("M{id:?}"),
    }
}

pub(super) fn child(
    instance: HINSTANCE,
    parent: HWND,
    class: PCWSTR,
    text: &str,
    bits: u32,
    r: Rect,
    id: usize,
) -> Option<HWND> {
    let title = wide(text);
    // SAFETY: creating a child control of our form window; the class is a system class, the text
    // buffer outlives the call.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(title.as_ptr()),
            WS_CHILD | WS_VISIBLE | style(bits),
            r.x,
            r.y,
            r.w,
            r.h,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(instance),
            None,
        )
    }
    .ok()?;
    // SAFETY: the stock GUI font is a shared object; WM_SETFONT only references it.
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    send(hwnd, WM_SETFONT, font.0 as usize, 1);
    Some(hwnd)
}

/// The form's selected item of a pop-up (0 for anything else).
fn selected_of(item: &Item) -> usize {
    match item {
        Item::Choice { selected, .. } => *selected,
        _ => 0,
    }
}

/// The tooltip a control carries: the hint of a field, the blocked items of a pop-up.
fn tip_text(item: &Item) -> String {
    match item {
        Item::Field { hint, .. } => hint.clone(),
        Item::Choice { items, selected, .. } => ChoiceList::new(items, *selected).blocked_note,
        _ => String::new(),
    }
}

fn is_pushlike(id: &ControlId) -> bool {
    matches!(id, ControlId::RefPoint(_) | ControlId::Palette(_))
}

fn make(instance: HINSTANCE, parent: HWND, placed: &crate::forms::layout::Placed, index: usize) -> Option<Ctl> {
    let r = placed.rect;
    let cid = CTL_BASE + index;
    let caption = match &placed.caption {
        Some((text, cr)) => child(instance, parent, w!("STATIC"), text, SS_LEFT, *cr, 0),
        None => None,
    };
    let tab = WS_TABSTOP.0;
    let (hwnd, id, shown) = match &placed.item {
        Item::Heading(t) => (child(instance, parent, w!("STATIC"), t, SS_LEFT, r, 0)?, None, String::new()),
        Item::Text(t) => (child(instance, parent, w!("STATIC"), t, SS_LEFT, r, 0)?, None, String::new()),
        Item::Problem(t) => {
            let text = format!("\u{26A0} {t}");
            (child(instance, parent, w!("STATIC"), &text, SS_LEFT, r, 0)?, None, String::new())
        }
        Item::Field { id, text, .. } => {
            let hwnd = child(instance, parent, w!("EDIT"), text, WS_BORDER.0 | tab | ES_AUTOHSCROLL, r, cid)?;
            // SAFETY: subclassing our own edit control to see Return; the subclass id is ours.
            let _ = unsafe { SetWindowSubclass(hwnd, Some(edit_proc), 2, 0) };
            (hwnd, Some(id.clone()), text.clone())
        }
        Item::Memo { id, text } => {
            // Return makes a new line (no subclass): a button commits the note.
            let bits = WS_BORDER.0 | WS_VSCROLL.0 | tab | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN;
            let hwnd = child(instance, parent, w!("EDIT"), &text.replace('\n', "\r\n"), bits, r, cid)?;
            (hwnd, Some(id.clone()), text.clone())
        }
        Item::Check { id, caption, .. } => (
            child(instance, parent, w!("BUTTON"), caption, tab | BS_AUTOCHECKBOX, r, cid)?,
            Some(id.clone()),
            String::new(),
        ),
        Item::Choice { id, items, .. } => {
            // The height of a combo box includes its drop-down list.
            let tall = Rect { h: r.h * 8, ..r };
            let hwnd = child(instance, parent, w!("COMBOBOX"), "", tab | WS_VSCROLL.0 | CBS_DROPDOWNLIST, tall, cid)?;
            // Blocked items are not offered at all (a system drop-down cannot grey one).
            for title in &ChoiceList::new(items, selected_of(&placed.item)).titles {
                let text = wide(title);
                send(hwnd, CB_ADDSTRING, 0, text.as_ptr() as isize);
            }
            (hwnd, Some(id.clone()), String::new())
        }
        Item::Button(b) => {
            let bits = tab | if is_pushlike(&b.id) { BS_AUTOCHECKBOX | BS_PUSHLIKE } else { BS_PUSHBUTTON };
            (child(instance, parent, w!("BUTTON"), &b.title, bits, r, cid)?, Some(b.id.clone()), String::new())
        }
        Item::List { id, multi, .. } => {
            let mut bits = tab | WS_BORDER.0 | WS_VSCROLL.0 | LBS_NOTIFY;
            if *multi {
                bits |= LBS_EXTENDEDSEL;
            }
            (child(instance, parent, w!("LISTBOX"), "", bits, r, cid)?, Some(id.clone()), String::new())
        }
    };
    let mut ctl = Ctl { hwnd, caption, id, item: placed.item.clone(), shown };
    push_values(&mut ctl, placed);
    Some(ctl)
}

fn list_selection(hwnd: HWND) -> Vec<usize> {
    let count = send(hwnd, LB_GETSELCOUNT, 0, 0).0.max(0) as usize;
    let mut rows = vec![0i32; count];
    let n = send(hwnd, LB_GETSELITEMS, count, rows.as_mut_ptr() as isize).0.max(0) as usize;
    rows.truncate(n);
    rows.into_iter().filter_map(|r| usize::try_from(r).ok()).collect()
}

/// Write the values of `placed` into `ctl` where they differ from what the control holds.
fn push_values(ctl: &mut Ctl, placed: &crate::forms::layout::Placed) {
    if let (Some(cap), Some((text, _))) = (ctl.caption, &placed.caption) {
        set_text(cap, text);
    }
    let enable = |hwnd: HWND, on: bool| {
        // SAFETY: plain state query/change on our own control.
        if unsafe { IsWindowEnabled(hwnd) }.as_bool() != on {
            // SAFETY: as above.
            let _ = unsafe { EnableWindow(hwnd, on) };
        }
    };
    match &placed.item {
        Item::Heading(t) | Item::Text(t) => set_text(ctl.hwnd, t),
        Item::Problem(t) => set_text(ctl.hwnd, &format!("\u{26A0} {t}")),
        Item::Field { text, enabled, .. } => {
            if *text != ctl.shown && window_text(ctl.hwnd) != *text {
                set_text(ctl.hwnd, text);
            }
            ctl.shown = text.clone();
            enable(ctl.hwnd, *enabled);
        }
        Item::Memo { text, .. } => {
            // What the user typed stays until the stored note changes.
            if *text != ctl.shown {
                set_text(ctl.hwnd, &text.replace('\n', "\r\n"));
            }
            ctl.shown = text.clone();
        }
        Item::Check { on, enabled, caption, .. } => {
            if window_text(ctl.hwnd) != *caption {
                set_text(ctl.hwnd, caption);
            }
            let state = if *on { BST_CHECKED } else { 0 };
            if send(ctl.hwnd, BM_GETCHECK, 0, 0).0 as usize != state {
                send(ctl.hwnd, BM_SETCHECK, state, 0);
            }
            enable(ctl.hwnd, *enabled);
        }
        Item::Choice { items, selected, enabled, .. } => {
            let list = ChoiceList::new(items, *selected);
            let same = matches!(&ctl.item, Item::Choice { items: old, selected: old_selected, .. }
                if ChoiceList::new(old, *old_selected).titles == list.titles);
            if !same {
                send(ctl.hwnd, CB_RESETCONTENT, 0, 0);
                for title in &list.titles {
                    let text = wide(title);
                    send(ctl.hwnd, CB_ADDSTRING, 0, text.as_ptr() as isize);
                }
            }
            let want = list.selected.map_or(-1, |p| p as isize);
            if !same || send(ctl.hwnd, CB_GETCURSEL, 0, 0).0 != want {
                send(ctl.hwnd, CB_SETCURSEL, list.selected.unwrap_or(usize::MAX), 0);
            }
            enable(ctl.hwnd, *enabled);
        }
        Item::Button(b) => {
            if window_text(ctl.hwnd) != b.title {
                set_text(ctl.hwnd, &b.title);
            }
            if is_pushlike(&b.id) {
                let state = if b.pressed { BST_CHECKED } else { 0 };
                if send(ctl.hwnd, BM_GETCHECK, 0, 0).0 as usize != state {
                    send(ctl.hwnd, BM_SETCHECK, state, 0);
                }
            }
            enable(ctl.hwnd, b.enabled);
        }
        Item::List { items, selected, multi, .. } => {
            let same_items = matches!(&ctl.item, Item::List { items: old, .. } if old == items);
            if !same_items {
                send(ctl.hwnd, LB_RESETCONTENT, 0, 0);
                for item in items {
                    let text = wide(item);
                    send(ctl.hwnd, LB_ADDSTRING, 0, text.as_ptr() as isize);
                }
            }
            let now = if *multi {
                list_selection(ctl.hwnd)
            } else {
                usize::try_from(send(ctl.hwnd, LB_GETCURSEL, 0, 0).0).ok().into_iter().collect()
            };
            if !same_items || now != *selected {
                if *multi {
                    send(ctl.hwnd, LB_SETSEL, 0, -1);
                    for row in selected {
                        send(ctl.hwnd, LB_SETSEL, 1, *row as isize);
                    }
                } else {
                    send(ctl.hwnd, LB_SETCURSEL, selected.first().copied().unwrap_or(usize::MAX), 0);
                }
            }
        }
    }
    ctl.item = placed.item.clone();
}

fn place(ctl: &Ctl, placed: &crate::forms::layout::Placed, scroll: i32) {
    let move_to = |hwnd: HWND, r: Rect, tall: bool| {
        let h = if tall { r.h * 8 } else { r.h };
        // SAFETY: moving one of our own child windows; repaint requested.
        let _ = unsafe { MoveWindow(hwnd, r.x, r.y - scroll, r.w, h, true) };
    };
    move_to(ctl.hwnd, placed.rect, matches!(placed.item, Item::Choice { .. }));
    if let (Some(cap), Some((_, cr))) = (ctl.caption, &placed.caption) {
        move_to(cap, *cr, false);
    }
}

impl FormWin {
    /// A container for `form` owned by (or, docked, a child of) `frame`.
    pub(super) fn create(
        instance: HINSTANCE,
        frame: HWND,
        document: DocumentId,
        form: &Form,
        scale: f64,
    ) -> Option<FormWin> {
        let floating = form.placement == Placement::Floating;
        let width = (f64::from(if floating { 380 } else { 260 }) * scale).round() as i32;
        let (bits, parent, x, y) = if floating {
            (
                WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_VSCROLL.0 | WS_CLIPCHILDREN.0,
                None,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
            )
        } else {
            (WS_CHILD.0 | WS_VISIBLE.0 | WS_VSCROLL.0 | WS_CLIPCHILDREN.0, Some(frame), 0, 0)
        };
        let title = wide(&form.title);
        // SAFETY: our registered form class; for a floating panel the frame is only the owner (a
        // top-level window with the frame as parent argument would be a child); strings outlive the call.
        let hwnd = unsafe {
            CreateWindowExW(
                // A docked panel sits in the frame's tab order: Tab goes on into its controls.
                if floating { WINDOW_EX_STYLE(0) } else { WS_EX_CONTROLPARENT },
                FORM_CLASS,
                PCWSTR(title.as_ptr()),
                style(bits),
                x,
                y,
                width,
                (f64::from(420) * scale).round() as i32,
                if floating { Some(frame) } else { parent },
                None,
                Some(instance),
                None,
            )
        }
        .ok()?;
        // SAFETY: the document id in our own window's user-data slot (accelerators and routing).
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                document.raw() as isize,
            )
        };
        let mut win = FormWin {
            hwnd,
            kind: form.kind,
            placement: form.placement,
            instance,
            ctls: Vec::new(),
            signature: Vec::new(),
            content_h: 0,
            scroll: 0,
            scale,
            last: form.clone(),
            tip: super::tips::create(hwnd, instance),
        };
        if let Some(tip) = win.tip {
            super::tips::set_area_tip(tip, hwnd, true);
        }
        win.sync(form);
        Some(win)
    }

    fn client_width(&self) -> i32 {
        let mut rc = RECT::default();
        // SAFETY: our own window; `rc` is a valid out-parameter.
        let _ = unsafe { GetClientRect(self.hwnd, &mut rc) };
        rc.right - rc.left
    }

    fn client_height(&self) -> i32 {
        let mut rc = RECT::default();
        // SAFETY: as above.
        let _ = unsafe { GetClientRect(self.hwnd, &mut rc) };
        rc.bottom - rc.top
    }

    /// Bring the controls in step with `form`: update in place when its shape is the same,
    /// otherwise build them again.
    pub(super) fn sync(&mut self, form: &Form) {
        self.last = form.clone();
        let width = self.client_width().max((100.0 * self.scale) as i32);
        let lay: Layout = layout(form, width, self.scale);
        let signature: Vec<String> = lay.items.iter().map(|p| signature_of(&p.item)).collect();
        if signature != self.signature {
            for ctl in self.ctls.drain(..) {
                if let Some(tip) = self.tip {
                    super::tips::set_control_tip(tip, self.hwnd, ctl.hwnd, "");
                }
                if let Some(cap) = ctl.caption {
                    // SAFETY: destroying our own child window; no message is in flight for it.
                    let _ = unsafe { DestroyWindow(cap) };
                }
                // SAFETY: as above.
                let _ = unsafe { DestroyWindow(ctl.hwnd) };
            }
            for (i, placed) in lay.items.iter().enumerate() {
                if let Some(ctl) = make(self.instance, self.hwnd, placed, i) {
                    self.ctls.push(ctl);
                }
            }
            self.signature = signature;
        } else {
            for (ctl, placed) in self.ctls.iter_mut().zip(&lay.items) {
                push_values(ctl, placed);
            }
        }
        if let Some(tip) = self.tip {
            for ctl in &self.ctls {
                super::tips::set_control_tip(tip, self.hwnd, ctl.hwnd, &tip_text(&ctl.item));
            }
        }
        self.content_h = lay.height;
        self.scroll = self.scroll.min((self.content_h - self.client_height()).max(0));
        for (ctl, placed) in self.ctls.iter().zip(&lay.items) {
            place(ctl, placed, self.scroll);
        }
        self.update_scrollbar();
    }

    fn update_scrollbar(&self) {
        let info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: self.content_h.max(0),
            nPage: self.client_height().max(0) as u32,
            nPos: self.scroll,
            nTrackPos: 0,
        };
        // SAFETY: `info` is fully initialised; our own window.
        unsafe { SetScrollInfo(self.hwnd, SB_VERT, &info, true) };
    }

    /// Scroll to `position` (clamped to the content).
    pub(super) fn scroll_to(&mut self, position: i32) {
        let max = (self.content_h - self.client_height()).max(0);
        let target = position.clamp(0, max);
        let delta = self.scroll - target;
        if delta != 0 {
            self.scroll = target;
            // SAFETY: scrolling the client area of our own window.
            let _ = unsafe { ScrollWindow(self.hwnd, 0, delta, None, None) };
            self.update_scrollbar();
        }
    }

    /// The scroll position for a `WM_VSCROLL` request.
    pub(super) fn scroll_request(&mut self, code: u16, track: i32) {
        let page = self.client_height();
        let line = (20.0 * self.scale) as i32;
        let target = match code {
            0 => self.scroll - line,
            1 => self.scroll + line,
            2 => self.scroll - page,
            3 => self.scroll + page,
            4 | 5 => track,
            6 => 0,
            7 => self.content_h,
            _ => return,
        };
        self.scroll_to(target);
    }

    /// The thumb position during a drag (`SB_THUMBTRACK`).
    pub(super) fn track_pos(&self) -> i32 {
        let mut info =
            SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_ALL, ..Default::default() };
        // SAFETY: `info` is initialised with its size; our own window.
        let _ = unsafe { GetScrollInfo(self.hwnd, SB_VERT, &mut info) };
        info.nTrackPos
    }

    /// Dock the container at a place in the frame (and lay its controls out for the new width).
    pub(super) fn dock(&mut self, x: i32, y: i32, w: i32, h: i32) {
        // SAFETY: moving our own child window.
        let _ = unsafe { MoveWindow(self.hwnd, x, y, w.max(1), h.max(1), true) };
        self.relayout();
    }

    /// Lay the controls out again for the container's current size.
    pub(super) fn relayout(&mut self) {
        let form = self.last.clone();
        self.sync(&form);
        if let Some(tip) = self.tip {
            super::tips::set_area_tip(tip, self.hwnd, false);
        }
    }

    pub(super) fn tip(&self) -> Option<HWND> {
        self.tip
    }

    /// Why the disabled field under `point` is off (its hint), for the area tool.
    pub(super) fn reason_at(&self, point: (i32, i32)) -> Option<String> {
        let lay = layout(&self.last, self.client_width().max((100.0 * self.scale) as i32), self.scale);
        disabled_tip_at(&lay, self.scroll, point)
    }

    /// Size a floating panel to its content (at most 640 logical pixels high).
    pub(super) fn fit_floating(&mut self) {
        let width = self.client_width().max(1);
        let lay = layout(&self.last, width, self.scale);
        let height = lay.height.min((640.0 * self.scale) as i32);
        // SAFETY: resizing our own top-level panel; position and z-order stay.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                width + (24.0 * self.scale) as i32,
                height + (40.0 * self.scale) as i32,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        self.relayout();
    }

    /// What every control holds now.
    pub(super) fn values(&self) -> Vec<(ControlId, ControlValue)> {
        self.ctls.iter().filter_map(|c| Some((c.id.clone()?, read(c)?))).collect()
    }

    /// The control event behind a `WM_COMMAND` of this container (`None`: nothing to report).
    pub(super) fn event(&self, control_id: usize, code: u16) -> Option<(ControlId, ControlValue)> {
        let ctl = self.ctls.get(control_id.checked_sub(CTL_BASE)?)?;
        let id = ctl.id.clone()?;
        let value = match (&ctl.item, code) {
            (Item::Field { .. }, EN_KILLFOCUS | EN_RETURN) => ControlValue::Text(window_text(ctl.hwnd)),
            (Item::Check { .. }, BN_CLICKED) => read(ctl)?,
            (Item::Button(_), BN_CLICKED) => {
                // SAFETY: plain keyboard-state query.
                if unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0 {
                    ControlValue::ShiftPress
                } else {
                    ControlValue::Press
                }
            }
            (Item::Choice { .. }, CBN_SELCHANGE) => read(ctl)?,
            (Item::List { .. }, LBN_SELCHANGE) => read(ctl)?,
            (Item::List { .. }, LBN_DBLCLK) => {
                ControlValue::Activate(usize::try_from(send(ctl.hwnd, LB_GETCURSEL, 0, 0).0).ok()?)
            }
            _ => return None,
        };
        Some((id, value))
    }

    pub(super) fn destroy(self) {
        // SAFETY: destroying our own container; its children go with it.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

fn read(ctl: &Ctl) -> Option<ControlValue> {
    Some(match &ctl.item {
        Item::Field { .. } => ControlValue::Text(window_text(ctl.hwnd)),
        // An edit control keeps CR LF; the note is stored with LF.
        Item::Memo { .. } => ControlValue::Text(window_text(ctl.hwnd).replace("\r\n", "\n")),
        Item::Check { .. } => ControlValue::Toggle(send(ctl.hwnd, BM_GETCHECK, 0, 0).0 as usize == BST_CHECKED),
        Item::Choice { items, selected, .. } => {
            let position = usize::try_from(send(ctl.hwnd, CB_GETCURSEL, 0, 0).0).ok()?;
            ControlValue::Choice(ChoiceList::new(items, *selected).item_at(position)?)
        }
        Item::List { multi, .. } => ControlValue::Rows(if *multi {
            list_selection(ctl.hwnd)
        } else {
            usize::try_from(send(ctl.hwnd, LB_GETCURSEL, 0, 0).0).ok().into_iter().collect()
        }),
        Item::Button(_) | Item::Heading(_) | Item::Text(_) | Item::Problem(_) => return None,
    })
}

/// Return in an edit field reports like losing the focus: the typed text is a commit.
extern "system" fn edit_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_GETDLGCODE {
        // SAFETY: for WM_GETDLGCODE `lparam` is null or points to the MSG being processed.
        let pressed = unsafe { (lparam.0 as *const MSG).as_ref() }
            .filter(|m| m.message == WM_KEYDOWN)
            .map(|m| (m.wParam.0 & 0xFFFF) as u16);
        // SAFETY: the subclass chain's default handling.
        let base = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        // Return commits the typed text (below); the dialog manager must not take it.
        return if pressed == Some(crate::host_panels::keys::VK_RETURN) {
            LRESULT(base.0 | DLGC_WANTMESSAGE as isize)
        } else {
            base
        };
    }
    if message == WM_KEYDOWN && wparam.0 == usize::from(VK_RETURN.0) {
        // SAFETY: reading the parent of our own edit field.
        let parent = unsafe { GetParent(hwnd) };
        // SAFETY: reading the control id of our own edit field.
        let id = unsafe { GetDlgCtrlID(hwnd) };
        if let Ok(parent) = parent {
            let packed = (usize::from(EN_RETURN) << 16) | (id as usize & 0xFFFF);
            // SAFETY: posting a notification to our own container window.
            let _ = unsafe { PostMessageW(Some(parent), WM_COMMAND, WPARAM(packed), LPARAM(hwnd.0 as isize)) };
            return LRESULT(0);
        }
    }
    // SAFETY: the subclass chain's default handling.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// What closing a floating panel means (the Mac panels have no close box; the buttons say it).
pub(super) fn close_command(kind: FormKind) -> Option<ControlId> {
    use crate::panels::Slot;
    match kind {
        FormKind::CutSettings => Some(ControlId::CutCancel),
        FormKind::Cam(_) => Some(ControlId::CamAction(burnburn_presentation::DialogActionKind::Close)),
        FormKind::GeometryDialog => Some(ControlId::GeometryCancel),
        FormKind::TextPath => Some(ControlId::Text(Slot::PathCancel)),
        FormKind::Barcode => Some(ControlId::Text(Slot::BcCancel)),
        FormKind::Schikken => None,
        // The notes close like their button; the others close with the menu item or the trace.
        FormKind::G5(crate::panels::G5Panel::Notes) => Some(ControlId::G5(crate::panels::G5Control::NotesClose)),
        // The generator dialog closes like its Cancel button (G6-editor).
        FormKind::G5(crate::panels::G5Panel::Generator) => Some(ControlId::G5(crate::panels::G5Control::GeneratorCancel)),
        FormKind::G5(_) => None,
        FormKind::Inspector | FormKind::Layers | FormKind::TextToolbar | FormKind::Variable | FormKind::Sketch => None,
    }
}
