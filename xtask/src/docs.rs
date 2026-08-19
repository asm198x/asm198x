//! `cargo xtask docs` — regenerate the book's generated blocks, and check them.
//!
//! The book carries prose and generated data side by side. A generated block is
//! marked in the source:
//!
//! ```text
//! <!-- generated: asm198x dialects --markdown -->
//! ...
//! <!-- /generated -->
//! ```
//!
//! Everything between the markers is produced by the command the opening marker
//! names, and nothing else in the file is touched. Editing inside a block is
//! pointless — the next run overwrites it — and leaving one stale fails
//! `--check`, which is what the CI gate runs.
//!
//! This exists because the alternative was measured and it does not work. The
//! dialect list lived in three hand-maintained places and two of them had
//! drifted: `--help` and the CLI reference were both missing five working
//! dialects, and the reference described the ROM-less MCS-48 parts as aliases
//! of the 8048 when they refuse instructions it accepts. Nothing failed. The
//! documentation was simply wrong for as long as nobody checked.

use std::path::{Path, PathBuf};

/// The marker that opens a generated block, followed by the command to run.
const OPEN: &str = "<!-- generated:";
/// The marker that closes one.
const CLOSE: &str = "<!-- /generated -->";

/// What a run of the generator found or did.
pub struct Report {
    /// Files whose generated blocks were rewritten (or would be, under
    /// `--check`).
    pub stale: Vec<String>,
    /// Files scanned, whether or not they held a block.
    pub scanned: usize,
    /// Generated blocks seen across every file.
    pub blocks: usize,
}

/// The book's source directory.
pub fn book_src(repo: &Path) -> PathBuf {
    repo.join("docs/book/src")
}

/// Regenerate every block in the book. With `check`, nothing is written and a
/// stale block is reported instead.
///
/// # Errors
/// A block whose command fails or is not recognised, an unterminated block, or
/// an unreadable source file — each names the file it came from, because a
/// generator that fails anonymously in CI is worse than no generator.
pub fn run(repo: &Path, check: bool) -> Result<Report, String> {
    let src = book_src(repo);
    let mut report = Report {
        stale: Vec::new(),
        scanned: 0,
        blocks: 0,
    };

    let mut files: Vec<PathBuf> = std::fs::read_dir(&src)
        .map_err(|e| format!("cannot read {}: {e}", src.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "md"))
        .collect();
    // Deterministic order, so a failure names the same file every run.
    files.sort();

    for path in files {
        report.scanned += 1;
        let original =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (rewritten, blocks) = regenerate(&original, &path)?;
        report.blocks += blocks;
        if rewritten == original {
            continue;
        }
        let name = path
            .strip_prefix(repo)
            .unwrap_or(&path)
            .display()
            .to_string();
        report.stale.push(name);
        if !check {
            std::fs::write(&path, rewritten).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }

    Ok(report)
}

/// Rewrite every generated block in one file, returning the new text and how
/// many blocks it held.
fn regenerate(source: &str, path: &Path) -> Result<(String, usize), String> {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    let mut blocks = 0;

    while let Some(open_at) = rest.find(OPEN) {
        let after_marker = open_at + OPEN.len();
        let marker_end = rest[after_marker..]
            .find("-->")
            .ok_or_else(|| format!("{}: a `generated:` marker is unterminated", path.display()))?
            + after_marker;
        let command = rest[after_marker..marker_end].trim();

        let body_start = marker_end + "-->".len();
        let close_at = rest[body_start..].find(CLOSE).ok_or_else(|| {
            format!(
                "{}: the generated block for `{command}` has no `{CLOSE}`",
                path.display()
            )
        })? + body_start;

        out.push_str(&rest[..body_start]);
        out.push('\n');
        out.push_str(&generate(command, path)?);
        out.push_str(&rest[close_at..close_at + CLOSE.len()]);

        rest = &rest[close_at + CLOSE.len()..];
        blocks += 1;
    }
    out.push_str(rest);
    Ok((out, blocks))
}

/// Produce one block's content.
///
/// Commands are matched, not shelled out to. A documentation generator that
/// ran arbitrary strings out of a markdown file would be a way to execute
/// whatever a pull request put there.
fn generate(command: &str, path: &Path) -> Result<String, String> {
    match command {
        "asm198x dialects --markdown" => Ok(asm198x::dialect_table::markdown()),
        other => Err(format!(
            "{}: no generator for `{other}`\n\
             \n\
             Generated blocks name a command this task knows how to run. Add it \
             to `xtask/src/docs.rs` rather than widening this to run whatever a \
             markdown file asks for.",
            path.display()
        )),
    }
}
