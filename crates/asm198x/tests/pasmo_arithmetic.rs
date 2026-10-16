//! Pasmo arithmetic is checked against captured independent native bytes.

#[path = "support/scratch.rs"]
mod scratch;

use asm198x::{AsmError, AssemblyResult};
use serde_json::Value;
use std::process::Command;

fn facts() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/pasmo-arithmetic.json"))
        .expect("native facts")["cases"]
        .as_array()
        .expect("cases")
        .clone()
}

fn bytes(case: &Value) -> Vec<u8> {
    let hex = case["hex"].as_str().expect("native output");
    assert_eq!(hex.len() % 2, 0);
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("native byte"))
        .collect()
}

fn assemble(dialect: &str, source: &str) -> Result<AssemblyResult, AsmError> {
    match dialect {
        "pasmo" => asm198x::assemble_pasmo(source),
        "pasmonext" => asm198x::assemble_pasmonext(source),
        _ => unreachable!("test dialect"),
    }
}

#[test]
fn arithmetic_matches_native_word_results_and_refusals() {
    let cases = facts();
    assert_eq!(cases.len(), 194);
    for case in cases {
        for dialect in ["pasmo", "pasmonext"] {
            let result = assemble(dialect, case["source"].as_str().expect("source"));
            if case["exit"] == 0 {
                assert_eq!(
                    result.expect("native-valid arithmetic").bytes,
                    bytes(&case),
                    "{dialect}: {case}"
                );
            } else {
                assert!(
                    result.is_err(),
                    "native refusal accepted by {dialect}: {case}"
                );
            }
        }
    }
}

#[test]
fn formatted_arithmetic_is_idempotent_and_matches_native_bytes() {
    let cases: Vec<_> = facts().into_iter().filter(|c| c["exit"] == 0).collect();
    assert_eq!(cases.len(), 182);
    for case in cases {
        let source = case["source"].as_str().expect("source");
        let text = asm198x::format_pasmo(source).expect("format native source");
        assert_eq!(asm198x::format_pasmo(&text).expect("format again"), text);
        for dialect in ["pasmo", "pasmonext"] {
            assert_eq!(
                assemble(dialect, &text).expect("formatted assembly").bytes,
                bytes(&case),
                "{case}"
            );
        }
    }
}

#[test]
fn cli_arithmetic_matches_native_bytes_and_preserves_failed_outputs() {
    let dir = scratch::dir("pasmo-arithmetic-cli");
    let mut checked = 0;
    for case in facts() {
        // Library/formatter replay covers every captured expression. Exercise
        // CLI contracts on the four issue expressions, symbols and all errors.
        let name = case["name"].as_str().expect("name");
        if case["exit"] == 0
            && !name.starts_with("index_")
            && ![
                "dw_0",
                "dw_1",
                "dw_2",
                "dw_3",
                "equ_usage",
                "forward",
                "condition",
                "defs",
                "repeat",
            ]
            .contains(&name)
        {
            continue;
        }
        let source = case["source"].as_str().expect("source");
        std::fs::write(dir.join("input.asm"), source).expect("source");
        for dialect in ["pasmo", "pasmonext"] {
            for mode in ["human", "json"] {
                std::fs::write(dir.join("image.bin"), b"previous image").expect("image");
                checked += 1;
                let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                    .current_dir(&dir)
                    .args([
                        "--dialect",
                        dialect,
                        "--message-format",
                        mode,
                        "input.asm",
                        "-o",
                        "image.bin",
                    ])
                    .output()
                    .expect("CLI arithmetic");
                let image = std::fs::read(dir.join("image.bin")).expect("image");
                if case["exit"] == 0 {
                    assert!(result.status.success(), "{dialect}: {result:?}; {case}");
                    assert_eq!(image, bytes(&case));
                    if mode == "json" {
                        assert!(result.stderr.is_empty());
                        let json: Value = serde_json::from_slice(&result.stdout).expect("result");
                        assert_eq!(json["bytes"], serde_json::json!(image));
                        assert_eq!(json["diagnostics"], serde_json::json!([]));
                        if name == "equ_usage" {
                            assert_eq!(json["symbols"]["V"], 65535);
                        }
                    }
                } else {
                    assert!(!result.status.success(), "{result:?}");
                    assert_eq!(image, b"previous image");
                    if mode == "json" {
                        assert!(result.stderr.is_empty());
                        let json: Value =
                            serde_json::from_slice(&result.stdout).expect("diagnostics");
                        assert!(!json.as_array().expect("diagnostics array").is_empty());
                    }
                }
                assert_eq!(
                    std::fs::read_to_string(dir.join("input.asm")).expect("source"),
                    source
                );
            }
        }
    }
    assert_eq!(checked, 104, "CLI coverage must not silently drop cases");
}

#[test]
fn sjasmplus_keeps_its_independently_measured_wide_arithmetic() {
    let fact: Value =
        serde_json::from_str(include_str!("fixtures/pasmo-arithmetic-isolation.json"))
            .expect("native SjASMPlus fact");
    let source = fact["source"].as_str().expect("source");
    for result in [
        asm198x::assemble_sjasmplus(source),
        asm198x::assemble_sjasmplus_next(source),
    ] {
        assert_eq!(result.expect("SjASMPlus source").bytes, bytes(&fact));
    }
}

#[test]
#[ignore = "requires native PasmoNext 0.1.3"]
fn fresh_native_arithmetic_agrees_with_recorded_facts() {
    let dir = scratch::dir("pasmo-arithmetic-native");
    let cases = facts();
    assert_eq!(cases.len(), 194);
    for case in cases {
        std::fs::write(
            dir.join("input.asm"),
            case["source"].as_str().expect("source"),
        )
        .expect("source");
        let destination = dir.join("image.bin");
        if destination.exists() {
            std::fs::remove_file(&destination).expect("remove previous output");
        }
        let result = Command::new("pasmo")
            .current_dir(&dir)
            .args(["--bin", "input.asm", "image.bin"])
            .output()
            .expect("native PasmoNext");
        assert_eq!(
            result.status.success(),
            case["exit"] == 0,
            "{case}: {result:?}"
        );
        if result.status.success() {
            assert_eq!(
                std::fs::read(&destination).expect("native image"),
                bytes(&case),
                "{case}"
            );
        } else {
            assert!(!destination.exists(), "failed native output: {case}");
        }
    }
}
