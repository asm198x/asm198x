//! Recording and replaying reference verdicts (#61, U4).
//!
//! Two halves of one contract, deliberately in one file so neither can drift
//! from the other:
//!
//! - **Recording** happens in live mode, where the reference assemblers exist.
//!   Each arbitration that already ran appends what the reference did.
//! - **Replay** happens everywhere, with no tools at all. It assembles the
//!   *recorded source text* with our own assembler and checks the bytes against
//!   what the reference produced.
//!
//! Replay is the guarantee, restated without the tools: *given this exact
//! source, the real assembler produced these exact bytes, and so do we.*
//!
//! # Why appending is idempotent
//!
//! Live mode records on every run, so a corpus that simply appended would
//! double in size each time the suite ran locally. A verdict already present,
//! byte for byte, is not written again — re-running a live suite that learned
//! nothing new leaves the corpus untouched, and a diff therefore shows only
//! facts that are genuinely new.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use verdict_corpus::{Arbiter, Corpus, Outcome, Record, Resolution, Suite, Verdict, encode_hex};

use super::tool_identity;

/// Where the committed corpus lives, one NDJSON file per CPU.
pub fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("verdicts")
}

/// A CPU's corpus file. The label is lower-cased and made path-safe, so `SC/MP`
/// does not become a directory.
pub fn corpus_path(cpu: &str) -> PathBuf {
    let slug: String = cpu
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    corpus_root().join(format!("{slug}.ndjson"))
}

/// Collects verdicts during a live run and writes them once at the end.
///
/// Buffered rather than written per case: a sweep produces thousands of
/// verdicts, and reopening the file for each would dominate the run.
#[derive(Default)]
pub struct Recorder {
    pending: BTreeMap<String, Vec<Verdict>>,
}

/// Which case a verdict is about — the arbiter, the source, and enough labels
/// for a diff to read.
///
/// Grouped rather than passed loose because `tool` and `dialect` are different
/// things that happen to share a spelling today: the tool is the executable
/// whose identity signs the fact, the dialect is the syntax the source is
/// written in. They diverge as soon as the sweep records 68000, where the tool
/// is `vasmm68k_mot` and the dialect is `vasm`.
pub struct CaseRef<'a> {
    /// Which suite is recording.
    pub suite: Suite,
    /// The CPU, and so which corpus file this lands in.
    pub cpu: &'a str,
    /// The executable to identify — what signs the verdict.
    pub tool: &'a str,
    /// The syntax the source is written in.
    pub dialect: &'a str,
    /// A short human label, for reading diffs. Never part of the key.
    pub case: String,
    /// The exact text the arbiter was given. This *is* the key.
    pub source: &'a str,
}

impl Recorder {
    /// A recorder with nothing pending.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what the arbiter did with this case.
    ///
    /// Silently does nothing if the tool cannot be identified — an unsigned
    /// verdict is worth less than no verdict, and U3's identity test is what
    /// makes an unidentifiable tool loud rather than silent.
    pub fn record(&mut self, case: CaseRef<'_>, outcome: Outcome) {
        let Some(id) = tool_identity::identify(case.tool) else {
            return;
        };
        self.pending
            .entry(case.cpu.to_string())
            .or_default()
            .push(Verdict {
                suite: case.suite,
                cpu: case.cpu.to_string(),
                dialect: case.dialect.to_string(),
                case: case.case,
                source: case.source.to_string(),
                arbiter: Arbiter {
                    tool: id.tool,
                    identity: id.identity,
                    digest: id.digest,
                },
                outcome,
            });
    }

    /// The common case: the reference assembled to these bytes.
    pub fn record_bytes(&mut self, case: CaseRef<'_>, bytes: &[u8]) {
        let hex = encode_hex(bytes);
        self.record(case, Outcome::Bytes { hex });
    }

    /// Write everything new to the per-CPU corpus files, skipping facts already
    /// recorded identically. Returns how many lines were actually added.
    pub fn flush(self) -> std::io::Result<usize> {
        let mut written = 0;
        for (cpu, verdicts) in self.pending {
            let path = corpus_path(&cpu);
            let existing = Corpus::read(&path).map_err(std::io::Error::other)?;
            let known: std::collections::HashSet<String> =
                existing.verdicts().map(Verdict::id).collect();

            let mut fresh: Vec<Record> = Vec::new();
            let mut seen_this_run = std::collections::HashSet::new();
            for v in verdicts {
                let id = v.id();
                if known.contains(&id) || !seen_this_run.insert(id) {
                    continue;
                }
                fresh.push(Record::Verdict(Box::new(v)));
            }
            if !fresh.is_empty() {
                written += fresh.len();
                verdict_corpus::append(&path, &fresh)?;
            }
        }
        Ok(written)
    }
}

/// Assemble `source` with our own assembler for `cpu`.
///
/// `None` for a CPU the replay does not know how to drive — which must read as
/// "not replayable" rather than as a pass, or a corpus could grow facts nothing
/// ever checks.
pub fn assemble_ours(cpu: &str, source: &str) -> Option<Result<Vec<u8>, String>> {
    let result = match cpu {
        "6502" => asm198x::assemble_acme(source),
        "Z80" => asm198x::assemble_pasmo(source),
        "65816" => asm198x::assemble_ca65_816(source),
        "huc6280" => asm198x::assemble_ca65_huc6280(source),
        "sm83" => asm198x::assemble_rgbasm(source),
        "8080" => asm198x::assemble_i8080(source),
        "6800" => asm198x::assemble_m6800(source),
        "1802" => asm198x::assemble_1802(source),
        "8048" => asm198x::assemble_8048(source),
        "8039" => asm198x::assemble_8039(source),
        "SC/MP" => asm198x::assemble_scmp(source),
        "F8" => asm198x::assemble_f8(source),
        "2650" => asm198x::assemble_2650(source),
        "TMS7000" => asm198x::assemble_tms7000(source),
        _ => return None,
    };
    Some(
        result
            .map(|r| r.bytes)
            .map_err(|e| format!("we rejected the source: {e}")),
    )
}

/// What a replay pass found.
#[derive(Debug, Default)]
pub struct ReplayReport {
    /// Facts checked against our assembler.
    pub checked: usize,
    /// Facts whose CPU has no replay assembler wired up.
    pub unreplayable: usize,
    /// Cases where our bytes and the reference's disagree, or we rejected
    /// source the reference accepted.
    pub failures: Vec<String>,
    /// Cases the corpus cannot settle without a human (R2).
    pub alarms: Vec<String>,
}

/// Replay every committed verdict for `cpu`.
pub fn replay_cpu(cpu: &str, report: &mut ReplayReport) {
    let path = corpus_path(cpu);
    let corpus = match Corpus::read(&path) {
        Ok(c) => c,
        Err(e) => {
            report.failures.push(format!("{cpu}: {e}"));
            return;
        }
    };

    for (key, resolution) in corpus.resolved() {
        match resolution {
            Resolution::Alarm { conflicting } => {
                report.alarms.push(format!(
                    "{cpu}: {} verdicts disagree for `{}` under {}",
                    conflicting.len(),
                    conflicting.first().map_or("?", |v| v.case.as_str()),
                    key.identity,
                ));
            }
            Resolution::Fact { outcome, .. } => {
                // Only byte outcomes are replayable here. A recorded rejection
                // says the reference refused source we never claim to accept;
                // pairing that with our own behaviour is a separate question.
                let Some(reference) = outcome.bytes() else {
                    continue;
                };
                let Some(ours) = assemble_ours(cpu, &key.source) else {
                    report.unreplayable += 1;
                    continue;
                };
                report.checked += 1;
                match ours {
                    Ok(bytes) if bytes == reference => {}
                    Ok(bytes) => report.failures.push(format!(
                        "{cpu}: ours {:02X?} vs reference {:02X?} for source:\n{}",
                        bytes, reference, key.source
                    )),
                    Err(e) => report.failures.push(format!(
                        "{cpu}: {e}\nreference produced {reference:02X?} for source:\n{}",
                        key.source
                    )),
                }
            }
        }
    }
}

/// Every CPU with a committed corpus file.
pub fn recorded_cpus() -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(corpus_root()) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "ndjson")
            && let Ok(corpus) = Corpus::read(&path)
            && let Some(first) = corpus.verdicts().next()
        {
            found.push(first.cpu.clone());
        }
    }
    found.sort();
    found.dedup();
    found
}
