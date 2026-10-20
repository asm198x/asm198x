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

fn verify_native_cases(fixture: &str, count: usize, accepted: usize, audits: &[(&str, &str)]) {
    let facts: serde_json::Value = serde_json::from_str(fixture).expect("byte-string facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), count);
    let excluded = facts["excluded_cases"].as_array().expect("excluded cases");
    let originals: Vec<(&str, serde_json::Value)> = audits
        .iter()
        .map(|(name, json)| (*name, serde_json::from_str(json).expect("native audit")))
        .collect();
    for (name, original) in &originals {
        for case in original["cases"].as_array().expect("native cases") {
            assert_eq!(
                cases
                    .iter()
                    .filter(|r| r["audit"] == *name && r["name"] == case["name"])
                    .count()
                    + excluded
                        .iter()
                        .filter(|r| r["audit"] == *name && r["name"] == case["name"])
                        .count(),
                1,
                "{name}/{}: coverage accounting",
                case["name"]
            );
        }
    }
    for case in cases {
        let original = originals
            .iter()
            .find(|(name, _)| case["audit"] == *name)
            .expect("known audit")
            .1["cases"]
            .as_array()
            .expect("native cases")
            .iter()
            .find(|row| row["name"] == case["name"])
            .expect("original case");
        if case["assembly_gaps"].is_array() {
            assert_eq!(case["audit"], "index_scope");
            assert_eq!(case["assembly_gaps"], serde_json::json!([795]));
            assert!(matches!(
                case["name"].as_str(),
                Some("scoped_shadow" | "named_reference")
            ));
        }
        for field in ["source", "config", "native", "native_hex"] {
            assert_eq!(case[field], original[field], "{}: {field}", case["name"]);
        }
    }
    for case in excluded {
        match case["issue"].as_u64() {
            Some(895) => {
                assert_eq!(case["audit"], "flat_literal");
                let name = case["name"].as_str().expect("excluded name");
                assert!(
                    ["_selected_", "_inactive_", "_zero_repeat_"]
                        .iter()
                        .any(|context| name.contains(context))
                );
            }
            _ => {
                assert_eq!(case["audit"], "lifetime");
                assert_eq!(case["issue"], 891);
            }
        }
    }
    assert_eq!(
        excluded.len(),
        if audits.iter().any(|(name, _)| *name == "lifetime") {
            9
        } else if audits.iter().any(|(name, _)| *name == "literal") {
            264
        } else {
            0
        }
    );
    let root = scratch::dir(&format!("ca65-byte-string-values-{count}"));
    for (name, text) in facts["files"].as_object().expect("include files") {
        std::fs::write(root.join(name), text.as_str().expect("file text")).expect("include file");
    }
    let mut accepted_count = 0;
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let dialect = case["dialect"].as_str().expect("dialect");
        std::fs::write(
            root.join("input.s"),
            case["source"].as_str().expect("source"),
        )
        .expect("source file");
        std::fs::write(
            root.join("input.cfg"),
            case["config"].as_str().expect("config"),
        )
        .expect("link config");
        let outputs = ["out.bin", "out.debug198x", "out.lst.json", "out.sym"];
        for output in outputs {
            std::fs::write(root.join(output), b"previous output").expect("previous output");
        }
        let assemble = |input| {
            let mut args = vec!["--dialect", dialect];
            if dialect == "ca65" {
                args.extend(["-C", "input.cfg"]);
            }
            args.extend([
                input,
                "-o",
                "out.bin",
                "--debug=out.debug198x",
                "--listing-json=out.lst.json",
                "--sym=out.sym",
                "--message-format=json",
            ]);
            run(&root, &args)
        };
        let authored_marker = case["authored_marker"].as_bool().unwrap_or(false);
        assert_eq!(
            authored_marker,
            case["source"]
                .as_str()
                .expect("source")
                .contains("ca65-string:")
        );
        let result = assemble("input.s");
        let native_accepted = case["native"]
            .as_array()
            .expect("native stages")
            .last()
            .expect("stage")["exit"]
            == 0
            && !case["assembly_gaps"].is_array();
        assert_eq!(
            result.status.code(),
            Some(if native_accepted { 0 } else { 1 }),
            "{name}: {result:?}"
        );
        for text in [&result.stdout, &result.stderr]
            .into_iter()
            .filter(|_| !authored_marker)
        {
            assert!(
                !String::from_utf8_lossy(text).contains("ca65-string:"),
                "{name}: handle in diagnostic"
            );
        }
        if !native_accepted {
            for output in outputs {
                assert_eq!(
                    std::fs::read(root.join(output)).expect("preserved output"),
                    b"previous output",
                    "{name}: {output}"
                );
            }
            continue;
        }
        accepted_count += 1;
        let bytes = std::fs::read(root.join("out.bin")).expect("image");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            case["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
        let before = shape(&root);
        debug198x::DebugInfo::read(
            &std::fs::read_to_string(root.join("out.debug198x")).expect("sidecar"),
        )
        .expect("official debug reader");
        for output in outputs[1..].iter().filter(|_| !authored_marker) {
            assert!(
                !std::fs::read_to_string(root.join(output))
                    .expect("text sidecar")
                    .contains("ca65-string:"),
                "{name}: handle in {output}"
            );
        }
        assert!(
            case["formatter_gap"].is_null(),
            "{name}: all formatter cases are required"
        );
        let formatted = run(&root, &["fmt", "--dialect", dialect, "input.s"]);
        assert!(formatted.status.success(), "{name}: {formatted:?}");
        assert!(
            authored_marker || !String::from_utf8_lossy(&formatted.stdout).contains("ca65-string:"),
            "{name}: handle in formatted source"
        );
        std::fs::write(root.join("formatted.s"), &formatted.stdout).expect("formatted source");
        let again = run(&root, &["fmt", "--dialect", dialect, "formatted.s"]);
        assert!(again.status.success(), "{name}: {again:?}");
        assert_eq!(again.stdout, formatted.stdout, "{name}: idempotence");
        let result = assemble("formatted.s");
        assert!(result.status.success(), "{name}: {result:?}");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("formatted image"),
            bytes,
            "{name}: formatted bytes"
        );
        assert_eq!(shape(&root), before, "{name}: symbols and sections");
    }
    assert_eq!(accepted_count, accepted);
}

#[test]
fn all_native_character_bytes_and_precision_boundaries_survive() {
    verify_native_cases(
        include_str!("fixtures/ca65-string-bytes.json"),
        287,
        274,
        &[(
            "bytes",
            include_str!(
                "../../../docs/plans/2026-10-20-ca65-format-argument-types/byte-native.json"
            ),
        )],
    );
}

#[test]
fn native_string_consumers_and_selected_capture_lifetimes_survive() {
    verify_native_cases(
        include_str!("fixtures/ca65-string-consumers.json"),
        132,
        104,
        &[
            (
                "consumers",
                include_str!(
                    "../../../docs/plans/2026-10-20-ca65-string-token-provenance/consumer-native.json"
                ),
            ),
            (
                "lifetime",
                include_str!(
                    "../../../docs/plans/2026-10-20-ca65-string-token-provenance/lifetime-native.json"
                ),
            ),
            (
                "guards",
                include_str!(
                    "../../../docs/plans/2026-10-20-ca65-string-token-provenance/guard-native.json"
                ),
            ),
            (
                "index_scope",
                include_str!(
                    "../../../docs/plans/2026-10-20-ca65-string-token-provenance/index-scope-native.json"
                ),
            ),
            (
                "numeric_arity",
                include_str!(
                    "../../../docs/plans/2026-10-20-ca65-string-token-provenance/numeric-string-arity-native.json"
                ),
            ),
        ],
    );
}

#[test]
fn flat_ca65_readers_keep_native_byte_values_through_parsing() {
    verify_native_cases(
        include_str!("fixtures/ca65-flat-string-bytes.json"),
        574,
        548,
        &[(
            "flat",
            include_str!(
                "../../../docs/plans/2026-10-20-ca65-string-token-provenance/flat-native.json"
            ),
        )],
    );
}

#[test]
fn literal_boundaries_and_asciiz_operand_types_match_native() {
    verify_native_cases(
        include_str!("fixtures/ca65-literal-boundaries.json"),
        798,
        477,
        &[
            (
                "literal",
                include_str!(
                    "../../../docs/plans/2026-10-20-ca65-literal-boundaries/literal-boundary-native.json"
                ),
            ),
            (
                "flat_literal",
                include_str!(
                    "../../../docs/plans/2026-10-20-ca65-literal-boundaries/flat-native.json"
                ),
            ),
        ],
    );
}
