//! `cargo xtask supersede` — retire a verdict that the world has moved past.
//!
//! The corpus is append-only, so a fact is never edited away. When a tracked
//! divergence closes — we implement the form we used to diverge on — the old
//! verdict does not become *false*: it was a true observation when it was made.
//! What changed is the world.
//!
//! A supersede record says so. It names the verdict it retires and why, the
//! retired record stays in the file, and the chain stays walkable. Deleting the
//! line instead would leave no trace that the difference ever existed, which is
//! the history most worth keeping.

use std::path::Path;

use verdict_corpus::{Corpus, Outcome, Record, Supersede, Verdict};

use crate::coverage;

/// Retire live verdicts carrying `tag`, narrowed by `only`, recording `reason`.
///
/// `only` matters more than it looks. One issue can track many divergences —
/// #93 covers twelve macro cases across six dialects — and they close one at a
/// time. Retiring by tag alone would sweep away eleven true, still-open facts
/// along with the one that closed. Every retired case is named on the way out,
/// so an over-broad run is visible rather than silent.
pub fn run(repo: &Path, tag: &str, reason: &str, only: &[String]) -> Result<Vec<String>, String> {
    let dir = coverage::stamp_path(repo)
        .parent()
        .map(Path::to_path_buf)
        .ok_or("no corpus directory")?;
    let mut retired: Vec<String> = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    files.sort();

    for path in files {
        let corpus = Corpus::read(&path).map_err(|e| e.to_string())?;
        let already: Vec<String> = corpus.retired().into_iter().map(str::to_string).collect();
        let doomed: Vec<&Verdict> = corpus
            .verdicts()
            .filter(|v| matches!(&v.outcome, Outcome::Divergence { divergence, .. } if divergence == tag))
            .filter(|v| !already.contains(&v.id()))
            .filter(|v| {
                only.iter()
                    .all(|needle| v.dialect.contains(needle.as_str()) || v.case.contains(needle.as_str()))
            })
            .collect();
        if doomed.is_empty() {
            continue;
        }
        let records: Vec<Record> = doomed
            .iter()
            .map(|v| {
                Record::Supersede(Supersede {
                    retires: v.id(),
                    reason: reason.to_string(),
                })
            })
            .collect();
        retired.extend(
            doomed
                .iter()
                .map(|v| format!("{} [{}] {}", v.cpu, v.dialect, v.case)),
        );
        verdict_corpus::append(&path, &records).map_err(|e| e.to_string())?;
    }
    Ok(retired)
}
