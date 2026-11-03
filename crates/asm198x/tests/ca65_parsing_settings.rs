//! The features that change how ca65 parses a line: `labels_without_colons`,
//! where a name in the first column is a label; `bracket_as_indirect`, where
//! square brackets mark indirection; `pc_assignment`, where `* = value` sets
//! the program counter; and `ubiquitous_idents`, where a macro may take an
//! instruction's name.
//!
//! `fixtures/ca65-parsing-settings.json` holds programs measured on ca65
//! V2.19 and ld65 under a flat link. The live test records what ca65 and
//! ld65 made of each; the other replays every one through the walk.

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-parsing-settings.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-parsing-settings.json")).expect("fixture")
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_parsing_setting_programs_record_their_results() {
    support::linked::record(facts(), FIXTURE, "ca65-parsing-settings");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_parsing_setting_program_matches_ca65() {
    assert_eq!(support::linked::replay(&facts()), 48);
}
