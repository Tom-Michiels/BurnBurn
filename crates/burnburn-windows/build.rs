//! Embeds the application icon in the Windows executable.
//!
//! `res/burnburn.ico` becomes a compiled resource file (`ico_res`, pure Rust, no dependencies) in
//! `OUT_DIR`; for an MSVC target the file is passed to the linker for the binaries, which turns it
//! into the `.rsrc` section (Explorer, taskbar and the window class use icon id 1). On other targets
//! the file is still written (the headless test reads it) and nothing is linked.

use std::path::PathBuf;
use std::process::ExitCode;

#[path = "build/ico_res.rs"]
mod ico_res;

const ICON: &str = "res/burnburn.ico";

fn main() -> ExitCode {
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed=build/ico_res.rs");
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("burnburn-windows build script: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let out_dir = std::env::var_os("OUT_DIR").map(PathBuf::from).ok_or("OUT_DIR is not set")?;
    let ico = std::fs::read(ICON).map_err(|e| format!("reading {ICON}: {e}"))?;
    let res = ico_res::icon_resource(&ico).map_err(|e| format!("{ICON}: {e}"))?;
    let path = out_dir.join("burnburn-icon.res");
    std::fs::write(&path, res).map_err(|e| format!("writing {}: {e}", path.display()))?;
    println!("cargo:rustc-env=BURNBURN_ICON_RES={}", path.display());
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg-bins={}", path.display());
    }
    Ok(())
}
