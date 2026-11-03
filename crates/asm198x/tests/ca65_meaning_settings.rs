//! The features that change what a ca65 line means: `force_range`, which
//! keeps data and immediates to the bytes they take; `org_per_seg`, which
//! gives each segment its own `.org`; `long_jsr_jmp_rts`, which lets the
//! 65816's `jsr` and `jmp` take a long address, and makes `rts` in a far
//! `.proc` under `.smart` an `rtl`; and `merge_scopes`, which lets a named
//! `.scope` open again. The deprecated `addrsize` is accepted with a
//! warning.
//!
//! `fixtures/ca65-meaning-settings.json` holds programs measured on ca65
//! V2.19 and ld65 under a flat link. The live test records what ca65 and
//! ld65 made of each; the other replays every one through the walk.

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-meaning-settings.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-meaning-settings.json")).expect("fixture")
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_meaning_setting_programs_record_their_results() {
    support::linked::record(facts(), FIXTURE, "ca65-meaning-settings");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_meaning_setting_program_matches_ca65() {
    assert_eq!(support::linked::replay(&facts()), 110);
}

/// ca65 accepts the deprecated `addrsize` feature, on or off and among
/// others, and warns that it is deprecated.
#[test]
fn the_addrsize_feature_is_deprecated() {
    for program in [
        ".feature addrsize\n",
        ".feature addrsize -\n",
        ".feature force_range, addrsize\n.byte 300\n",
    ] {
        let result = asm198x::assemble_ca65(program).expect(program);
        let messages: Vec<&str> = result.warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages, ["Deprecated feature `addrsize`"], "{program}");
    }
    let result = asm198x::assemble_ca65(".feature force_range\n").expect("accepted");
    assert!(result.warnings.is_empty());
}
