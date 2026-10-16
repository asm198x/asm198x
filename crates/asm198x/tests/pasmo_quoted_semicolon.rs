//! Quoted data must survive comment stripping in both Pasmo frontends.

#[path = "support/scratch.rs"]
mod scratch;

use serde_json::Value;
use std::process::Command;

fn fixtures() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/pasmo-quoted-semicolon.json"))
        .expect("native facts")["cases"]
        .as_array()
        .expect("cases")
        .clone()
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
fn quoted_semicolons_match_native_bytes_in_both_frontends() {
    let cases = fixtures();
    assert_eq!(cases.iter().filter(|case| case["exit"] == 0).count(), 7);
    for case in cases {
        let source = case["source"].as_str().expect("source");
        for assemble in [asm198x::assemble_pasmo, asm198x::assemble_pasmonext] {
            if case["exit"] == 0 {
                let result = assemble(source).expect("native-valid quoted data");
                assert_eq!(result.bytes, bytes(&case), "{}", case["name"]);
            } else {
                assert!(assemble(source).is_err(), "{}", case["name"]);
            }
        }
    }
}

#[test]
fn formatting_preserves_quoted_data_comments_and_native_bytes() {
    for case in fixtures().into_iter().filter(|case| case["exit"] == 0) {
        let source = case["source"].as_str().expect("source");
        let formatted = asm198x::format_pasmo(source).expect("format nested source");
        assert!(formatted.contains(';'));
        assert_eq!(
            asm198x::format_pasmo(&formatted).expect("format again"),
            formatted
        );
        assert_eq!(
            asm198x::assemble_pasmo(&formatted)
                .expect("formatted source")
                .bytes,
            bytes(&case)
        );
    }
}

#[test]
fn cli_assembly_and_formatting_preserve_quoted_data() {
    for case in fixtures().into_iter().filter(|case| case["exit"] == 0) {
        let source = case["source"].as_str().expect("source");
        for dialect in ["pasmo", "pasmonext"] {
            for mode in ["human", "json"] {
                let dir = scratch::dir("pasmo-quoted-cli");
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
                    .expect("assemble nested macro");
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
                    .expect("format nested macro");
                assert!(formatted.status.success(), "{formatted:?}");
                let text =
                    std::fs::read_to_string(dir.join("formatted.asm")).expect("formatted source");
                assert_eq!(
                    asm198x::assemble_pasmo(&text)
                        .expect("formatted assembly")
                        .bytes,
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
fn invalid_quoted_data_preserves_previous_image_and_source() {
    let dir = scratch::dir("pasmo-quoted-invalid");
    let source = " org 8000h\n db \"rept;endm\n";
    std::fs::write(dir.join("input.asm"), source).expect("source");
    for dialect in ["pasmo", "pasmonext"] {
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
