//! `cargo xtask <task>`: developer build and verification tasks, written in Rust.

mod bundle;
mod layers;
mod trace_corpus;
mod workspace_deps;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo xtask <verify|layers|workspace-deps|gen-trace-corpus|help>

  verify            cargo fmt --check, layers, workspace-deps, clippy -D warnings, tests and doc tests (all --locked)
  layers            check crate layering and forbidden dependencies against docs/design/layers.toml
  workspace-deps    every crate dependency must be `{ workspace = true, ... }`
  gen-trace-corpus  (re)write test-data/trace/ from the vector references in xtask/src/trace_corpus.rs";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("verify") => verify(),
        Some("layers") => check_layers(),
        Some("bundle") => bundle::run(&root(), args.get(1..).unwrap_or_default()),
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

/// The checks of `research/implementatieplan/01-test-driven-design.md` § CI.
fn verify() -> Result<(), String> {
    let steps: &[&[&str]] = &[
        &["fmt", "--all", "--check"],
        &["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
        &["test", "--workspace", "--locked"],
        &["test", "--doc", "--workspace", "--locked"],
    ];
    for (i, step) in steps.iter().enumerate() {
        run(step)?;
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

fn run(args: &[&str]) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    println!("$ cargo {}", args.join(" "));
    let status = Command::new(cargo).args(args).current_dir(root()).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("`cargo {}` failed ({status})", args.join(" "))) }
}
