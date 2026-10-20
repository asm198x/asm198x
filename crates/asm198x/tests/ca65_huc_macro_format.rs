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
        .stdout(Stdio::from(std::fs::File::create(&stdout).expect("stdout")))
        .stderr(Stdio::from(std::fs::File::create(&stderr).expect("stderr")))
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
            panic!("CLI timed out: {args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Output {
        status: child.wait().expect("completed CLI"),
        stdout: std::fs::read(stdout).expect("stdout"),
        stderr: std::fs::read(stderr).expect("stderr"),
    }
}

fn shape(root: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(root.join("out.debug198x")).expect("debug output");
    debug198x::DebugInfo::read(&text).expect("official debug reader");
    text.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("debug record"))
        .filter(|row| row["t"] == "symbol" || row["t"] == "section")
        .collect()
}

fn macro_names(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let head = words.next()?;
            if head.eq_ignore_ascii_case(".macro") || head.eq_ignore_ascii_case(".mac") {
                words.next()
            } else {
                None
            }
        })
        .collect()
}

fn calls(source: &str, names: &[&str]) -> usize {
    source
        .lines()
        .filter(|line| {
            let mut words = line.split_whitespace();
            let Some(first) = words.next() else {
                return false;
            };
            let word = if first.ends_with(':') {
                words.next().unwrap_or("")
            } else {
                first
            };
            names.contains(&word)
        })
        .count()
}

#[test]
fn native_macro_programs_keep_templates_calls_and_bytes() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-huc-macro-format.json"))
            .expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 32);
    let root = scratch::dir("ca65-huc-macro-format");
    for fact in cases {
        let name = fact["name"].as_str().expect("case name");
        let source = fact["source"].as_str().expect("source");
        let dialect = fact["dialect"].as_str().expect("dialect");
        assert_eq!(
            fact["native"]
                .as_array()
                .expect("native")
                .last()
                .expect("result")["exit"],
            0
        );
        std::fs::write(root.join("input.s"), source).expect("source");
        let assemble = |input| {
            run(
                &root,
                &[
                    "--dialect",
                    dialect,
                    input,
                    "-o",
                    "out.bin",
                    "--debug=out.debug198x",
                    "--sym=out.sym",
                    "--listing-json=out.lst.json",
                    "--message-format=json",
                ],
            )
        };
        let before = assemble("input.s");
        assert!(before.status.success(), "{name}: {before:?}");
        let bytes = std::fs::read(root.join("out.bin")).expect("image");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            fact["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
        let payload: serde_json::Value =
            serde_json::from_slice(&before.stdout).expect("JSON success");
        assert_eq!(payload["bytes"], serde_json::json!(&bytes), "{name}");
        let debug = shape(&root);
        let formatted = run(&root, &["fmt", "--dialect", dialect, "input.s"]);
        assert!(formatted.status.success(), "{name}: {formatted:?}");
        let text = std::str::from_utf8(&formatted.stdout).expect("formatted UTF-8");
        let names = macro_names(source);
        assert!(!names.is_empty());
        assert_eq!(macro_names(text), names, "{name}: definitions survive");
        assert_eq!(
            calls(text, &names),
            calls(source, &names),
            "{name}: calls survive"
        );
        for marker in ["MISSING", ".repeat", ".endrepeat", "missing.inc"] {
            if source.contains(marker) {
                assert!(text.contains(marker), "{name}: template lost {marker}");
            }
        }
        std::fs::write(root.join("formatted.s"), text).expect("formatted source");
        let again = run(&root, &["fmt", "--dialect", dialect, "formatted.s"]);
        assert!(again.status.success(), "{name}: {again:?}");
        assert_eq!(again.stdout, formatted.stdout, "{name}: idempotence");
        let after = assemble("formatted.s");
        assert!(after.status.success(), "{name}: {after:?}");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("image"),
            bytes,
            "{name}"
        );
        assert_eq!(shape(&root), debug, "{name}: symbols and addresses");
    }
}

#[test]
fn formatting_keeps_missing_include_and_unread_templates() {
    let root = scratch::dir("ca65-huc-macro-format-lazy");
    let source = ".include \"absent.inc\"\n.macro unused operand\n.local here\nhere: .byte operand\n.res MISSING\n.endmacro\n.byte 7\n";
    std::fs::write(root.join("input.s"), source).expect("source");
    assert!(!root.join("absent.inc").exists());
    let formatted = run(&root, &["fmt", "--dialect", "huc6280", "input.s"]);
    assert!(formatted.status.success(), "{formatted:?}");
    let text = std::str::from_utf8(&formatted.stdout).expect("UTF-8");
    for marker in [
        "absent.inc",
        ".macro unused operand",
        ".local here",
        "MISSING",
        ".endmacro",
    ] {
        assert!(text.contains(marker), "missing {marker}: {text}");
    }
    assert_eq!(
        std::fs::read_to_string(root.join("input.s")).expect("source"),
        source
    );
}

#[test]
fn included_macro_errors_keep_outputs_and_locations_after_formatting() {
    let facts: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/plans/2026-10-20-huc6280-macro-format/native-errors.json"
    ))
    .expect("native error facts");
    let root = scratch::dir("ca65-huc-macro-format-errors");
    let broken = facts["files"]["broken.inc"]
        .as_str()
        .expect("included source");
    for case in facts["cases"].as_array().expect("native cases") {
        let dialect = case["name"].as_str().expect("dialect");
        assert_eq!(case["native"][0]["exit"], 1);
        std::fs::write(
            root.join("input.s"),
            case["source"].as_str().expect("source"),
        )
        .expect("root source");
        std::fs::write(root.join("broken.inc"), broken).expect("included source");
        for formatted in [false, true] {
            if formatted {
                let result = run(&root, &["fmt", "--dialect", dialect, "broken.inc"]);
                assert!(result.status.success(), "{dialect}: {result:?}");
                std::fs::write(root.join("broken.inc"), result.stdout).expect("formatted include");
            }
            let outputs = ["out.bin", "out.debug198x", "out.sym", "out.lst.json"];
            for output in outputs {
                std::fs::write(root.join(output), b"previous output").expect("seed output");
            }
            let result = run(
                &root,
                &[
                    "--dialect",
                    dialect,
                    "input.s",
                    "-o",
                    "out.bin",
                    "--debug=out.debug198x",
                    "--sym=out.sym",
                    "--listing-json=out.lst.json",
                    "--message-format=json",
                ],
            );
            assert_eq!(result.status.code(), Some(1), "{dialect}: {result:?}");
            let diagnostics: serde_json::Value =
                serde_json::from_slice(&result.stdout).expect("JSON diagnostics");
            let path = diagnostics[0]["span"]["path"]
                .as_str()
                .expect("include path");
            assert!(Path::new(path).ends_with("broken.inc"), "{path}");
            assert_eq!(diagnostics[0]["span"]["line"], 4);
            let frames = diagnostics[0]["span"]["expansion_frames"]
                .as_array()
                .expect("macro frames");
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0]["macro_name"], "emit");
            assert_eq!(frames[0]["defined_at"]["line"], 1);
            for key in ["defined_at", "invoked_at"] {
                assert!(
                    Path::new(frames[0][key]["path"].as_str().expect("frame path"))
                        .ends_with("broken.inc")
                );
            }
            for output in outputs {
                assert_eq!(
                    std::fs::read(root.join(output)).expect("preserved output"),
                    b"previous output",
                    "{dialect}: {output}"
                );
            }
        }
    }
}
