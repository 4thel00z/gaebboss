//! Repository chores: `cargo xtask gen` regenerates the schema-derived
//! sources of gaebboss-core from the vendored GAEB XSDs.

mod schema;

fn main() -> std::process::ExitCode {
    eprintln!("usage: cargo xtask gen [--check]");
    std::process::ExitCode::FAILURE
}
