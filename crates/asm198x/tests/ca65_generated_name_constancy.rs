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
fn native_bound_generated_names_are_visible_to_constancy_queries() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-generated-name-constancy.json"))
            .expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 23);
    let audit: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/plans/2026-10-20-ca65-generated-name-constancy/native.json"
    ))
    .expect("complete native audit");
    let all = audit["cases"].as_array().expect("all native cases");
    assert_eq!(all.len(), 32);
    let excluded = facts["excluded_cases"]
        .as_array()
        .expect("separate query controls");
    assert_eq!(excluded.len(), 9);
    for item in excluded {
        assert_eq!(item["issue"], 795);
    }
    for original in all {
        assert_eq!(
            cases
                .iter()
                .filter(|case| case["name"] == original["name"])
                .count()
                + excluded
                    .iter()
                    .filter(|case| case["name"] == original["name"])
                    .count(),
            1,
            "{}: coverage accounting",
            original["name"]
        );
    }
    for case in cases {
        let original = all
            .iter()
            .find(|original| original["name"] == case["name"])
            .expect("original case");
        for field in ["source", "config", "native", "native_hex"] {
            assert_eq!(case[field], original[field], "{}: {field}", case["name"]);
        }
    }
    let root = scratch::dir("ca65-generated-name-constancy");
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
    assert_eq!(gaps, 0);
}
