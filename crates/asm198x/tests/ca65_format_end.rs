use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[path = "support/scratch.rs"]
mod scratch;

fn run(root: &Path, args: &[&str]) -> Output {
    let stdout = root.join("cli.stdout");
    let stderr = root.join("cli.stderr");
    let mut child = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(root)
        .args(args)
        .stdout(Stdio::from(
            std::fs::File::create(&stdout).expect("stdout log"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&stderr).expect("stderr log"),
        ))
        .spawn()
        .expect("spawn CLI");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().expect("poll CLI").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill timed-out CLI");
            child.wait().expect("reap timed-out CLI");
            panic!("CLI failed to complete within five seconds: {args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Output {
        status: child.wait().expect("completed CLI"),
        stdout: std::fs::read(stdout).expect("stdout"),
        stderr: std::fs::read(stderr).expect("stderr"),
    }
}

// The formatter reads complete source syntax; it must never execute `.end`.
fn lexical_content(source: &str) -> (String, Vec<String>) {
    let mut tokens = String::new();
    let mut comments = Vec::new();
    for line in source.lines() {
        let mut quote = None;
        let mut chars = line.chars();
        while let Some(ch) = chars.next() {
            if let Some(delimiter) = quote {
                tokens.push(ch);
                if ch == delimiter {
                    quote = None;
                }
            } else if ch == ';' {
                comments.push(chars.as_str().to_string());
                break;
            } else if ch == '\'' || ch == '"' {
                quote = Some(ch);
                tokens.push(ch);
            } else if !ch.is_whitespace() {
                tokens.push(ch);
            }
        }
    }
    (tokens, comments)
}

#[test]
fn canonical_reader_retains_complete_end_source_without_evaluation() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-format-end.json")).expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 26);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let source = case["source"].as_str().expect("source");
        let formatted = asm198x::format_ca65(source).expect(name);
        // The formatter keeps the line endings it is given, and a last line
        // with none.
        for (variant, expected) in [
            (
                source.replace('\n', "\r\n"),
                formatted.replace('\n', "\r\n"),
            ),
            (
                source.trim_end_matches('\n').to_string(),
                formatted.trim_end_matches('\n').to_string(),
            ),
        ] {
            assert_eq!(
                asm198x::format_ca65(&variant).expect(name),
                expected,
                "{name}: newline convention"
            );
        }
        assert_eq!(
            lexical_content(&formatted),
            lexical_content(source),
            "{name}: complete tokens and comments"
        );
        assert_eq!(
            asm198x::format_ca65(&formatted).expect(name),
            formatted,
            "{name}: idempotence"
        );
        // Lines after `.end` are kept as written. That is always safe, even
        // where ca65 reads them, as here, where `.repeat 0` skips the `.end`.
        if name == "zero_repeat" {
            assert_eq!(
                formatted,
                "        .repeat 0\n        .end\n.endrepeat\n.byte 8\n"
            );
        }
        // A statement keeps its own text, so an assignment is not aligned.
        if name == "constant_runs" {
            assert!(
                formatted.starts_with("        SHORT=1\n        LONG_NAME=$02\n"),
                "{formatted}"
            );
        }
        // A label stays on its statement's line, in column 1.
        if name == "ordinary_labels" {
            assert!(
                formatted.starts_with("entry:  .byte 7\n@cheap: .byte 8\n:       .word entry\n"),
                "{formatted}"
            );
        }
    }
}

#[test]
fn formatted_source_keeps_native_bytes_and_failed_outputs() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-format-end.json")).expect("native facts");
    let root = scratch::dir("ca65-format-end-outputs");
    let outputs = [
        "out.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for case in facts["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().expect("name");
        let source = case["source"].as_str().expect("source");
        let formatted = asm198x::format_ca65(source).expect(name);
        std::fs::write(root.join("input.s"), source).expect("source");
        std::fs::write(root.join("formatted.s"), &formatted).expect("formatted source");
        std::fs::write(
            root.join("input.cfg"),
            case["config"].as_str().expect("config"),
        )
        .expect("config");
        let accepted = case["native"]
            .as_array()
            .expect("native")
            .last()
            .expect("native stage")["exit"]
            == 0
            && case["formatted_assembly_gap"].is_null();
        // A named divergence: the walk's own answer is recorded beside ca65's.
        let divergent = case["divergence"].is_string();
        let accepted = if divergent {
            case["walk_hex"].is_string()
        } else {
            accepted
        };
        let want_hex = if divergent {
            &case["walk_hex"]
        } else {
            &case["native_hex"]
        };
        for mode in ["human", "json"] {
            for output in outputs {
                std::fs::write(root.join(output), format!("previous {output}"))
                    .expect("previous output");
            }
            let result = run(
                &root,
                &[
                    "--dialect",
                    "ca65",
                    "-C",
                    "input.cfg",
                    "--message-format",
                    mode,
                    "formatted.s",
                    "-o",
                    "out.bin",
                    "--sym=out.sym",
                    "--debug=out.debug198x",
                    "--listing=out.lst",
                    "--listing-json=out.lst.json",
                ],
            );
            assert_eq!(
                result.status.code(),
                Some(if accepted { 0 } else { 1 }),
                "{name}/{mode}: {result:?}"
            );
            if accepted {
                let hex: String = std::fs::read(root.join("out.bin"))
                    .expect("image")
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                assert_eq!(
                    hex,
                    want_hex.as_str().expect("expected bytes"),
                    "{name}/{mode}"
                );
            } else {
                for output in outputs {
                    assert_eq!(
                        std::fs::read(root.join(output)).expect("preserved output"),
                        format!("previous {output}").as_bytes(),
                        "{name}/{mode}/{output}"
                    );
                }
                if mode == "json" {
                    let errors: serde_json::Value =
                        serde_json::from_slice(&result.stdout).expect("diagnostics");
                    assert!(!errors.as_array().expect("errors").is_empty(), "{name}");
                }
            }
            assert_eq!(
                std::fs::read_to_string(root.join("input.s")).expect("source"),
                source
            );
            assert_eq!(
                std::fs::read_to_string(root.join("formatted.s")).expect("formatted source"),
                formatted
            );
        }
    }
}
