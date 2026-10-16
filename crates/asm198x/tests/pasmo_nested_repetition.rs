//! Pasmo shares ENDM between repetitions and macros; native bytes set the boundary.

#[path = "support/scratch.rs"]
mod scratch;

use serde_json::Value;
use std::process::Command;

fn fixtures() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/pasmo-nested-repetition.json"))
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
fn nested_repetitions_match_native_pasmo_bytes_in_both_frontends() {
    let cases = fixtures();
    assert_eq!(cases.iter().filter(|case| case["exit"] == 0).count(), 6);
    for case in cases {
        let source = case["source"].as_str().expect("source");
        for assemble in [asm198x::assemble_pasmo, asm198x::assemble_pasmonext] {
            if case["exit"] == 0 {
                let result = assemble(source).expect("native-valid nesting");
                assert_eq!(result.bytes, bytes(&case), "{}", case["name"]);
            } else {
                assert!(assemble(source).is_err(), "{}", case["name"]);
            }
        }
    }
}

#[test]
fn formatting_preserves_unexpanded_nested_macros_and_native_bytes() {
    for case in fixtures().into_iter().filter(|case| case["exit"] == 0) {
        let source = case["source"].as_str().expect("source");
        let formatted = asm198x::format_pasmo(source).expect("format nested source");
        assert!(formatted.to_ascii_lowercase().contains("macro"));
        assert!(formatted.to_ascii_lowercase().contains("rept"));
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
fn cli_assembly_and_formatting_preserve_nested_macro_boundaries() {
    for case in fixtures().into_iter().filter(|case| case["exit"] == 0) {
        let source = case["source"].as_str().expect("source");
        for dialect in ["pasmo", "pasmonext"] {
            for mode in ["human", "json"] {
                let dir = scratch::dir("pasmo-nesting-cli");
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
fn errors_inside_nested_repetitions_keep_macro_provenance() {
    let case = fixtures()
        .into_iter()
        .find(|case| case["name"] == "body_error")
        .expect("error fixture");
    let error = asm198x::assemble_pasmo(case["source"].as_str().expect("source"))
        .expect_err("invalid body");
    assert_eq!(error.line, 7, "{error:?}");
    let span = error.span.expect("source span");
    assert!(
        span.expansion_frames
            .iter()
            .any(|frame| frame.macro_name == "setv"
                && frame.defined_at.as_ref().is_some_and(|site| site.line == 2)
                && frame.invoked_at.line == 7),
        "{span:?}"
    );
}

#[test]
fn cli_nested_errors_keep_provenance_and_previous_images() {
    let case = fixtures()
        .into_iter()
        .find(|case| case["name"] == "body_error")
        .expect("error fixture");
    let source = case["source"].as_str().expect("source");
    for dialect in ["pasmo", "pasmonext"] {
        for mode in ["human", "json"] {
            let dir = scratch::dir("pasmo-nesting-error");
            std::fs::write(dir.join("input.asm"), source).expect("source");
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
                .expect("assemble invalid body");
            assert!(!result.status.success(), "{result:?}");
            assert_eq!(
                std::fs::read(dir.join("image.bin")).expect("previous image"),
                b"previous image"
            );
            assert_eq!(
                std::fs::read_to_string(dir.join("input.asm")).expect("preserved source"),
                source
            );
            if mode == "json" {
                assert!(result.stderr.is_empty(), "{result:?}");
                let errors: Value = serde_json::from_slice(&result.stdout).expect("diagnostics");
                assert_eq!(errors[0]["span"]["line"], 7, "{errors}");
                assert_eq!(errors[0]["span"]["path"], "input.asm", "{errors}");
                let frame = &errors[0]["span"]["expansion_frames"][0];
                assert_eq!(frame["macro_name"], "setv", "{errors}");
                assert_eq!(frame["defined_at"]["line"], 2, "{errors}");
                assert_eq!(frame["invoked_at"]["line"], 7, "{errors}");
            } else {
                assert!(result.stdout.is_empty(), "{result:?}");
                let error = String::from_utf8_lossy(&result.stderr);
                assert!(
                    error.contains("input.asm:7:")
                        && error.contains("defined at input.asm:2")
                        && error.contains("invoked at input.asm:7"),
                    "{error}"
                );
            }
        }
    }
}

#[test]
fn conversion_keeps_its_verification_gate_after_parsing_nested_macros() {
    let case = fixtures()
        .into_iter()
        .find(|case| case["name"] == "one_level")
        .expect("fixture");
    let source = case["source"].as_str().expect("source");
    let dir = scratch::dir("pasmo-nesting-convert");
    std::fs::write(dir.join("input.asm"), source).expect("source");
    std::fs::write(dir.join("out.asm"), b"previous source").expect("previous output");
    let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(&dir)
        .args([
            "convert",
            "--from",
            "pasmo",
            "--to",
            "sjasmplus",
            "input.asm",
            "-o",
            "out.asm",
        ])
        .output()
        .expect("convert nested source");
    assert!(
        !result.status.success(),
        "translation remains outside converter v1: {result:?}"
    );
    assert!(result.stdout.is_empty(), "{result:?}");
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("the converted output does not assemble under sjasmplus"),
        "{result:?}"
    );
    assert_eq!(
        std::fs::read(dir.join("out.asm")).expect("preserved output"),
        b"previous source"
    );
}
