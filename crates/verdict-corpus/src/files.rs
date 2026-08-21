//! Filesystem walks shared by the repository's tooling and its tests.
//!
//! This crate is where they meet: `xtask` and the test harnesses both depend on
//! it, and neither can depend on the other. That is the whole reason a walk
//! lives here rather than being written twice.
//!
//! It has been written twice, and it cost something both times. The curriculum
//! walk named the games it visited, so four tracks went unchecked. The book
//! walk did not recurse, so moving the pages into `reference/` and `guide/`
//! silently dropped the dialect table's staleness check and every book sample —
//! the sample suite found two pages and zero samples, and only its own
//! "this cannot be right" guard caught it.

use std::path::{Path, PathBuf};

/// Every `.md` under `dir`, at any depth, sorted.
///
/// Sorted so a failure names the same file every run.
///
/// # Errors
/// If a directory cannot be read.
pub fn markdown_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    walk(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}
