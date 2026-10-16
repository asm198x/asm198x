//! Quoted comment delimiters survive native-compatible SjASMPlus scanning.

#[path = "support/scratch.rs"]
mod scratch;

use serde_json::Value;
use std::process::Command;

fn fixtures() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/sjasmplus-quoted-comments.json"))
        .expect("native facts")["cases"]
        .as_array()
        .expect("cases")
        .clone()
}

fn dialects(_case: &Value) -> [&'static str; 2] {
    ["sjasmplus", "sjasmplus-next"]
}

fn assemble(dialect: &str, source: &str) -> Result<asm198x::AssemblyResult, asm198x::AsmError> {
    match dialect {
        "sjasmplus" => asm198x::assemble_sjasmplus(source),
        "sjasmplus-next" => asm198x::assemble_sjasmplus_next(source),
        _ => unreachable!("fixture dialect"),
    }
}

fn format(dialect: &str, source: &str) -> Result<String, asm198x::AsmError> {
    match dialect {
        "sjasmplus" => asm198x::format_sjasmplus(source),
        "sjasmplus-next" => asm198x::format_sjasmplus_next(source),
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
fn quoted_comments_match_native_bytes_in_both_cpu_entry_points() {
    let cases = fixtures();
    assert_eq!(cases.iter().filter(|case| case["exit"] == 0).count(), 12);
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
fn formatting_preserves_quoted_data_and_native_bytes() {
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
fn cli_preserves_quoted_data_in_human_and_json_modes() {
    for case in fixtures().into_iter().filter(|case| case["exit"] == 0) {
        let source = case["source"].as_str().expect("source");
        for dialect in dialects(&case) {
            for mode in ["human", "json"] {
                let dir = scratch::dir("sjasm-comments-cli");
                std::fs::write(dir.join("input.asm"), source).expect("source");
                let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                    .current_dir(&dir)
                    .args([
                        "--dialect",
                        "sjasmplus",
                        "--cpu",
                        if dialect == "sjasmplus" {
                            "z80"
                        } else {
                            "z80n"
                        },
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
                        "sjasmplus",
                        "--cpu",
                        if dialect == "sjasmplus" {
                            "z80"
                        } else {
                            "z80n"
                        },
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
fn malformed_quoted_data_preserves_previous_images_and_source() {
    let dir = scratch::dir("sjasm-comments-invalid");
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
                        "sjasmplus",
                        "--cpu",
                        if dialect == "sjasmplus" {
                            "z80"
                        } else {
                            "z80n"
                        },
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
fn formatting_retains_actual_comments_after_quoted_data() {
    for (name, comment) in [
        ("real_semicolon", "; db 99, \"unterminated"),
        ("real_slashes", "// db 99, \"unterminated"),
        ("mixed", "; actual comment"),
    ] {
        let case = fixtures()
            .into_iter()
            .find(|case| case["name"] == name)
            .expect("comment fixture");
        for dialect in dialects(&case) {
            let formatted =
                format(dialect, case["source"].as_str().expect("source")).expect("format");
            assert!(formatted.contains(comment), "{formatted}");
        }
    }
}

#[test]
fn escape_boundaries_emit_native_bytes_or_fail_explicitly_without_writing() {
    let facts: Value =
        serde_json::from_str(include_str!("fixtures/sjasmplus-escape-boundaries.json"))
            .expect("escape facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 95);
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["unsupported"] == true)
            .count(),
        23
    );
    let dir = scratch::dir("sjasm-escape-boundaries");
    for case in cases {
        let source = case["source"].as_str().expect("source");
        std::fs::write(dir.join("input.asm"), source).expect("source");
        for dialect in dialects(case) {
            let formatted = format(dialect, source).expect("preserve escaped source");
            assert_eq!(
                formatted.lines().nth(1).expect("formatted data").trim(),
                source.lines().nth(1).expect("source data").trim()
            );
            if case["unsupported"] == true {
                let error =
                    assemble(dialect, source).expect_err("escapes must not emit undecoded bytes");
                assert_eq!(error.code, asm198x::Code::UnsupportedFeature);
                assert!(error.message.contains("escape"), "{error:?}");
                for mode in ["human", "json"] {
                    std::fs::write(dir.join("image.bin"), b"previous image").expect("old image");
                    let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                        .current_dir(&dir)
                        .args([
                            "--dialect",
                            "sjasmplus",
                            "--cpu",
                            if dialect == "sjasmplus" {
                                "z80"
                            } else {
                                "z80n"
                            },
                            "--message-format",
                            mode,
                            "input.asm",
                            "-o",
                            "image.bin",
                        ])
                        .output()
                        .expect("escape refusal");
                    assert!(!result.status.success(), "{result:?}");
                    if mode == "json" {
                        let diagnostics: Value =
                            serde_json::from_slice(&result.stdout).expect("diagnostics");
                        assert_eq!(diagnostics[0]["code"], "UnsupportedFeature");
                        assert_eq!(diagnostics[0]["span"]["line"], 2);
                    } else {
                        assert!(String::from_utf8_lossy(&result.stderr).contains("escape"));
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
            } else {
                assert_eq!(
                    assemble(dialect, source).expect("literal backslash").bytes,
                    bytes(case)
                );
                let formatted = format(dialect, source).expect("format literal backslash");
                assert_eq!(
                    assemble(dialect, &formatted)
                        .expect("formatted literal backslash")
                        .bytes,
                    bytes(case)
                );
            }
        }
    }
}

#[test]
fn decoded_escapes_fail_explicitly_across_data_directive_spellings() {
    let facts: Value =
        serde_json::from_str(include_str!("fixtures/sjasmplus-escape-boundaries.json"))
            .expect("escape facts");
    let cases = facts["directives"].as_array().expect("directive probes");
    assert_eq!(cases.len(), 51);
    for case in cases {
        for dialect in dialects(case) {
            let error = assemble(dialect, case["source"].as_str().expect("source"))
                .expect_err("no undecoded data");
            assert_eq!(
                error.code,
                asm198x::Code::UnsupportedFeature,
                "{}: {error:?}",
                case["word"]
            );
        }
    }
}

#[test]
#[ignore = "needs sjasmplus on PATH"]
fn escaped_source_formatting_preserves_native_bytes() {
    let facts: Value =
        serde_json::from_str(include_str!("fixtures/sjasmplus-escape-boundaries.json"))
            .expect("escape facts");
    let dir = scratch::dir("sjasm-escaped-format-native");
    for case in facts["cases"].as_array().expect("cases") {
        let source = case["source"].as_str().expect("source");
        let formatted = format("sjasmplus", source).expect("format escaped source");
        assert_eq!(
            format("sjasmplus-next", source).expect("format for Next"),
            formatted
        );
        std::fs::write(dir.join("formatted.asm"), formatted).expect("formatted source");
        if dir.join("image.bin").exists() {
            std::fs::remove_file(dir.join("image.bin")).expect("remove previous native image");
        }
        let result = Command::new("sjasmplus")
            .current_dir(&dir)
            .args(["--raw=image.bin", "formatted.asm"])
            .output()
            .expect("native assembler");
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            std::fs::read(dir.join("image.bin")).expect("fresh native image"),
            bytes(case),
            "{}",
            case["escape"]
        );
    }
}
