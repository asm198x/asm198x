//! `cargo xtask corpus`, `close-divergence` and `drop` — keeping the verdict
//! corpus to one current answer per case.
//!
//! The corpus answers *what do the reference tools in use now do?*
//! (`decisions/verdicts-track-current-tools.md`). Growth records and replaces;
//! these commands cover the rest:
//!
//! - `corpus --check` refuses a file that is not in canonical form, so a merge
//!   that doubles a line, or a hand edit, cannot land unnoticed;
//! - `corpus --write` rewrites every file canonically, migrating the old
//!   append-only format on the way;
//! - `corpus --prune` drops verdicts recorded by a tool version no longer in
//!   use, which is how a growth run against new tools finishes regenerating;
//! - `close-divergence` turns a tracked divergence that now matches back into
//!   an ordinary verdict;
//! - `drop` removes the verdicts a listing change has stranded.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use verdict_corpus::{Corpus, Key, Outcome, Suite, Verdict};

use crate::coverage;

/// The tool versions in use: `ops/arbiter/identities`, the file the arbiter
/// image verifies itself against. Tool name to version self-report.
///
/// # Errors
///
/// Returns a read failure or a malformed line.
pub fn current_identities(repo: &Path) -> Result<BTreeMap<String, String>, String> {
    let path = repo.join("ops/arbiter/identities");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (tool, identity) = line
            .split_once('\t')
            .ok_or_else(|| format!("{}:{}: expected `tool<TAB>identity`", path.display(), n + 1))?;
        out.insert(tool.to_string(), identity.to_string());
    }
    Ok(out)
}

/// Every corpus file, sorted.
fn corpus_files(repo: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = coverage::stamp_path(repo)
        .parent()
        .map(Path::to_path_buf)
        .ok_or("no corpus directory")?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    files.sort();
    Ok(files)
}

/// Read one file, migrating the pre-October-2026 format if it is in it.
fn read(path: &Path, current: &BTreeMap<String, String>) -> Result<Corpus, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if legacy::is_legacy(&text) {
        legacy::migrate(&text, current).map_err(|e| format!("{}: {e}", path.display()))
    } else {
        Corpus::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Files that are not in canonical form, with the reason. Empty means every
/// file is.
///
/// # Errors
///
/// Returns a read or parse failure.
pub fn check(repo: &Path) -> Result<Vec<String>, String> {
    let current = current_identities(repo)?;
    let mut problems = Vec::new();
    for path in corpus_files(repo)? {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if legacy::is_legacy(&text) {
            problems.push(format!("{name}: in the old append-only format"));
            continue;
        }
        let corpus = read(&path, &current)?;
        if corpus.render().map_err(|e| e.to_string())? != text {
            problems.push(format!(
                "{name}: not in canonical order, or one case is recorded twice"
            ));
        }
    }
    Ok(problems)
}

/// Verdicts from a tool version other than the one in use, per tool. Not an
/// error: between a version bump and the growth run that follows it, this is
/// what the corpus looks like.
///
/// # Errors
///
/// Returns a read or parse failure.
pub fn stale(repo: &Path) -> Result<BTreeMap<String, usize>, String> {
    let current = current_identities(repo)?;
    let mut out = BTreeMap::new();
    for path in corpus_files(repo)? {
        for v in read(&path, &current)?.verdicts() {
            if current.get(&v.arbiter.tool) != Some(&v.arbiter.identity) {
                *out.entry(v.arbiter.tool.clone()).or_default() += 1;
            }
        }
    }
    Ok(out)
}

/// Rewrite every file canonically. Returns the files that changed.
///
/// # Errors
///
/// Returns a read, parse or write failure.
pub fn write(repo: &Path) -> Result<Vec<String>, String> {
    let current = current_identities(repo)?;
    let mut changed = Vec::new();
    for path in corpus_files(repo)? {
        let before = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let corpus = read(&path, &current)?;
        if corpus.render().map_err(|e| e.to_string())? != before {
            corpus.write(&path).map_err(|e| e.to_string())?;
            changed.push(path.display().to_string());
        }
    }
    Ok(changed)
}

/// Drop verdicts recorded by a version of a tool in `present` other than the
/// one in use. Only tools a growth run actually had are pruned: a verdict from
/// a tool that was absent was not given the chance to be re-recorded.
///
/// Returns the count removed per tool.
///
/// # Errors
///
/// Returns a read, parse or write failure.
pub fn prune(repo: &Path, present: &HashSet<String>) -> Result<BTreeMap<String, usize>, String> {
    let current = current_identities(repo)?;
    let mut removed: BTreeMap<String, usize> = BTreeMap::new();
    for path in corpus_files(repo)? {
        let mut corpus = read(&path, &current)?;
        let mut dropped = Vec::new();
        corpus.retain(|v| {
            let stale = present.contains(&v.arbiter.tool)
                && current.get(&v.arbiter.tool) != Some(&v.arbiter.identity);
            if stale {
                dropped.push(v.arbiter.tool.clone());
            }
            !stale
        });
        if !dropped.is_empty() {
            for tool in dropped {
                *removed.entry(tool).or_default() += 1;
            }
            corpus.write(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(removed)
}

/// Rewrite the verdicts `select` picks with `change`, or remove them when it
/// answers `None`. Returns a line naming each one touched.
fn edit(
    repo: &Path,
    select: impl Fn(&Verdict) -> bool,
    change: impl Fn(&Verdict) -> Option<Verdict>,
) -> Result<Vec<String>, String> {
    let current = current_identities(repo)?;
    let mut touched = Vec::new();
    for path in corpus_files(repo)? {
        let mut corpus = read(&path, &current)?;
        let chosen: Vec<Verdict> = corpus.verdicts().filter(|v| select(v)).cloned().collect();
        if chosen.is_empty() {
            continue;
        }
        let keys: HashSet<Key> = chosen.iter().map(Verdict::key).collect();
        corpus.retain(|v| !keys.contains(&v.key()));
        for v in &chosen {
            touched.push(format!("{} [{}] {}", v.cpu, v.dialect, v.case));
            if let Some(replacement) = change(v) {
                corpus.insert(replacement);
            }
        }
        corpus.write(&path).map_err(|e| e.to_string())?;
    }
    Ok(touched)
}

/// Close a tracked divergence: every verdict carrying `tag` (narrowed by
/// `only`, each of which must appear in its dialect or case) becomes an
/// ordinary verdict for the same reference bytes.
///
/// `only` matters: one issue can track many divergences, closing one at a time,
/// and closing by tag alone would also close the ones still open.
///
/// # Errors
///
/// Returns a read, parse or write failure.
pub fn close_divergence(repo: &Path, tag: &str, only: &[String]) -> Result<Vec<String>, String> {
    edit(
        repo,
        |v| {
            matches!(&v.outcome, Outcome::Divergence { divergence, .. } if divergence == tag)
                && only.iter().all(|needle| {
                    v.dialect.contains(needle.as_str()) || v.case.contains(needle.as_str())
                })
        },
        |v| {
            let Outcome::Divergence { hex, .. } = &v.outcome else {
                return Some(v.clone());
            };
            let mut closed = v.clone();
            closed.outcome = Outcome::Bytes { hex: hex.clone() };
            Some(closed)
        },
    )
}

/// Remove every verdict for `cpu` in `suites`. Both are required: a listing
/// change strands the verdicts built from listings and leaves a curriculum
/// program or a hand-written probe exactly as true as it was.
///
/// # Errors
///
/// Returns a read, parse or write failure.
pub fn drop_scope(repo: &Path, cpu: &str, suites: &[Suite]) -> Result<Vec<String>, String> {
    edit(
        repo,
        |v| v.cpu.eq_ignore_ascii_case(cpu) && suites.contains(&v.suite),
        |_| None,
    )
}

/// The pre-October-2026 format: append-only, verdicts keyed on the tool's
/// version string with a binary digest, conflicts settled by supersede records.
/// Read once by `corpus --write` to migrate; nothing writes it.
mod legacy {
    use std::collections::{BTreeMap, HashSet};

    use verdict_corpus::{Corpus, Key, Verdict};

    /// Whether `text` carries anything only the old format had.
    pub fn is_legacy(text: &str) -> bool {
        text.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line).is_ok_and(|v| {
                v.get("t").and_then(serde_json::Value::as_str) == Some("supersede")
                    || v.pointer("/arbiter/digest").is_some()
            })
        })
    }

    /// The old record id, exactly as it was computed: FNV-1a over the labels,
    /// source, arbiter and serialized outcome. Supersede records name these.
    pub(super) fn old_id(v: &Verdict, digest: &str) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |s: &str| {
            for b in s.as_bytes() {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        mix(&v.cpu);
        mix(&v.dialect);
        mix(&v.case);
        mix(&v.source);
        mix(&v.arbiter.tool);
        mix(&v.arbiter.identity);
        mix(digest);
        mix(&serde_json::to_string(&v.outcome).unwrap_or_default());
        format!("{h:016x}")
    }

    /// Collapse an old-format file to one verdict per case: drop the records
    /// supersedes retired, then prefer the verdict from the version in use,
    /// then the latest in file order. Lines of any other kind are kept.
    pub fn migrate(text: &str, current: &BTreeMap<String, String>) -> Result<Corpus, String> {
        let lines: Vec<serde_json::Value> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(n, line)| serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1)))
            .collect::<Result<_, _>>()?;
        let kind = |v: &serde_json::Value| {
            v.get("t")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };

        // First pass: a supersede can come after the verdict it retires.
        let retired: HashSet<&str> = lines
            .iter()
            .filter(|v| kind(v).as_deref() == Some("supersede"))
            .filter_map(|v| v.get("retires").and_then(serde_json::Value::as_str))
            .collect();

        let in_use = |v: &Verdict| current.get(&v.arbiter.tool) == Some(&v.arbiter.identity);
        let mut chosen: BTreeMap<Key, Verdict> = BTreeMap::new();
        let mut kept = Vec::new();
        for value in &lines {
            match kind(value).as_deref() {
                Some("supersede") => {}
                Some("verdict") => {
                    let Ok(verdict) = serde_json::from_value::<Verdict>(value.clone()) else {
                        kept.push(value.clone());
                        continue;
                    };
                    let digest = value
                        .pointer("/arbiter/digest")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    if retired.contains(old_id(&verdict, digest).as_str()) {
                        continue;
                    }
                    // A later line replaces an earlier one, unless the earlier
                    // came from the version in use and the later did not.
                    let keep_held = chosen
                        .get(&verdict.key())
                        .is_some_and(|held| in_use(held) && !in_use(&verdict));
                    if !keep_held {
                        chosen.insert(verdict.key(), verdict);
                    }
                }
                _ => kept.push(value.clone()),
            }
        }

        let mut out = String::new();
        for verdict in chosen.values() {
            out.push_str(&verdict_corpus::to_line(verdict).map_err(|e| e.to_string())?);
            out.push('\n');
        }
        for value in kept {
            out.push_str(&serde_json::to_string(&value).map_err(|e| e.to_string())?);
            out.push('\n');
        }
        Corpus::parse(&out).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use verdict_corpus::Arbiter;

    fn verdict(cpu: &str, suite: Suite, case: &str, outcome: Outcome) -> Verdict {
        Verdict {
            suite,
            cpu: cpu.to_string(),
            dialect: "asl".to_string(),
            case: case.to_string(),
            source: format!("\t{case}\n"),
            arbiter: Arbiter {
                tool: "asl".to_string(),
                identity: "new".to_string(),
            },
            outcome,
        }
    }

    fn bytes() -> Outcome {
        Outcome::Bytes {
            hex: "00".to_string(),
        }
    }

    /// A throwaway repository with an identities file and a corpus directory.
    fn repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("asm198x-corpus-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ops/arbiter")).expect("ops");
        std::fs::write(
            dir.join("ops/arbiter/identities"),
            "# tool\tidentity\nasl\tnew\n",
        )
        .expect("identities");
        std::fs::create_dir_all(coverage::stamp_path(&dir).parent().expect("dir")).expect("corpus");
        dir
    }

    fn corpus_file(repo: &Path, cpu: &str) -> PathBuf {
        coverage::stamp_path(repo)
            .parent()
            .expect("dir")
            .join(format!("{cpu}.ndjson"))
    }

    /// The old format collapses to one verdict per case: retired records go,
    /// the version in use wins over an older one, and corroborating duplicates
    /// become one line.
    #[test]
    fn legacy_files_migrate_to_one_current_verdict_per_case() {
        let dir = repo("legacy");
        let old = |identity: &str, digest: &str, hex: &str| {
            format!(
                r#"{{"arbiter":{{"digest":"{digest}","identity":"{identity}","tool":"asl"}},"case":"nop","cpu":"8080","dialect":"asl","hex":"{hex}","outcome":"bytes","source":"\tnop\n","suite":"form","t":"verdict"}}"#
            )
        };
        let wrong = old("new", "d1", "FF");
        let wrong_verdict: Verdict = serde_json::from_str(&wrong).expect("parse");
        let text = [
            old("old", "d0", "01"),
            old("new", "d1", "00"),
            old("new", "d2", "00"),
            wrong,
            format!(
                r#"{{"reason":"bad build","retires":"{}","t":"supersede"}}"#,
                legacy::old_id(&wrong_verdict, "d1")
            ),
        ]
        .join("\n");
        std::fs::write(corpus_file(&dir, "8080"), format!("{text}\n")).expect("seed");

        assert_eq!(
            check(&dir).expect("check").len(),
            1,
            "the old format is reported"
        );
        assert_eq!(write(&dir).expect("write").len(), 1);
        let corpus = Corpus::read(&corpus_file(&dir, "8080")).expect("read");
        let only: Vec<_> = corpus.verdicts().collect();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].arbiter.identity, "new");
        assert_eq!(only[0].outcome, bytes());
        assert!(
            check(&dir).expect("check").is_empty(),
            "canonical after writing"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Pruning drops verdicts from a version no longer in use, but only for
    /// tools the run had.
    #[test]
    fn prune_drops_old_versions_of_present_tools_only() {
        let dir = repo("prune");
        let mut old = verdict("8080", Suite::Form, "old", bytes());
        old.arbiter.identity = "old".to_string();
        let mut absent = verdict("8080", Suite::Form, "absent", bytes());
        absent.arbiter.tool = "acme".to_string();
        absent.arbiter.identity = "anything".to_string();
        verdict_corpus::merge(
            &corpus_file(&dir, "8080"),
            [
                old,
                absent,
                verdict("8080", Suite::Form, "current", bytes()),
            ],
        )
        .expect("seed");

        assert_eq!(stale(&dir).expect("stale").get("asl"), Some(&1));
        let removed = prune(&dir, &HashSet::from(["asl".to_string()])).expect("prune");
        assert_eq!(removed.get("asl"), Some(&1));
        let left: Vec<String> = Corpus::read(&corpus_file(&dir, "8080"))
            .expect("read")
            .verdicts()
            .map(|v| v.case.clone())
            .collect();
        assert_eq!(left, vec!["absent", "current"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Closing a divergence keeps the reference bytes and drops only the tag,
    /// for the cases selected.
    #[test]
    fn closing_a_divergence_keeps_the_reference_bytes() {
        let dir = repo("close");
        let div = |hex: &str| Outcome::Divergence {
            divergence: "issue-7".to_string(),
            hex: hex.to_string(),
        };
        verdict_corpus::merge(
            &corpus_file(&dir, "8080"),
            [
                verdict("8080", Suite::Probe, "closed case", div("AA")),
                verdict("8080", Suite::Probe, "open case", div("BB")),
            ],
        )
        .expect("seed");
        // The filter must select by case alone: "closed" appears in no dialect.
        let closed = close_divergence(&dir, "issue-7", &["closed".to_string()]).expect("close");
        assert_eq!(closed.len(), 1, "{closed:?}");
        let corpus = Corpus::read(&corpus_file(&dir, "8080")).expect("read");
        let by_case: BTreeMap<_, _> = corpus
            .verdicts()
            .map(|v| (v.case.as_str(), &v.outcome))
            .collect();
        assert_eq!(
            by_case["closed case"],
            &Outcome::Bytes {
                hex: "AA".to_string()
            }
        );
        assert!(
            matches!(by_case["open case"], Outcome::Divergence { .. }),
            "the open one stays"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Dropping by scope removes the named suites for the named CPU and nothing
    /// else.
    #[test]
    fn drop_removes_the_named_suites_and_leaves_the_rest() {
        let dir = repo("drop");
        verdict_corpus::merge(
            &corpus_file(&dir, "cp1610"),
            [
                verdict("CP1610", Suite::SweepChunk, "sweep a", bytes()),
                verdict("CP1610", Suite::SweepChunk, "sweep b", bytes()),
                verdict("CP1610", Suite::Curriculum, "a program", bytes()),
            ],
        )
        .expect("seed");
        verdict_corpus::merge(
            &corpus_file(&dir, "z80"),
            [verdict("Z80", Suite::SweepChunk, "another cpu", bytes())],
        )
        .expect("seed");
        let dropped = drop_scope(&dir, "cp1610", &[Suite::SweepChunk]).expect("drop");
        assert_eq!(dropped.len(), 2);
        let left = |cpu| -> Vec<String> {
            Corpus::read(&corpus_file(&dir, cpu))
                .expect("read")
                .verdicts()
                .map(|v| v.case.clone())
                .collect()
        };
        assert_eq!(left("cp1610"), vec!["a program"]);
        assert_eq!(left("z80"), vec!["another cpu"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
