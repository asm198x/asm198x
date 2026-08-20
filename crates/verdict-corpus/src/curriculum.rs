//! Which Code198x files the curriculum suite is answerable for.
//!
//! The suite proves byte-identity against the real curriculum, so the set it
//! walks *is* the claim. A file the walk never reaches is not reported as
//! unchecked — it is simply absent, which reads exactly like passing.
//!
//! That failure has already happened once. The walk named the game directories
//! it visited and took a single `.asm` per unit, so four `meet-the-machine`
//! tracks and every `steps/`-based track were invisible; widening it took the
//! suite from 162 comparisons to 617 and turned up four real divergences.
//!
//! The rule lives here, in a crate both the test harness and `xtask` depend on,
//! for one reason: a second copy is a second thing to forget to widen. The
//! published parity figures count what this returns, and the suite checks what
//! this returns, so the two cannot disagree about what the curriculum is.

use std::fs;
use std::path::{Path, PathBuf};

/// The Code198x checkout the curriculum suite reads from.
///
/// `ASM198X_CODE_SAMPLES` wins when set, which is how CI points at a checkout
/// inside the workspace; otherwise the sibling container two levels above the
/// workspace, as on a development machine.
pub fn root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ASM198X_CODE_SAMPLES") {
        let p = PathBuf::from(dir);
        return p.is_dir().then_some(p);
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../Code198x");
    let p = p.canonicalize().ok()?;
    p.is_dir().then_some(p)
}

/// Every game directory under a machine's assembly tree.
///
/// Enumerated rather than named, so a new track is covered the day it lands.
pub fn games(machine: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(machine) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// The buildable `.asm` files in a game directory.
///
/// The curriculum stores a unit one of two ways, and both are buildable:
///
/// - a single `.asm` directly in the `unit-*` directory; or
/// - a **cumulative build** in `steps/`, one file per step, each of which
///   carries its own `org` and `end` and runs on its own.
///
/// `snippets/` is skipped: those are fragments quoted by the prose, not
/// programs. `capture/` holds screenshot scripts rather than source. Anything
/// outside a `unit-*` directory is skipped too, which is what keeps the
/// Spectrum's `prototype/` tree — scratch work, not curriculum — out of the
/// count.
pub fn sources(game: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(units) = fs::read_dir(game) else {
        return out;
    };
    for unit in units.flatten() {
        let dir = unit.path();
        let is_unit = dir.is_dir()
            && dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("unit-"));
        if !is_unit {
            continue;
        }
        push_asms(&dir, &mut out);
        push_asms(&dir.join("steps"), &mut out);
    }
    out.sort();
    out
}

/// Every buildable `.asm` under a machine's assembly tree, across all games.
pub fn machine_sources(machine: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = games(machine).iter().flat_map(|g| sources(g)).collect();
    out.sort();
    out
}

/// Append every `.asm` directly inside `dir`, if it exists.
fn push_asms(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(files) = fs::read_dir(dir) else {
        return;
    };
    for f in files.flatten() {
        let fp = f.path();
        if fp.is_file() && fp.extension().and_then(|e| e.to_str()) == Some("asm") {
            out.push(fp);
        }
    }
}
