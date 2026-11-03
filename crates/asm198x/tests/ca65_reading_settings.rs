//! Settings that change how ca65 reads names: `.case off`, which reads
//! them in upper case, and `.localchar`, which chooses `@` or `?` to start
//! a cheap local. Lang198x reads the lines after each in the new mode; the
//! walk sees the names it made.
//!
//! `fixtures/ca65-reading-settings.json` holds programs measured on ca65
//! V2.19 and ld65 under a flat link, so labels are placed by the linker.
//! The live test records what ca65 and ld65 made of each; the other
//! replays every one through the walk.

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-reading-settings.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-reading-settings.json")).expect("fixture")
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_reading_setting_programs_record_their_results() {
    support::linked::record(facts(), FIXTURE, "ca65-reading-settings");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_reading_setting_program_matches_ca65() {
    assert_eq!(support::linked::replay(&facts()), 46);
}
