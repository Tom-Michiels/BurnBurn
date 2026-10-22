//! `cargo xtask <task>`: developer build and verification tasks, written in Rust.

mod bundle;
mod dist;
mod layers;
mod msix;
mod trace_corpus;
mod workflows;
mod workspace_deps;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo xtask <verify|layers|workspace-deps|bundle|msix-layout|gen-trace-corpus|help>

  verify            cargo fmt --check, layers, workspace-deps, clippy -D warnings, tests, the 100k performance guard (work counts, no clock) and doc tests (all --locked);
                    native macOS tests run quietly (transparent windows, no focus taken) unless --native
  layers            check crate layering and forbidden dependencies against docs/design/layers.toml
  workspace-deps    every crate dependency must be `{ workspace = true, ... }`
  bundle            macOS: build BurnBurn.app [--release] [--zip] [--dmg] [--sign-adhoc] (dist files in <target>/<profile>/dist/)
  msix-layout       Windows: write the MSIX package directory [--exe <burnburn-win.exe>] [--out <dir>] [--publisher CN=...]
  gen-trace-corpus  (re)write test-data/trace/ from the vector references in xtask/src/trace_corpus.rs";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("verify") => verify(&args[1..]),
        Some("layers") => check_layers(),
        Some("bundle") => bundle::run(&root(), args.get(1..).unwrap_or_default()),
        Some("msix-layout") => msix::run(&root(), args.get(1..).unwrap_or_default()),
        Some("workspace-deps") => check_workspace_deps(),
        Some("gen-trace-corpus") => gen_trace_corpus(),
        Some("help" | "-h" | "--help") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown task `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

/// The cargo steps of `verify`, in order (layers and workspace-deps run after the first one).
fn verify_steps() -> Vec<Vec<&'static str>> {
    vec![
        vec!["fmt", "--all", "--check"],
        vec!["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
        // The general run leaves out the performance guard (`perf_guard_*` tests), which has a step
        // of its own below so that it is visible and cannot be filtered away by accident.
        vec!["test", "--workspace", "--locked", "--", "--skip", "perf_guard_"],
        // X04: deterministic work counts of the 100k route (click, select all, scene, compile scale).
        vec!["test", "-p", "burnburn-scenarios", "--locked", "--test", "perf_guard_100k"],
        vec!["test", "--doc", "--workspace", "--locked"],
    ]
}

/// Environment of the cargo steps of `verify`: quiet native tests (transparent windows that let
/// clicks through, no floating, no focus taken) unless `--native` asks for visible windows.
fn verify_env(args: &[String]) -> Vec<(&'static str, &'static str)> {
    if args.iter().any(|a| a == "--native") { Vec::new() } else { vec![("BURNBURN_NATIVE_QUIET", "1")] }
}

/// The checks of `research/implementatieplan/01-test-driven-design.md` § CI.
fn verify(args: &[String]) -> Result<(), String> {
    let env = verify_env(args);
    for (i, step) in verify_steps().iter().enumerate() {
        run_with(step, &env)?;
        if i == 0 {
            println!("$ cargo xtask layers");
            check_layers()?;
            println!("$ cargo xtask workspace-deps");
            check_workspace_deps()?;
        }
    }
    Ok(())
}

fn cargo_json(args: &[&str]) -> Result<serde_json::Value, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo).args(args).current_dir(root()).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("`cargo {}` failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr)));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata output: {e}"))
}

/// `[lints] workspace = true` in the manifest at `path`.
fn inherits_workspace_lints(path: &str) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
        .and_then(|m| m.get("lints")?.get("workspace")?.as_bool())
        .unwrap_or(false)
}

/// R1–R8 of `docs/design/layers.toml`, over the current workspace and every listed triple.
fn check_layers() -> Result<(), String> {
    let table_path = root().join("docs/design/layers.toml");
    let text = std::fs::read_to_string(&table_path).map_err(|e| format!("{}: {e}", table_path.display()))?;
    let table = layers::parse_table(&text)?;
    let meta = cargo_json(&["metadata", "--format-version", "1", "--no-deps", "--locked"])?;
    let packages = layers::packages_from_metadata(&meta, inherits_workspace_lints)?;
    let mut closures = layers::Closures::new();
    for triple in &table.triples {
        let meta = cargo_json(&["metadata", "--format-version", "1", "--locked", "--filter-platform", triple])?;
        closures.insert(triple.clone(), layers::closure_from_metadata(&meta)?);
    }
    let violations = layers::check(&table, &packages, &closures);
    for p in &packages {
        let spec = table.crates.get(&p.name);
        println!(
            "  {:<24} {}",
            p.name,
            spec.map_or("UNREGISTERED".to_owned(), |s| format!("L{} {:?}", s.layer, s.kind))
        );
    }
    if violations.is_empty() {
        println!("layers: {} crates, {} triples, no violations", packages.len(), table.triples.len());
        return Ok(());
    }
    for v in &violations {
        eprintln!("  {v}");
    }
    Err(format!("{} layering violation(s)", violations.len()))
}

/// DR-07: no crate pins its own dependency versions or sources.
fn check_workspace_deps() -> Result<(), String> {
    let meta = cargo_json(&["metadata", "--format-version", "1", "--no-deps", "--locked"])?;
    let packages = meta["packages"].as_array().ok_or("cargo metadata: no packages")?;
    let mut violations = Vec::new();
    for p in packages {
        let (Some(name), Some(path)) = (p["name"].as_str(), p["manifest_path"].as_str()) else {
            return Err("cargo metadata: package without name or manifest_path".into());
        };
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        violations.extend(workspace_deps::check_manifest(name, &text)?);
    }
    if violations.is_empty() {
        println!("workspace-deps: {} crates, every dependency inherits from [workspace.dependencies]", packages.len());
        return Ok(());
    }
    for v in &violations {
        eprintln!("  {v}");
    }
    Err(format!("{} dependency/ies without `workspace = true`", violations.len()))
}

fn gen_trace_corpus() -> Result<(), String> {
    let dir = root().join("test-data/trace");
    let n = trace_corpus::write(&dir)?;
    println!("gen-trace-corpus: wrote {n} files to {}; review the diff before committing", dir.display());
    Ok(())
}

fn run_with(args: &[&str], env: &[(&str, &str)]) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let shown: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v} ")).collect();
    println!("$ {}cargo {}", shown.concat(), args.join(" "));
    let status = Command::new(cargo)
        .args(args)
        .envs(env.iter().copied())
        .current_dir(root())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("`cargo {}` failed ({status})", args.join(" "))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// X04: the deterministic performance guard of the 100k route is a step of its own in `verify`
    /// (visible in the output), and the general workspace run does not run it a second time.
    #[test]
    fn verify_runs_the_100k_performance_guard_once_as_its_own_step() {
        let steps = verify_steps();
        let guards: Vec<_> = steps.iter().filter(|s| s.contains(&"perf_guard_100k")).collect();
        assert_eq!(guards.len(), 1, "one step names the guard test binary: {steps:?}");
        let step = guards[0];
        assert_eq!(step[..2], ["test", "-p"], "{step:?}");
        assert!(step.contains(&"burnburn-scenarios") && step.contains(&"--locked"), "{step:?}");
        let general = steps
            .iter()
            .find(|s| s.first() == Some(&"test") && s.contains(&"--workspace") && !s.contains(&"--doc"))
            .expect("the general workspace test step");
        let skip = general.iter().position(|a| *a == "--skip");
        assert_eq!(
            skip.and_then(|i| general.get(i + 1)),
            Some(&"perf_guard_"),
            "the general run skips it: {general:?}"
        );
    }

    /// Quiet by default (the computer stays usable); `--native` shows the windows as before.
    #[test]
    fn verify_runs_native_tests_quietly_unless_native_is_asked() {
        assert_eq!(verify_env(&[]), vec![("BURNBURN_NATIVE_QUIET", "1")]);
        assert_eq!(verify_env(&["--native".to_owned()]), Vec::<(&str, &str)>::new());
    }

    /// Wall-clock benchmarks stay out of `verify`: no step asks for ignored tests.
    #[test]
    fn verify_never_runs_the_wall_clock_benchmarks() {
        for step in verify_steps() {
            assert!(!step.contains(&"--ignored") && !step.contains(&"--include-ignored"), "{step:?}");
        }
    }
}
