//! Embeds `assets/icons/ui/{svg/*.svg, icons.toml, palette.toml}` into `$OUT_DIR/icon_data.rs`.
//! New files in the folder are picked up without a code change. Missing assets embed as empty
//! (the registry is then empty); the tests assert that the registry matches the folder.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(e) = run() {
        eprintln!("burnburn-icons build.rs: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    let root = Path::new(&manifest).join("../../assets/icons/ui");
    // The folder itself (added/removed files) and every file in it.
    println!("cargo:rerun-if-changed={}", root.display());
    println!("cargo:rerun-if-changed={}", root.join("svg").display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut out = String::new();
    for (name, file) in [("PALETTE_TOML", "palette.toml"), ("ICONS_TOML", "icons.toml")] {
        let path = root.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(out, "pub(crate) const {name}: &str = {};", include_expr(&path)?);
    }

    let mut svgs: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join("svg")) {
        for entry in rd.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "svg")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                println!("cargo:rerun-if-changed={}", path.display());
                svgs.push((stem.to_owned(), path));
            }
        }
    }
    svgs.sort();
    let items: Vec<String> = svgs
        .iter()
        .map(|(stem, path)| include_expr(path).map(|e| format!("({stem:?}, {e})")))
        .collect::<Result<_, _>>()?;
    let _ = writeln!(out, "pub(crate) static SVGS: &[(&str, &str)] = &[{}];", items.join(", "));

    let dest = Path::new(&std::env::var("OUT_DIR").map_err(|e| e.to_string())?).join("icon_data.rs");
    std::fs::write(dest, out).map_err(|e| e.to_string())
}

/// `include_str!("<abs path>")`, or `""` when the file does not exist (yet).
fn include_expr(path: &Path) -> Result<String, String> {
    match path.canonicalize() {
        Ok(abs) => Ok(format!("include_str!({:?})", abs.display().to_string())),
        Err(_) => Ok("\"\"".to_owned()),
    }
}
