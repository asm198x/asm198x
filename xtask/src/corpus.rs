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
//! - [`prune`], run by growth, drops what a growth run superseded, judged only
//!   from what that run recorded;
//! - `close-divergence` turns a tracked divergence that now matches back into
//!   an ordinary verdict;
//! - `drop` removes the verdicts a listing change has stranded.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use verdict_corpus::{Corpus, Key, Outcome, Recorded, Suite, Verdict};

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

/// Prune what a growth run superseded, using its manifest as the only evidence.
///
/// Only a (file, suite, tool) group the run recorded into is touched, so a
/// suite that was skipped (a missing helper tool, an absent curriculum
/// checkout, a filtered run) loses nothing. Within such a group:
///
/// - a verdict from a version other than the one this run recorded is stale,
///   and goes;
/// - a curriculum verdict whose program this run re-recorded under a new
///   content digest is about source that no longer exists, and goes.
///
/// A group the run recorded under two versions is ambiguous and left alone.
/// Returns the count removed per tool.
///
/// # Errors
///
/// Returns a read, parse or write failure.
pub fn prune(repo: &Path, manifest: &[Recorded]) -> Result<BTreeMap<String, usize>, String> {
    // (file, suite, tool) -> the versions this run recorded under.
    let mut versions: BTreeMap<(&str, Suite, &str), BTreeSet<&str>> = BTreeMap::new();
    // (file, tool) -> curriculum keys this run recorded.
    let mut programs: BTreeMap<(&str, &str), HashSet<&str>> = BTreeMap::new();
    for r in manifest {
        versions
            .entry((&r.file, r.suite, &r.tool))
            .or_default()
            .insert(&r.identity);
        if let Some(source) = &r.source {
            programs
                .entry((&r.file, &r.tool))
                .or_default()
                .insert(source);
        }
    }
    // A curriculum key is `path#variant@digest`; the program is everything
    // before the `@`.
    let program = |key: &str| key.rsplit_once('@').map(|(p, _)| p.to_string());

    let mut removed: BTreeMap<String, usize> = BTreeMap::new();
    for path in corpus_files(repo)? {
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut corpus = Corpus::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut dropped = Vec::new();
        corpus.retain(|v| {
            let Some(seen) = versions.get(&(file.as_str(), v.suite, v.arbiter.tool.as_str()))
            else {
                return true;
            };
            let [current] = seen.iter().copied().collect::<Vec<_>>()[..] else {
                return true;
            };
            let stale = v.arbiter.identity != current;
            let orphaned = v.suite == Suite::Curriculum
                && programs
                    .get(&(file.as_str(), v.arbiter.tool.as_str()))
                    .is_some_and(|keys| {
                        !keys.contains(v.source.as_str())
                            && keys.iter().any(|k| program(k) == program(&v.source))
                    });
            if stale || orphaned {
                dropped.push(v.arbiter.tool.clone());
            }
            !(stale || orphaned)
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

    fn recorded(file: &str, suite: Suite, identity: &str, source: Option<&str>) -> Recorded {
        Recorded {
            file: file.to_string(),
            suite,
            tool: "asl".to_string(),
            identity: identity.to_string(),
            source: source.map(str::to_string),
        }
    }

    fn cases(dir: &Path, cpu: &str) -> Vec<String> {
        Corpus::read(&corpus_file(dir, cpu))
            .expect("read")
            .verdicts()
            .map(|v| v.case.clone())
            .collect()
    }

    /// Pruning drops old-version verdicts only in a group the run recorded
    /// into: a suite it never reached, or another tool, loses nothing.
    #[test]
    fn prune_touches_only_what_the_run_recorded() {
        let dir = repo("prune");
        let aged = |suite, case: &str| {
            let mut v = verdict("8080", suite, case, bytes());
            v.arbiter.identity = "old".to_string();
            v
        };
        let mut other_tool = aged(Suite::Form, "other tool");
        other_tool.arbiter.tool = "acme".to_string();
        verdict_corpus::merge(
            &corpus_file(&dir, "8080"),
            [
                aged(Suite::Form, "stale form"),
                aged(Suite::SweepChunk, "unreached sweep"),
                other_tool,
                verdict("8080", Suite::Form, "fresh form", bytes()),
            ],
        )
        .expect("seed");

        let manifest = [recorded("8080.ndjson", Suite::Form, "new", None)];
        let removed = prune(&dir, &manifest).expect("prune");
        assert_eq!(removed.get("asl"), Some(&1));
        assert_eq!(
            cases(&dir, "8080"),
            vec!["fresh form", "other tool", "unreached sweep"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A run that recorded one group under two versions has no single current
    /// version, so nothing in that group is pruned.
    #[test]
    fn prune_leaves_an_ambiguous_group_alone() {
        let dir = repo("ambiguous");
        let mut old = verdict("8080", Suite::Form, "old", bytes());
        old.arbiter.identity = "old".to_string();
        verdict_corpus::merge(&corpus_file(&dir, "8080"), [old]).expect("seed");
        let manifest = [
            recorded("8080.ndjson", Suite::Form, "new", None),
            recorded("8080.ndjson", Suite::Form, "newer", None),
        ];
        assert!(prune(&dir, &manifest).expect("prune").is_empty());
        assert_eq!(cases(&dir, "8080"), vec!["old"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A curriculum program re-recorded under a new content digest retires
    /// the verdict for its old digest, and only that one.
    #[test]
    fn prune_retires_a_curriculum_program_s_old_digest() {
        let dir = repo("curriculum");
        let program = |case: &str, key: &str| {
            let mut v = verdict(
                "6502",
                Suite::Curriculum,
                case,
                Outcome::Digest {
                    digest: "00".to_string(),
                },
            );
            v.source = key.to_string();
            v
        };
        verdict_corpus::merge(
            &corpus_file(&dir, "6502"),
            [
                program("a old", "a.s#acme@d1"),
                program("a new", "a.s#acme@d2"),
                program("b", "b.s#acme@d1"),
            ],
        )
        .expect("seed");
        let manifest = [recorded(
            "6502.ndjson",
            Suite::Curriculum,
            "new",
            Some("a.s#acme@d2"),
        )];
        let removed = prune(&dir, &manifest).expect("prune");
        assert_eq!(removed.get("asl"), Some(&1));
        assert_eq!(cases(&dir, "6502"), vec!["a new", "b"]);
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
