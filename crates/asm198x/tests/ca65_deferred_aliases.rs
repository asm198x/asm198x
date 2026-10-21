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
    let text = std::fs::read_to_string(root.join("out.debug198x")).expect("debug output");
    debug198x::DebugInfo::read(&text).expect("official debug reader");
    text.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("debug record"))
        .filter(|row| row["t"] == "symbol" || row["t"] == "section")
        .collect()
}

#[test]
fn native_deferred_aliases_resolve_without_changing_source_time_forms() {
    verify_fixture(include_str!("fixtures/ca65-deferred-aliases.json"), 63);
}

#[test]
fn source_constancy_follows_native_alias_classification() {
    let fixture = include_str!("fixtures/ca65-source-constancy.json");
    let facts: serde_json::Value = serde_json::from_str(fixture).expect("native captures");
    assert_eq!(facts["audit_case_count"], 314);
    assert_eq!(facts["excluded_audit_cases"], serde_json::json!({}));
    verify_fixture(fixture, 314);
}

#[test]
fn selected_query_names_require_final_native_binding() {
    let fixture = include_str!("fixtures/ca65-query-admission.json");
    let facts: serde_json::Value = serde_json::from_str(fixture).expect("native captures");
    assert_eq!(facts["audit_case_count"], 54);
    assert_eq!(
        facts["excluded_audit_cases"],
        serde_json::json!({
            "missing_logical_const_direct": [795],
            "missing_logical_const_included": [795],
            "missing_logical_const_macro": [795],
        })
    );
    verify_fixture(fixture, 51);
}

#[test]
fn conditional_source_queries_follow_native_read_order() {
    verify_fixture(include_str!("fixtures/ca65-conditional-context.json"), 197);
}

#[test]
fn data_source_queries_follow_native_read_order() {
    let fixture = include_str!("fixtures/ca65-data-context.json");
    let facts: serde_json::Value = serde_json::from_str(fixture).expect("native captures");
    assert_eq!(facts["audit_case_count"], 515);
    assert_eq!(facts["excluded_audit_cases"], serde_json::json!({}));
    verify_fixture(fixture, 515);
}

fn verify_fixture(fixture: &str, count: usize) {
    let facts: serde_json::Value = serde_json::from_str(fixture).expect("native captures");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), count);
    let root = scratch::dir(&format!("ca65-deferred-aliases-{count}"));
    for (name, text) in facts["files"].as_object().expect("includes") {
        std::fs::create_dir_all(root.join(name).parent().expect("include parent"))
            .expect("include directories");
        std::fs::write(root.join(name), text.as_str().expect("include text")).expect("include");
    }
    for fact in cases {
        let name = fact["name"].as_str().expect("name");
        let dialect = if name.starts_with("65816_") {
            "65816"
        } else {
            "ca65"
        };
        std::fs::write(
            root.join("input.s"),
            fact["source"].as_str().expect("source"),
        )
        .expect("source");
        std::fs::write(
            root.join("input.cfg"),
            fact["config"].as_str().expect("config"),
        )
        .expect("config");
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
                "--sym=out.sym",
                "--listing-json=out.lst.json",
                "--message-format=json",
            ]);
            run(&root, &args)
        };
        let outputs = ["out.bin", "out.debug198x", "out.sym", "out.lst.json"];
        for output in outputs {
            std::fs::write(root.join(output), b"previous image").expect("seed output");
        }
        let result = assemble("input.s");
        let accepted = fact["native"]
            .as_array()
            .expect("native stages")
            .last()
            .expect("native result")["exit"]
            == 0;
        assert_eq!(result.status.success(), accepted, "{name}: {result:?}");
        if !accepted {
            let errors: serde_json::Value =
                serde_json::from_slice(&result.stdout).expect("diagnostics");
            assert!(!errors.as_array().expect("error list").is_empty(), "{name}");
            if let Some(expected) = fact.get("query_error") {
                let span = &errors[0]["span"];
                let path = span["path"].as_str().expect("source path");
                assert_eq!(
                    Path::new(path).file_name().expect("file name").to_str(),
                    expected["path"].as_str(),
                    "{name}"
                );
                assert_eq!(span["line"], expected["line"], "{name}");
                let frames: Vec<_> = span["expansion_frames"]
                    .as_array()
                    .expect("frames")
                    .iter()
                    .map(|f| f["macro_name"].clone())
                    .collect();
                assert_eq!(serde_json::json!(frames), expected["frames"], "{name}");
            }
            for output in outputs {
                assert_eq!(
                    std::fs::read(root.join(output)).expect("preserved output"),
                    b"previous image",
                    "{name}: {output}"
                );
            }
            continue;
        }
        let bytes = std::fs::read(root.join("out.bin")).expect("image");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            fact["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
        let payload: serde_json::Value =
            serde_json::from_slice(&result.stdout).expect("JSON output");
        assert_eq!(
            payload["bytes"],
            serde_json::json!(&bytes),
            "{name}: JSON bytes"
        );
        let debug = shape(&root);
        let formatted = run(&root, &["fmt", "--dialect", dialect, "input.s"]);
        assert!(formatted.status.success(), "{name}: {formatted:?}");
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
        let payload: serde_json::Value =
            serde_json::from_slice(&result.stdout).expect("formatted JSON output");
        assert_eq!(
            payload["bytes"],
            serde_json::json!(&bytes),
            "{name}: formatted JSON bytes"
        );
        assert_eq!(shape(&root), debug, "{name}: addresses and symbols");
    }
}

#[test]
fn included_unresolved_alias_preserves_outputs_and_source() {
    let root = scratch::dir("ca65-record-allocator-preserve");
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-deferred-aliases.json"))
            .expect("native captures");
    let config = facts["cases"][0]["config"].as_str().expect("config");
    std::fs::write(root.join("input.cfg"), config).expect("config");
    let source = ".include \"definitions.inc\"\n";
    let invalid = "; included definition\nOTHER=MISSING\n.byte 7\n";
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
            "; included definition\nOTHER=7\n.byte OTHER\n",
        )
        .expect("valid include");
        let success = execute();
        assert!(success.status.success(), "{success:?}");
        assert_eq!(std::fs::read(root.join("out.bin")).expect("image"), [7]);
        let previous: Vec<_> = outputs
            .iter()
            .map(|name| std::fs::read(root.join(name)).expect("output"))
            .collect();
        std::fs::write(root.join("definitions.inc"), invalid).expect("invalid include");
        let failure = execute();
        assert!(!failure.status.success(), "{failure:?}");
        if mode == "json" {
            let errors: serde_json::Value =
                serde_json::from_slice(&failure.stdout).expect("diagnostics");
            assert_eq!(errors[0]["span"]["line"], 2);
            assert!(
                errors[0]["span"]["path"]
                    .as_str()
                    .expect("path")
                    .ends_with("definitions.inc")
            );
        } else {
            assert!(
                String::from_utf8_lossy(&failure.stderr).contains("definitions.inc:2:"),
                "{failure:?}"
            );
        }
        for (name, bytes) in outputs.iter().zip(previous) {
            assert_eq!(
                std::fs::read(root.join(name)).expect("preserved output"),
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
