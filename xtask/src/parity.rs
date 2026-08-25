//! `cargo xtask parity` — what the curriculum suite has actually proved.
//!
//! The site's front door makes a claim per machine: this curriculum assembles
//! byte-for-byte to what the reference tool produces. Those numbers were typed
//! by hand and drifted — the page said 80 C64 units, 32 NES, 20 Spectrum, when
//! the corpus held 138, 51 and 161. A figure nobody can regenerate is a figure
//! nobody notices going stale.
//!
//! # Counted, not scored
//!
//! There is no percentage here, for the reason `coverage` gives: a denominator
//! has to be real. "64 of 97 units" would divide by a unit count that includes
//! units carrying no assembly at all, which is an invented denominator dressed
//! as a measurement.
//!
//! What is real is the file set — `verdict_corpus::curriculum` defines it, and
//! the suite walks exactly it. So the honest figure is a count of sources with
//! a recorded verdict, the reference tool that gave it, and, when a checkout is
//! present, whether any source in that set has no verdict yet.
//!
//! # Sources against comparisons
//!
//! They differ, and both are worth having. The Spectrum's 161 sources are each
//! arbitrated by two independent tools, and 37 of the Amiga's 69 are built both
//! as a hunk executable and as a flat binary. 419 sources, 617 comparisons.
//!
//! # What is not counted
//!
//! The 6809 and 65816 sections compare hand-written stand-in programs, because
//! no curriculum exists for those machines yet. They are real comparisons and
//! they are not curriculum parity, so they are not in this file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use verdict_corpus::{Corpus, Suite};

/// The generated data file, committed so the site can read it without a
/// Code198x checkout — the same trust model as the corpus it is derived from.
const DATA: &str = "crates/asm198x/tests/verdicts/parity.json";

/// The curriculum revision the recorded verdicts describe.
const PIN: &str = "crates/asm198x/tests/verdicts/code-samples.pin";

/// That revision's commit date, kept beside it rather than in it.
///
/// It cannot go *in* the pin file: CI reads that one with `cat` straight into
/// a checkout ref, so anything but a bare SHA breaks the checkout.
const DATE: &str = "crates/asm198x/tests/verdicts/code-samples.date";

/// One reference tool's contribution to a machine's parity.
struct ArbiterCount {
    tool: String,
    /// The tool's behavioural self-report — the corpus keys on this, so it is
    /// what "which version" actually means here.
    identity: String,
    variants: BTreeSet<String>,
    comparisons: usize,
}

/// What one machine's curriculum has recorded against it.
pub struct MachineParity {
    slug: String,
    cpu: String,
    /// Distinct curriculum files with at least one live verdict.
    sources: usize,
    /// Verdicts over those files: a file built two ways, or arbitrated by two
    /// tools, counts once for each.
    comparisons: usize,
    /// Files the shape rule finds in the checkout, when one is present.
    in_checkout: Option<usize>,
    /// Files in the checkout with no verdict for their current contents — the
    /// gap that matters. A stale verdict under the same path does not count.
    unverified: Vec<String>,
    arbiters: Vec<ArbiterCount>,
}

/// The whole picture.
pub struct Report {
    pin: Option<String>,
    machines: Vec<MachineParity>,
    /// Whether a Code198x checkout was readable when this was generated. Without
    /// one the file still describes the corpus, but cannot say what is missing.
    checkout_seen: bool,
}

impl Report {
    /// Distinct sources across every machine.
    fn sources(&self) -> usize {
        self.machines.iter().map(|m| m.sources).sum()
    }

    /// Comparisons across every machine.
    fn comparisons(&self) -> usize {
        self.machines.iter().map(|m| m.comparisons).sum()
    }

    /// Every source the checkout holds that has no verdict.
    fn unverified(&self) -> usize {
        self.machines.iter().map(|m| m.unverified.len()).sum()
    }
}

/// Take a curriculum key apart: `<relpath>#<variant>@<digest>`.
fn parse_key(key: &str) -> Option<(&str, &str, &str)> {
    let (path, rest) = key.split_once('#')?;
    let (variant, digest) = rest.split_once('@')?;
    Some((path, variant, digest))
}

/// The digest a curriculum key would carry for this file's current contents.
///
/// The key is built from the file's text (`curriculum_key` hashes the string
/// the harness read), so hashing the bytes gives the same answer. Returns
/// `None` for a file that cannot be read, which the caller treats as
/// unverified rather than silently present.
fn content_digest(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).ok()?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Some(crate::ledger::hex_lower(&h.finalize()))
}

/// The machine slug in a curriculum relpath: `code-samples/<slug>/assembly/…`.
fn machine_of(relpath: &str) -> Option<&str> {
    let rest = relpath.strip_prefix("code-samples/")?;
    let (slug, tail) = rest.split_once('/')?;
    tail.starts_with("assembly/").then_some(slug)
}

/// Read every corpus file and build the report.
#[must_use]
pub fn compute(repo: &Path) -> Report {
    let dir = repo.join("crates/asm198x/tests/verdicts");
    let mut by_machine: BTreeMap<String, Machine> = BTreeMap::new();

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Report {
            pin: None,
            machines: Vec::new(),
            checkout_seen: false,
        };
    };
    let mut files: Vec<PathBuf> = entries
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
            if verdict.suite != Suite::Curriculum {
                continue;
            }
            if retired.contains(&verdict.id().as_str()) {
                continue;
            }
            let Some((relpath, variant, digest)) = parse_key(&verdict.source) else {
                continue;
            };
            let Some(slug) = machine_of(relpath) else {
                continue;
            };
            let m = by_machine.entry(slug.to_string()).or_default();
            m.cpu = verdict.cpu.clone();
            m.sources.insert(relpath.to_string());
            // Keyed on the content, not the path: that is what makes the
            // receipt say "arbitrated against *this* text" rather than "a file
            // of this name was arbitrated once".
            m.arbitrated
                .insert((relpath.to_string(), digest.to_string()));
            m.comparisons += 1;
            let a = m
                .arbiters
                .entry((
                    verdict.arbiter.tool.clone(),
                    verdict.arbiter.identity.clone(),
                ))
                .or_default();
            a.0 += 1;
            a.1.insert(variant.to_string());
        }
    }

    let root = verdict_corpus::curriculum::root();
    let machines = by_machine
        .into_iter()
        .map(|(slug, m)| {
            let (in_checkout, unverified) = match &root {
                Some(root) => {
                    let dir = root.join(format!("code-samples/{slug}/assembly"));
                    let found = verdict_corpus::curriculum::machine_sources(&dir);
                    // A file is verified when the corpus holds a verdict for
                    // *its current contents*. Matching on the path alone would
                    // let a pin bump that edits a file ride on the old pin's
                    // verdict — the partial re-arbitration the receipt exists
                    // to refuse (asm198x#264).
                    let missing: Vec<String> = found
                        .iter()
                        .filter_map(|p| {
                            let rel = p.strip_prefix(root).ok()?;
                            let rel = rel.to_string_lossy().replace('\\', "/");
                            let fresh = content_digest(p)
                                .is_some_and(|d| m.arbitrated.contains(&(rel.clone(), d)));
                            (!fresh).then_some(rel)
                        })
                        .collect();
                    (Some(found.len()), missing)
                }
                None => (None, Vec::new()),
            };
            let mut arbiters: Vec<ArbiterCount> = m
                .arbiters
                .into_iter()
                .map(|((tool, identity), (comparisons, variants))| ArbiterCount {
                    tool,
                    identity,
                    variants,
                    comparisons,
                })
                .collect();
            arbiters.sort_by(|a, b| a.tool.cmp(&b.tool).then(a.identity.cmp(&b.identity)));
            MachineParity {
                slug,
                cpu: m.cpu,
                sources: m.sources.len(),
                comparisons: m.comparisons,
                in_checkout,
                unverified,
                arbiters,
            }
        })
        .collect();

    Report {
        pin: std::fs::read_to_string(repo.join(PIN))
            .ok()
            .map(|s| s.trim().to_string()),
        machines,
        checkout_seen: root.is_some(),
    }
}

/// Accumulator while reading the corpus.
#[derive(Default)]
struct Machine {
    cpu: String,
    sources: BTreeSet<String>,
    /// Every (path, source digest) the corpus holds a live verdict for.
    arbitrated: BTreeSet<(String, String)>,
    comparisons: usize,
    arbiters: BTreeMap<(String, String), (usize, BTreeSet<String>)>,
}

/// Render the report as the committed JSON.
#[must_use]
pub fn render(report: &Report) -> String {
    let machines: Vec<serde_json::Value> = report
        .machines
        .iter()
        .map(|m| {
            let arbiters: Vec<serde_json::Value> = m
                .arbiters
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "tool": a.tool,
                        "identity": a.identity,
                        "variants": a.variants.iter().collect::<Vec<_>>(),
                        "comparisons": a.comparisons,
                    })
                })
                .collect();
            let mut obj = serde_json::json!({
                "slug": m.slug,
                "cpu": m.cpu,
                "sources": m.sources,
                "comparisons": m.comparisons,
                "arbiters": arbiters,
            });
            if let Some(n) = m.in_checkout {
                obj["in_checkout"] = n.into();
            }
            if !m.unverified.is_empty() {
                obj["unverified"] = m.unverified.clone().into();
            }
            obj
        })
        .collect();

    let doc = serde_json::json!({
        "note": "Generated by `cargo xtask parity --write`. Counts of recorded byte-identity verdicts over the Code198x curriculum at `pin`. Do not edit by hand.",
        "pin": report.pin,
        "checkout_seen": report.checkout_seen,
        "machines": machines,
        "totals": {
            "sources": report.sources(),
            "comparisons": report.comparisons(),
        },
    });
    let mut out = serde_json::to_string_pretty(&doc).unwrap_or_default();
    out.push('\n');
    out
}

/// Where the data file lives under `repo`.
#[must_use]
pub fn data_path(repo: &Path) -> PathBuf {
    repo.join(DATA)
}

/// A human summary, for the default (no-flag) run.
#[must_use]
pub fn render_summary(report: &Report) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for m in &report.machines {
        let tools: Vec<&str> = m.arbiters.iter().map(|a| a.tool.as_str()).collect();
        let _ = writeln!(
            out,
            "{:<32} {:>4} sources  {:>4} comparisons  {}",
            m.slug,
            m.sources,
            m.comparisons,
            tools.join(" + ")
        );
        for rel in &m.unverified {
            let _ = writeln!(out, "{:<32}   no verdict: {rel}", "");
        }
    }
    let _ = writeln!(
        out,
        "{:<32} {:>4} sources  {:>4} comparisons",
        "total",
        report.sources(),
        report.comparisons()
    );
    if !report.checkout_seen {
        let _ = writeln!(
            out,
            "no Code198x checkout: counts describe the corpus, gaps cannot be reported"
        );
    }
    out
}

/// Whether the corpus still backs at least what the committed file records.
///
/// A machine that verifies fewer sources than the file says is the regression
/// worth failing on: something stopped being checked while the suite stayed
/// green. A machine verifying *more* is ordinary growth.
#[must_use]
pub fn regressions(report: &Report, committed: &str) -> Vec<String> {
    let Ok(old) = serde_json::from_str::<serde_json::Value>(committed) else {
        return vec!["the committed parity.json is not valid JSON".to_string()];
    };
    let mut out = Vec::new();
    let Some(machines) = old["machines"].as_array() else {
        return out;
    };
    for was in machines {
        let slug = was["slug"].as_str().unwrap_or_default();
        let before = was["sources"].as_u64().unwrap_or_default() as usize;
        let now = report
            .machines
            .iter()
            .find(|m| m.slug == slug)
            .map_or(0, |m| m.sources);
        if now < before {
            out.push(format!("{slug}: {before} sources -> {now}"));
        }
    }
    if report.unverified() > 0 {
        out.push(format!(
            "{} curriculum source(s) in the checkout have no verdict",
            report.unverified()
        ));
    }
    out
}

/// Whether the recorded pin describes the checkout that is actually here.
pub enum PinVerdict {
    /// HEAD and its commit date both match what the sidecars record.
    Matches,
    /// The claim cannot be tested here — no checkout, or one with no git
    /// metadata. Reported rather than passed over: silence is what let three
    /// separate gaps read as success.
    Unverifiable(String),
    /// A concrete disagreement, one line each.
    Wrong(Vec<String>),
}

/// Check the recorded pin and date against the curriculum checkout's own git.
///
/// The SHA is read by CI straight into a checkout ref, so it is exercised
/// every run and cannot drift unnoticed. The **date** beside it is
/// hand-maintained, and nothing has ever checked it — a pin bump that updates
/// one and not the other publishes a wrong date in the one document whose
/// purpose is being trustworthy.
///
/// Asking git also proves the checkout is *at* the pin, which nothing
/// currently asserts. A run against some other revision would otherwise
/// verify curriculum files that the recorded verdicts never described.
pub fn verify_pin(repo: &Path) -> PinVerdict {
    let read = |rel: &str| {
        std::fs::read_to_string(repo.join(rel))
            .ok()
            .map(|s| s.trim().to_string())
    };
    let (Some(pin), Some(date)) = (read(PIN), read(DATE)) else {
        return PinVerdict::Wrong(vec![format!(
            "no `{PIN}` or no `{DATE}` — the ledger names both, so both must exist"
        )]);
    };
    let Some(root) = verdict_corpus::curriculum::root() else {
        return PinVerdict::Unverifiable(format!(
            "no curriculum checkout, so `{pin}` ({date}) is unchecked here"
        ));
    };
    let checkout = root.join("code-samples");
    if !checkout.join(".git").exists() {
        return PinVerdict::Unverifiable(format!(
            "{} has no git metadata, so `{pin}` ({date}) is unchecked here",
            checkout.display()
        ));
    }
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    };
    let (Some(head), Some(head_date)) = (
        git(&["rev-parse", "HEAD"]),
        // Committer date, short: the same `YYYY-MM-DD` the sidecar holds.
        git(&["show", "-s", "--format=%cs", "HEAD"]),
    ) else {
        return PinVerdict::Unverifiable(format!(
            "{} would not answer `git rev-parse`, so `{pin}` ({date}) is unchecked here",
            checkout.display()
        ));
    };
    match compare_pin(&pin, &date, &head, &head_date) {
        v if v.is_empty() => PinVerdict::Matches,
        v => PinVerdict::Wrong(v),
    }
}

/// The comparison itself, kept free of IO so it can be tested without a
/// curriculum checkout — which is exactly the environment that cannot run it.
fn compare_pin(pin: &str, date: &str, head: &str, head_date: &str) -> Vec<String> {
    let mut out = Vec::new();
    if pin != head {
        out.push(format!(
            "the checkout is at `{head}`, but `{PIN}` records `{pin}` — \
             the recorded verdicts describe a revision this run is not reading"
        ));
    }
    if date != head_date {
        out.push(format!(
            "`{DATE}` records {date}, but `{head}` was committed {head_date} — \
             the ledger publishes that date, so it is wrong until this agrees"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{PinVerdict, compare_pin, parse_key, verify_pin};

    /// A checkout that agrees with both sidecars produces no complaint.
    #[test]
    fn an_agreeing_checkout_says_nothing() {
        assert!(compare_pin("abc123", "2026-08-14", "abc123", "2026-08-14").is_empty());
    }

    /// The SHA half. The message names *both* revisions: "the pin is wrong" is
    /// not actionable, and the reader needs to know which of the two to move.
    #[test]
    fn a_checkout_at_another_revision_names_both() {
        let out = compare_pin("recorded", "2026-08-14", "actual", "2026-08-14");
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("recorded"), "{}", out[0]);
        assert!(out[0].contains("actual"), "{}", out[0]);
    }

    /// The date half — the one nothing checked before, and the reason this
    /// exists. The ledger publishes it, so a stale sidecar is a wrong fact in
    /// a document whose whole purpose is being trustworthy.
    #[test]
    fn a_stale_date_is_caught_even_when_the_sha_agrees() {
        let out = compare_pin("abc123", "2026-01-01", "abc123", "2026-08-14");
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("2026-01-01"), "{}", out[0]);
        assert!(out[0].contains("2026-08-14"), "{}", out[0]);
    }

    /// Both wrong reports both. A check that stopped at the first would hide
    /// the second behind a fix for the first.
    #[test]
    fn two_disagreements_are_two_lines() {
        let out = compare_pin("recorded", "2026-01-01", "actual", "2026-08-14");
        assert_eq!(out.len(), 2, "{out:?}");
    }

    /// A missing sidecar is a fault, not an excuse. The ledger names both, so
    /// neither may simply be absent — this is the one input case that must not
    /// resolve to `Unverifiable`.
    #[test]
    fn a_missing_sidecar_is_wrong_rather_than_unverifiable() {
        let dir = std::env::temp_dir().join("asm198x-parity-pin-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        match verify_pin(&dir) {
            PinVerdict::Wrong(lines) => assert!(!lines.is_empty()),
            PinVerdict::Matches => panic!("a repo with no pin cannot match"),
            PinVerdict::Unverifiable(why) => {
                panic!("a missing sidecar is a fault, not an unverifiable claim: {why}")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The key carries the source digest, and the receipt needs it.
    ///
    /// Dropping it is what let a pin bump that *edits* a file ride on the old
    /// pin's verdict: the path still matched, so the file read as verified
    /// (asm198x#264).
    #[test]
    fn a_key_yields_its_path_variant_and_digest() {
        let key = "code-samples/sinclair-zx-spectrum/assembly/gloaming/unit-06/survives.asm\
                   #pasmonext@2ab0169788193e27060e999b51707a29cc88eb7c226d682a38ab12987349a0d7";
        let (path, variant, digest) = parse_key(key).expect("parses");
        assert!(path.ends_with("survives.asm"));
        assert_eq!(variant, "pasmonext");
        assert_eq!(
            digest,
            "2ab0169788193e27060e999b51707a29cc88eb7c226d682a38ab12987349a0d7"
        );
    }

    /// A key without a digest is not a curriculum key, and is skipped rather
    /// than treated as a file with no content.
    #[test]
    fn a_key_without_a_digest_is_not_one() {
        assert!(parse_key("path.asm#pasmonext").is_none());
        assert!(parse_key("path.asm").is_none());
    }
}
