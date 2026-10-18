//! The thin Win32 layer (Windows only, ADR 0006 route 8a + Win32 controls): one frame window per
//! document with a list box (objects), edit controls (inspector), a status line and a child canvas
//! window with the shared wgpu DX12 surface. Every message goes through
//! [`adapter::on_message`](crate::adapter::on_message); this file only reads and writes widgets.
//!
//! **Unconfirmed**: this module is type-checked from macOS (`cargo check --target
//! x86_64-pc-windows-msvc`) but has never been linked or run. See `docs/dev-notes/IM08.md`.
//!
//! Invariants (why the `unsafe` here is sound):
//! - Single UI thread (STA): all windows, the controller and the canvas live on the thread that
//!   runs [`run_app`]; workers only call `PostMessageW` (thread-safe) with a plain window handle.
//! - Window procedures never unwind: each body runs in [`guard`] (`catch_unwind`) and falls back to
//!   `DefWindowProcW`.
//! - The host state is in a thread-local `RefCell`; window procedures borrow it with `try_borrow*`
//!   and drop the borrow before calling anything that can re-enter (`DestroyWindow`, dialogs).
//! - Each `SurfaceCanvas` owns a factory holding the canvas `HWND`; the surface is dropped before
//!   the canvas window is destroyed (`DocWin::drop` order, `WM_DESTROY` takes the canvas first).
//! - Strings passed to Win32 are NUL-terminated UTF-16 buffers that outlive the call.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::NonZeroIsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use burnburn_presentation::{
    AppController, AppPorts, AppUpdate, AppViewModel, CommandKey, FieldKey, HostEffect, UiIntent,
};
use burnburn_render::surface::SurfaceCanvas;
use burnburn_render::{Theme, build_packet};
use burnburn_types::{DocumentId, StableId};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_MENU};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    DefSubclassProc, FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
    SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ACCEL, AppendMenuW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateAcceleratorTableW, CreateMenu, CreateWindowExW,
    DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL, EnableMenuItem, FCONTROL, FSHIFT, FVIRTKEY,
    GWLP_USERDATA, GetClientRect, GetCursorPos, GetMessageTime, GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, HACCEL, HMENU, IDC_ARROW, KillTimer, LB_ADDSTRING, LB_GETSELCOUNT, LB_GETSELITEMS, LB_RESETCONTENT,
    LB_SETSEL, LBS_EXTENDEDSEL, LBS_NOTIFY, LoadCursorW, MB_ICONQUESTION, MB_ICONWARNING, MB_YESNOCANCEL,
    MESSAGEBOX_RESULT, MF_ENABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, MessageBoxW, MoveWindow,
    PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW, SendMessageW, SetTimer, SetWindowLongPtrW,
    SetWindowTextW, ShowWindow, TranslateAcceleratorW, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_DESTROY,
    WM_INITMENUPOPUP, WM_TIMER, WNDCLASSEXW, WS_BORDER, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW, WS_TABSTOP,
    WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use crate::adapter::{
    CharDecoder, Control, INSPECTOR_FIELDS, MenuItem, MessageContext, Reaction, accelerators, command_id, field_text,
    menu_enabled, menu_items, message_text, msg, object_labels, on_close_box, on_message, on_open_dialog,
    on_recovery_box, on_save_dialog, recovery_box_text, recovery_dir, selected_rows, vk, window_title,
};
use crate::tasks::{TaskInbox, task_channel};

/// Posted by workers to the message window: task outcomes are waiting.
const WM_TASKS_DONE: u32 = WM_APP + 1;
/// Posted after a caught panic: autosave unsaved work into recovery files.
const WM_AUTOSAVE: u32 = WM_APP + 2;
/// The autosave timer on the message window (every 30 s; only changed documents are written).
const AUTOSAVE_TIMER: usize = 1;
const AUTOSAVE_MS: u32 = 30_000;
const SIDEBAR_PX: i32 = 220;
const INSPECTOR_PX: i32 = 260;

fn set_text(hwnd: HWND, text: &str) {
    // SAFETY: `hwnd` is one of our windows on this (UI) thread; the HSTRING outlives the call.
    let _ = unsafe { SetWindowTextW(hwnd, &HSTRING::from(text)) };
}

/// `SendMessageW` to one of our own controls. Pointer-carrying `lparam`s must point to buffers
/// that live for the (synchronous) call.
fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> LRESULT {
    // SAFETY: our own control on the UI thread; see the function contract for pointer arguments.
    unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct DocWin {
    // Field order: the surface goes before the windows it draws into.
    canvas_surface: Option<SurfaceCanvas>,
    frame: HWND,
    canvas: HWND,
    list: HWND,
    edits: Vec<(FieldKey, HWND)>,
    status: HWND,
    syncing: bool,
}

struct Host {
    controller: AppController,
    windows: BTreeMap<DocumentId, DocWin>,
    inbox: TaskInbox,
    accel: HACCEL,
    instance: HINSTANCE,
    message_window: HWND,
    /// `WM_CHAR` UTF-16 units of the canvas (typed or IME text) to final text.
    chars: CharDecoder,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn with_host<T>(f: impl FnOnce(&mut Host) -> T) -> Option<T> {
    HOST.with(|h| h.try_borrow_mut().ok().and_then(|mut h| h.as_mut().map(f)))
}

fn guard(what: &str, default: LRESULT, body: impl FnOnce() -> LRESULT) -> LRESULT {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| {
        eprintln!("burnburn-win: internal error in {what}; the message was dropped, unsaved work is autosaved");
        if let Some(target) = with_host(|h| h.message_window) {
            // SAFETY: posting to our own message window (handled after this message returns).
            let _ = unsafe { PostMessageW(Some(target), WM_AUTOSAVE, WPARAM(0), LPARAM(0)) };
        }
        default
    })
}

fn document_of(hwnd: HWND) -> Option<DocumentId> {
    // SAFETY: reading the user-data slot of a window we created; 0 means "not set".
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    (raw > 0).then(|| DocumentId::from_raw(raw as u64))
}

fn view() -> Arc<AppViewModel> {
    with_host(|h| h.controller.view())
        .unwrap_or_else(|| Arc::new(AppViewModel { documents: Vec::new(), active: None, commands: BTreeMap::new() }))
}

fn window_text(hwnd: HWND) -> String {
    // SAFETY: the length query and the copy use a buffer of the reported length + NUL.
    let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` is a valid, writable buffer for the duration of the call.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(len)])
}

fn list_selection(list: HWND) -> Vec<usize> {
    // SAFETY: LB_GETSELCOUNT/LB_GETSELITEMS on our list box; the buffer holds `count` ints.
    let count = unsafe { SendMessageW(list, LB_GETSELCOUNT, None, None) }.0.max(0) as usize;
    let mut rows = vec![0i32; count];
    // SAFETY: as above; the pointer stays valid during the synchronous call.
    let n =
        unsafe { SendMessageW(list, LB_GETSELITEMS, Some(WPARAM(count)), Some(LPARAM(rows.as_mut_ptr() as isize))) }.0;
    rows.truncate(n.max(0) as usize);
    rows.into_iter().filter_map(|r| usize::try_from(r).ok()).collect()
}

/// Send an intent, then perform the effects with the host borrow released.
fn dispatch(intent: UiIntent) {
    let Some(update) = with_host(|h| h.controller.handle(intent)) else { return };
    apply(update);
}

fn react(reaction: Reaction) -> bool {
    match reaction {
        Reaction::PassOn => false,
        Reaction::Nothing => true,
        Reaction::Intent(intent) => {
            dispatch(intent);
            true
        }
        Reaction::ShowOpenDialog => {
            let path = open_dialog();
            react(on_open_dialog(path));
            true
        }
    }
}

fn apply(update: AppUpdate) {
    for effect in &update.effects {
        match effect {
            HostEffect::OpenWindow { document } => open_window(*document),
            HostEffect::CloseWindow { document } => {
                let win = with_host(|h| h.windows.remove(document)).flatten();
                if let Some(mut win) = win {
                    win.canvas_surface = None;
                    // SAFETY: our own frame window; destroying it destroys its children.
                    let _ = unsafe { DestroyWindow(win.frame) };
                }
            }
            HostEffect::ShowSavePanel { document, suggested_name } => {
                let frame = with_host(|h| h.windows.get(document).map(|w| w.frame)).flatten();
                let path = frame.and_then(|f| save_dialog(f, suggested_name));
                react(on_save_dialog(*document, path, &view()));
            }
            HostEffect::ConfirmClose { document } => {
                let frame = with_host(|h| h.windows.get(document).map(|w| w.frame)).flatten();
                let title = window_title(&view(), *document);
                let text = HSTRING::from(format!("Wil je de wijzigingen in “{title}” bewaren?"));
                // SAFETY: modal message box owned by our frame; strings outlive the call.
                let result: MESSAGEBOX_RESULT =
                    unsafe { MessageBoxW(frame, &text, w!("BurnBurn"), MB_YESNOCANCEL | MB_ICONWARNING) };
                react(on_close_box(*document, result.0, &view()));
            }
            HostEffect::ShowError { document, message } => {
                let text = message_text(*message, system_locale());
                eprintln!("burnburn-win: {text}");
                if let Some(status) =
                    document.and_then(|d| with_host(|h| h.windows.get(&d).map(|w| w.status)).flatten())
                {
                    set_text(status, &text);
                }
            }
            HostEffect::Redraw { document } => redraw(*document),
            HostEffect::OfferRecovery { files } => {
                let owner = with_host(|h| h.windows.values().next().map(|w| w.frame)).flatten();
                for (i, path) in files.iter().enumerate() {
                    let text = HSTRING::from(recovery_box_text(path, i + 1, files.len(), system_locale()));
                    // SAFETY: modal message box owned by our frame (or none); strings outlive the call.
                    let result: MESSAGEBOX_RESULT =
                        unsafe { MessageBoxW(owner, &text, w!("BurnBurn"), MB_YESNOCANCEL | MB_ICONQUESTION) };
                    react(on_recovery_box(path, result.0));
                }
            }
            _ => {}
        }
    }
    if update.view_changed || !update.scenes_changed.is_empty() {
        refresh();
    }
    for document in &update.scenes_changed {
        redraw(*document);
    }
}

fn refresh() {
    let vm = view();
    with_host(|h| {
        for (document, w) in h.windows.iter_mut() {
            let Some(doc) = vm.document(*document) else { continue };
            set_text(w.frame, &window_title(&vm, *document));
            for (key, edit) in &w.edits {
                let text = field_text(&vm, *document, *key).unwrap_or_default();
                let has_error = doc.inspector.as_ref().and_then(|i| i.field(*key)).is_some_and(|f| f.error.is_some());
                if !has_error && window_text(*edit) != text {
                    set_text(*edit, &text);
                }
            }
            let message = doc
                .inspector
                .as_ref()
                .and_then(|i| i.fields.iter().find_map(|f| f.error))
                .or(doc.messages.last().map(|m| m.key))
                .map(|m| message_text(m, system_locale()))
                .unwrap_or_default();
            set_text(w.status, &message);
            w.syncing = true;
            send(w.list, LB_RESETCONTENT, 0, 0);
            for label in object_labels(&vm, *document) {
                let text = wide(&label);
                send(w.list, LB_ADDSTRING, 0, text.as_ptr() as isize);
            }
            for row in selected_rows(&vm, *document) {
                send(w.list, LB_SETSEL, 1, row as isize);
            }
            w.syncing = false;
        }
    });
}

fn redraw(document: DocumentId) {
    with_host(|h| {
        let Some(scene) = h.controller.scene(document) else { return };
        let packet = build_packet(&scene, &Theme::light());
        if let Some(canvas) = h.windows.get_mut(&document).and_then(|w| w.canvas_surface.as_mut())
            && let Err(e) = canvas.draw(&packet)
        {
            eprintln!("burnburn-win: canvas: {e}");
        }
    });
}

fn surface_for(instance: HINSTANCE, canvas: HWND) -> Option<SurfaceCanvas> {
    let hwnd = NonZeroIsize::new(canvas.0 as isize)?;
    let hinstance = NonZeroIsize::new(instance.0 as isize);
    let factory = Box::new(move |wgpu_instance: &wgpu::Instance| {
        let mut handle = Win32WindowHandle::new(hwnd);
        handle.hinstance = hinstance;
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
            raw_window_handle: RawWindowHandle::Win32(handle),
        };
        // SAFETY: `hwnd` is our canvas child window, alive until DocWin drops this surface first.
        unsafe { wgpu_instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())
    });
    match SurfaceCanvas::new(factory) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("burnburn-win: no DX12 canvas: {e}");
            None
        }
    }
}

fn open_window(document: DocumentId) {
    let Some((instance, menu)) = with_host(|h| (h.instance, build_menu())) else { return };
    let child = |class: PCWSTR, style, parent: HWND, id: usize| {
        // SAFETY: creating a child control of our frame; class names are system or ours.
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                PCWSTR::null(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                10,
                10,
                Some(parent),
                Some(HMENU(id as *mut _)),
                Some(instance),
                None,
            )
        }
    };
    // SAFETY: our registered frame class; the menu is handed to the window.
    let Ok(frame) = (unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BurnBurnFrame"),
            w!("BurnBurn"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1200,
            760,
            None,
            menu,
            Some(instance),
            None,
        )
    }) else {
        return;
    };
    // SAFETY: storing the document id in our own window's user-data slot.
    unsafe { SetWindowLongPtrW(frame, GWLP_USERDATA, document.raw() as isize) };
    let list = child(
        w!("LISTBOX"),
        WS_BORDER
            | WS_VSCROLL
            | WS_TABSTOP
            | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE((LBS_NOTIFY | LBS_EXTENDEDSEL) as u32),
        frame,
        1002,
    );
    let canvas = child(w!("BurnBurnCanvas"), WS_TABSTOP, frame, 1003);
    let status = child(w!("STATIC"), windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0), frame, 1004);
    let mut edits = Vec::new();
    for (i, key) in INSPECTOR_FIELDS.iter().enumerate() {
        if let Ok(edit) = child(
            w!("EDIT"),
            WS_BORDER | WS_TABSTOP | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            frame,
            1010 + i,
        ) {
            // SAFETY: subclassing our own edit control to see Enter; ref data is the document id.
            let _ = unsafe { SetWindowSubclass(edit, Some(edit_proc), 1, document.raw() as usize) };
            edits.push((*key, edit));
        }
    }
    let (Ok(list), Ok(canvas), Ok(status)) = (list, canvas, status) else { return };
    // The document id on every child too, so accelerators work wherever the focus is.
    for hwnd in [list, canvas, status].into_iter().chain(edits.iter().map(|(_, e)| *e)) {
        // SAFETY: user data on our own child windows (unused by the system classes).
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, document.raw() as isize) };
    }
    let canvas_surface = surface_for(instance, canvas);
    with_host(|h| {
        h.windows.insert(document, DocWin { canvas_surface, frame, canvas, list, edits, status, syncing: false });
    });
    layout(frame);
    // SAFETY: showing our own frame.
    let _ = unsafe { ShowWindow(frame, SW_SHOW) };
    refresh();
    redraw(document);
}

fn layout(frame: HWND) {
    let Some(document) = document_of(frame) else { return };
    let mut rc = RECT::default();
    // SAFETY: our frame; `rc` is a valid out-parameter.
    if unsafe { GetClientRect(frame, &mut rc) }.is_err() {
        return;
    }
    // SAFETY: GetDpiForWindow on our frame.
    let scale = f64::from(unsafe { GetDpiForWindow(frame) }.max(96)) / 96.0;
    let px = |v: i32| (f64::from(v) * scale).round() as i32;
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    let (side, insp) = (px(SIDEBAR_PX), px(INSPECTOR_PX));
    let parts =
        with_host(|host| host.windows.get(&document).map(|d| (d.list, d.canvas, d.status, d.edits.clone()))).flatten();
    let Some((list, canvas, status, edits)) = parts else { return };
    let place = |hwnd: HWND, x: i32, y: i32, width: i32, height: i32| {
        // SAFETY: one of our child windows on the UI thread; repaint requested.
        let _ = unsafe { MoveWindow(hwnd, x, y, width, height, true) };
    };
    place(list, 0, 0, side, h);
    place(canvas, side, 0, (w - side - insp).max(1), h);
    for (i, (_, edit)) in edits.iter().enumerate() {
        place(*edit, w - insp + px(12), px(16) + i as i32 * px(36), insp - px(24), px(24));
    }
    place(status, w - insp + px(12), px(16) + edits.len() as i32 * px(36), insp - px(24), px(40));
}

fn new_menu() -> Option<HMENU> {
    // SAFETY: creates a menu this thread owns (handed to a window or destroyed with it).
    unsafe { CreateMenu() }.ok()
}

fn append(menu: HMENU, flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS, id: usize, title: &str) {
    let text = HSTRING::from(title);
    // SAFETY: our menu; for MF_POPUP `id` is a menu handle we created; the string outlives the call.
    let _ = unsafe { AppendMenuW(menu, flags, id, &text) };
}

fn build_menu() -> Option<HMENU> {
    {
        let bar = new_menu()?;
        let add =
            |menu: HMENU, item: MenuItem, title: &str| append(menu, MF_STRING, usize::from(command_id(item)), title);
        let c = MenuItem::Command;
        let file = new_menu()?;
        add(file, c(CommandKey::New), "&Nieuw\tCtrl+N");
        add(file, MenuItem::Open, "&Open…\tCtrl+O");
        append(file, MF_SEPARATOR, 0, "");
        add(file, c(CommandKey::Save), "&Bewaar\tCtrl+S");
        add(file, c(CommandKey::SaveAs), "Bewaar &als…\tCtrl+Shift+S");
        add(file, c(CommandKey::Close), "&Sluit\tCtrl+W");
        let edit = new_menu()?;
        add(edit, c(CommandKey::Undo), "&Herstel\tCtrl+Z");
        add(edit, c(CommandKey::Redo), "&Opnieuw\tCtrl+Y");
        add(edit, c(CommandKey::Delete), "&Verwijder\tDel");
        add(edit, c(CommandKey::SelectAll), "Selecteer &alles");
        let tools = new_menu()?;
        for item in menu_items() {
            if let MenuItem::Command(
                k @ (CommandKey::ToolSelect
                | CommandKey::ToolRectangle
                | CommandKey::ToolEllipse
                | CommandKey::ToolLine),
            ) = item
            {
                add(tools, item, &format!("{k:?}"));
            }
        }
        append(bar, MF_POPUP, file.0 as usize, "&Archief");
        append(bar, MF_POPUP, edit.0 as usize, "&Wijzig");
        append(bar, MF_POPUP, tools.0 as usize, "&Gereedschap");
        Some(bar)
    }
}

fn file_filter() -> [COMDLG_FILTERSPEC; 1] {
    [COMDLG_FILTERSPEC { pszName: w!("BurnBurn-project"), pszSpec: w!("*.burnburn") }]
}

/// The chosen file of a closed dialog (`Show` returned OK).
fn dialog_path(dialog: &windows::Win32::UI::Shell::IFileDialog) -> Option<PathBuf> {
    // SAFETY: COM call on the STA UI thread on a live dialog.
    let item = unsafe { dialog.GetResult() }.ok()?;
    // SAFETY: as above, on the returned shell item.
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
    // SAFETY: `name` is a valid NUL-terminated string owned by us until freed below.
    let path = unsafe { name.to_string() }.ok().map(PathBuf::from);
    // SAFETY: freeing the CoTaskMem string GetDisplayName allocated, exactly once.
    unsafe { CoTaskMemFree(Some(name.0 as *const _)) };
    path
}

fn open_dialog() -> Option<PathBuf> {
    // SAFETY: COM on the STA UI thread (CoInitializeEx in run_app); released on drop.
    let dialog: IFileOpenDialog = unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let filter = file_filter();
    // SAFETY: the filter specs point to static strings and live for the call.
    unsafe { dialog.SetFileTypes(&filter) }.ok()?;
    // SAFETY: modal dialog on the UI thread; Cancel returns an error (→ None).
    unsafe { dialog.Show(None) }.ok()?;
    dialog_path(&dialog)
}

fn save_dialog(owner: HWND, suggested: &str) -> Option<PathBuf> {
    // SAFETY: COM on the STA UI thread (CoInitializeEx in run_app); released on drop.
    let dialog: IFileSaveDialog = unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let filter = file_filter();
    // SAFETY: the filter specs point to static strings and live for the call.
    unsafe { dialog.SetFileTypes(&filter) }.ok()?;
    // SAFETY: static string.
    unsafe { dialog.SetDefaultExtension(w!("burnburn")) }.ok()?;
    let name = HSTRING::from(suggested);
    // SAFETY: the string outlives the call.
    unsafe { dialog.SetFileName(&name) }.ok()?;
    // SAFETY: modal dialog owned by our frame on the UI thread; Cancel returns an error (→ None).
    unsafe { dialog.Show(Some(owner)) }.ok()?;
    dialog_path(&dialog)
}

fn context_for<'a>(
    document: DocumentId,
    control: Control,
    hwnd: HWND,
    text: &'a str,
    rows: &'a [usize],
) -> MessageContext<'a> {
    let mut cursor = POINT::default();
    // SAFETY: `cursor` is a valid out-parameter.
    let _ = unsafe { GetCursorPos(&mut cursor) };
    // SAFETY: our window; `cursor` is a valid in/out parameter.
    let _ = unsafe { ScreenToClient(hwnd, &mut cursor) };
    // SAFETY: plain query on our window.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    // SAFETY: plain keyboard-state query on the UI thread.
    let alt = unsafe { GetKeyState(i32::from(VK_MENU.0)) } < 0;
    // SAFETY: plain query about the message being processed.
    let time_ms = unsafe { GetMessageTime() } as u32;
    MessageContext { document, control, dpi, text, selected_rows: rows, alt, cursor_px: [cursor.x, cursor.y], time_ms }
}

extern "system" fn frame_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: default handling of a message for our own window.
    let default = || unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    guard("frame_proc", LRESULT(0), || {
        let Some(document) = document_of(hwnd) else { return default() };
        match message {
            msg::WM_SIZE => {
                layout(hwnd);
                return LRESULT(0);
            }
            WM_INITMENUPOPUP => {
                let vm = view();
                for item in menu_items() {
                    let flag = if menu_enabled(item, &vm) { MF_ENABLED } else { MF_GRAYED };
                    // SAFETY: the popup handle comes with the message; ids are ours.
                    let _ = unsafe { EnableMenuItem(HMENU(wparam.0 as *mut _), u32::from(command_id(item)), flag) };
                }
                return LRESULT(0);
            }
            WM_DESTROY => {
                let remaining = with_host(|h| {
                    if let Some(mut w) = h.windows.remove(&document) {
                        w.canvas_surface = None;
                    }
                    h.windows.len()
                });
                if remaining == Some(0) {
                    // SAFETY: ends our message loop.
                    unsafe { PostQuitMessage(0) };
                }
                return LRESULT(0);
            }
            _ => {}
        }
        // Which control a WM_COMMAND notification came from (lParam is its HWND).
        let source = HWND(lparam.0 as *mut _);
        let parts = with_host(|h| h.windows.get(&document).map(|w| (w.list, w.edits.clone(), w.syncing))).flatten();
        let Some((list, edits, syncing)) = parts else { return default() };
        let control = if message == msg::WM_COMMAND && source == list {
            if syncing {
                return LRESULT(0);
            }
            Control::ObjectList
        } else if let Some((key, _)) = edits.iter().find(|(_, e)| message == msg::WM_COMMAND && *e == source) {
            Control::Field(*key)
        } else {
            Control::Frame
        };
        let text = match control {
            Control::Field(_) => window_text(source),
            _ => String::new(),
        };
        let rows = if control == Control::ObjectList { list_selection(list) } else { Vec::new() };
        let ctx = context_for(document, control, hwnd, &text, &rows);
        if react(on_message(&ctx, message, wparam.0, lparam.0, &view())) { LRESULT(0) } else { default() }
    })
}

extern "system" fn canvas_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: default handling of a message for our own window.
    let default = || unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    guard("canvas_proc", LRESULT(0), || {
        let Some(document) = document_of(hwnd) else { return default() };
        match message {
            msg::WM_LBUTTONDOWN | msg::WM_RBUTTONDOWN | msg::WM_MBUTTONDOWN => {
                // SAFETY: mouse capture for our own canvas window during a drag (UI thread).
                let _ = unsafe { SetCapture(hwnd) };
                // SAFETY: keyboard focus to our own canvas window.
                let _ = unsafe { SetFocus(Some(hwnd)) };
            }
            msg::WM_LBUTTONUP | msg::WM_RBUTTONUP | msg::WM_MBUTTONUP => {
                // SAFETY: releases the capture this thread took on button down.
                let _ = unsafe { ReleaseCapture() };
            }
            _ => {}
        }
        // WM_CHAR carries one UTF-16 unit (IME results arrive the same way): decode to text.
        let text = if message == msg::WM_CHAR {
            with_host(|h| h.chars.push(wparam.0 as u16)).flatten().unwrap_or_default()
        } else {
            String::new()
        };
        let ctx = context_for(document, Control::Canvas, hwnd, &text, &[]);
        if react(on_message(&ctx, message, wparam.0, lparam.0, &view())) { LRESULT(0) } else { default() }
    })
}

extern "system" fn edit_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: the subclass chain's default handling.
    let default = || unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    guard("edit_proc", LRESULT(0), || {
        if message != msg::WM_KEYDOWN || wparam.0 != vk::RETURN {
            return default();
        }
        let document = DocumentId::from_raw(data as u64);
        let key = with_host(|h| {
            h.windows.get(&document).and_then(|w| w.edits.iter().find(|(_, e)| *e == hwnd).map(|(k, _)| *k))
        })
        .flatten();
        let Some(key) = key else { return default() };
        let text = window_text(hwnd);
        let ctx = context_for(document, Control::Field(key), hwnd, &text, &[]);
        if react(on_message(&ctx, message, wparam.0, lparam.0, &view())) { LRESULT(0) } else { default() }
    })
}

extern "system" fn message_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    guard("message_proc", LRESULT(0), || {
        if message == WM_TASKS_DONE {
            let outcomes = with_host(|h| h.inbox.drain()).unwrap_or_default();
            for outcome in outcomes {
                if let Some(update) = with_host(|h| h.controller.task_finished(outcome)) {
                    apply(update);
                }
            }
            return LRESULT(0);
        }
        if message == WM_AUTOSAVE || (message == WM_TIMER && wparam.0 == AUTOSAVE_TIMER) {
            dispatch(UiIntent::Autosave);
            return LRESULT(0);
        }
        // SAFETY: default handling for our message-only window.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    })
}

fn random_uuid() -> burnburn_io::ProjectUuid {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_usize(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        chunk.copy_from_slice(&h.finish().to_le_bytes()[..chunk.len()]);
    }
    burnburn_io::ProjectUuid::from_bytes(bytes)
}

fn register(instance: HINSTANCE, name: PCWSTR, proc_: windows::Win32::UI::WindowsAndMessaging::WNDPROC) -> bool {
    // SAFETY: loads the shared system arrow cursor (no ownership).
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: proc_,
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: name,
        ..Default::default()
    };
    // SAFETY: a fully initialised class description with a static name and our window procedure.
    unsafe { RegisterClassExW(&class) != 0 }
}

/// `burnburn-win`: one document window, the message loop, quit when the last window closes.
pub fn run_app() -> ExitCode {
    // SAFETY: process-wide DPI mode, set once before any window exists.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    // SAFETY: this thread becomes the STA UI thread (file dialogs are COM objects).
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // SAFETY: the module handle of this executable (no ownership).
    let instance = match unsafe { GetModuleHandleW(None) } {
        Ok(m) => HINSTANCE(m.0),
        Err(_) => return ExitCode::FAILURE,
    };
    if !(register(instance, w!("BurnBurnFrame"), Some(frame_proc))
        && register(instance, w!("BurnBurnCanvas"), Some(canvas_proc))
        && register(instance, w!("BurnBurnMessages"), Some(message_proc)))
    {
        return ExitCode::FAILURE;
    }
    // SAFETY: a hidden top-level window that only receives WM_TASKS_DONE.
    let Ok(message_window) = (unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BurnBurnMessages"),
            PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }) else {
        return ExitCode::FAILURE;
    };
    let target = message_window.0 as isize;
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        // SAFETY: PostMessageW is thread-safe; the handle is our message window (a plain value).
        let _ = unsafe { PostMessageW(Some(HWND(target as *mut _)), WM_TASKS_DONE, WPARAM(0), LPARAM(0)) };
    });
    let (port, inbox) = task_channel(wake);
    let uuid = Arc::new(random_uuid);
    let ports =
        AppPorts { tasks: Arc::new(port), fs: Arc::new(burnburn_io::std_fs::StdFileSystem), new_project_uuid: uuid };
    let table: Vec<ACCEL> = accelerators()
        .into_iter()
        .map(|a| {
            let mut f = FVIRTKEY;
            if a.ctrl {
                f |= FCONTROL;
            }
            if a.shift {
                f |= FSHIFT;
            }
            ACCEL { fVirt: f, key: a.key, cmd: a.id }
        })
        .collect();
    // SAFETY: the table is a valid slice for the call.
    let accel = unsafe { CreateAcceleratorTableW(&table) }.unwrap_or_default();
    let mut controller = AppController::new(ports);
    // Canvas text needs a face: the first readable system font (none: presentation says so).
    if let Some(font) =
        burnburn_runtime::fonts::first_readable_font(&burnburn_runtime::fonts::default_font_candidates())
    {
        controller.attach_fonts(font.source, font.font);
    }
    // Autosave into %LOCALAPPDATA%\BurnBurn\Recovery, never next to a project.
    if let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| recovery_dir(&PathBuf::from(d))) {
        match std::fs::create_dir_all(&dir) {
            Ok(()) => {
                controller.set_recovery_dir(dir);
                // SAFETY: a timer on our own message window (WM_TIMER on this thread).
                unsafe { SetTimer(Some(message_window), AUTOSAVE_TIMER, AUTOSAVE_MS, None) };
            }
            Err(e) => eprintln!("burnburn-win: recovery folder {}: {e}", dir.display()),
        }
    }
    HOST.with(|h| {
        if let Ok(mut h) = h.try_borrow_mut() {
            *h = Some(Host {
                controller,
                windows: BTreeMap::new(),
                inbox,
                accel,
                instance,
                message_window,
                chars: CharDecoder::default(),
            });
        }
    });
    dispatch(UiIntent::NewDocument);
    dispatch(UiIntent::CheckRecovery);
    let mut message = MSG::default();
    // The standard message loop on the thread that owns every window (each call below is a
    // plain Win32 call on this thread with a valid `MSG`).
    // SAFETY: see above; `message` is a valid out-parameter.
    while unsafe { GetMessageW(&mut message, None, 0, 0) } != BOOL(0) {
        let frame = document_of(message.hwnd).and_then(|d| with_host(|h| h.windows.get(&d).map(|w| w.frame)).flatten());
        let accel = with_host(|h| h.accel).unwrap_or_default();
        let translated = frame.is_some_and(|frame| {
            // SAFETY: our frame, our accelerator table, the message just read.
            (unsafe { TranslateAcceleratorW(frame, accel, &message) }) != 0
        });
        if translated {
            continue;
        }
        // SAFETY: the message just read.
        let _ = unsafe { TranslateMessage(&message) };
        // SAFETY: dispatches to our window procedures (which never unwind).
        unsafe { DispatchMessageW(&message) };
    }
    // SAFETY: stops the timer set above on our own message window.
    let _ = unsafe { KillTimer(Some(message_window), AUTOSAVE_TIMER) };
    HOST.with(|h| {
        if let Ok(mut h) = h.try_borrow_mut() {
            *h = None;
        }
    });
    ExitCode::SUCCESS
}

/// The UI language from the user's default locale name (`nl-BE`, `en-US`, …), as the macOS host
/// follows `NSLocale.preferredLanguages`.
#[allow(unsafe_code)]
fn system_locale() -> crate::l10n::Locale {
    let mut buffer = [0u16; 85];
    // SAFETY: a writable buffer of LOCALE_NAME_MAX_LENGTH (85) UTF-16 units, as the API requires.
    let len = unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut buffer) };
    let len = usize::try_from(len).unwrap_or(0).saturating_sub(1).min(buffer.len());
    let name = String::from_utf16_lossy(&buffer[..len]);
    crate::l10n::locale_from_preferred([name.as_str()])
}
