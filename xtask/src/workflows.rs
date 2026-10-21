//! Structural checks of `.github/workflows/*.yml` (X07). CI cannot be run from here, so the files
//! are at least checked for the mistakes that make GitHub reject a workflow outright.
//!
//! This is NOT a YAML parser: it knows the shape our workflows use (block style, two-space
//! indentation, no anchors or flow mappings for jobs). A pass proves the shape, never that a
//! workflow runs.

/// Findings (empty = the shape is fine). Line numbers are 1-based.
#[cfg(test)]
fn check_workflow(text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut top_keys: Vec<&str> = Vec::new();
    // (job name, has runs-on, has steps)
    let mut jobs: Vec<(String, bool, bool)> = Vec::new();
    let mut in_jobs = false;
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.contains('\t') {
            findings.push(format!("line {n}: tab character"));
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent % 2 != 0 {
            findings.push(format!("line {n}: odd indent of {indent}"));
        }
        if trimmed.matches('"').count() % 2 != 0 {
            findings.push(format!("line {n}: unbalanced double quote"));
        }
        if let Some(value) = trimmed.trim_start_matches("- ").strip_prefix("uses:")
            && !value.contains('@')
        {
            findings.push(format!("line {n}: action is not pinned to a version (@...)"));
        }
        if indent == 0 {
            let key = trimmed.split(':').next().unwrap_or("").trim_matches('"');
            top_keys.push(key);
            in_jobs = key == "jobs";
        } else if in_jobs && indent == 2 {
            jobs.push((trimmed.trim_end_matches(':').to_owned(), false, false));
        } else if in_jobs
            && indent == 4
            && let Some(job) = jobs.last_mut()
        {
            job.1 |= trimmed.starts_with("runs-on:");
            job.2 |= trimmed.starts_with("steps:");
        }
    }
    for (key, label) in [("name", "`name`"), ("on", "`on`"), ("jobs", "`jobs`")] {
        if !top_keys.contains(&key) {
            findings.push(format!("missing top-level {label}"));
        }
    }
    if top_keys.contains(&"jobs") && jobs.is_empty() {
        findings.push("`jobs` has no job".to_owned());
    }
    for (name, runs_on, steps) in jobs {
        if !runs_on {
            findings.push(format!("job `{name}` has no runs-on"));
        }
        if !steps {
            findings.push(format!("job `{name}` has no steps"));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const GOOD: &str = "name: T\non: [push]\njobs:\n  a:\n    runs-on: ubuntu-24.04\n    steps:\n      - uses: actions/checkout@v4\n      - run: echo hi\n";

    #[test]
    fn a_well_formed_workflow_has_no_findings() {
        assert_eq!(check_workflow(GOOD), Vec::<String>::new());
    }

    #[test]
    fn tabs_odd_indentation_and_missing_parts_are_found() {
        let findings = |t: &str| check_workflow(t).join("\n");
        assert!(findings(&GOOD.replace("  a:", "\ta:")).contains("tab"), "tab");
        assert!(findings(&GOOD.replace("    runs-on", "   runs-on")).contains("indent"), "odd indent");
        assert!(findings(&GOOD.replace("name: T\n", "")).contains("name"), "no name");
        assert!(findings(&GOOD.replace("on: [push]\n", "")).contains("`on`"), "no trigger");
        assert!(findings(&GOOD.replace("    runs-on: ubuntu-24.04\n", "")).contains("runs-on"), "no runner");
        assert!(findings(&GOOD.replace("    steps:\n", "    stepz:\n")).contains("steps"), "no steps");
        assert!(findings(&GOOD.replace("checkout@v4", "checkout")).contains("pinned"), "unpinned action");
        assert!(findings(&GOOD.replace("echo hi", "echo \"hi")).contains("quote"), "open quote");
    }

    #[test]
    fn every_committed_workflow_passes_the_structural_check() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "yml" || e == "yaml") {
                let text = std::fs::read_to_string(&path).unwrap();
                assert_eq!(check_workflow(&text), Vec::<String>::new(), "{}", path.display());
                seen += 1;
            }
        }
        assert!(seen >= 2, "ci.yml and release.yml are expected, found {seen}");
    }

    #[test]
    fn ci_runs_the_core_on_macos_and_windows_and_tests_the_windows_host() {
        let ci =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/ci.yml")).unwrap();
        assert!(ci.contains("macos-15") && ci.contains("windows-2025"));
        assert!(ci.contains("cargo xtask verify"));
        assert!(ci.contains("cargo test -p burnburn-windows --locked"));
        assert!(ci.contains("cargo check -p burnburn-windows --all-targets --locked"));
    }

    #[test]
    fn the_release_workflow_builds_both_routes_without_any_secret() {
        let rel =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/release.yml"))
                .unwrap();
        assert!(rel.contains("cargo xtask bundle --release --zip --dmg --sign-adhoc"));
        assert!(rel.contains("cargo xtask msix-layout"));
        assert!(rel.contains("workflow_dispatch"));
        assert!(!rel.contains("secrets."), "notarisation and signing identities stay manual");
    }
}
