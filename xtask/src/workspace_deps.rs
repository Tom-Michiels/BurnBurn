//! `cargo xtask workspace-deps`: every dependency of a workspace crate inherits its version from
//! the root `[workspace.dependencies]` (`name = { workspace = true, ... }`). Team rule in
//! `docs/team.md`; proposed by Design as DR-07 (it would have caught DR-01 automatically).
//!
//! Checked sections: `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` and their
//! `[target.<cfg>.*]` variants. Features and `optional` may be added per crate; a version, path
//! or git source may not.

const SECTIONS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// Violations in one crate manifest, as `<crate>: [<section>] <dependency>` lines.
pub fn check_manifest(krate: &str, manifest: &str) -> Result<Vec<String>, String> {
    let table: toml::Table = toml::from_str(manifest).map_err(|e| format!("{krate}: {e}"))?;
    let mut out = Vec::new();
    check_sections(krate, &table, "", &mut out);
    if let Some(targets) = table.get("target").and_then(toml::Value::as_table) {
        for (cfg, target) in targets {
            if let Some(target) = target.as_table() {
                check_sections(krate, target, &format!("target.{}.", quote_key(cfg)), &mut out);
            }
        }
    }
    Ok(out)
}

/// TOML key as written in the manifest: bare when possible, otherwise single-quoted.
fn quote_key(key: &str) -> String {
    if key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        key.to_owned()
    } else {
        format!("'{key}'")
    }
}

fn check_sections(krate: &str, table: &toml::Table, prefix: &str, out: &mut Vec<String>) {
    for section in SECTIONS {
        let Some(deps) = table.get(section).and_then(toml::Value::as_table) else { continue };
        for (name, spec) in deps {
            let inherits = spec.get("workspace").and_then(toml::Value::as_bool) == Some(true);
            if !inherits {
                out.push(format!("{krate}: [{prefix}{section}] {name}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str) -> Vec<String> {
        check_manifest("demo", text).unwrap_or_else(|e| vec![format!("error: {e}")])
    }

    #[test]
    fn workspace_inherited_dependencies_pass() {
        let text = r#"
            [package]
            name = "demo"
            [dependencies]
            serde = { workspace = true }
            objc2-app-kit = { workspace = true, features = ["NSWindow"], optional = true }
            [dev-dependencies]
            proptest.workspace = true
        "#;
        assert_eq!(check(text), Vec::<String>::new());
    }

    #[test]
    fn inline_version_is_reported() {
        let text = "[dependencies]\nobjc2 = \"0.6\"\nserde = { workspace = true }\n";
        assert_eq!(check(text), ["demo: [dependencies] objc2"]);
    }

    #[test]
    fn version_table_without_workspace_is_reported() {
        let text = "[dependencies]\nobjc2-foundation = { version = \"0.3\", default-features = false }\n";
        assert_eq!(check(text), ["demo: [dependencies] objc2-foundation"]);
    }

    #[test]
    fn path_dependency_is_reported() {
        let text = "[dependencies]\nburnburn-types = { path = \"../burnburn-types\" }\n";
        assert_eq!(check(text), ["demo: [dependencies] burnburn-types"]);
    }

    #[test]
    fn dev_build_and_target_sections_are_checked() {
        let text = r#"
            [dev-dependencies]
            tempfile = "3"
            [build-dependencies]
            cc = { version = "1" }
            [target.'cfg(target_os = "macos")'.dependencies]
            objc2 = "0.6"
            raw-window-handle = { workspace = true }
            [target.'cfg(windows)'.dev-dependencies]
            windows = "0.61"
        "#;
        assert_eq!(
            check(text),
            [
                "demo: [dev-dependencies] tempfile",
                "demo: [build-dependencies] cc",
                "demo: [target.'cfg(target_os = \"macos\")'.dependencies] objc2",
                "demo: [target.'cfg(windows)'.dev-dependencies] windows",
            ]
        );
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(check_manifest("demo", "[dependencies\n").is_err());
    }
}
