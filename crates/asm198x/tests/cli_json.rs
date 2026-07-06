//! U4: `--message-format=json` CLI mode (R3). The result and diagnostics are
//! machine-consumable on stdout; the human default is unchanged.

use std::path::PathBuf;
use std::process::Command;

use asm198x::{AssemblyResult, Diagnostic, Severity};

/// The built `asm198x` binary under test.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
}

/// Write `source` to a uniquely-named temp file and return its path (so parallel
/// tests never share an input).
fn temp_source(tag: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("asm198x-cli-json-{tag}.s"));
    std::fs::write(&path, source).expect("write temp source");
    path
}

#[test]
fn json_success_emits_result_with_bytes_and_symbols() {
    let src = temp_source("ok", "*=$8000\nstart:\n        lda #$05\n        rts\n");
    let out = bin()
        .args(["--cpu", "6502", "--message-format=json"])
        .arg(&src)
        .output()
        .expect("run asm198x");

    assert!(out.status.success(), "clean assemble should succeed");
    let result: AssemblyResult =
        serde_json::from_slice(&out.stdout).expect("stdout is an AssemblyResult JSON");
    assert_eq!(result.bytes, vec![0xA9, 0x05, 0x60], "lda #$05 / rts");
    assert_eq!(result.origin, Some(0x8000));
    assert_eq!(result.symbols.get("start"), Some(&0x8000));
    assert!(result.diagnostics.is_empty(), "no diagnostics on success");
}

#[test]
fn json_failure_emits_diagnostics_with_span_and_code() {
    // `lda #$fff` — an immediate that overflows a byte, a fatal error.
    let src = temp_source("bad", "*=$8000\n        lda #$fff\n");
    let out = bin()
        .args(["--cpu", "6502", "--message-format=json"])
        .arg(&src)
        .output()
        .expect("run asm198x");

    assert!(!out.status.success(), "a bad program should fail");
    let diagnostics: Vec<Diagnostic> =
        serde_json::from_slice(&out.stdout).expect("stdout is a Diagnostic array JSON");
    assert_eq!(diagnostics.len(), 1, "one fatal diagnostic");
    let diag = &diagnostics[0];
    assert!(matches!(diag.severity, Severity::Error));
    // A column-accurate span (U3): the line points at the offending
    // instruction, the column at its operand field (`#$fff`, column 13).
    assert_eq!(diag.span.as_ref().map(|s| s.line), Some(2));
    assert_eq!(diag.span.as_ref().map(|s| s.col), Some(13));
    assert!(!diag.message.is_empty(), "a human-readable message");
}

#[test]
fn json_success_still_writes_output_file() {
    let src = temp_source("write", "*=$8000\n        rts\n");
    let obj = std::env::temp_dir().join("asm198x-cli-json-write.bin");
    let _ = std::fs::remove_file(&obj);
    let out = bin()
        .args(["--cpu", "6502", "--message-format=json"])
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .output()
        .expect("run asm198x");

    assert!(out.status.success());
    assert_eq!(
        std::fs::read(&obj).expect("the -o file was written"),
        vec![0x60],
        "json mode still writes the byte output to -o",
    );
}

#[test]
fn human_mode_is_unchanged_without_the_flag() {
    let src = temp_source("human", "*=$8000\n        rts\n");
    let obj = std::env::temp_dir().join("asm198x-cli-json-human.bin");
    let out = bin()
        .args(["--cpu", "6502"])
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .output()
        .expect("run asm198x");

    assert!(out.status.success());
    // The human summary goes to stderr; stdout stays empty (no JSON).
    assert!(out.stdout.is_empty(), "no JSON on stdout in human mode");
    let summary = String::from_utf8_lossy(&out.stderr);
    assert!(
        summary.contains("assembled") && summary.contains("byte"),
        "human summary on stderr: {summary}"
    );
}

#[test]
fn invalid_message_format_errors_cleanly() {
    let src = temp_source("xml", "*=$8000\n        rts\n");
    let out = bin()
        .args(["--cpu", "6502", "--message-format=xml"])
        .arg(&src)
        .output()
        .expect("run asm198x");

    assert!(!out.status.success(), "an unknown format is an error");
    assert!(out.stdout.is_empty(), "no JSON emitted for a bad flag");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("invalid --message-format"),
        "clear error message: {err}"
    );
}
