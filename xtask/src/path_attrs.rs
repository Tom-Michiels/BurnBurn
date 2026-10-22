//! `cargo xtask path-attrs`: a crate may not compile another crate's source files through
//! `#[path = "../../other-crate/src/x.rs"]`. Shared code lives in a crate both sides depend on
//! (ADR 0009; it replaced the interim `#[path]` sharing of the macOS and Windows hosts).
//!
//! A `#[path]` that stays inside the crate's own directory is fine.

use std::path::{Component, Path, PathBuf};

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
}
