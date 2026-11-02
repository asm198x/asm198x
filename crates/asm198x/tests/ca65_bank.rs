//! ca65's `.bank`: the `bank` attribute of the memory area a label's
//! segment runs in, which ld65 works out at the link.
//!
//! `fixtures/ca65-bank.json` holds programs that each check one rule,
//! measured on ca65 V2.19 and ld65 under a config of its own, since the
//! flat config the other verdicts use has no banks. The live test records
//! what ca65 and ld65 made of each; the other replays every one through
//! the walk with no reference tools present.

use std::fs;
use std::process::Command;

mod support;

const FIXTURE: &str = "tests/fixtures/ca65-bank.json";

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-bank.json")).expect("fixture")
}

fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// What ld65 or ca65 said first, without the file and line, which ld65
/// does not give for `.bank`, and with the scratch folder hidden.
fn first_error(stderr: &[u8], scratch: &std::path::Path) -> String {
    let text =
        String::from_utf8_lossy(stderr).replace(&format!("{}/", scratch.to_string_lossy()), "");
    let line = text
        .lines()
        .find(|line| line.contains("Error"))
        .unwrap_or_default();
    line.split_once("Error: ")
        .map_or(line, |(_, message)| message)
        .to_owned()
}

/// Runs ca65 and ld65 on each program and records what they made.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the results"]
fn the_bank_programs_record_their_results() {
    assert!(have("ca65") && have("ld65"), "ca65 and ld65 are required");
    let mut facts = facts();
    let tmp = support::scratch::dir("ca65-bank");
    let cfg = tmp.join("bank.cfg");
    fs::write(&cfg, facts["config"].as_str().expect("config")).expect("config");
    for case in facts["cases"].as_array_mut().expect("cases") {
        let program = format!("{}\n", case["program"].as_str().expect("program"));
        let (src, obj, bin) = (tmp.join("t.s"), tmp.join("t.o"), tmp.join("t.bin"));
        fs::write(&src, &program).expect("source");
        let _ = fs::remove_file(&bin);
        let assembled = Command::new("ca65")
            .arg(&src)
            .arg("-o")
            .arg(&obj)
            .output()
            .expect("ca65");
        let native = if assembled.status.success() {
            let linked = Command::new("ld65")
                .arg("-C")
                .arg(&cfg)
                .arg(&obj)
                .arg("-o")
                .arg(&bin)
                .output()
                .expect("ld65");
            if linked.status.success() {
                serde_json::json!({ "hex": verdict_corpus::encode_hex(&fs::read(&bin).expect("image")) })
            } else {
                serde_json::json!({ "refused": first_error(&linked.stderr, &tmp) })
            }
        } else {
            serde_json::json!({ "refused": first_error(&assembled.stderr, &tmp) })
        };
        case["native"] = native;
    }
    let text = serde_json::to_string_pretty(&facts).expect("json") + "\n";
    fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE),
        text,
    )
    .expect("record");
}

/// Every program gives what ca65 and ld65 gave: the same bytes, or a
/// refusal.
#[test]
fn every_bank_program_matches_ca65() {
    let facts = facts();
    let cfg = facts["config"].as_str().expect("config");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 47);
    for case in cases {
        let program = format!("{}\n", case["program"].as_str().expect("program"));
        let ours = asm198x::assemble_ca65_files_with_config(
            &program,
            "form.s",
            &asm198x::source::MemoryLoader::new(),
            cfg,
        );
        match (&case["native"]["hex"], ours) {
            (serde_json::Value::String(hex), Ok(result)) => {
                assert_eq!(&verdict_corpus::encode_hex(&result.bytes), hex, "{program}");
            }
            (serde_json::Value::String(_), Err(failure)) => {
                panic!("{program}: ca65 accepts it, we refuse: {}", failure.error);
            }
            (_, Ok(_)) => panic!("{program}: ca65 refuses it, we accept"),
            (_, Err(_)) => {}
        }
    }
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
