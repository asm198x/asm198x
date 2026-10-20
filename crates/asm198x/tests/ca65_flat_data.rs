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

fn verify(facts: serde_json::Value, count: usize) {
    let root = scratch::dir(&format!("ca65-flat-data-{count}"));
    for (name, source) in facts["files"].as_object().expect("files") {
        std::fs::write(root.join(name), source.as_str().expect("source")).expect("include");
    }
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), count);
    for fact in cases {
        let name = fact["name"].as_str().expect("name");
        let dialect = fact["dialect"].as_str().expect("dialect");
        std::fs::write(
            root.join("input.s"),
            fact["source"].as_str().expect("source"),
        )
        .expect("source");
        let accepted = fact["native"]
            .as_array()
            .expect("native")
            .last()
            .expect("result")["exit"]
            == 0;
        let outputs = ["out.bin", "out.debug198x", "out.sym", "out.lst.json"];
        for output in outputs {
            std::fs::write(root.join(output), b"previous output").expect("seed output");
        }
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
        let result = assemble("input.s");
        assert_eq!(
            result.status.code(),
            Some(if accepted { 0 } else { 1 }),
            "{name}: {result:?}"
        );
        let payload: serde_json::Value = serde_json::from_slice(&result.stdout).expect("JSON");
        if accepted {
            let bytes = std::fs::read(root.join("out.bin")).expect("image");
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(
                hex,
                fact["native_hex"].as_str().expect("native image"),
                "{name}"
            );
            assert_eq!(payload["bytes"], serde_json::json!(&bytes), "{name}");
            let before = shape(&root);
            let formatted = run(&root, &["fmt", "--dialect", dialect, "input.s"]);
            assert!(formatted.status.success(), "{name}: {formatted:?}");
            std::fs::write(root.join("formatted.s"), &formatted.stdout).expect("formatted source");
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
            assert_eq!(shape(&root), before, "{name}: symbols and sections");
        } else {
            for output in outputs {
                assert_eq!(
                    std::fs::read(root.join(output)).expect("output"),
                    b"previous output",
                    "{name}: {output}"
                );
            }
            if let Some(expected) = fact.get("native_error") {
                let span = &payload[0]["span"];
                assert!(
                    Path::new(span["path"].as_str().expect("path"))
                        .ends_with(expected["path"].as_str().expect("native path")),
                    "{name}"
                );
                assert_eq!(span["line"], expected["line"], "{name}");
            }
            if let Some(expected) = fact.get("expected_context") {
                let span = &payload[0]["span"];
                assert!(
                    Path::new(span["path"].as_str().expect("path"))
                        .ends_with(expected["path"].as_str().expect("expected path")),
                    "{name}"
                );
                assert_eq!(span["line"], expected["line"], "{name}");
                let frames = span["expansion_frames"].as_array().expect("frames");
                let names: Vec<_> = frames.iter().map(|f| f["macro_name"].clone()).collect();
                assert_eq!(serde_json::json!(names), expected["frames"], "{name}");
                for frame in frames {
                    for key in ["defined_at", "invoked_at"] {
                        assert!(
                            Path::new(frame[key]["path"].as_str().expect("frame path"))
                                .ends_with(expected["path"].as_str().expect("expected path")),
                            "{name}: {key}"
                        );
                    }
                    assert_eq!(
                        frame["defined_at"]["line"],
                        expected["definition_lines"]
                            [frame["macro_name"].as_str().expect("macro name")],
                        "{name}"
                    );
                }
            }
        }
    }
}

#[test]
fn flat_macro_diagnostics_retain_nested_file_context_and_outputs() {
    verify(
        serde_json::from_str(include_str!("fixtures/ca65-flat-macro-diagnostics.json"))
            .expect("facts"),
        80,
    );
}

#[test]
fn flat_far_addresses_match_native_unsigned_limits_and_roundtrips() {
    verify(
        serde_json::from_str(include_str!("fixtures/ca65-flat-faraddr.json")).expect("facts"),
        132,
    );
}

#[test]
fn flat_wide_data_matches_native_unsigned_limits_and_roundtrips() {
    verify(
        serde_json::from_str(include_str!("fixtures/ca65-flat-wide-data.json")).expect("facts"),
        104,
    );
}

#[test]
fn flat_addresses_match_native_cpu_specific_extraction_and_roundtrips() {
    verify(
        serde_json::from_str(include_str!("fixtures/ca65-flat-addresses.json")).expect("facts"),
        148,
    );
}
