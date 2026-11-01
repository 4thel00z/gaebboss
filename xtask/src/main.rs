//! Repository chores: `cargo xtask gen` regenerates the schema-derived
//! sources of gaebboss-core from the vendored GAEB XSDs.

mod emit_names;
mod emit_phase;
#[allow(dead_code)]
mod schema;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use schema::{Schema, SchemaVersion};

const SOURCES: &str = "standard/3.1, standard/3.2 and standard/3.3";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    match args.first().map(String::as_str) {
        Some("gen") => run(generate(check)),
        _ => {
            eprintln!("usage: cargo xtask gen [--check]");
            ExitCode::FAILURE
        }
    }
}

fn run(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn load_schemas(root: &Path) -> Result<Vec<Schema>, String> {
    [
        ("3.1", SchemaVersion::V31),
        ("3.2", SchemaVersion::V32),
        ("3.3", SchemaVersion::V33),
    ]
    .into_iter()
    .map(|(dir, version)| {
        schema::load(&root.join("standard").join(dir), version).map_err(|e| e.to_string())
    })
    .collect()
}

fn generate(check: bool) -> Result<(), String> {
    let root = repo_root();
    let schemas = load_schemas(&root)?;
    let generated = root.join("crates/gaebboss-core/src/generated");
    let names = emit_names::collect(&schemas);
    let latest = schemas.last().ok_or("no schemas")?;
    let phases = emit_phase::codes(latest);
    let outputs = [
        (
            generated.join("names.rs"),
            emit_names::emit(&names, SOURCES),
        ),
        (
            generated.join("phase.rs"),
            emit_phase::emit(&phases, "standard/3.3 (tgDP)"),
        ),
    ];
    let mut stale = Vec::new();
    for (path, raw) in &outputs {
        let content = &rustfmt(raw)?;
        let current = std::fs::read_to_string(path).unwrap_or_default();
        if current == *content {
            continue;
        }
        if check {
            stale.push(path.display().to_string());
            continue;
        }
        std::fs::write(path, content).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    if stale.is_empty() {
        return Ok(());
    }
    Err(format!(
        "generated sources are stale: {}\nrun `cargo xtask gen`",
        stale.join(", ")
    ))
}

/// The source as `cargo fmt` would leave it, so checked-in output is stable.
fn rustfmt(source: &str) -> Result<String, String> {
    use std::io::Write as _;
    let mut child = std::process::Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout", "--quiet"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("rustfmt: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(source.as_bytes())
            .map_err(|e| format!("rustfmt stdin: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("rustfmt: {e}"))?;
    if !output.status.success() {
        return Err("rustfmt failed on generated source".to_string());
    }
    String::from_utf8(output.stdout).map_err(|e| format!("rustfmt output: {e}"))
}
