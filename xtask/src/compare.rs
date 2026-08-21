//! What we measured against, from the corpus that measured it.
//!
//! `/compare` has to keep two kinds of claim apart. Byte-level behaviour is
//! recorded: every verdict carries the reference tool's own version
//! self-report, so a table of what each tool arbitrated is a table the corpus
//! can produce. Feature claims about software we do not control are not
//! recorded anywhere and are not generated here — the page keeps those few and
//! frames them as what asm198x provides.
//!
//! The version column is deliberately "what we measured against" rather than
//! "the current version". It stays true when a reference tool ships, because it
//! is a statement about an observation and not about the world.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

/// What one reference tool arbitrated.
#[derive(Default)]
struct Tool {
    /// The tool's own version self-report, as recorded. More than one means
    /// the corpus spans versions, which the table says rather than hides.
    identities: BTreeSet<String>,
    cpus: BTreeSet<String>,
    verdicts: usize,
}

/// The "measured against" table.
#[must_use]
pub fn markdown(repo: &Path) -> String {
    let mut tools: BTreeMap<String, Tool> = BTreeMap::new();

    for path in crate::divergences::corpus_files(repo) {
        let Ok(corpus) = verdict_corpus::Corpus::read(&path) else {
            continue;
        };
        let retired = corpus.retired();
        for verdict in corpus.verdicts() {
            if retired.contains(&verdict.id().as_str()) {
                continue;
            }
            let entry = tools.entry(verdict.arbiter.tool.clone()).or_default();
            entry.identities.insert(verdict.arbiter.identity.clone());
            entry.cpus.insert(verdict.cpu.clone());
            entry.verdicts += 1;
        }
    }

    let mut out = String::from(
        "| Reference tool | The version we measured against | Instruction sets | Verdicts |\n\
         |---|---|---|---|\n",
    );
    for (tool, data) in &tools {
        let _ = writeln!(
            out,
            "| `{tool}` | {} | {} | {} |",
            data.identities
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("<br>"),
            data.cpus.len(),
            data.verdicts
        );
    }
    out
}
