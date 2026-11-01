//! Where the test files are: the official fixtures in the repository and
//! the real-world corpus named by `GAEBBOSS_CORPUS`.

use std::path::{Path, PathBuf};

/// The repository's `tests/fixtures/official` files.
pub fn official_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/official");
    let mut files = gaeb_files_under(&root);
    files.sort();
    files
}

/// Every GAEB DA XML file under `GAEBBOSS_CORPUS`, sorted; empty when unset.
pub fn corpus_files() -> Vec<PathBuf> {
    let Some(dir) = std::env::var_os("GAEBBOSS_CORPUS") else {
        return Vec::new();
    };
    let mut files = gaeb_files_under(Path::new(&dir));
    files.sort();
    files
}

fn gaeb_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_uppercase();
            if ext.starts_with('X') && ext.len() >= 3 {
                files.push(path);
            }
        }
    }
    files
}
