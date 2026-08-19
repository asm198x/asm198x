//! The byte-identical guarantee, checked without a single reference assembler.
//!
//! Everything else that proves this claim shells out to `acme`, `ca65`, `asl`,
//! `pasmo`, `sjasmplus`, `vasm`, `lwasm` or `rgbasm`, and is therefore
//! `#[ignore]`d and runs on one machine. This file is **not** ignored. It reads
//! the committed corpus and, for every recorded fact, assembles the *same
//! source text* with our own assembler and compares against what the reference
//! actually produced.
//!
//! So the claim it enforces is the real one — *given this source, the reference
//! produced these bytes, and so do we* — on any machine, in any pull request,
//! for a contributor who has installed nothing.
//!
//! Growing the corpus is the live suites' job (`cargo test -- --ignored` with
//! the tools present). This only ever reads it.

mod support;

use support::verdicts::{ReplayReport, recorded_cpus, replay_cpu};

/// Replay every committed verdict.
#[test]
fn committed_verdicts_replay_without_the_reference_tools() {
    let cpus = recorded_cpus();
    let mut report = ReplayReport::default();
    for cpu in &cpus {
        replay_cpu(cpu, &mut report);
    }

    eprintln!(
        "replayed {} recorded verdict(s) across {} CPU(s), {} not replayable",
        report.checked,
        cpus.len(),
        report.unreplayable,
    );
    for (suite, n) in &report.by_suite {
        eprintln!("  {suite}: {n}");
    }

    // An alarm means two runs of the same tool version disagreed about the same
    // source. Recency cannot settle that; a person has to, with a supersede
    // record. Failing here is the only honest response.
    assert!(
        report.alarms.is_empty(),
        "{} unresolved verdict conflict(s) — adjudicate with a supersede record:\n  {}",
        report.alarms.len(),
        report.alarms.join("\n  "),
    );

    assert!(
        report.failures.is_empty(),
        "{} recorded verdict(s) no longer match our output:\n  {}",
        report.failures.len(),
        report.failures.join("\n  "),
    );

    // A corpus that quietly emptied — or a key derivation that quietly changed,
    // so every lookup misses — would leave this suite green while checking
    // nothing. That is the failure mode the whole net exists to prevent, so it
    // is worth an assertion of its own.
    assert!(
        report.checked > 0,
        "no verdicts replayed at all: the corpus is empty, unreadable, or no \
         longer keyed the way it was recorded"
    );
}
