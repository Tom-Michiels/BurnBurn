//! The thin Win32 layer (Windows only, ADR 0006 route 8a + Win32 controls): one frame window per
//! document with a tool strip, a list box (objects), docked and floating panels built from the
//! headless forms in [`crate::forms`] (inspector, layers, Cut Settings, CAM dialogs, Schikken, text,
//! sketch), a status line and a child canvas window with the shared wgpu DX12 surface. Every message goes through
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

mod camera;
mod dialogs;
mod forms;
mod menu;
mod strip;
pub(crate) mod tips;
mod util;

use dialogs::{FileKind, open_dialog, open_dialog_of, open_many_dialog_of, save_dialog, save_dialog_of};
use forms::{FORM_CLASS, FormWin, close_command};
use menu::{build_menu, refresh_popup};
use strip::{STRIP_BASE, StripWin, choose_variant};
use util::{list_selection, send, set_text, wide};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::NonZeroIsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use burnburn_presentation::{
    AppController, AppPorts, AppUpdate, AppViewModel, CamDialog, DialogViewModel, HostEffect, UiIntent,
};
use burnburn_render::surface::SurfaceCanvas;
use burnburn_render::{Theme, build_packet};
use burnburn_types::{DocumentId, OperationId, StableId};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
use windows::Win32::Foundation::{COLORREF, ERROR_SUCCESS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, GetSysColor, GetSysColorBrush, HDC, InvalidateRect, ScreenToClient, SetBkColor, SetTextColor,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_MENU};
use windows::Win32::UI::WindowsAndMessaging::{
    ACCEL, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateAcceleratorTableW, CreateWindowExW, DLGC_WANTMESSAGE,
    DefWindowProcW, DestroyWindow, DispatchMessageW, FALT, FCONTROL, FSHIFT, FVIRTKEY, GA_ROOT, GWLP_USERDATA,
    GetAncestor, GetClassNameW, GetClientRect, GetCursorPos, GetMessageTime, GetMessageW, GetWindowLongPtrW, HACCEL,
    HMENU, IDC_ARROW, IsDialogMessageW, KillTimer, LB_ADDSTRING, LB_RESETCONTENT, LB_SETSEL, LBS_EXTENDEDSEL,
    LBS_NOTIFY, LoadCursorW, LoadIconW, MB_DEFBUTTON2, MB_ICONQUESTION, MB_ICONWARNING, MB_YESNO, MB_YESNOCANCEL,
    MESSAGEBOX_RESULT, MSG, MessageBoxW, MoveWindow, PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW,
    SW_SHOWNOACTIVATE, SetTimer, SetWindowLongPtrW, ShowWindow, TranslateAcceleratorW, TranslateMessage,
    WINDOW_EX_STYLE, WM_APP, WM_CTLCOLORSTATIC, WM_DESTROY, WM_GETDLGCODE, WM_INITMENUPOPUP, WM_NOTIFY,
    WM_SETTINGCHANGE, WM_TIMER, WM_VSCROLL, WNDCLASSEXW, WS_BORDER, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
    WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use crate::adapter::{
    CharDecoder, Control, MessageContext, Reaction, accelerators, close_guard, message_text, msg, object_labels,
    on_close_box, on_guard_box, on_message, on_open_dialog, on_recovery_box, on_save_dialog, recovery_box_text,
    recovery_dir, selected_rows, window_title, window_title_localized,
};
use crate::appearance::{
    APPS_USE_LIGHT_THEME, Appearance, PERSONALIZE_KEY, appearance_from_apps_use_light_theme, is_theme_change,
};
use crate::forms::toolstrip::{StripAction, strip, tool_intent, variant_intent};
use crate::forms::{
    ControlId, ControlValue, FilePick, FormAux, FormContext, FormKind, FormOutcome, PanelState, Placement,
    cam as cam_forms, cut_settings as cut_forms, g5 as g5_forms, react as react_form, text as text_forms,
};
use crate::tasks::{TaskInbox, task_channel};

mod panels_bridge;

/// Posted by workers to the message window: task outcomes are waiting.
const WM_TASKS_DONE: u32 = WM_APP + 1;
/// Posted after a caught panic: autosave unsaved work into recovery files.
const WM_AUTOSAVE: u32 = WM_APP + 2;
/// Posted by the camera capture thread (at most ten times a second): a new frame is waiting.
const WM_CAMERA_FRAME: u32 = WM_APP + 3;
/// The autosave timer on the message window (every 30 s; only changed documents are written).
const AUTOSAVE_TIMER: usize = 1;
const AUTOSAVE_MS: u32 = 30_000;
/// Width of the left and right columns, in logical pixels.
const LEFT_PX: i32 = 280;
const RIGHT_PX: i32 = 290;
const STATUS_PX: i32 = 44;

struct DocWin {
    // Field order: the surface goes before the windows it draws into.
    canvas_surface: Option<SurfaceCanvas>,
    frame: HWND,
    canvas: HWND,
    list: HWND,
    status: HWND,
    /// The colour of the status line (`COLORREF`), after the tone of its message.
    status_color: u32,
    syncing: bool,
    /// Panels of this window: docked ones in the columns, floating ones as tool windows.
    forms: Vec<FormWin>,
    strip: StripWin,
    /// The frame's tooltip control (strip buttons).
    tip: Option<HWND>,
    /// Window state of the panels that is not in the view model.
    state: PanelState,
    /// The operation whose Cut Settings draft the sheet's controls hold.
    cut_op: Option<OperationId>,
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
    /// Desktop automation (API01); taken out while its pump runs (it dispatches through this host).
    automation: Option<crate::automation_host::Automation>,
    /// The clock the automation pump measures its time limit with.
    started: std::time::Instant,
    /// The system's app theme: the canvas is drawn in `Theme::dark()` or `Theme::light()` after it.
    appearance: Appearance,
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
    with_host(|h| h.controller.view()).unwrap_or_else(|| {
        Arc::new(AppViewModel {
            documents: Vec::new(),
            active: None,
            commands: BTreeMap::new(),
            toolbar: Default::default(),
        })
    })
}

fn dispatch(intent: UiIntent) {
    let Some(update) = with_host(|h| h.controller.handle(intent)) else { return };
    apply(update);
}

/// The host as the automation pump sees it: the same dispatcher the UI uses.
struct AutomationAdapter;

impl crate::automation_host::AutomationApp for AutomationAdapter {
    fn view(&self) -> Arc<AppViewModel> {
        view()
    }

    fn dispatch(&mut self, intent: UiIntent) -> Vec<HostEffect> {
        let Some(update) = with_host(|h| h.controller.handle(intent)) else { return Vec::new() };
        let effects = update.effects.clone();
        apply(update);
        effects
    }
}

/// Moves the current automation job on, or starts the next one (finished tasks and the listener's
/// wake-up land here). The automation is out of the host while its pump runs, so a call from inside
/// (an intent the pump sent triggered an update) does nothing; the outer call sees the result.
fn automation_step() {
    let Some(Some(mut automation)) = with_host(|h| h.automation.take()) else { return };
    let now_ms = with_host(|h| u64::try_from(h.started.elapsed().as_millis()).unwrap_or(u64::MAX)).unwrap_or(0);
    automation.step(&mut AutomationAdapter, now_ms);
    with_host(|h| h.automation = Some(automation));
}

/// The listener runs exactly while the preference is on; the status it answers with follows `vm`.
fn automation_follow(vm: &AppViewModel) {
    with_host(|h| {
        let enabled = h.controller.preferences().desktop_automation;
        if let Some(automation) = h.automation.as_mut() {
            if let Err(e) = automation.sync_preference(enabled) {
                eprintln!("burnburn-win: desktop automation stays off: {e}");
            }
            automation.publish_status(vm);
        }
    });
}

fn react(reaction: Reaction) -> bool {
    react_for(None, reaction)
}

/// Perform a reaction for the window of `document` (the active window when unknown).
fn react_for(document: Option<DocumentId>, reaction: Reaction) -> bool {
    match reaction {
        Reaction::PassOn => false,
        Reaction::Nothing => true,
        Reaction::Intent(intent) => {
            dispatch(intent);
            true
        }
        Reaction::ToggleSchikkenPanel => {
            if let Some(document) = document.or_else(|| view().active) {
                with_host(|h| {
                    if let Some(w) = h.windows.get_mut(&document) {
                        w.state.schikken_shown = !w.state.schikken_shown;
                    }
                });
                refresh();
            }
            true
        }
        Reaction::ToggleG5Panel(panel) => {
            if let Some(document) = document.or_else(|| view().active) {
                with_host(|h| {
                    if let Some(w) = h.windows.get_mut(&document) {
                        match w.state.g5_shown.iter().position(|p| *p == panel) {
                            Some(at) => {
                                w.state.g5_shown.remove(at);
                            }
                            None => w.state.g5_shown.push(panel),
                        }
                    }
                });
                refresh();
            }
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
    with_host(|h| {
        if let Some(automation) = h.automation.as_mut() {
            automation.observe(&update.effects);
        }
    });
    for effect in &update.effects {
        // W4-L8b: RD export, template and bundle dialogs, recent lists.
        crate::win32_panels::manager::effect(effect);
        // The recent lists are stored as soon as the controller announces a change.
        if let Some(dir) = panels_bridge::library_dir()
            && let Some(Err(e)) = crate::persist::persist_effect(&dir, effect)
        {
            eprintln!("burnburn-win: recent list in {}: {e}", dir.display());
        }
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
                let line = crate::status_line::StatusLine::message(*message, system_locale());
                let shown = document.and_then(|d| {
                    with_host(|h| {
                        h.windows.get_mut(&d).map(|w| {
                            set_status_color(w, line.colorref());
                            w.status
                        })
                    })
                    .flatten()
                });
                if let Some(status) = shown {
                    set_text(status, &line.text);
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

/// What the panels of one window are built from, read out of the host.
struct FormInputs {
    document: DocumentId,
    state: PanelState,
    cam: Vec<(CamDialog, DialogViewModel)>,
    /// The values of the Cut Settings controls when they hold a draft of the sheet that is open.
    cut_live: Vec<(ControlId, ControlValue)>,
}

fn form_inputs(vm: &AppViewModel, locale: crate::l10n::Locale) -> Vec<FormInputs> {
    with_host(|h| {
        h.windows
            .iter()
            .map(|(document, w)| {
                let cam = CamDialog::ALL
                    .iter()
                    .filter_map(|d| h.controller.cam_dialog(*document, *d).map(|m| (*d, m)))
                    .collect();
                let ctx = FormContext { vm, document: *document, locale };
                let open = cut_forms::open_operation(&ctx);
                let cut_live = match w.forms.iter().find(|f| f.kind == FormKind::CutSettings) {
                    Some(f) if open.is_some() && open == w.cut_op => f.values(),
                    _ => Vec::new(),
                };
                FormInputs { document: *document, state: w.state.clone(), cam, cut_live }
            })
            .collect()
    })
    .unwrap_or_default()
}

fn refresh() {
    let vm = view();
    let locale = system_locale();
    // The panels first: they are built from the view model and the controls' current values.
    for input in form_inputs(&vm, locale) {
        if vm.document(input.document).is_none() {
            continue;
        }
        let ctx = FormContext { vm: &vm, document: input.document, locale };
        let aux = FormAux { state: &input.state, cam: &input.cam };
        let wanted = crate::forms::forms(&ctx, &aux, &input.cut_live);
        let open = cut_forms::open_operation(&ctx);
        with_host(|h| sync_forms(h, input.document, &wanted, open));
    }
    with_host(|h| {
        for (document, w) in h.windows.iter_mut() {
            let Some(doc) = vm.document(*document) else { continue };
            set_text(w.frame, &window_title_localized(&vm, *document, locale));
            let line = crate::status_line::status_line(doc, locale);
            set_text(w.status, &line.text);
            set_status_color(w, line.colorref());
            w.syncing = true;
            send(w.list, LB_RESETCONTENT, 0, 0);
            for label in object_labels(&vm, *document) {
                let text = wide(&crate::l10n_names::display_name(&label, locale));
                send(w.list, LB_ADDSTRING, 0, text.as_ptr() as isize);
            }
            for row in selected_rows(&vm, *document) {
                send(w.list, LB_SETSEL, 1, row as isize);
            }
            w.syncing = false;
        }
    });
    let frames: Vec<HWND> = with_host(|h| h.windows.values().map(|w| w.frame).collect()).unwrap_or_default();
    for frame in frames {
        layout(frame);
    }
    crate::win32_panels::manager::refresh();
    automation_follow(&vm);
}

/// Create, update and remove the panels of `document` so that they are `wanted`.
fn sync_forms(h: &mut Host, document: DocumentId, wanted: &[crate::forms::Form], open_cut: Option<OperationId>) {
    let instance = h.instance;
    let Some(w) = h.windows.get_mut(&document) else { return };
    // SAFETY: GetDpiForWindow on our frame.
    let scale = f64::from(unsafe { GetDpiForWindow(w.frame) }.max(96)) / 96.0;
    let (gone, kept): (Vec<FormWin>, Vec<FormWin>) =
        std::mem::take(&mut w.forms).into_iter().partition(|f| !wanted.iter().any(|x| x.kind == f.kind));
    w.forms = kept;
    for fw in gone {
        fw.destroy();
    }
    for form in wanted {
        if let Some(fw) = w.forms.iter_mut().find(|f| f.kind == form.kind) {
            fw.sync(form);
        } else if let Some(mut fw) = FormWin::create(instance, w.frame, document, form, scale) {
            if form.placement == Placement::Floating {
                fw.fit_floating();
                // SAFETY: showing our own tool window without taking the focus from the canvas.
                let _ = unsafe { ShowWindow(fw.hwnd, SW_SHOWNOACTIVATE) };
            }
            w.forms.push(fw);
        }
    }
    w.cut_op = open_cut;
}

/// The system's app theme from `HKCU\...\Personalize\AppsUseLightTheme` (light when it is unreadable).
fn system_appearance() -> Appearance {
    let key: Vec<u16> = PERSONALIZE_KEY.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = APPS_USE_LIGHT_THEME.encode_utf16().chain(Some(0)).collect();
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: NUL-terminated key and value names that outlive the call; `data` and `size` are a valid
    // writable DWORD and its byte length, as RRF_RT_REG_DWORD requires.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut data).cast()),
            Some(&mut size),
        )
    };
    appearance_from_apps_use_light_theme((status == ERROR_SUCCESS).then_some(data))
}

/// Stores the status colour and repaints the line when it changed.
fn set_status_color(w: &mut DocWin, colorref: u32) {
    if w.status_color != colorref {
        w.status_color = colorref;
        // SAFETY: repaint request for our own status control.
        let _ = unsafe { InvalidateRect(Some(w.status), None, true) };
    }
}

/// `WM_CTLCOLORSTATIC` for a document's status line: the text in the colour of the message's tone on
/// the dialog background. Any other static control keeps the default.
fn status_paint(wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let control = HWND(lparam.0 as *mut _);
    let colorref = with_host(|h| h.windows.values().find(|w| w.status == control).map(|w| w.status_color)).flatten()?;
    let hdc = HDC(wparam.0 as *mut _);
    // SAFETY: the device context the system passed for this message, valid while it is handled.
    let _ = unsafe { SetTextColor(hdc, COLORREF(colorref)) };
    // SAFETY: a system colour index (no pointers).
    let face = unsafe { GetSysColor(COLOR_BTNFACE) };
    // SAFETY: as above, the device context of this message.
    let _ = unsafe { SetBkColor(hdc, COLORREF(face)) };
    // SAFETY: a shared system brush (not owned, never deleted).
    let brush = unsafe { GetSysColorBrush(COLOR_BTNFACE) };
    Some(LRESULT(brush.0 as isize))
}

/// `WM_SETTINGCHANGE`: reads the theme again when the broadcast says it changed, then redraws.
fn theme_changed(lparam: LPARAM) {
    let setting = if lparam.0 == 0 {
        None
    } else {
        // SAFETY: for WM_SETTINGCHANGE a non-zero lParam is a NUL-terminated UTF-16 string that
        // lives for the duration of the message.
        unsafe { PCWSTR(lparam.0 as *const u16).to_string() }.ok()
    };
    if !is_theme_change(setting.as_deref()) {
        return;
    }
    let appearance = system_appearance();
    let documents = with_host(|h| {
        h.appearance = appearance;
        h.windows.keys().copied().collect::<Vec<_>>()
    })
    .unwrap_or_default();
    for document in documents {
        redraw(document);
    }
}

fn redraw(document: DocumentId) {
    with_host(|h| {
        let Some(scene) = h.controller.scene(document) else { return };
        let theme = if h.appearance == Appearance::Dark { Theme::dark() } else { Theme::light() };
        let packet = build_packet(&scene, &theme);
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
    let vm = view();
    let built = build_menu(&vm, system_locale());
    let menu = built.as_ref().map(|b| b.bar);
    let Some(instance) = with_host(|h| h.instance) else { return };
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
    let (Ok(list), Ok(canvas), Ok(status)) = (list, canvas, status) else { return };
    // The document id on every child too, so accelerators work wherever the focus is.
    for hwnd in [list, canvas, status] {
        // SAFETY: user data on our own child windows (unused by the system classes).
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, document.raw() as isize) };
    }
    let tip = tips::create(frame, instance);
    if let Some(tip) = tip {
        // Disabled strip buttons get no mouse input: the area tool asks the frame (WM_NOTIFY).
        tips::set_area_tip(tip, frame, true);
    }
    let canvas_surface = surface_for(instance, canvas);
    with_host(|h| {
        h.windows.insert(
            document,
            DocWin {
                canvas_surface,
                frame,
                canvas,
                list,
                status,
                status_color: crate::status_line::StatusLine::toned("", crate::style::StatusTone::Neutral).colorref(),
                syncing: false,
                forms: Vec::new(),
                strip: StripWin::new(),
                tip,
                state: PanelState::default(),
                cut_op: None,
            },
        );
    });
    with_host(|h| {
        if let Some(w) = h.windows.get_mut(&document) {
            w.strip.set_tip(w.tip);
        }
    });
    if let Some(built) = &built {
        crate::win32_panels::manager::register_recent_menus(frame, built.recent_projects, built.recent_templates);
    }
    // SAFETY: showing our own frame.
    let _ = unsafe { ShowWindow(frame, SW_SHOW) };
    refresh();
    redraw(document);
}

/// Place the strip, the object list, the canvas, the docked panels and the status line.
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
    let (left, right, status_h) = (px(LEFT_PX), px(RIGHT_PX), px(STATUS_PX));
    let panels = strip(&view(), system_locale());
    let place = |hwnd: HWND, x: i32, y: i32, width: i32, height: i32| {
        // SAFETY: one of our child windows on the UI thread; repaint requested.
        let _ = unsafe { MoveWindow(hwnd, x, y, width.max(1), height.max(1), true) };
    };
    with_host(|host| {
        let instance = host.instance;
        let Some(d) = host.windows.get_mut(&document) else { return };
        let top = d.strip.sync(instance, frame, &panels, w, scale);
        if let Some(tip) = d.tip {
            tips::set_area_tip(tip, frame, false);
        }
        let body = (h - top).max(1);
        let list_h = body * 35 / 100;
        place(d.list, 0, top, left, list_h);
        place(d.canvas, left, top, (w - left - right).max(1), body);
        place(d.status, w - right + px(8), h - status_h, right - px(16), status_h - px(4));
        for form in &mut d.forms {
            match form.placement {
                Placement::DockLeft => form.dock(0, top + list_h, left, body - list_h),
                Placement::DockRight => form.dock(w - right, top, right, body - status_h),
                Placement::Floating => {}
            }
        }
    });
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
        // W4-L8b: the menu ids 6000..=7999 belong to the panel families.
        if message == msg::WM_COMMAND
            && ((wparam.0 >> 16) & 0xFFFF) <= 1
            && crate::host_panels::ids::RANGE.contains(&((wparam.0 & 0xFFFF) as u16))
            && crate::win32_panels::manager::handle_menu((wparam.0 & 0xFFFF) as u16)
        {
            return LRESULT(0);
        }
        match message {
            msg::WM_SIZE => {
                layout(hwnd);
                return LRESULT(0);
            }
            // The status line is a STATIC control: its parent paints its text in the tone's colour.
            WM_CTLCOLORSTATIC => {
                if let Some(brush) = status_paint(wparam, lparam) {
                    return brush;
                }
            }
            // The user switched between light and dark: the canvas follows (the broadcast reaches
            // every top-level window; the first one updates the host).
            WM_SETTINGCHANGE => theme_changed(lparam),
            // Closing the last window quits: while the machine is busy that needs a confirmation
            // (X07 update gate, same reason and words as the macOS alert).
            msg::WM_CLOSE if !quit_allowed(hwnd) => return LRESULT(0),
            msg::WM_MOUSEMOVE => {
                // A disabled strip button gets no mouse input: say why from here (then go on).
                let (x, y) = ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
                let hovered =
                    with_host(|h| h.windows.get(&document).map(|w| (w.tip, w.strip.disabled_tip_at((x, y))))).flatten();
                if let Some((tip, reason)) = hovered {
                    tips::hover(tip, hwnd, reason.as_deref());
                }
            }
            WM_NOTIFY if tips::answer(hwnd, lparam) => return LRESULT(0),
            WM_INITMENUPOPUP => {
                refresh_popup(HMENU(wparam.0 as *mut _), &view(), system_locale());
                crate::win32_panels::manager::refresh_menu_state(HMENU(wparam.0 as *mut _));
                return LRESULT(0);
            }
            WM_DESTROY => {
                tips::forget(hwnd);
                crate::win32_panels::manager::forget_recent_menus(hwnd);
                let remaining = with_host(|h| {
                    if let Some(mut w) = h.windows.remove(&document) {
                        w.canvas_surface = None;
                    }
                    h.windows.len()
                });
                if remaining == Some(0) {
                    // The application quits: the automation listener ends before the process does.
                    with_host(|h| {
                        if let Some(automation) = h.automation.as_mut() {
                            automation.shutdown();
                        }
                    });
                    // SAFETY: ends our message loop.
                    unsafe { PostQuitMessage(0) };
                }
                return LRESULT(0);
            }
            _ => {}
        }
        // A tool-strip button: its child id is in the strip's range (lParam is its HWND).
        let id = wparam.0 & 0xFFFF;
        if message == msg::WM_COMMAND && lparam.0 != 0 && (STRIP_BASE..STRIP_BASE + 1000).contains(&id) {
            strip_clicked(document, id);
            return LRESULT(0);
        }
        // Which control a WM_COMMAND notification came from (lParam is its HWND).
        let source = HWND(lparam.0 as *mut _);
        let parts = with_host(|h| h.windows.get(&document).map(|w| (w.list, w.syncing))).flatten();
        let Some((list, syncing)) = parts else { return default() };
        let control = if message == msg::WM_COMMAND && source == list {
            if syncing {
                return LRESULT(0);
            }
            Control::ObjectList
        } else {
            Control::Frame
        };
        let rows = if control == Control::ObjectList { list_selection(list) } else { Vec::new() };
        let ctx = context_for(document, control, hwnd, "", &rows);
        if react_for(Some(document), on_message(&ctx, message, wparam.0, lparam.0, &view())) {
            LRESULT(0)
        } else {
            default()
        }
    })
}

extern "system" fn canvas_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: default handling of a message for our own window.
    let default = || unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    guard("canvas_proc", LRESULT(0), || {
        let Some(document) = document_of(hwnd) else { return default() };
        match message {
            // The dialog manager (Tab) asks which keys the canvas keeps: all but Tab.
            WM_GETDLGCODE => {
                // SAFETY: for WM_GETDLGCODE `lparam` is null or points to the MSG being processed.
                let pressed = unsafe { (lparam.0 as *const MSG).as_ref() }
                    .filter(|m| m.message == msg::WM_KEYDOWN)
                    .map(|m| (m.wParam.0 & 0xFFFF) as u16);
                let base = default();
                return match pressed {
                    Some(vk) if crate::host_panels::keys::canvas_wants_key(vk) => {
                        LRESULT(base.0 | DLGC_WANTMESSAGE as isize)
                    }
                    _ => base,
                };
            }
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
        if react_for(Some(document), on_message(&ctx, message, wparam.0, lparam.0, &view())) {
            LRESULT(0)
        } else {
            default()
        }
    })
}

/// A tool-strip button was clicked: a tool is chosen, a drop-down offers the variants of its group.
fn strip_clicked(document: DocumentId, id: usize) {
    let found =
        with_host(|h| h.windows.get(&document).map(|w| (w.strip.action(id).cloned(), w.strip.button(id), w.frame)))
            .flatten();
    let Some((Some(action), button, frame)) = found else { return };
    let vm = view();
    match action {
        StripAction::Tool(tool) => {
            if let Some(intent) = tool_intent(&vm, document, &tool) {
                dispatch(intent);
            }
        }
        StripAction::Dropdown { group } => {
            let panels = strip(&vm, system_locale());
            let variants = panels
                .iter()
                .flat_map(|p| &p.groups)
                .find(|g| g.group.as_deref() == Some(group.as_str()))
                .map(|g| g.variants.clone())
                .unwrap_or_default();
            let chosen = button.and_then(|b| choose_variant(frame, b, &variants));
            if let Some(intent) = chosen.and_then(|id| variant_intent(&vm, document, &group, &id)) {
                dispatch(intent);
            }
        }
    }
    refresh();
}

/// Everything `forms::react` needs, copied out of the host so that no borrow is held meanwhile.
struct FormEvent {
    document: DocumentId,
    kind: FormKind,
    values: Vec<(ControlId, ControlValue)>,
    state: PanelState,
    cam: Vec<(CamDialog, DialogViewModel)>,
    frame: HWND,
}

fn form_event(hwnd: HWND) -> Option<FormEvent> {
    with_host(|h| {
        h.windows.iter().find_map(|(document, w)| {
            let f = w.forms.iter().find(|f| f.hwnd == hwnd)?;
            let cam =
                CamDialog::ALL.iter().filter_map(|d| h.controller.cam_dialog(*document, *d).map(|m| (*d, m))).collect();
            Some(FormEvent {
                document: *document,
                kind: f.kind,
                values: f.values(),
                state: w.state.clone(),
                cam,
                frame: w.frame,
            })
        })
    })
    .flatten()
}

/// A control of a panel reported `value`: ask the form what it means and do it.
fn form_control(hwnd: HWND, id: &ControlId, value: &ControlValue) {
    let Some(ev) = form_event(hwnd) else { return };
    let vm = view();
    let ctx = FormContext { vm: &vm, document: ev.document, locale: system_locale() };
    let aux = FormAux { state: &ev.state, cam: &ev.cam };
    let outcome = react_form(ev.kind, id, value, &ev.values, &ctx, &aux);
    perform(outcome, &ev, &ctx, &aux);
    refresh();
}

fn perform(outcome: FormOutcome, ev: &FormEvent, ctx: &FormContext<'_>, aux: &FormAux<'_>) {
    match outcome {
        FormOutcome::Nothing | FormOutcome::Redraw => {}
        FormOutcome::Intent(intent) => dispatch(intent),
        FormOutcome::SelectLayerRow(row) => {
            with_host(|h| {
                if let Some(w) = h.windows.get_mut(&ev.document) {
                    w.state.layer_row = row;
                }
            });
        }
        FormOutcome::PickFile(FilePick::CamExportRd(dialog)) => {
            let path = save_dialog_of(ev.frame, "test.rd", FileKind::Rd);
            if let Some(path) = path {
                let model = aux.cam.iter().find(|(d, _)| *d == dialog).map(|(_, m)| m);
                if let FormOutcome::Intent(intent) = cam_forms::file_chosen(dialog, model, path, ctx) {
                    dispatch(intent);
                }
            }
        }
        FormOutcome::PickFile(FilePick::TraceBatch) => {
            let paths = open_many_dialog_of(FileKind::Image);
            if let FormOutcome::Intent(intent) = g5_forms::batch_files_chosen(paths, ctx) {
                dispatch(intent);
            }
        }
        FormOutcome::PickFile(FilePick::Csv) => {
            let file = open_dialog_of(FileKind::Csv).and_then(|path| {
                let bytes = std::fs::read(&path).ok()?;
                let name = path.file_name()?.to_string_lossy().into_owned();
                Some((name, bytes))
            });
            if let FormOutcome::Intent(intent) = text_forms::file_chosen(file, ctx) {
                dispatch(intent);
            }
        }
    }
}

/// Closing a floating panel: the dialogs cancel, the Schikken panel hides, the others stay.
fn form_closed(hwnd: HWND) {
    let Some(ev) = form_event(hwnd) else { return };
    if ev.kind == FormKind::Schikken {
        with_host(|h| {
            if let Some(w) = h.windows.get_mut(&ev.document) {
                w.state.schikken_shown = false;
            }
        });
        refresh();
        return;
    }
    // Offset and repair and the structure panel wait for their menu item: the close box asks it away.
    if let FormKind::G5(panel) = ev.kind
        && !crate::panels::g5_opens_by_itself(panel)
    {
        with_host(|h| {
            if let Some(w) = h.windows.get_mut(&ev.document) {
                w.state.g5_shown.retain(|p| *p != panel);
            }
        });
        refresh();
        return;
    }
    if let Some(id) = close_command(ev.kind) {
        form_control(hwnd, &id, &ControlValue::Press);
    }
}

extern "system" fn form_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: default handling of a message for our own window.
    let default = || unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    guard("form_proc", LRESULT(0), || match message {
        msg::WM_MOUSEMOVE => {
            // A disabled field gets no mouse input: say why from here.
            let (x, y) = ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
            let hovered = with_host(|h| {
                h.windows
                    .values()
                    .find_map(|w| w.forms.iter().find(|f| f.hwnd == hwnd).map(|f| (f.tip(), f.reason_at((x, y)))))
            })
            .flatten();
            if let Some((tip, reason)) = hovered {
                tips::hover(tip, hwnd, reason.as_deref());
            }
            default()
        }
        WM_NOTIFY if tips::answer(hwnd, lparam) => LRESULT(0),
        msg::WM_COMMAND => {
            let (id, code) = (wparam.0 & 0xFFFF, ((wparam.0 >> 16) & 0xFFFF) as u16);
            let event = with_host(|h| {
                h.windows.values().find_map(|w| w.forms.iter().find(|f| f.hwnd == hwnd).map(|f| f.event(id, code)))
            })
            .flatten()
            .flatten();
            if let Some((control, value)) = event {
                form_control(hwnd, &control, &value);
            }
            LRESULT(0)
        }
        WM_VSCROLL => {
            let code = (wparam.0 & 0xFFFF) as u16;
            with_host(|h| {
                for w in h.windows.values_mut() {
                    if let Some(f) = w.forms.iter_mut().find(|f| f.hwnd == hwnd) {
                        let track = f.track_pos();
                        f.scroll_request(code, track);
                    }
                }
            });
            LRESULT(0)
        }
        msg::WM_MOUSEWHEEL => {
            let notches = i32::from(((wparam.0 >> 16) & 0xFFFF) as u16 as i16) / 120;
            with_host(|h| {
                for w in h.windows.values_mut() {
                    if let Some(f) = w.forms.iter_mut().find(|f| f.hwnd == hwnd) {
                        f.scroll_request(if notches > 0 { 0 } else { 1 }, 0);
                        for _ in 1..notches.abs() {
                            f.scroll_request(if notches > 0 { 0 } else { 1 }, 0);
                        }
                    }
                }
            });
            LRESULT(0)
        }
        msg::WM_SIZE => {
            with_host(|h| {
                for w in h.windows.values_mut() {
                    if let Some(f) = w.forms.iter_mut().find(|f| f.hwnd == hwnd) {
                        f.relayout();
                    }
                }
            });
            default()
        }
        msg::WM_CLOSE => {
            form_closed(hwnd);
            LRESULT(0)
        }
        _ => default(),
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
            // The automation listener's wake-up lands here too: requests start, finished tasks end them.
            automation_step();
            return LRESULT(0);
        }
        if message == WM_CAMERA_FRAME {
            // The camera panel redraws from the latest frame (every panel follows the controller).
            crate::win32_panels::manager::refresh();
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

/// Resource id of the application icon group that `build.rs` embeds (`APP_ICON_ID` in
/// `build/ico_res.rs`; the lowest id, so Explorer shows it too).
const APP_ICON_RESOURCE_ID: usize = 1;

/// The embedded application icon (title bar, taskbar, Alt+Tab), or none when the executable has no
/// such resource (the system then shows its default icon).
fn app_icon(instance: HINSTANCE) -> windows::Win32::UI::WindowsAndMessaging::HICON {
    // SAFETY: loads the icon group of this executable by its integer resource id; the handle is shared
    // and owned by the module (never destroyed).
    unsafe { LoadIconW(Some(instance), PCWSTR(APP_ICON_RESOURCE_ID as *const u16)) }.unwrap_or_default()
}

fn register(instance: HINSTANCE, name: PCWSTR, proc_: windows::Win32::UI::WindowsAndMessaging::WNDPROC) -> bool {
    // SAFETY: loads the shared system arrow cursor (no ownership).
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
    let icon = app_icon(instance);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: proc_,
        hInstance: instance,
        hIcon: icon,
        hCursor: cursor,
        lpszClassName: name,
        ..Default::default()
    };
    // SAFETY: a fully initialised class description with a static name and our window procedure.
    unsafe { RegisterClassExW(&class) != 0 }
}

/// The panel container class: like the others, with the dialog background colour.
fn register_form(instance: HINSTANCE) -> bool {
    // SAFETY: loads the shared system arrow cursor (no ownership).
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
    // SAFETY: a shared system brush (not owned, never deleted).
    let background = unsafe { GetSysColorBrush(COLOR_BTNFACE) };
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(form_proc),
        hInstance: instance,
        hCursor: cursor,
        hbrBackground: background,
        lpszClassName: FORM_CLASS,
        ..Default::default()
    };
    // SAFETY: a fully initialised class description with a static name and our window procedure.
    unsafe { RegisterClassExW(&class) != 0 }
}

/// `burnburn-win`: one document window, the message loop, quit when the last window closes.
/// `args`: the command-line arguments after the program name (a project path to open).
pub fn run_app(args: Vec<std::ffi::OsString>) -> ExitCode {
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
        && register(instance, w!("BurnBurnMessages"), Some(message_proc))
        && register_form(instance))
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
    // The automation listener wakes the UI thread the way a finished task does.
    let automation =
        crate::automation_host::Automation::new(Arc::new(burnburn_io::std_fs::StdFileSystem), Arc::clone(&wake));
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
            if a.alt {
                f |= FALT;
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
                automation: Some(automation),
                started: std::time::Instant::now(),
                appearance: system_appearance(),
            });
        }
    });
    panels_bridge::install();
    // The stored recent lists first (the menus of the first window show them), then the project
    // named on the command line, or a blank document.
    if let Some(dir) = panels_bridge::library_dir() {
        for intent in crate::persist::load_intents(&dir) {
            dispatch(intent);
        }
    }
    for intent in crate::startup::startup_intents(args) {
        dispatch(intent);
    }
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
        // Tab, Shift+Tab, Return on a button and Escape in the windows with controls: the dialog
        // manager of the window the message belongs to (controls ask for the keys they keep).
        if matches!(message.message, msg::WM_KEYDOWN | 0x0101 /* WM_KEYUP */ | msg::WM_CHAR)
            && let Some(root) = dialog_root(message.hwnd)
            // SAFETY: our own top-level window and the message just read.
            && unsafe { IsDialogMessageW(root, &message) }.as_bool()
        {
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

/// The top-level window of `hwnd` when it is one of ours that holds controls (a document frame, a
/// floating form or a panel window): the window to run the dialog manager for.
fn dialog_root(hwnd: HWND) -> Option<HWND> {
    // SAFETY: plain query on a window handle from the message just read (null gives null).
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.0.is_null() {
        return None;
    }
    let mut name = [0u16; 32];
    // SAFETY: a writable buffer of the stated length.
    let len = usize::try_from(unsafe { GetClassNameW(root, &mut name) }).ok()?;
    let class = String::from_utf16_lossy(&name[..len.min(name.len())]);
    matches!(class.as_str(), "BurnBurnFrame" | "BurnBurnForm" | "BurnBurnPanel").then_some(root)
}

/// The UI language from the user's default locale name (`nl-BE`, `en-US`, …), as the macOS host
/// follows `NSLocale.preferredLanguages`.
#[allow(unsafe_code)]
/// Asks before the last window closes while the machine is busy; `true` when closing may go on.
fn quit_allowed(frame: HWND) -> bool {
    let reason = with_host(|h| close_guard(&h.controller.machine_activity(), h.windows.len())).flatten();
    let Some(reason) = reason else { return true };
    let locale = system_locale();
    let (title, info) = crate::quit_guard::guard_message(reason, locale);
    let question = match locale {
        crate::l10n::Locale::Nl => "Toch afsluiten?",
        crate::l10n::Locale::En => "Quit anyway?",
    };
    let text = HSTRING::from(format!("{title}\n\n{info}\n\n{question}"));
    // SAFETY: modal message box owned by our frame; the strings outlive the call. No is the default.
    let result: MESSAGEBOX_RESULT =
        unsafe { MessageBoxW(Some(frame), &text, w!("BurnBurn"), MB_YESNO | MB_DEFBUTTON2 | MB_ICONWARNING) };
    on_guard_box(result.0) == crate::quit_guard::GuardChoice::QuitAnyway
}

fn system_locale() -> crate::l10n::Locale {
    let mut buffer = [0u16; 85];
    // SAFETY: a writable buffer of LOCALE_NAME_MAX_LENGTH (85) UTF-16 units, as the API requires.
    let len = unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut buffer) };
    let len = usize::try_from(len).unwrap_or(0).saturating_sub(1).min(buffer.len());
    let name = String::from_utf16_lossy(&buffer[..len]);
    crate::l10n::locale_from_preferred([name.as_str()])
}
