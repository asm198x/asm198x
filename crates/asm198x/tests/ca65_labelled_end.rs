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

fn shape(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(root.join("out.debug198x"))
        .expect("debug output")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("debug record"))
        .filter(|row| row["t"] == "symbol" || row["t"] == "section")
        .collect()
}

#[test]
fn native_end_labels_bind_before_termination() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-labelled-end.json"))
            .expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 115);
    let root = scratch::dir("ca65-labelled-end");
    for (name, text) in facts["files"].as_object().expect("includes") {
        std::fs::write(root.join(name), text.as_str().expect("include text")).expect("include");
    }
    let mut gaps = 0;
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let source = case["source"].as_str().expect("source");
        std::fs::write(root.join("input.s"), source).expect("source");
        std::fs::write(
            root.join("input.cfg"),
            case["config"].as_str().expect("config"),
        )
        .expect("config");
        std::fs::write(root.join("out.bin"), b"previous image").expect("previous image");
        let gap = case["assembly_gaps"].is_array();
        if gap {
            gaps += 1;
        }
        let accepted = case["native"]
            .as_array()
            .expect("native stages")
            .last()
            .expect("stage")["exit"]
            == 0
            && !gap;
        let assemble = |input| {
            run(
                &root,
                &[
                    "--dialect",
                    "ca65",
                    "-C",
                    "input.cfg",
                    input,
                    "-o",
                    "out.bin",
                    "--debug=out.debug198x",
                ],
            )
        };
        let result = assemble("input.s");
        assert_eq!(
            result.status.code(),
            Some(if accepted { 0 } else { 1 }),
            "{name}: {result:?}"
        );
        if !accepted {
            assert_eq!(
                std::fs::read(root.join("out.bin")).expect("preserved image"),
                b"previous image",
                "{name}"
            );
            continue;
        }
        let bytes = std::fs::read(root.join("out.bin")).expect("image");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            case["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
        let before = shape(&root);
        if let Some(symbols) = case["symbol_values"].as_object() {
            let debug = debug198x::DebugInfo::read(
                &std::fs::read_to_string(root.join("out.debug198x")).expect("sidecar"),
            )
            .expect("official reader");
            for (symbol, value) in symbols {
                assert_eq!(
                    debug.addr_of(symbol, None),
                    value.as_u64(),
                    "{name}: {symbol}"
                );
                assert!(
                    before.iter().any(|r| r["t"] == "symbol"
                        && r["name"] == symbol.as_str()
                        && r["kind"] == "label"),
                    "{name}: address label kind"
                );
            }
        }
        let formatted = run(&root, &["fmt", "--dialect", "ca65", "input.s"]);
        assert!(formatted.status.success(), "{name}: {formatted:?}");
        std::fs::write(root.join("formatted.s"), &formatted.stdout).expect("formatted source");
        let again = run(&root, &["fmt", "--dialect", "ca65", "formatted.s"]);
        assert!(again.status.success(), "{name}: {again:?}");
        assert_eq!(again.stdout, formatted.stdout, "{name}: idempotence");
        let result = assemble("formatted.s");
        assert!(result.status.success(), "{name}: {result:?}");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("formatted bytes"),
            bytes,
            "{name}"
        );
        assert_eq!(shape(&root), before, "{name}: symbols and sections");
    }
    assert_eq!(gaps, 13);
}

#[test]
fn included_end_labels_preserve_failed_outputs_and_source_frames() {
    let root = scratch::dir("ca65-labelled-end-preserve");
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-labelled-end.json"))
            .expect("native facts");
    std::fs::write(
        root.join("input.cfg"),
        facts["cases"][0]["config"].as_str().expect("config"),
    )
    .expect("config");
    let source = ".include \"definitions.inc\"\n.byte 9\n";
    std::fs::write(root.join("input.s"), source).expect("source");
    let outputs = [
        "out.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for mode in ["human", "json"] {
        let execute = || {
            run(
                &root,
                &[
                    "--dialect",
                    "ca65",
                    "-C",
                    "input.cfg",
                    "--message-format",
                    mode,
                    "input.s",
                    "-o",
                    "out.bin",
                    "--sym=out.sym",
                    "--debug=out.debug198x",
                    "--listing=out.lst",
                    "--listing-json=out.lst.json",
                ],
            )
        };
        std::fs::write(
            root.join("definitions.inc"),
            ".word finish\nfinish:.end\n.byte 8\n",
        )
        .expect("valid include");
        let success = execute();
        assert!(success.status.success(), "{success:?}");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("image"),
            [2, 0x10]
        );
        let previous: Vec<_> = outputs
            .iter()
            .map(|name| std::fs::read(root.join(name)).expect("output"))
            .collect();
        for (invalid, line) in [
            ("; include context\nA:.end\n", 2),
            ("; include context\nlda:.end\n", 2),
            ("; include context\n.ident(UNDEFINED):.end\n", 2),
            (
                "; include context\n.macro stop\nfinish:.end,$100000000\n.endmacro\nstop\n",
                3,
            ),
        ] {
            std::fs::write(root.join("definitions.inc"), invalid).expect("invalid include");
            let failure = execute();
            assert_eq!(failure.status.code(), Some(1), "{mode}: {failure:?}");
            if mode == "json" {
                let errors: serde_json::Value =
                    serde_json::from_slice(&failure.stdout).expect("diagnostics");
                assert_eq!(errors[0]["span"]["line"], line);
                assert!(
                    errors[0]["span"]["path"]
                        .as_str()
                        .expect("path")
                        .ends_with("definitions.inc")
                );
            } else {
                assert!(
                    String::from_utf8_lossy(&failure.stderr)
                        .contains(&format!("definitions.inc:{line}:")),
                    "{failure:?}"
                );
            }
            for (name, bytes) in outputs.iter().zip(&previous) {
                assert_eq!(
                    &std::fs::read(root.join(name)).expect("preserved output"),
                    bytes,
                    "{name}"
                );
            }
            assert_eq!(
                std::fs::read_to_string(root.join("input.s")).expect("source"),
                source
            );
            assert_eq!(
                std::fs::read_to_string(root.join("definitions.inc")).expect("include"),
                invalid
            );
        }
    }
}
