//! The conformance ledger: what the corpus proves, per CPU, at one revision.
//!
//! A release makes a claim — that this assembler's output is byte-identical to
//! the real one. The ledger is the accounting behind that claim: which arbiter
//! established it, at which version, over how many cases, and where a
//! difference is known and tracked rather than fixed.
//!
//! # Deterministic by construction
//!
//! Identical inputs produce byte-identical output. Every collection is ordered,
//! nothing is timestamped, and the corpus hash names exactly which corpus is
//! being described. A ledger you cannot regenerate is a claim you cannot check.
//!
//! Two things are deliberately absent:
//!
//! - **Replay pass rate.** It is structurally 100%: a failing replay fails CI,
//!   so a ledger could only ever report success. A number that cannot vary
//!   measures nothing.
//! - **The pinned curriculum's age.** Age is a fact about when you look, not
//!   about the corpus, and putting it here would make two runs of the same
//!   command disagree. The ref is named; how old it is belongs to whoever is
//!   reading, on the day they read it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use verdict_corpus::{Corpus, Outcome, Suite, Verdict};

use crate::coverage;

/// One arbiter, as the corpus knows it.
#[derive(Default)]
struct Arbiter {
    /// Distinct binaries seen behind this version — provenance, not rows.
    digests: BTreeSet<String>,
    /// Verdicts it produced, by suite.
    suites: BTreeMap<String, usize>,
}

/// Everything the ledger reports about one CPU.
#[derive(Default)]
struct CpuEntry {
    /// Keyed by (tool, behavioural identity). Two builds of one version are one
    /// row, with the second digest recorded as corroboration.
    arbiters: BTreeMap<(String, String), Arbiter>,
    /// Tracked differences, by tag.
    divergences: BTreeMap<String, usize>,
}

/// Render the ledger for the corpus under `repo`.
#[must_use]
pub fn render(repo: &Path) -> String {
    let dir = repo.join("crates/asm198x/tests/verdicts");
    let mut cpus: BTreeMap<String, CpuEntry> = BTreeMap::new();
    let mut hasher_input: Vec<(String, Vec<u8>)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "ndjson") {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                hasher_input.push((name, bytes));
            }
            let Ok(corpus) = Corpus::read(&path) else {
                continue;
            };
            for verdict in corpus.verdicts() {
                let entry = cpus.entry(verdict.cpu.clone()).or_default();
                let arbiter = entry
                    .arbiters
                    .entry((
                        verdict.arbiter.tool.clone(),
                        verdict.arbiter.identity.clone(),
                    ))
                    .or_default();
                arbiter.digests.insert(verdict.arbiter.digest.clone());
                *arbiter.suites.entry(suite_of(verdict)).or_default() += 1;
                if let Outcome::Divergence { divergence, .. } = &verdict.outcome {
                    *entry.divergences.entry(divergence.clone()).or_default() += 1;
                }
            }
        }
    }

    // Sorted by filename so the hash depends on the corpus, never on the order
    // the filesystem happened to hand back.
    hasher_input.sort_by(|a, b| a.0.cmp(&b.0));
    let corpus_hash = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for (name, bytes) in &hasher_input {
            h.update(name.as_bytes());
            h.update(bytes);
        }
        hex_lower(h.finalize().as_slice())
    };

    let cover: BTreeMap<String, coverage::Row> = coverage::compute(repo)
        .rows
        .into_iter()
        .map(|r| (r.cpu.clone(), r))
        .collect();
    let pin = std::fs::read_to_string(dir.join("code-samples.pin"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "none".to_string());

    let mut out = String::new();
    out.push_str("# Conformance ledger\n\n");
    out.push_str(
        "What the recorded verdict corpus proves, per CPU. Every row is an\n\
         observation of a real reference assembler, not an expectation written\n\
         by hand. Regenerate with `cargo xtask ledger`.\n\n",
    );
    let _ = writeln!(out, "- **Corpus hash:** `{corpus_hash}`");
    let _ = writeln!(out, "- **Pinned curriculum:** `{pin}`");
    let _ = writeln!(
        out,
        "- **CPUs:** {}, holding {} verdict(s)\n",
        cpus.len(),
        cpus.values()
            .flat_map(|c| c.arbiters.values())
            .flat_map(|a| a.suites.values())
            .sum::<usize>()
    );

    for (cpu, entry) in &cpus {
        let _ = writeln!(out, "## {cpu}\n");
        if let Some(row) = cover.get(cpu) {
            let p = row.permille();
            let _ = writeln!(
                out,
                "Form coverage: **{}/{}** ({}.{}%)\n",
                row.arbitrated,
                row.total,
                p / 10,
                p % 10
            );
        }
        out.push_str("| arbiter | version | binaries | verdicts |\n");
        out.push_str("|---|---|---|---|\n");
        for ((tool, identity), arbiter) in &entry.arbiters {
            let counts: Vec<String> = arbiter
                .suites
                .iter()
                .map(|(suite, n)| format!("{suite} {n}"))
                .collect();
            let _ = writeln!(
                out,
                "| `{tool}` | {identity} | {} | {} |",
                arbiter.digests.len(),
                counts.join(", ")
            );
        }
        if entry.divergences.is_empty() {
            out.push_str("\nNo tracked divergences.\n\n");
        } else {
            out.push_str("\nTracked divergences — differences we know about and check:\n\n");
            for (tag, n) in &entry.divergences {
                let _ = writeln!(out, "- `{tag}` — {n} case(s)");
            }
            out.push('\n');
        }
    }
    out
}

/// The suite's name as the corpus spells it, so a ledger row and a corpus line
/// use one vocabulary.
fn suite_of(v: &Verdict) -> String {
    match v.suite {
        Suite::Form => "form",
        Suite::SweepChunk => "sweep-chunk",
        Suite::Probe => "probe",
        Suite::Fuzz => "fuzz",
        Suite::Curriculum => "curriculum",
    }
    .to_string()
}

/// Lower-case hex, which is how every digest in the corpus is written.
///
/// sha2 0.11 returns a `hybrid_array::Array` rather than a `GenericArray`, and
/// that type implements no `LowerHex`, so `format!("{:x}", …)` no longer
/// compiles. `verdict_corpus::encode_hex` is the wrong replacement: it emits
/// **upper**-case, for the byte payloads a verdict carries. Swapping the case
/// of a recorded digest would change every `Verdict::id` and leave the corpus
/// mixed, so the case is part of the format rather than a detail.
pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ledger is a claim about a revision, so it must be reproducible from
    /// that revision. Two runs over the same corpus produce the same bytes —
    /// no timestamps, no map iteration order, no filesystem ordering.
    #[test]
    fn the_ledger_is_byte_stable_across_runs() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        assert_eq!(render(&repo), render(&repo));
    }

    /// The corpus hash is what ties a ledger to the corpus it describes, so it
    /// must actually appear.
    #[test]
    fn the_ledger_names_the_corpus_it_describes() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let out = render(&repo);
        assert!(out.contains("**Corpus hash:**"), "no corpus hash");
        assert!(out.contains("**Pinned curriculum:**"), "no pinned ref");
    }
}
