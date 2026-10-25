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

    assert!(
        report.failures.is_empty(),
        "{} recorded verdict(s) no longer match our output:\n  {}",
        report.failures.len(),
        report.failures.join("\n  "),
    );

    // CI names the curriculum checkout, at the pin the verdicts were recorded
    // from. There, every pattern verdict must replay, and every pattern
    // source in the checkout must have one: a source with no verdict is
    // unchecked, and would otherwise read exactly like one that passed. A
    // developer's sibling checkout may have moved on, so there the gaps are
    // only reported.
    if let Some(root) = support::verdicts::code_samples_root() {
        let missing: Vec<String> = support::patterns::all_sources(&root)
            .iter()
            .filter_map(|p| p.strip_prefix(&root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .filter(|rel| !report.patterns_reached.contains(rel))
            .collect();
        eprintln!(
            "pattern sources: {} replayed, {} with no verdict that replays, {} verdict(s) not reached",
            report.patterns_reached.len(),
            missing.len(),
            report.patterns_unreached.len(),
        );
        if std::env::var_os("ASM198X_CODE_SAMPLES").is_some() {
            assert!(
                report.patterns_unreached.is_empty() && missing.is_empty(),
                "with ASM198X_CODE_SAMPLES set, every pattern verdict must replay and every \
                 pattern source must have one.\nnot reached: {:#?}\nno verdict: {missing:#?}",
                report.patterns_unreached,
            );
        }
    }

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

/// The pattern table agrees with the curriculum checkout: every machine with
/// pattern sources has an entry, and every dialect its Makefiles name is the
/// entry's. Without a checkout there is nothing to hold it to, which CI rules
/// out by naming one.
#[test]
fn the_pattern_table_matches_the_curriculum() {
    let Some(root) = support::verdicts::code_samples_root() else {
        assert!(
            std::env::var_os("ASM198X_CODE_SAMPLES").is_none(),
            "ASM198X_CODE_SAMPLES is set but names no checkout"
        );
        eprintln!("SKIP: no Code198x checkout");
        return;
    };
    let problems = verdict_corpus::patterns::table_problems(&root.join("code-samples"));
    assert!(
        problems.is_empty(),
        "the pattern table disagrees with the checkout:\n  {}",
        problems.join("\n  ")
    );
}
