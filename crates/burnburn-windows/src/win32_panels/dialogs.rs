//! Native dialogs for the panel families (W4-L8b): `IFileOpenDialog`, `IFileSaveDialog` and the
//! confirmation box. Type-checked from macOS only; never run on Windows (see the dev note).
//!
//! The COM apartment is the UI thread's (`CoInitializeEx` in `run_app`). Strings handed to the
//! shell are `HSTRING`s that outlive the call.

#![allow(unsafe_code)]

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_DEFBUTTON2, MB_ICONWARNING, MB_OKCANCEL, MESSAGEBOX_RESULT, MessageBoxW,
};
use windows::core::{HSTRING, PCWSTR};

use crate::host_panels::FileFilter;

/// `*.png;*.jpg` for a filter's extensions.
pub fn filter_pattern(filter: &FileFilter) -> String {
    filter.extensions.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";")
}

/// The filter list plus the `All files` fallback the shell dialogs conventionally end with.
fn filter_strings(filters: &[FileFilter]) -> Vec<(HSTRING, HSTRING)> {
    filters
        .iter()
        .map(|f| (HSTRING::from(f.name.as_str()), HSTRING::from(filter_pattern(f))))
        .chain(std::iter::once((HSTRING::from("Alle bestanden / All files"), HSTRING::from("*.*"))))
        .collect()
}

fn apply_filters(dialog: &IFileDialog, strings: &[(HSTRING, HSTRING)]) -> Option<()> {
    let specs: Vec<COMDLG_FILTERSPEC> = strings
        .iter()
        .map(|(name, pattern)| COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(pattern.as_ptr()) })
        .collect();
    // SAFETY: COM call on the UI thread; the specs point into `strings`, which outlives the call.
    unsafe { dialog.SetFileTypes(&specs) }.ok()
}

fn chosen_path(dialog: &IFileDialog) -> Option<PathBuf> {
    // SAFETY: COM call on the UI thread, after `Show` returned OK.
    let item = unsafe { dialog.GetResult() }.ok()?;
    // SAFETY: as above, on the returned shell item.
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
    // SAFETY: `name` is a valid NUL-terminated string owned by us until freed below.
    let path = unsafe { name.to_string() }.ok().map(PathBuf::from);
    // SAFETY: freeing the CoTaskMem string GetDisplayName allocated, exactly once.
    unsafe { CoTaskMemFree(Some(name.0 as *const _)) };
    path
}

/// A file to open; `None` when the user cancelled or the dialog could not be shown.
pub fn pick_open(owner: Option<HWND>, title: &str, filters: &[FileFilter]) -> Option<PathBuf> {
    // SAFETY: COM on the STA UI thread (CoInitializeEx in run_app); released on drop.
    let dialog: IFileOpenDialog = unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let strings = filter_strings(filters);
    apply_filters(&dialog.clone().into(), &strings)?;
    let caption = HSTRING::from(title);
    // SAFETY: the string outlives the call.
    unsafe { dialog.SetTitle(&caption) }.ok()?;
    // SAFETY: modal dialog on the UI thread; Cancel returns an error (-> None).
    unsafe { dialog.Show(owner) }.ok()?;
    chosen_path(&dialog.into())
}

/// A file to write; the first filter's first extension is the default extension.
pub fn pick_save(owner: Option<HWND>, title: &str, suggested: &str, filters: &[FileFilter]) -> Option<PathBuf> {
    // SAFETY: COM on the STA UI thread (CoInitializeEx in run_app); released on drop.
    let dialog: IFileSaveDialog = unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let strings = filter_strings(filters);
    apply_filters(&dialog.clone().into(), &strings)?;
    if let Some(ext) = filters.first().and_then(|f| f.extensions.first()) {
        let ext = HSTRING::from(*ext);
        // SAFETY: the string outlives the call.
        unsafe { dialog.SetDefaultExtension(&ext) }.ok()?;
    }
    let caption = HSTRING::from(title);
    let name = HSTRING::from(suggested);
    // SAFETY: the strings outlive the calls.
    unsafe { dialog.SetTitle(&caption) }.ok()?;
    // SAFETY: as above.
    unsafe { dialog.SetFileName(&name) }.ok()?;
    // SAFETY: modal dialog on the UI thread; Cancel returns an error (-> None).
    unsafe { dialog.Show(owner) }.ok()?;
    chosen_path(&dialog.into())
}

/// A question with a warning icon: `true` only for OK. The button captions are the system's (a
/// task dialog with the typed captions is not built); the body says what OK does.
pub fn confirm(owner: Option<HWND>, title: &str, body: &str, confirm: &str) -> bool {
    let text = HSTRING::from(format!("{body}\n\nOK: {confirm}"));
    let caption = HSTRING::from(title);
    // SAFETY: modal message box on the UI thread; strings outlive the call. Cancel is the default.
    let result: MESSAGEBOX_RESULT =
        unsafe { MessageBoxW(owner, &text, &caption, MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2) };
    result == IDOK
}

/// A plain message (About).
pub fn message(owner: Option<HWND>, title: &str, body: &str) {
    let text = HSTRING::from(body);
    let caption = HSTRING::from(title);
    // SAFETY: modal message box on the UI thread; strings outlive the call.
    let _ = unsafe { MessageBoxW(owner, &text, &caption, windows::Win32::UI::WindowsAndMessaging::MB_OK) };
}
