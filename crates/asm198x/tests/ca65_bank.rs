//! ca65's `.bank`: the `bank` attribute of the memory area a label's
//! segment runs in, which ld65 works out at the link.
//!
//! `fixtures/ca65-bank.json` holds programs that each check one rule,
//! measured on ca65 V2.19 and ld65 under a config of its own, since the
//! flat config the other verdicts use has no banks. The live test records
//! what ca65 and ld65 made of each; the other replays every one through
//! the walk with no reference tools present.

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-bank.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-bank.json")).expect("fixture")
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_bank_programs_record_their_results() {
    support::linked::record(facts(), FIXTURE, "ca65-bank");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_bank_program_matches_ca65() {
    assert_eq!(support::linked::replay(&facts()), 47);
}

/// The name the walk gives a `.bank` until the link never reaches an
/// output.
#[test]
fn the_name_bank_stands_for_reaches_no_output() {
    let facts = facts();
    let cfg = facts["config"].as_str().expect("config");
    let source = "Z9 = .bank(l1)\n.dword .bank(l1), Z9\n.segment \"B1\"\nl1: rts\n";
    let result = asm198x::assemble_ca65_files_with_config(
        source,
        "form.s",
        &asm198x::source::MemoryLoader::new(),
        cfg,
    )
    .expect("assembles");
    assert_eq!(
        verdict_corpus::encode_hex(&result.bytes),
        "452301004523010060"
    );
    let symbols = serde_json::to_string(&result.symbols).expect("symbols");
    assert!(!symbols.contains('\u{1}'), "{symbols}");
    assert!(!symbols.contains("bank0"), "{symbols}");
}
