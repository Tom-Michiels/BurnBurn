//! `cargo xtask path-attrs`: a crate may not compile another crate's source files through
//! `#[path = "../../other-crate/src/x.rs"]`. Shared code lives in a crate both sides depend on
//! (ADR 0009; it replaced the interim `#[path]` sharing of the macOS and Windows hosts).
//!
//! A `#[path]` that stays inside the crate's own directory is fine.

use std::path::{Component, Path, PathBuf};

/// Directories of a crate that are scanned: production sources and the integration tests.
pub const SCANNED_DIRS: &[&str] = &["src", "tests"];

/// Scans `subdirs` of `<root>/<crate_dir>` (those that exist) and returns the violations and the number of
/// files read. `crate_dir` is relative to the workspace root, with `/` separators.
pub fn check_tree(root: &Path, crate_dir: &str, subdirs: &[&str]) -> Result<(Vec<String>, usize), String> {
    let mut violations = Vec::new();
    let mut files = 0;
    for sub in subdirs {
        let dir = root.join(crate_dir).join(sub);
        if !dir.is_dir() {
            continue;
        }
        let mut sources = Vec::new();
        collect_rs(&dir, &mut sources)?;
        for file in sources {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let rel = file.strip_prefix(root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
            files += 1;
            violations.extend(check_source(crate_dir, &rel, &text));
        }
    }
    Ok((violations, files))
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target" && n != ".git") {
                collect_rs(&path, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Violations in one source file, as `<file>:<line>: #[path = "..."] leaves <crate dir>` lines.
/// `crate_dir` and `file` are relative to the workspace root, with `/` separators.
pub fn check_source(crate_dir: &str, file: &str, text: &str) -> Vec<String> {
    let dir = Path::new(file).parent().unwrap_or_else(|| Path::new(""));
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let Some(target) = path_attribute(line) else { continue };
        let resolved = normalize(&dir.join(target));
        if !resolved.starts_with(crate_dir) {
            out.push(format!(
                "{file}:{}: #[path = \"{target}\"] leaves {crate_dir} (-> {})",
                i + 1,
                resolved.display()
            ));
        }
    }
    out
}

/// The string of `#[path = "..."]` when the line starts with that attribute.
fn path_attribute(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("#[")?.trim_start().strip_prefix("path")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start().strip_prefix('"')?;
    rest.split_once('"').map(|(target, _)| target)
}

/// Lexically normalised path (resolves `.` and `..` without touching the filesystem).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: &str = "crates/burnburn-windows";

    #[test]
    fn a_path_into_another_crate_is_reported() {
        let text = "pub mod a;\n#[path = \"../../burnburn-macos/src/l10n.rs\"]\npub mod l10n;\n";
        let v = check_source(WIN, "crates/burnburn-windows/src/lib.rs", text);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].starts_with("crates/burnburn-windows/src/lib.rs:2:"), "{v:?}");
        assert!(v[0].contains("burnburn-macos/src/l10n.rs"), "{v:?}");
    }

    #[test]
    fn a_path_inside_the_own_crate_is_fine() {
        let text = "#[path = \"l10n_schikken.rs\"]\nmod l10n_schikken;\n#[path = \"../tests/helper.rs\"]\nmod h;\n";
        assert_eq!(check_source(WIN, "crates/burnburn-windows/src/l10n.rs", text), Vec::<String>::new());
    }

    #[test]
    fn the_path_is_resolved_from_the_directory_of_the_file() {
        // From src/panels/mod.rs, `../../..` leaves the crate; `../x.rs` stays inside.
        let out = "#[path = \"../../../burnburn-macos/src/x.rs\"]\nmod x;\n";
        assert_eq!(check_source(WIN, "crates/burnburn-windows/src/panels/mod.rs", out).len(), 1);
        let inside = "#[path = \"../x.rs\"]\nmod x;\n";
        assert!(check_source(WIN, "crates/burnburn-windows/src/panels/mod.rs", inside).is_empty());
    }

    #[test]
    fn spacing_variants_and_comments_are_handled() {
        let text = "// #[path = \"../../other/src/a.rs\"]\n#[path=\"../../other/src/b.rs\"]\nmod b;\n  #[ path = \"../../other/src/c.rs\" ]\nmod c;\n";
        let v = check_source(WIN, "crates/burnburn-windows/src/lib.rs", text);
        assert_eq!(v.len(), 2, "the commented line is not an attribute: {v:?}");
        assert!(v[0].contains(":2:") && v[1].contains(":4:"), "{v:?}");
    }

    #[test]
    fn a_path_that_climbs_to_the_workspace_root_is_reported() {
        let text = "#[path = \"../../../../tests/shared.rs\"]\nmod s;\n";
        assert_eq!(check_source(WIN, "crates/burnburn-windows/src/lib.rs", text).len(), 1);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("burnburn-path-attrs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_path_in_the_tests_directory_that_leaves_the_crate_is_reported() {
        let root = scratch("tests-dir");
        write(&root, "crates/a/src/lib.rs", "pub mod x;\n");
        write(&root, "crates/a/tests/t.rs", "#[path = \"../../b/tests/common/mod.rs\"]\nmod common;\n");
        let (v, files) = check_tree(&root, "crates/a", SCANNED_DIRS).unwrap();
        assert_eq!(files, 2);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].starts_with("crates/a/tests/t.rs:1:"), "{v:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_path_inside_the_tests_directory_of_the_own_crate_is_fine_and_missing_directories_are_skipped() {
        let root = scratch("tests-own");
        write(&root, "crates/a/tests/t.rs", "#[path = \"common/camera.rs\"]\nmod camera;\n");
        let (v, files) = check_tree(&root, "crates/a", SCANNED_DIRS).unwrap();
        assert_eq!((v.len(), files), (0, 1), "{v:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
