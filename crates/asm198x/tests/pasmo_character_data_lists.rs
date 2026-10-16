//! Native character literals must remain intact when splitting data lists.

#[path = "support/scratch.rs"]
mod scratch;

use serde_json::Value;
use std::process::Command;

fn fixtures() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/pasmo-character-data-lists.json"))
        .expect("native facts")["cases"]
        .as_array()
        .expect("cases")
        .clone()
}

fn dialects(case: &Value) -> Vec<&'static str> {
    if case["dialect"] == "pasmo" {
        vec!["pasmo", "pasmonext"]
    } else {
        vec!["sjasmplus"]
    }
}

fn assemble(dialect: &str, source: &str) -> Result<asm198x::AssemblyResult, asm198x::AsmError> {
    match dialect {
        "pasmo" => asm198x::assemble_pasmo(source),
        "pasmonext" => asm198x::assemble_pasmonext(source),
        "sjasmplus" => asm198x::assemble_sjasmplus(source),
        _ => unreachable!("fixture dialect"),
    }
}

fn format(dialect: &str, source: &str) -> Result<String, asm198x::AsmError> {
    match dialect {
        "pasmo" | "pasmonext" => asm198x::format_pasmo(source),
        "sjasmplus" => asm198x::format_sjasmplus(source),
        _ => unreachable!("fixture dialect"),
    }
}

fn bytes(case: &Value) -> Vec<u8> {
    let hex = case["hex"].as_str().expect("native bytes");
    assert_eq!(hex.len() % 2, 0);
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("native hex"))
        .collect()
}

#[test]
fn character_data_lists_match_both_native_assemblers() {
    let cases = fixtures();
    assert_eq!(cases.iter().filter(|case| case["exit"] == 0).count(), 17);
    for case in cases {
        let source = case["source"].as_str().expect("source");
        for dialect in dialects(&case) {
            if case["exit"] == 0 {
                let result = assemble(dialect, source).expect("native-valid quoted data");
                assert_eq!(result.bytes, bytes(&case), "{}", case["name"]);
            } else {
                assert!(assemble(dialect, source).is_err(), "{}", case["name"]);
            }
        }
    }
}

#[test]
fn formatting_preserves_data_lists_and_native_bytes() {
    for case in fixtures().into_iter().filter(|case| case["exit"] == 0) {
        let source = case["source"].as_str().expect("source");
        for dialect in dialects(&case) {
            let formatted = format(dialect, source).expect("format native-valid source");
            assert_eq!(
                format(dialect, &formatted).expect("format again"),
                formatted
            );
            assert_eq!(
                assemble(dialect, &formatted)
                    .expect("formatted assembly")
                    .bytes,
                bytes(&case),
                "{} / {dialect}",
                case["name"]
            );
        }
    }
}

#[test]
fn cli_assembly_and_formatting_preserve_character_data_lists() {
    for case in fixtures().into_iter().filter(|case| case["exit"] == 0) {
        let source = case["source"].as_str().expect("source");
        for dialect in dialects(&case) {
            for mode in ["human", "json"] {
                let dir = scratch::dir("character-data-cli");
                std::fs::write(dir.join("input.asm"), source).expect("source");
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
                    .expect("assemble character data");
                assert!(result.status.success(), "{dialect}/{mode}: {result:?}");
                assert_eq!(
                    std::fs::read(dir.join("image.bin")).expect("image"),
                    bytes(&case)
                );
                if mode == "json" {
                    let result: Value =
                        serde_json::from_slice(&result.stdout).expect("structured result");
                    assert!(result.is_object(), "{result}");
                }
                let formatted = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                    .current_dir(&dir)
                    .args([
                        "fmt",
                        "--dialect",
                        dialect,
                        "input.asm",
                        "-o",
                        "formatted.asm",
                    ])
                    .output()
                    .expect("format character data");
                assert!(formatted.status.success(), "{formatted:?}");
                let text =
                    std::fs::read_to_string(dir.join("formatted.asm")).expect("formatted source");
                assert_eq!(
                    assemble(dialect, &text).expect("formatted assembly").bytes,
                    bytes(&case)
                );
                assert_eq!(
                    std::fs::read_to_string(dir.join("input.asm")).expect("preserved source"),
                    source
                );
            }
        }
    }
}

#[test]
fn malformed_data_lists_preserve_previous_images_and_source() {
    let dir = scratch::dir("character-data-invalid");
    for case in fixtures().into_iter().filter(|case| case["exit"] != 0) {
        let source = case["source"].as_str().expect("source");
        std::fs::write(dir.join("input.asm"), source).expect("source");
        for dialect in dialects(&case) {
            for mode in ["human", "json"] {
                std::fs::write(dir.join("image.bin"), b"previous image").expect("previous image");
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
                    .expect("invalid assembly");
                assert!(!result.status.success(), "{result:?}");
                if mode == "json" {
                    let diagnostics: Value =
                        serde_json::from_slice(&result.stdout).expect("diagnostics");
                    assert_eq!(diagnostics[0]["span"]["line"], 2);
                } else {
                    assert!(String::from_utf8_lossy(&result.stderr).contains("error:"));
                }
                assert_eq!(
                    std::fs::read(dir.join("image.bin")).expect("preserved image"),
                    b"previous image"
                );
                assert_eq!(
                    std::fs::read_to_string(dir.join("input.asm")).expect("preserved source"),
                    source
                );
            }
        }
    }
}

#[test]
fn conversion_preserves_native_data_bytes() {
    for case in fixtures().into_iter().filter(|case| {
        case["dialect"] == "pasmo" && case["exit"] == 0 && case["name"] != "semicolon"
    }) {
        let source = case["source"].as_str().expect("source");
        for dialect in dialects(&case) {
            let conversion =
                asm198x::convert(dialect, "sjasmplus", source).expect("verified conversion");
            assert_eq!(
                asm198x::assemble_sjasmplus(&conversion.output)
                    .expect("converted assembly")
                    .bytes,
                bytes(&case),
                "{} / {dialect}",
                case["name"]
            );
        }
    }
}
