//! How ca65 sizes an address it links: the size a `.proc` declares its name
//! with, and the range check on a one-byte address operand.
//!
//! `fixtures/ca65-address-sizes.json` holds programs that each check one
//! rule, measured on ca65 V2.19 and ld65 under a flat link without `.org`,
//! since `.org` makes labels constants and these rules are about labels
//! the linker places. The live test records what ca65 and ld65 made of
//! each; the other replays every one through the walk.

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-address-sizes.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-address-sizes.json")).expect("fixture")
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_address_size_programs_record_their_results() {
    support::linked::record(facts(), FIXTURE, "ca65-address-sizes");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_address_size_program_matches_ca65() {
    assert_eq!(support::linked::replay(&facts()), 51);
}
