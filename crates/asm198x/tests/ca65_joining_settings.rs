//! The settings that decide where a ca65 line ends: `.linecont`, which
//! joins a line ending in a backslash to the next, and the `c_comments`
//! feature, whose comments may run across lines. Lang198x reads the rest of
//! a file again when either changes; the walk sees the lines it made.
//!
//! `fixtures/ca65-joining-settings.json` holds programs measured on ca65
//! V2.19 and ld65 under a flat link. The live test records what ca65 and
//! ld65 made of each; the other replays every one through the walk.

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-joining-settings.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-joining-settings.json")).expect("fixture")
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_joining_setting_programs_record_their_results() {
    support::linked::record(facts(), FIXTURE, "ca65-joining-settings");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_joining_setting_program_matches_ca65() {
    assert_eq!(support::linked::replay(&facts()), 44);
}
