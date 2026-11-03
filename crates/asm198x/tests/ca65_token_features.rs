//! The `.feature`s that change how ca65 reads a token: names that may hold
//! `@` or `$` or start with a dot, `_` in numbers, `$` as the address, and
//! the string and character features. Lang198x reads the lines after each
//! `.feature` accordingly; the walk sees the tokens it made.
//!
//! `fixtures/ca65-token-features.json` holds programs measured on ca65
//! V2.19 and ld65 under a flat link. The live test records what ca65 and
//! ld65 made of each; the other replays every one through the walk.

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-token-features.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-token-features.json")).expect("fixture")
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_token_feature_programs_record_their_results() {
    support::linked::record(facts(), FIXTURE, "ca65-token-features");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_token_feature_program_matches_ca65() {
    assert_eq!(support::linked::replay(&facts()), 85);
}
