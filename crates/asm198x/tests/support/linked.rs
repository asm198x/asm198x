//! Programs measured on ca65 and ld65 under a link config of their own,
//! rather than under `.org`, which makes labels constants. A fixture holds
//! the config and the programs; recording runs the reference tools on each
//! and keeps what they made, and replaying checks each through the walk.

// Every test binary that includes `support` compiles its own copy; only
// the linked suites use this one.
#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::process::Command;

fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// What ld65 or ca65 said first, without the file and line, which ld65
/// does not always give, and with the scratch folder hidden.
fn first_error(stderr: &[u8], scratch: &Path) -> String {
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

/// Runs ca65 and ld65 on each program in `facts` and records what they
/// made into the fixture at `path`, relative to this crate.
pub fn record(mut facts: serde_json::Value, path: &str, scratch: &str) {
    assert!(have("ca65") && have("ld65"), "ca65 and ld65 are required");
    let tmp = super::scratch::dir(scratch);
    let cfg = tmp.join("link.cfg");
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
    fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join(path), text).expect("record");
}

/// Checks that every program in `facts` gives what ca65 and ld65 gave: the
/// same bytes, or a refusal. Returns how many there were.
pub fn replay(facts: &serde_json::Value) -> usize {
    let cfg = facts["config"].as_str().expect("config");
    let cases = facts["cases"].as_array().expect("cases");
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
    cases.len()
}
