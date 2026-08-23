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

use verdict_corpus::{Corpus, Outcome, Record, Suite, Supersede, Verdict};

use crate::coverage;

/// Retire every live verdict for `cpu` in `suites`, recording `reason`.
///
/// The other kind of retirement, and the one #214 was filed for. A tracked
/// divergence closes because *we* changed; a listing changes because the text
/// we hand the reference changed, and then every verdict keyed on the old text
/// is stranded — still true, and about source this project no longer produces.
/// Replay reads those as failures, because the source in them no longer
/// assembles here.
///
/// [`run`] cannot express it: it selects on a divergence tag, and these
/// verdicts have none. So the selector is the scope of the change — a CPU and
/// the suites the changed generator feeds. A listing edit is a whole-CPU
/// re-arbitration event, which is what the verdict plan said it would be.
///
/// **Both are required.** `--cpu` alone would take a CPU's curriculum and probe
/// verdicts too, which no listing change touches, and retiring a true fact
/// because it shares a file with a stale one is how a corpus quietly shrinks.
pub fn run_by_scope(
    repo: &Path,
    cpu: &str,
    suites: &[Suite],
    reason: &str,
) -> Result<Vec<String>, String> {
    retire(repo, reason, |v| {
        v.cpu.eq_ignore_ascii_case(cpu) && suites.contains(&v.suite)
    })
}

/// Retire live verdicts carrying `tag`, narrowed by `only`, recording `reason`.
///
/// `only` matters more than it looks. One issue can track many divergences —
/// #93 covers twelve macro cases across six dialects — and they close one at a
/// time. Retiring by tag alone would sweep away eleven true, still-open facts
/// along with the one that closed. Every retired case is named on the way out,
/// so an over-broad run is visible rather than silent.
pub fn run(repo: &Path, tag: &str, reason: &str, only: &[String]) -> Result<Vec<String>, String> {
    retire(repo, reason, |v| {
        matches!(&v.outcome, Outcome::Divergence { divergence, .. } if divergence == tag)
            && only.iter().all(|needle| {
                v.dialect.contains(needle.as_str()) || v.case.contains(needle.as_str())
            })
    })
}

/// The shared retirement walk: append a supersede record for every live verdict
/// `doomed` selects, and name each one on the way out so an over-broad run is
/// visible rather than silent.
fn retire(
    repo: &Path,
    reason: &str,
    doomed: impl Fn(&Verdict) -> bool,
) -> Result<Vec<String>, String> {
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
            .filter(|v| !already.contains(&v.id()))
            .filter(|v| doomed(v))
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

#[cfg(test)]
mod tests {
    use super::*;
    use verdict_corpus::{Arbiter, Suite};

    fn verdict(cpu: &str, suite: Suite, case: &str) -> Record {
        Record::Verdict(Box::new(Verdict {
            suite,
            cpu: cpu.to_string(),
            dialect: "asl".to_string(),
            case: case.to_string(),
            source: format!("\t{case}\n"),
            arbiter: Arbiter {
                tool: "asl".to_string(),
                identity: "1.42".to_string(),
                digest: "d".to_string(),
            },
            outcome: Outcome::Bytes {
                hex: "00".to_string(),
            },
        }))
    }

    /// The scope selector retires the suites it is given, for the CPU it is
    /// given, and **nothing else** — a listing change strands the verdicts
    /// built from listings, and leaves a curriculum program or a hand-written
    /// probe exactly as true as it was.
    ///
    /// That is the whole risk of retiring by scope rather than by tag, so it is
    /// the thing worth pinning.
    #[test]
    fn scope_retires_the_named_suites_and_leaves_the_rest() {
        let dir = std::env::temp_dir().join("asm198x-supersede-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("crates/asm198x/tests/verdicts")).expect("temp corpus");
        let path = dir.join("crates/asm198x/tests/verdicts/cp1610.ndjson");
        verdict_corpus::append(
            &path,
            &[
                verdict("CP1610", Suite::SweepChunk, "sweep a"),
                verdict("CP1610", Suite::SweepChunk, "sweep b"),
                verdict("CP1610", Suite::Curriculum, "a program"),
                verdict("Z80", Suite::SweepChunk, "another cpu"),
            ],
        )
        .expect("seed");

        let retired =
            run_by_scope(&dir, "CP1610", &[Suite::SweepChunk], "the listing changed").expect("run");
        assert_eq!(retired.len(), 2, "{retired:?}");

        let corpus = Corpus::read(&path).expect("read back");
        let live: Vec<&str> = corpus
            .verdicts()
            .filter(|v| !corpus.retired().contains(&v.id().as_str()))
            .map(|v| v.case.as_str())
            .collect();
        assert_eq!(
            live,
            vec!["a program", "another cpu"],
            "only the named CPU's named suites are retired"
        );

        // Retiring is idempotent: a second run finds the same verdicts already
        // superseded and appends nothing, so a repeated command cannot bury
        // the corpus in duplicate records.
        assert!(
            run_by_scope(&dir, "CP1610", &[Suite::SweepChunk], "again")
                .expect("run")
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
