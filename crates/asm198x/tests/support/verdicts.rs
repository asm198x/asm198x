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

/// Assemble a **form-audit** case with our own assembler.
///
/// Its source is a complete listing our disassembler wrote, so it needs no
/// framing. `asl` serves nine CPUs, so the dialect alone cannot pick the
/// assembler — the pair does.
///
/// `None` for a case this replay cannot drive, which must read as "not
/// replayable" rather than as a pass, or the corpus could grow facts nothing
/// ever checks.
pub fn assemble_form(cpu: &str, dialect: &str, source: &str) -> Option<Result<Vec<u8>, String>> {
    let result = match (dialect, cpu) {
        ("acme", _) => asm198x::assemble_acme(source),
        ("pasmo", _) => asm198x::assemble_pasmo(source),
        ("ca65", "65816") => asm198x::assemble_ca65_816(source),
        ("ca65", "huc6280") => asm198x::assemble_ca65_huc6280(source),
        ("rgbasm", _) => asm198x::assemble_rgbasm(source),
        ("lwasm", _) => asm198x::assemble_lwasm(source),
        ("vasm", _) => asm198x::assemble_vasm(source),
        ("asl", "8080") => asm198x::assemble_i8080(source),
        ("asl", "6800") => asm198x::assemble_m6800(source),
        ("asl", "1802") => asm198x::assemble_1802(source),
        ("asl", "8048") => asm198x::assemble_8048(source),
        ("asl", "8039") => asm198x::assemble_8039(source),
        ("asl", "SC/MP") => asm198x::assemble_scmp(source),
        ("asl", "F8") => asm198x::assemble_f8(source),
        ("asl", "2650") => asm198x::assemble_2650(source),
        ("asl", "TMS7000") => asm198x::assemble_tms7000(source),
        // Sweep CPUs: no per-form audit, so these appear only as sweep chunks.
        ("asl", "PDP-11") => asm198x::assemble_pdp11(source),
        ("asl", "TMS9900") => asm198x::assemble_tms9900(source),
        ("asl", "CP1610") => asm198x::assemble_cp1610(source),
        ("asl", "Z8000") => asm198x::assemble_z8000(source),
        ("asl", "Z8001") => asm198x::assemble_z8001(source),
        _ => return None,
    };
    Some(ours(result))
}

/// Assemble a **differential probe** with our own assembler.
///
/// A probe is a bare snippet, so each dialect frames it the way that dialect
/// requires — ACME insists on an origin before any code, for instance. Live
/// arbitration and replay both come through here, so the framing cannot differ
/// between recording a fact and checking it. Two copies of this, drifting by
/// one line, would make every replay lookup miss and leave the suite green
/// while checking nothing.
pub fn assemble_probe(dialect: &str, body: &str) -> Option<Result<Vec<u8>, String>> {
    let result = match dialect {
        // ACME requires `*=` before code or data, and the reference is given
        // the same $0000 origin.
        "acme" => asm198x::assemble_acme(&format!("* = $0000\n{body}")),
        "pasmo" => asm198x::assemble_pasmo(body),
        "sjasmplus" => asm198x::assemble_sjasmplus(body),
        "z80n" => asm198x::assemble_sjasmplus_next(body),
        "lwasm" => asm198x::assemble_lwasm(body),
        "vasm" => asm198x::assemble_vasm(body),
        "ca65-816" => asm198x::assemble_ca65_816(body),
        _ => return None,
    };
    Some(ours(result))
}

/// Format a probe body, with the **same framing** `assemble_probe` gives it.
///
/// The framing has to match or the invariant that uses this compares two
/// different sources: acme's probes are assembled with a `* = $0000` prepended,
/// and a formatter handed the bare body would fail for want of an origin rather
/// than for the reason under test.
pub fn format_probe(dialect: &str, body: &str) -> Option<Result<String, String>> {
    let result = match dialect {
        "acme" => asm198x::format_acme(&format!("* = $0000\n{body}")),
        "pasmo" => asm198x::format_pasmo(body),
        "sjasmplus" => asm198x::format_sjasmplus(body),
        "z80n" => asm198x::format_sjasmplus_next(body),
        "lwasm" => asm198x::format_lwasm(body),
        "vasm" => asm198x::format_vasm(body),
        "ca65-816" => asm198x::format_ca65_816(body),
        _ => return None,
    };
    Some(result.map_err(|e| e.to_string()))
}

/// Shared tail: bytes, or why we would not produce any.
fn ours(result: Result<asm198x::AssemblyResult, asm198x::AsmError>) -> Result<Vec<u8>, String> {
    result
        .map(|r| r.bytes)
        .map_err(|e| format!("we rejected the source: {e}"))
}

/// Lower-case hex, which is how every digest in the corpus is written.
///
/// sha2 0.11 returns a `hybrid_array::Array` rather than a `GenericArray`, and
/// that type implements no `LowerHex`, so `format!("{:x}", …)` no longer
/// compiles. `verdict_corpus::encode_hex` is the wrong replacement: it emits
/// **upper**-case, for the byte payloads a verdict carries. Swapping the case
/// of a recorded digest would change every `Verdict::id` and leave the corpus
/// mixed, so the case is part of the format rather than a detail.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// SHA-256 of some bytes, lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex_lower(h.finalize().as_slice())
}

/// The Code198x checkout the curriculum suite reads from.
///
/// Re-exported from `verdict_corpus` so recording, replay and the parity
/// figures all resolve the checkout the same way.
pub fn code_samples_root() -> Option<PathBuf> {
    verdict_corpus::curriculum::root()
}

/// The key for a curriculum verdict.
///
/// Curriculum source belongs to Code198x, so it is **not** copied into this
/// repo — which means a curriculum verdict cannot key on the source text the
/// way every other suite does. It keys on an identity instead: the path, the
/// build variant, and a digest of the file's contents.
///
/// The content digest is the load-bearing part. Key on the path alone and an
/// edited curriculum file would silently be checked against the reference bytes
/// for its *previous* contents — a green suite asserting something that stopped
/// being true. With the digest in the key, changed source simply has no
/// recorded fact yet, which is a coverage gap rather than a false pass.
///
/// The variant is in the key because one file can be built more than one way:
/// the Amiga path builds each unit both as a hunk executable and as a flat
/// binary. Without it, those two would key identically with different outcomes
/// and read as a conflict.
pub fn curriculum_key(relpath: &str, variant: &str, source: &str) -> String {
    format!("{relpath}#{variant}@{}", sha256_hex(source.as_bytes()))
}

/// Take a curriculum key apart again: (path, variant, source digest).
pub fn parse_curriculum_key(key: &str) -> Option<(&str, &str, &str)> {
    let (path, rest) = key.split_once('#')?;
    let (variant, digest) = rest.split_once('@')?;
    Some((path, variant, digest))
}

/// Assemble a curriculum file the way its variant is built.
pub fn assemble_curriculum(variant: &str, source: &str) -> Option<Result<Vec<u8>, String>> {
    let result = match variant {
        "acme" => asm198x::assemble_acme(source),
        "ca65-nes" => asm198x::assemble_ca65(source),
        "pasmonext" => asm198x::assemble_pasmonext(source),
        "sjasmplus" => asm198x::assemble_sjasmplus(source),
        "vasm-exe" => asm198x::assemble_vasm_exe(source),
        "vasm-bin" => asm198x::assemble_vasm(source),
        _ => return None,
    };
    Some(ours(result))
}

/// What a replay pass found.
#[derive(Debug, Default)]
pub struct ReplayReport {
    /// Facts checked against our assembler.
    pub checked: usize,
    /// Facts checked, per suite. A whole suite that stops being replayed —
    /// because its outcome shape changed, or its source moved — would otherwise
    /// hide behind the other suites' totals, which is exactly how the
    /// curriculum leg was silently skipped while it was being written.
    pub by_suite: BTreeMap<String, usize>,
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
    match Corpus::read(&path) {
        Ok(corpus) => replay_corpus(cpu, &corpus, report),
        Err(e) => report.failures.push(format!("{cpu}: {e}")),
    }
}

/// Replay a corpus that is already in hand, so the rules can be tested against
/// a constructed one rather than only against what happens to be committed.
pub fn replay_corpus(cpu: &str, corpus: &Corpus, report: &mut ReplayReport) {
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
            Resolution::Fact {
                outcome, verdict, ..
            } => {
                // Curriculum first: its outcome is a digest, not bytes, so the
                // byte guard below would skip every one of them.
                if verdict.suite == Suite::Curriculum {
                    replay_curriculum(cpu, verdict, &key.source, outcome, report);
                    continue;
                }
                let Some(reference) = outcome.bytes() else {
                    // A recorded rejection says the reference refused source we
                    // never claim to accept. Pairing that against our own
                    // behaviour is a separate question from byte identity.
                    continue;
                };
                let ours = match verdict.suite {
                    // A fuzz case's source is a full listing, like a form's —
                    // the difference is how it was generated, not how it is
                    // assembled.
                    // A fuzz case and a sweep chunk are both full listings,
                    // like a form's. What differs between the three suites is
                    // how the case was generated, never how it is checked.
                    Suite::Form | Suite::Fuzz | Suite::SweepChunk => {
                        assemble_form(&verdict.cpu, &verdict.dialect, &key.source)
                    }
                    Suite::Probe => assemble_probe(&verdict.dialect, &key.source),
                    // Handled above — its outcome is a digest, not bytes.
                    Suite::Curriculum => None,
                };
                let Some(ours) = ours else {
                    report.unreplayable += 1;
                    continue;
                };
                report.checked += 1;
                *report
                    .by_suite
                    .entry(format!("{:?}", verdict.suite))
                    .or_default() += 1;

                // A divergence is a *tracked* difference: the reference accepts
                // and we knowingly do not match. Agreement is therefore the
                // failure — the gap closed and its marker is now a lie.
                if let Outcome::Divergence { divergence, .. } = outcome {
                    if ours.as_deref() == Ok(reference.as_slice()) {
                        report.failures.push(format!(
                            "{cpu}: tracked divergence `{divergence}` now matches the \
                             reference — delete its marker so the ledger stays honest\n{}",
                            key.source
                        ));
                    }
                    continue;
                }

                match ours {
                    Ok(bytes) if bytes == reference => {}
                    Ok(bytes) => report.failures.push(format!(
                        "{cpu} [{}]: ours {:02X?} vs reference {:02X?} for source:\n{}",
                        verdict.dialect, bytes, reference, key.source
                    )),
                    Err(e) => report.failures.push(format!(
                        "{cpu} [{}]: {e}\nreference produced {reference:02X?} for source:\n{}",
                        verdict.dialect, key.source
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

/// Replay one curriculum verdict.
///
/// Unlike every other suite, this needs the *source* — it was never copied into
/// this repo — but still needs no reference assembler. A file that is absent, or
/// whose contents have changed since the fact was recorded, is not a failure:
/// there is simply no recorded fact for what is on disk now. That is a coverage
/// gap, counted as unreplayable, never a silent pass.
fn replay_curriculum(
    cpu: &str,
    verdict: &Verdict,
    key: &str,
    outcome: &Outcome,
    report: &mut ReplayReport,
) {
    let Outcome::Digest { digest } = outcome else {
        report.failures.push(format!(
            "{cpu}: curriculum verdict `{}` is not a digest",
            verdict.case
        ));
        return;
    };
    let (Some(root), Some((relpath, variant, source_digest))) =
        (code_samples_root(), parse_curriculum_key(key))
    else {
        report.unreplayable += 1;
        return;
    };
    let Ok(source) = std::fs::read_to_string(root.join(relpath)) else {
        report.unreplayable += 1;
        return;
    };
    if sha256_hex(source.as_bytes()) != source_digest {
        // The curriculum moved on. The recorded fact is about different source.
        report.unreplayable += 1;
        return;
    }
    let Some(ours) = assemble_curriculum(variant, &source) else {
        report.unreplayable += 1;
        return;
    };
    report.checked += 1;
    *report.by_suite.entry("Curriculum".to_string()).or_default() += 1;
    match ours {
        Ok(bytes) if &sha256_hex(&bytes) == digest => {}
        Ok(bytes) => report.failures.push(format!(
            "{cpu} [{variant}] {relpath}: our output digests {} , reference {digest}",
            sha256_hex(&bytes)
        )),
        Err(e) => report
            .failures
            .push(format!("{cpu} [{variant}] {relpath}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use verdict_corpus::{Record, encode_hex};

    /// A corpus of exactly one fact about the 8080, whose source our own
    /// assembler can read.
    fn one_fact(outcome: Outcome) -> Corpus {
        let verdict = Verdict {
            suite: Suite::Form,
            cpu: "8080".to_string(),
            dialect: "asl".to_string(),
            case: "mvi a,imm".to_string(),
            source: "\tcpu 8080\n\torg 00000H\n\tmvi a,012H\n".to_string(),
            arbiter: Arbiter {
                tool: "asl".to_string(),
                identity: "test".to_string(),
                digest: "test".to_string(),
            },
            outcome,
        };
        let line = verdict_corpus::to_line(&Record::Verdict(Box::new(verdict))).expect("line");
        Corpus::parse(&line).expect("parse")
    }

    fn replay(outcome: Outcome) -> ReplayReport {
        let mut report = ReplayReport::default();
        replay_corpus("8080", &one_fact(outcome), &mut report);
        report
    }

    /// The ordinary case: the reference's bytes are ours too.
    #[test]
    fn a_fact_we_still_match_replays_clean() {
        let report = replay(Outcome::Bytes {
            hex: encode_hex(&[0x3E, 0x12]),
        });
        assert_eq!(report.checked, 1);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
    }

    /// The regression this whole net exists to catch, proven to fail rather
    /// than assumed to.
    #[test]
    fn a_fact_we_no_longer_match_fails() {
        let report = replay(Outcome::Bytes {
            hex: encode_hex(&[0x3E, 0x99]),
        });
        assert_eq!(report.checked, 1);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(
            report.failures[0].contains("3E, 99"),
            "{:?}",
            report.failures
        );
    }

    /// A tracked divergence is a claim that we *do not* match. While that holds,
    /// replay is quiet.
    #[test]
    fn a_divergence_that_still_diverges_replays_clean() {
        let report = replay(Outcome::Divergence {
            divergence: "issue-99".to_string(),
            hex: encode_hex(&[0x3E, 0x99]),
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
    }

    /// And when the gap closes, the marker becomes a lie — so replay fails,
    /// naming the divergence, rather than letting a stale ledger stand.
    #[test]
    fn a_divergence_that_now_matches_fails() {
        let report = replay(Outcome::Divergence {
            divergence: "issue-99".to_string(),
            hex: encode_hex(&[0x3E, 0x12]),
        });
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(
            report.failures[0].contains("issue-99") && report.failures[0].contains("delete"),
            "{:?}",
            report.failures
        );
    }

    /// A CPU no replay can drive counts as unreplayable, never as a pass.
    #[test]
    fn a_cpu_we_cannot_drive_is_not_a_silent_pass() {
        let mut report = ReplayReport::default();
        let corpus = one_fact(Outcome::Bytes {
            hex: encode_hex(&[0x3E, 0x12]),
        });
        // Same corpus, but asked for under a CPU whose dialect pair is unknown.
        let rewritten = corpus
            .verdicts()
            .map(|v| {
                let mut v = v.clone();
                v.dialect = "no-such-dialect".to_string();
                Record::Verdict(Box::new(v))
            })
            .collect::<Vec<_>>();
        let text = rewritten
            .iter()
            .map(|r| verdict_corpus::to_line(r).expect("line"))
            .collect::<Vec<_>>()
            .join("\n");
        replay_corpus("8080", &Corpus::parse(&text).expect("parse"), &mut report);
        assert_eq!(report.checked, 0);
        assert_eq!(report.unreplayable, 1);
        assert!(report.failures.is_empty());
    }
}
