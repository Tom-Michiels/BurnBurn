//! File dialogs (`IFileOpenDialog` / `IFileSaveDialog`) on the STA UI thread.

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH};
use windows::core::{HSTRING, w};

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

pub(super) fn open_dialog() -> Option<PathBuf> {
    // SAFETY: COM on the STA UI thread (CoInitializeEx in run_app); released on drop.
    let dialog: IFileOpenDialog = unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let filter = file_filter();
    // SAFETY: the filter specs point to static strings and live for the call.
    unsafe { dialog.SetFileTypes(&filter) }.ok()?;
    // SAFETY: modal dialog on the UI thread; Cancel returns an error (→ None).
    unsafe { dialog.Show(None) }.ok()?;
    dialog_path(&dialog)
}

pub(super) fn save_dialog(owner: HWND, suggested: &str) -> Option<PathBuf> {
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
