//! `burnburn`: the macOS application (no app bundle yet; `cargo run -p burnburn-macos`).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    burnburn_macos::appkit::run_app()
}

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    eprintln!("burnburn: this host runs on macOS only; the Windows host is burnburn-windows (IM08)");
    ExitCode::FAILURE
}
