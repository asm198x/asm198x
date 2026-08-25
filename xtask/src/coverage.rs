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

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use verdict_corpus::{Corpus, Suite, Verdict};

/// The stamp file, tracked so a change to it is reviewable.
const STAMP: &str = "crates/asm198x/tests/verdicts/coverage.stamp";

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

/// Count verdicts per CPU, by suite, from the committed corpus.
fn counts(root: &Path) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut out: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
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
            *out.entry(verdict.cpu.clone())
                .or_default()
                .entry(suite_name(verdict).to_string())
                .or_default() += 1;
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
    let counts = counts(&repo.join("crates/asm198x/tests/verdicts"));
    let rows = cpus()
        .into_iter()
        .map(|cpu| Row {
            arbitrated: counts
                .get(cpu.name)
                .and_then(|s| s.get("form"))
                .copied()
                .unwrap_or(0),
            total: cpu.forms,
            cpu: cpu.name.to_string(),
        })
        .collect();
    Report { rows, counts }
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
    s.push_str("# Below 100% is not automatically wrong: the 8039 forbids the four\n");
    s.push_str("# BUS-port ops its ROM-less bus is committed to, and the audit skips\n");
    s.push_str("# them deliberately. What matters is the number not falling.\n");
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
