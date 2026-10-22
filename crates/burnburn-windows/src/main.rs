//! `burnburn-win`: the Windows host (IM08 proof). Unconfirmed until it ran on a Windows machine.

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    // `burnburn-win.exe "<pad>"` (the MSIX file association) opens that project.
    burnburn_windows::win32::run_app(std::env::args_os().skip(1).collect())
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("burnburn-win: this host runs on Windows only; the macOS host is burnburn-macos");
    ExitCode::FAILURE
}
