//! Arbitration coverage: how much of the spec has a recorded reference verdict.
//!
//! The corpus can only hold the project to what it has recorded. A form nobody
//! ever arbitrated is not wrong — it is *unchecked*, and the difference matters,
//! because a change that quietly stops arbitrating something leaves the suite
//! green while proving less than it did. Coverage is the number that makes that
//! visible.
//!
//! # What is counted, and what is not
//!
//! Coverage is reported for the **form audit** only: one verdict per `isa` form
//! the reference accepted, over every form the spec declares. That denominator
//! is derivable from `isa` alone — no reference tools, no test harness, no
//! network — which is what makes it checkable in CI on any machine.
//!
//! The other suites have no such denominator. There is no total number of
//! differential probes, fuzz programs or curriculum files that *ought* to exist;
//! those sets are chosen, not enumerated. Reporting a percentage over an
//! invented denominator would be worse than reporting none, so they are counted
//! and not scored.
//!
//! # Why a committed stamp
//!
//! The base revision's coverage is not derivable from the append-only corpus:
//! the denominator is the base's spec, which needs the base's code. The
//! alternative to a committed stamp is checking out the base ref and counting
//! again — a second build on every PR.
//!
//! The stamp is cheaper, and better in the way that matters: a coverage drop
//! shows up **in the diff**, next to the change that caused it, with the
//! de-arbitrated CPUs named in the file. That is the grep-able residue a
//! reviewer needs, rather than a number buried in a log.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use verdict_corpus::{Corpus, Suite, Verdict};

/// The stamp file, tracked so a change to it is reviewable.
const STAMP: &str = "crates/asm198x/tests/verdicts/coverage.stamp";

/// The accepted-shortfalls file, authored rather than generated.
const ACCEPTED: &str = "crates/asm198x/tests/verdicts/coverage.accepted";

/// A CPU whose spec forms can be counted: the corpus label, and its spec.
struct Cpu {
    /// The label used in the corpus, and in the stamp.
    name: &'static str,
    /// Total forms the spec declares.
    forms: usize,
}

/// Every CPU whose spec declares rows. The 8039 is the ROM-less MCS-48 kin: it
/// shares the 8048's spec but is arbitrated separately, so it is counted
/// separately — and the Z8001 stands in the same relation to the Z8000.
///
/// The denominator is [`isa::InstructionSet::rows`] and its per-module
/// equivalents, not a form count, so the specs that author their encodings
/// some other way are here too (`decisions/every-spec-enumerates-its-forms.md`).
/// For the `Form` specs the two are the same number, asserted in `isa` — this
/// change moves the source of the denominator without moving any denominator.
fn cpus() -> Vec<Cpu> {
    fn total(set: &isa::InstructionSet) -> usize {
        set.rows().count()
    }
    vec![
        Cpu {
            name: "1802",
            forms: total(&isa::cdp1802::SET),
        },
        Cpu {
            name: "2650",
            forms: total(&isa::s2650::SET),
        },
        Cpu {
            name: "6502",
            forms: total(&isa::mos6502::SET),
        },
        Cpu {
            name: "6800",
            forms: total(&isa::m6800::SET),
        },
        // The 65816 and HuC6280 audits sweep the 6502 base *and* the
        // extension, exactly as their assemblers accept both, so the base's
        // forms belong in the denominator. Counting the extension alone put
        // both over 100% — the first thing this metric caught was itself.
        Cpu {
            name: "65816",
            forms: total(&isa::mos6502::SET) + total(&isa::mos65816::SET),
        },
        Cpu {
            name: "8039",
            forms: total(&isa::i8048::SET),
        },
        Cpu {
            name: "8048",
            forms: total(&isa::i8048::SET),
        },
        Cpu {
            name: "8080",
            forms: total(&isa::i8080::SET),
        },
        Cpu {
            name: "F8",
            forms: total(&isa::f8::SET),
        },
        Cpu {
            name: "SC/MP",
            forms: total(&isa::scmp::SET),
        },
        Cpu {
            name: "TMS7000",
            forms: total(&isa::tms7000::SET),
        },
        Cpu {
            name: "Z80",
            forms: total(&isa::z80::SET),
        },
        Cpu {
            name: "huc6280",
            forms: total(&isa::mos6502::SET) + total(&isa::huc6280::SET),
        },
        Cpu {
            name: "sm83",
            forms: total(&isa::sm83::SET),
        },
        // The specs that author their encodings outside `InstructionSet`.
        // They have had no denominator until now, which is why a missing
        // mnemonic could not lower a score — see #225, and the record.
        Cpu {
            name: "6809",
            forms: isa::mos6809::rows().count(),
        },
        Cpu {
            name: "CP1610",
            forms: isa::cp1610::rows().count(),
        },
        Cpu {
            name: "PDP-11",
            forms: isa::pdp11::rows().count(),
        },
        Cpu {
            name: "TMS9900",
            forms: isa::tms9900::rows().count(),
        },
        Cpu {
            name: "Z8000",
            forms: isa::z8000::rows().count(),
        },
        Cpu {
            name: "Z8001",
            forms: isa::z8000::rows().count(),
        },
        Cpu {
            name: "68000",
            forms: isa::m68k::rows().count(),
        },
    ]
}

/// One CPU's line in the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Corpus label.
    pub cpu: String,
    /// Forms with a recorded verdict.
    pub arbitrated: usize,
    /// Forms the spec declares.
    pub total: usize,
}

impl Row {
    /// Coverage in tenths of a percent, so the stamp compares as an integer and
    /// cannot drift on floating-point formatting.
    #[must_use]
    pub fn permille(&self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        u32::try_from(self.arbitrated * 1000 / self.total).unwrap_or(u32::MAX)
    }
}

/// What one read of the corpus yields: verdicts counted per CPU per suite, and
/// the **distinct forms** arbitrated per CPU.
///
/// The two are different questions and were once the same number. A verdict is
/// one observation, so a form arbitrated by two tools — or by two versions of
/// one tool, which the corpus records separately by design — is two verdicts of
/// the same form. Counting verdicts as coverage reads the second arbiter as
/// twice the work: sweeping the 68000 under both vasm 2.0b and 2.0f scores
/// `1676/838`, which the ledger publishes as 200.0%.
///
/// The form's identity is its **case** label, not its source text. Two forms
/// can share source: `move.w d1,a2` and `movea.w d1,a2` are one line to the
/// assembler, which canonicalises the first, and the 68000 corpus has fourteen
/// such pairs. Keying on source would score them 824/838 and call a complete
/// audit incomplete.
struct Tally {
    per_suite: BTreeMap<String, BTreeMap<String, usize>>,
    forms: BTreeMap<String, BTreeSet<String>>,
}

/// Read the committed corpus once, tallying both.
fn tally(root: &Path) -> Tally {
    let mut out = Tally {
        per_suite: BTreeMap::new(),
        forms: BTreeMap::new(),
    };
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "ndjson") {
            continue;
        }
        let Ok(corpus) = Corpus::read(&path) else {
            continue;
        };
        for verdict in corpus.verdicts() {
            *out.per_suite
                .entry(verdict.cpu.clone())
                .or_default()
                .entry(suite_name(verdict).to_string())
                .or_default() += 1;
            if verdict.suite == Suite::Form {
                out.forms
                    .entry(verdict.cpu.clone())
                    .or_default()
                    .insert(verdict.case.clone());
            }
        }
    }
    out
}

fn suite_name(v: &Verdict) -> &'static str {
    match v.suite {
        Suite::Form => "form",
        Suite::SweepChunk => "sweep-chunk",
        Suite::Probe => "probe",
        Suite::Fuzz => "fuzz",
        Suite::Curriculum => "curriculum",
    }
}

/// The full report: scored form coverage, plus unscored counts for the rest.
pub struct Report {
    /// One row per CPU with a form audit.
    pub rows: Vec<Row>,
    /// Verdict counts per CPU per suite, including CPUs with no form audit.
    pub counts: BTreeMap<String, BTreeMap<String, usize>>,
}

/// Compute coverage against the corpus under `repo`.
#[must_use]
pub fn compute(repo: &Path) -> Report {
    let tally = tally(&repo.join("crates/asm198x/tests/verdicts"));
    let rows = cpus()
        .into_iter()
        .map(|cpu| Row {
            arbitrated: tally.forms.get(cpu.name).map_or(0, BTreeSet::len),
            total: cpu.forms,
            cpu: cpu.name.to_string(),
        })
        .collect();
    Report {
        rows,
        counts: tally.per_suite,
    }
}

/// Render the stamp: one line per CPU, sorted, plus the unscored counts as
/// comments so the file also reads as a summary of what the corpus holds.
#[must_use]
pub fn render_stamp(report: &Report) -> String {
    let mut s = String::new();
    s.push_str("# Arbitration coverage — forms with a recorded reference verdict.\n");
    s.push_str("# Regenerate with `cargo xtask coverage --write`.\n");
    s.push_str("# A drop here means a change stopped arbitrating something; say why\n");
    s.push_str("# in the commit, and record the debt if it is not being recovered now.\n");
    s.push_str("# Below 100% is not automatically wrong, but it is never unexplained:\n");
    s.push_str("# every shortfall states its reason and its size in coverage.accepted,\n");
    s.push_str("# and the check holds it to both.\n");
    for row in &report.rows {
        let _ = writeln!(
            s,
            "{}\t{}/{}\t{}.{}%",
            row.cpu,
            row.arbitrated,
            row.total,
            row.permille() / 10,
            row.permille() % 10
        );
    }
    s.push_str("#\n# Verdicts held, by CPU and suite (counted, not scored — these\n");
    s.push_str("# suites have no denominator that ought to exist).\n");
    for (cpu, suites) in &report.counts {
        let rendered: Vec<String> = suites.iter().map(|(k, n)| format!("{k}={n}")).collect();
        let _ = writeln!(s, "# {cpu}\t{}", rendered.join(" "));
    }
    s
}

/// The scored rows of a stamp file, for comparison.
#[must_use]
pub fn parse_stamp(text: &str) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(cpu), Some(ratio)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some((arbitrated, total)) = ratio.split_once('/') else {
            continue;
        };
        let (Ok(a), Ok(t)) = (arbitrated.parse::<usize>(), total.parse::<usize>()) else {
            continue;
        };
        out.insert(
            cpu.to_string(),
            Row {
                cpu: cpu.to_string(),
                arbitrated: a,
                total: t,
            }
            .permille(),
        );
    }
    out
}

/// Where the stamp lives under `repo`.
#[must_use]
pub fn stamp_path(repo: &Path) -> PathBuf {
    repo.join(STAMP)
}

/// The per-CPU status, for a human reading a CI log.
///
/// R11 asks that a pull request's arbitration status be *visible*, not merely
/// enforced. A check that prints one line when it passes tells a reader the
/// gate held; it does not tell them what it held to.
#[must_use]
pub fn render_status(report: &Report, accepted: &BTreeMap<String, Accepted>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "arbitration coverage, {} CPU(s):", report.rows.len());
    for row in &report.rows {
        let short = row.total.saturating_sub(row.arbitrated);
        let note = match (short, accepted.get(&row.cpu)) {
            (0, _) => String::new(),
            (n, Some(a)) if a.owed() => format!("  {n} owed — {}", a.reason),
            (n, Some(a)) => format!("  {n} accepted — {}", a.reason),
            (n, None) => format!("  {n} unarbitrated, undeclared"),
        };
        let _ = writeln!(
            out,
            "  {:<10} {:>5}/{:<5} {:>5}.{}%{note}",
            row.cpu,
            row.arbitrated,
            row.total,
            row.permille() / 10,
            row.permille() % 10
        );
    }
    out
}

/// The movement between a base stamp and the current corpus, for a pull
/// request's own delta.
///
/// The stamp is the baseline because it is committed and exact: a change that
/// moves coverage must move the stamp with it, so the difference between the
/// base's stamp and this one *is* the delta the pull request carries.
#[must_use]
pub fn render_delta(report: &Report, base: &BTreeMap<String, u32>) -> String {
    let moved = drift(report, base);
    let mut out = String::new();
    let appeared: Vec<&Row> = report
        .rows
        .iter()
        .filter(|r| !base.contains_key(&r.cpu))
        .collect();
    if moved.is_empty() && appeared.is_empty() {
        out.push_str("arbitration coverage: no change from the base\n");
        return out;
    }
    out.push_str("arbitration coverage, change from the base:\n");
    for m in &moved {
        let arrow = if m.fell() { "down" } else { "up" };
        let _ = writeln!(
            out,
            "  {:<10} {}.{}% -> {}.{}%  ({arrow})",
            m.cpu,
            m.was / 10,
            m.was % 10,
            m.now / 10,
            m.now % 10
        );
    }
    for row in appeared {
        let _ = writeln!(
            out,
            "  {:<10} new — {}/{} arbitrated",
            row.cpu, row.arbitrated, row.total
        );
    }
    out
}

/// Where the accepted-shortfalls file lives under `repo`.
#[must_use]
pub fn accepted_path(repo: &Path) -> PathBuf {
    repo.join(ACCEPTED)
}

/// What a CPU declares about the rows it does not arbitrate.
#[derive(Debug, PartialEq, Eq)]
pub struct Accepted {
    /// How many rows, so the shortfall cannot widen quietly.
    pub rows: usize,
    /// Why. Prose for a reader, except for the one word that classifies it.
    pub reason: String,
}

impl Accepted {
    /// Is this a debt rather than a decision?
    ///
    /// A shortfall is normally permanent — a form the part forbids, or one a
    /// decision puts out of the audit's reach. Those are settled, and a release
    /// carrying them is fine.
    ///
    /// A reason opening with `owed` says the opposite: these rows are meant to
    /// come back, and the entry is a note to self. Work can still land on that
    /// basis, because the alternative is blocking a merge on a growth run that
    /// needs the reference tools. What it cannot do is reach a release: the
    /// pre-tag gate stays red until a growth run clears it. That bounds how
    /// long a de-arbitrated form can hide.
    #[must_use]
    pub fn owed(&self) -> bool {
        self.reason
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("owed")
    }
}

/// What each CPU declares about its shortfall.
///
/// Same shape as the stamp: tab-separated, `#` comments, one CPU per line.
#[must_use]
pub fn parse_accepted(text: &str) -> BTreeMap<String, Accepted> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let cpu = parts.next()?.trim().to_string();
            let rows = parts.next()?.trim().parse().ok()?;
            let reason = parts.next().unwrap_or_default().trim().to_string();
            Some((cpu, Accepted { rows, reason }))
        })
        .collect()
}

/// Every shortfall still owed, in the order the file lists them.
///
/// This is the growth-debt residue the release ratchet gates on. A coverage
/// number cannot express it: acknowledging a drop lowers that number, and
/// nothing afterwards remembers it was a drop.
#[must_use]
pub fn owed(accepted: &BTreeMap<String, Accepted>) -> Vec<(&String, &Accepted)> {
    accepted.iter().filter(|(_, a)| a.owed()).collect()
}

/// A shortfall that does not match what the CPU declares it accepts.
#[derive(Debug, PartialEq, Eq)]
pub enum Unaccepted {
    /// Rows go unarbitrated and nothing says why. This is the debt case: until
    /// it is declared, nobody can tell a decision from an outstanding one.
    Undeclared { cpu: String, rows: usize },
    /// More rows go unarbitrated than the CPU declares. The excess is debt even
    /// though the rest is accepted.
    Wider {
        cpu: String,
        declared: usize,
        rows: usize,
    },
    /// The CPU arbitrates everything, and still declares a shortfall. The entry
    /// outlived what it described.
    Stale { cpu: String, declared: usize },
    /// The CPU arbitrates nothing at all.
    ///
    /// A spec can be declared before it is arbitrated — nothing stops the rows
    /// existing — but it must not *merge* that way. Unlike every other case
    /// here, this one cannot be declared away: an entry accepting the whole
    /// spec would be a CPU claiming compatibility that no reference has ever
    /// checked, which is the one thing this project cannot ship.
    Unarbitrated { cpu: String, rows: usize },
}

/// Hold every shortfall to what its CPU declares in the accepted file, and
/// every CPU to arbitrating something.
///
/// A number below 100% is not itself a fault — the 8039 cannot reach the four
/// BUS-port ops its ROM-less bus commits, and the 6809's three undocumented
/// opcodes are input-only by decision. What would be a fault is not being able
/// to tell those from a form that quietly stopped arbitrating. So the shortfall
/// declares its size and its reason, and this checks the size.
#[must_use]
pub fn unaccepted(report: &Report, accepted: &BTreeMap<String, Accepted>) -> Vec<Unaccepted> {
    report
        .rows
        .iter()
        .filter_map(|row| {
            let rows = row.total.saturating_sub(row.arbitrated);
            // Before consulting the file, because this one is not the file's to
            // excuse. A growth run is the precondition, not a declaration.
            if row.total > 0 && row.arbitrated == 0 {
                return Some(Unaccepted::Unarbitrated {
                    cpu: row.cpu.clone(),
                    rows,
                });
            }
            match (rows, accepted.get(&row.cpu).map(|a| a.rows)) {
                (0, None) => None,
                (0, Some(declared)) => Some(Unaccepted::Stale {
                    cpu: row.cpu.clone(),
                    declared,
                }),
                (rows, None) => Some(Unaccepted::Undeclared {
                    cpu: row.cpu.clone(),
                    rows,
                }),
                (rows, Some(declared)) if rows > declared => Some(Unaccepted::Wider {
                    cpu: row.cpu.clone(),
                    declared,
                    rows,
                }),
                _ => None,
            }
        })
        .collect()
}

/// A CPU whose coverage differs from the stamp, and in which direction.
#[derive(Debug, PartialEq, Eq)]
pub struct Move {
    /// Corpus label.
    pub cpu: String,
    /// Recorded coverage, in tenths of a percent.
    pub was: u32,
    /// Coverage now.
    pub now: u32,
}

impl Move {
    /// Coverage fell: something stopped being arbitrated.
    #[must_use]
    pub const fn fell(&self) -> bool {
        self.now < self.was
    }
}

/// Compare a computed report against a stamp. Returns every CPU that moved.
///
/// Both directions are reported, for different reasons.
///
/// A **fall** is the alarm: something stopped being arbitrated, and the stamp
/// must not move to cover it without a reason in the commit.
///
/// A **rise** used to pass unremarked, on the reasoning that a stale-but-lower
/// stamp never blocks work. It also never protects it. While the Z8000's stamp
/// read 0 of 271 and the corpus arbitrated 145, a regression all the way down
/// to a single row would have passed this check — and it stayed that way across
/// three merged pull requests, because nothing asks about a rise. The stamp is
/// the ratchet's entire memory, so a lagging stamp is a ratchet that has quietly
/// let go.
///
/// It also costs the thing this module's own rationale asks for: coverage
/// showing up in the diff beside the change that caused it. A rise recorded
/// later, in some unrelated pull request, is not that.
#[must_use]
pub fn drift(report: &Report, stamp: &BTreeMap<String, u32>) -> Vec<Move> {
    report
        .rows
        .iter()
        .filter_map(|row| {
            let was = *stamp.get(&row.cpu)?;
            let now = row.permille();
            (now != was).then_some(Move {
                cpu: row.cpu.clone(),
                was,
                now,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(rows: &[(&str, usize, usize)]) -> Report {
        Report {
            rows: rows
                .iter()
                .map(|(c, a, t)| Row {
                    cpu: (*c).to_string(),
                    arbitrated: *a,
                    total: *t,
                })
                .collect(),
            counts: BTreeMap::new(),
        }
    }

    /// A second arbiter over the same forms is corroboration, not coverage.
    ///
    /// The regression this guards: `arbitrated` counted verdict *lines*, so
    /// sweeping the 68000 under both installed vasm versions scored 1676 of
    /// 838 forms and the ledger published "200.0%". The two identities here
    /// are the two the 68000 corpus actually holds — an invented version would
    /// be a version claim, and `versions --check` refuses those.
    #[test]
    fn a_second_arbiter_does_not_inflate_coverage() {
        let dir = std::env::temp_dir().join("asm198x-coverage-tally");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let line = |case: &str, identity: &str, source: &str| {
            format!(
                "{{\"t\":\"verdict\",\"suite\":\"form\",\"cpu\":\"68000\",\
                 \"dialect\":\"vasm\",\"case\":\"{case}\",\"source\":\"{source}\",\
                 \"arbiter\":{{\"tool\":\"vasmm68k_mot\",\"identity\":\"{identity}\",\
                 \"digest\":\"00\"}},\"outcome\":\"bytes\",\"hex\":\"4E71\"}}\n"
            )
        };
        let older = "vasm 2.0b (c) in 2002-2025 Volker Barthelmann";
        let newer = "vasm 2.0f (c) in 2002-2026 Volker Barthelmann";
        let corpus = format!(
            "{}{}{}",
            line("NOP", older, "\\tnop\\n"),
            // The same form, seen again by the other installed version.
            line("NOP", newer, "\\tnop\\n"),
            line("RTS", older, "\\trts\\n"),
        );
        std::fs::write(dir.join("68000.ndjson"), corpus).expect("write corpus");

        let tally = tally(&dir);
        assert_eq!(
            tally.forms.get("68000").map(BTreeSet::len),
            Some(2),
            "two forms were arbitrated, one of them twice"
        );
        // The per-suite count is the other question, and still counts every
        // observation: three verdicts were recorded.
        assert_eq!(tally.per_suite["68000"]["form"], 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stamp round-trips its scored rows, so what CI compares is what the
    /// file says.
    #[test]
    fn a_stamp_round_trips_its_rows() {
        let r = report(&[("6502", 150, 300), ("Z80", 700, 700)]);
        let parsed = parse_stamp(&render_stamp(&r));
        assert_eq!(parsed.get("6502"), Some(&500));
        assert_eq!(parsed.get("Z80"), Some(&1000));
    }

    /// Comments and blank lines are not rows — the stamp doubles as a summary,
    /// and the summary must not be mistaken for data.
    #[test]
    fn comments_are_not_rows() {
        let parsed = parse_stamp("# 6502\tform=10\n\n6502\t1/2\t50.0%\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.get("6502"), Some(&500));
    }

    /// Falling coverage is the whole point: it is reported, per CPU, with both
    /// numbers so the size of the loss is legible.
    #[test]
    fn a_fall_is_reported_with_both_numbers() {
        let stamp = parse_stamp(&render_stamp(&report(&[("6502", 300, 300)])));
        let now = report(&[("6502", 290, 300)]);
        assert_eq!(
            drift(&now, &stamp),
            vec![Move {
                cpu: "6502".to_string(),
                was: 1000,
                now: 966
            }]
        );
    }

    /// A rise is drift too, and is not a fall.
    ///
    /// It used to pass unremarked. That let the Z8000's stamp sit at 0 of 271
    /// while the corpus arbitrated 145, across three merged pull requests —
    /// and a stamp reading 0 protects nothing above 0.
    #[test]
    fn rising_coverage_is_drift_but_not_a_fall() {
        let stamp = parse_stamp(&render_stamp(&report(&[("6502", 100, 300)])));
        let moved = drift(&report(&[("6502", 200, 300)]), &stamp);
        assert_eq!(moved.len(), 1, "a rise is reported");
        assert!(!moved[0].fell(), "and it is not a fall");
    }

    /// A shortfall with no entry is debt until someone says otherwise.
    ///
    /// This is the case that motivated the file: the 6809 gained three
    /// unarbitrable rows and the stamp's prose header, which named only the
    /// 8039, did not gain a line — because nothing asked it to.
    #[test]
    fn an_undeclared_shortfall_is_not_accepted() {
        let accepted = parse_accepted("6502\t1\twhy\n");
        assert_eq!(
            unaccepted(&report(&[("6809", 277, 280)]), &accepted),
            vec![Unaccepted::Undeclared {
                cpu: "6809".into(),
                rows: 3,
            }]
        );
    }

    /// A shortfall may not quietly grow past what it declares.
    #[test]
    fn a_shortfall_beyond_its_declaration_is_not_accepted() {
        let accepted = parse_accepted("6809\t1\twhy\n");
        assert_eq!(
            unaccepted(&report(&[("6809", 277, 280)]), &accepted),
            vec![Unaccepted::Wider {
                cpu: "6809".into(),
                declared: 1,
                rows: 3,
            }]
        );
    }

    /// A declaration outliving its shortfall comes out, so the file cannot
    /// accumulate permissions nobody needs.
    #[test]
    fn a_declaration_for_a_complete_cpu_is_stale() {
        let accepted = parse_accepted("Z80\t5\twhy\n");
        assert_eq!(
            unaccepted(&report(&[("Z80", 700, 700)]), &accepted),
            vec![Unaccepted::Stale {
                cpu: "Z80".into(),
                declared: 5,
            }]
        );
    }

    /// A shortfall matching its declaration passes, and so does a whole CPU.
    #[test]
    fn a_declared_shortfall_is_accepted() {
        let accepted = parse_accepted("# comment\n\n6809\t3\tinput-only, by decision\n");
        assert!(unaccepted(&report(&[("6809", 277, 280)]), &accepted).is_empty());
        assert!(unaccepted(&report(&[("Z80", 700, 700)]), &accepted).is_empty());
    }

    /// The delta names both directions and a CPU the base did not have.
    #[test]
    fn a_delta_reports_movement_and_arrivals() {
        let base = parse_stamp(&render_stamp(&report(&[("Z8000", 145, 271)])));
        let now = report(&[("Z8000", 271, 271), ("Z8001", 271, 271)]);
        let out = render_delta(&now, &base);
        assert!(out.contains("Z8000"), "movement is named: {out}");
        assert!(out.contains("(up)"), "with its direction: {out}");
        assert!(
            out.contains("Z8001") && out.contains("new"),
            "arrivals too: {out}"
        );
    }

    /// An unchanged corpus says so rather than printing an empty table.
    #[test]
    fn an_unchanged_corpus_has_no_delta() {
        let base = parse_stamp(&render_stamp(&report(&[("Z80", 704, 704)])));
        let out = render_delta(&report(&[("Z80", 704, 704)]), &base);
        assert!(out.contains("no change"), "{out}");
    }

    /// The status names a shortfall's reason, and says when it is owed rather
    /// than settled — that difference is the point of printing it at all.
    #[test]
    fn the_status_distinguishes_owed_from_accepted() {
        let r = report(&[("6809", 277, 280), ("8039", 210, 214)]);
        let a = parse_accepted("6809\t3\tinput-only\n8039\t4\towed: coming back\n");
        let out = render_status(&r, &a);
        assert!(out.contains("3 accepted — input-only"), "{out}");
        assert!(out.contains("4 owed — owed: coming back"), "{out}");
    }

    /// A CPU arbitrating nothing fails, and the accepted file cannot excuse it.
    ///
    /// This is the new-CPU gate: a spec may land before its verdicts exist, but
    /// not merge that way. Declaring the whole spec away would be a CPU
    /// claiming compatibility no reference has checked.
    #[test]
    fn a_cpu_arbitrating_nothing_cannot_be_declared_away() {
        let none = parse_accepted("");
        assert_eq!(
            unaccepted(&report(&[("Z8000", 0, 271)]), &none),
            vec![Unaccepted::Unarbitrated {
                cpu: "Z8000".into(),
                rows: 271,
            }]
        );
        // And the file does not help.
        let excused = parse_accepted("Z8000\t271\tnot arbitrated yet\n");
        assert_eq!(
            unaccepted(&report(&[("Z8000", 0, 271)]), &excused),
            vec![Unaccepted::Unarbitrated {
                cpu: "Z8000".into(),
                rows: 271,
            }]
        );
        // One verdict is enough to leave the gate — the ratchet takes over.
        assert!(
            unaccepted(
                &report(&[("Z8000", 1, 271)]),
                &parse_accepted("Z8000\t270\towed: growth run pending\n")
            )
            .is_empty()
        );
    }

    /// `owed` classifies; everything else is a settled decision.
    #[test]
    fn a_reason_opening_with_owed_is_debt() {
        let a = parse_accepted(
            "6809\t3\tinput-only, by decision\n\
             8039\t4\tOwed #1: a listing change stranded these\n",
        );
        assert!(!a["6809"].owed(), "a decision is not debt");
        assert!(a["8039"].owed(), "and `owed` is, whatever its case");

        let debt = owed(&a);
        assert_eq!(debt.len(), 1);
        assert_eq!(debt[0].0, "8039");
    }

    /// Debt still counts as declared, so it does not block a merge — only a
    /// release. Blocking the merge would mean blocking on a growth run, which
    /// needs the reference tools.
    #[test]
    fn debt_is_still_a_declaration() {
        let a = parse_accepted("6809\t3\towed #1: coming back\n");
        assert!(unaccepted(&report(&[("6809", 277, 280)]), &a).is_empty());
        assert_eq!(owed(&a).len(), 1);
    }

    /// Coverage that matches the stamp is not drift in either direction.
    #[test]
    fn matching_coverage_is_not_drift() {
        let stamp = parse_stamp(&render_stamp(&report(&[("6502", 100, 300)])));
        assert!(drift(&report(&[("6502", 100, 300)]), &stamp).is_empty());
    }

    /// A CPU absent from the stamp is new, not regressed — a spec added since
    /// the stamp was written has nothing to fall from.
    #[test]
    fn a_cpu_absent_from_the_stamp_is_new_not_drift() {
        let stamp = parse_stamp(&render_stamp(&report(&[("6502", 300, 300)])));
        assert!(drift(&report(&[("Z80", 0, 700)]), &stamp).is_empty());
    }

    /// Coverage is integer tenths of a percent, so a stamp comparison can never
    /// turn on how a float was formatted.
    #[test]
    fn coverage_is_integer_tenths_of_a_percent() {
        assert_eq!(
            Row {
                cpu: "x".into(),
                arbitrated: 1,
                total: 3
            }
            .permille(),
            333
        );
        assert_eq!(
            Row {
                cpu: "x".into(),
                arbitrated: 0,
                total: 0
            }
            .permille(),
            0
        );
    }
}
