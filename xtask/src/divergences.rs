//! Where our output differs from the reference tool, on purpose and on record.
//!
//! Every differential suite compares our bytes against a real assembler's. When
//! they differ and the difference is known, the verdict is recorded as
//! [`verdict_corpus::Outcome::Divergence`] carrying a join id, which ties the
//! recorded fact to the in-repo expectation of our own output. Neither half can
//! go missing without the other noticing.
//!
//! # Why publish them
//!
//! A claim of parity asks to be believed. A list of every place we knowingly
//! differ can be read instead — and it is the more useful document, because
//! someone deciding whether to move a working project across needs to know what
//! will change, not to be reassured that nothing will.
//!
//! # What this does not decide
//!
//! Whether a difference is a bug or a settled choice. `issue-110` is closed as
//! completed: our 68000 output sits between vasm's optimising and
//! non-optimising modes, deliberately, and no single vasm invocation reproduces
//! it. `issue-93` is open. This generates what the corpus knows — which tool,
//! which dialect, how many cases — and the prose around it says which is which,
//! because a closed issue rendered as pending work would be a lie the corpus
//! cannot catch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use verdict_corpus::{Corpus, Outcome};

/// One tracked difference, aggregated across every verdict that names it.
struct Divergence {
    cpus: BTreeSet<String>,
    dialects: BTreeSet<String>,
    tools: BTreeSet<String>,
    cases: usize,
}

/// Read the corpus and group every divergence by its join id.
fn collect(repo: &Path) -> BTreeMap<String, Divergence> {
    let dir = repo.join("crates/asm198x/tests/verdicts");
    let mut out: BTreeMap<String, Divergence> = BTreeMap::new();

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ndjson"))
        .collect();
    files.sort();

    for path in files {
        let Ok(corpus) = Corpus::read(&path) else {
            continue;
        };
        let retired = corpus.retired();
        for verdict in corpus.verdicts() {
            let Outcome::Divergence { divergence, .. } = &verdict.outcome else {
                continue;
            };
            if retired.contains(&verdict.id().as_str()) {
                continue;
            }
            let entry = out.entry(divergence.clone()).or_insert_with(|| Divergence {
                cpus: BTreeSet::new(),
                dialects: BTreeSet::new(),
                tools: BTreeSet::new(),
                cases: 0,
            });
            entry.cpus.insert(verdict.cpu.clone());
            entry.dialects.insert(verdict.dialect.clone());
            entry.tools.insert(verdict.arbiter.tool.clone());
            entry.cases += 1;
        }
    }
    out
}

/// The generated table for `divergences.md`.
#[must_use]
pub fn markdown(repo: &Path) -> String {
    use std::fmt::Write as _;
    let found = collect(repo);

    let mut out = String::new();
    if found.is_empty() {
        out.push_str("No tracked divergences are recorded.\n");
        return out;
    }

    let total: usize = found.values().map(|d| d.cases).sum();
    let _ = writeln!(
        out,
        "{} tracked difference{} across {} recorded case{}.\n",
        found.len(),
        if found.len() == 1 { "" } else { "s" },
        total,
        if total == 1 { "" } else { "s" }
    );
    out.push_str("| Difference | CPU | Dialect | Reference tool | Cases |\n");
    out.push_str("|---|---|---|---|---|\n");

    for (id, d) in &found {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            link(id),
            join(&d.cpus),
            join(&d.dialects),
            join(&d.tools),
            d.cases
        );
    }
    out
}

/// An `issue-NN` id links to the issue tracking it; anything else is a name.
fn link(id: &str) -> String {
    match id
        .strip_prefix("issue-")
        .and_then(|n| n.parse::<u32>().ok())
    {
        Some(n) => format!("[`{id}`](https://github.com/asm198x/asm198x/issues/{n})"),
        None => format!("`{id}`"),
    }
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}
