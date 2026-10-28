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

fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-text-consumers.json")).expect("text facts")
}

#[test]
fn native_audits_have_complete_portable_and_memory_coverage() {
    let facts = facts();
    let cases = facts["cases"].as_array().expect("cases");
    let posix = facts["posix_cases"].as_array().expect("POSIX cases");
    assert_eq!(cases.len(), 472);
    assert_eq!(posix.len(), 48);
    assert_eq!(
        facts["excluded_cases"]
            .as_array()
            .expect("exclusions")
            .len(),
        48
    );
    for (audit, source) in [
        (
            "text",
            include_str!("../../../docs/plans/2026-10-20-ca65-text-consumers/native.json"),
        ),
        (
            "assertion",
            include_str!(
                "../../../docs/plans/2026-10-20-ca65-text-consumers/assertion-native.json"
            ),
        ),
    ] {
        let original: serde_json::Value = serde_json::from_str(source).expect("native audit");
        for native in original["cases"].as_array().expect("native cases") {
            let rows: Vec<_> = cases
                .iter()
                .chain(posix)
                .filter(|c| c["audit"] == audit && c["name"] == native["name"])
                .collect();
            assert_eq!(rows.len(), 1, "{audit}/{}", native["name"]);
            for field in ["source", "config", "native", "native_hex"] {
                assert_eq!(rows[0][field], native[field], "{}: {field}", native["name"]);
            }
            if audit == "text" {
                for field in ["consumer", "value"] {
                    assert_eq!(rows[0][field], native[field]);
                }
                let expected = if matches!(
                    native["consumer"].as_str(),
                    Some("include" | "incbin" | "assert_true")
                ) {
                    serde_json::json!([])
                } else {
                    serde_json::json!([native["value"]])
                };
                assert_eq!(
                    rows[0]["expected_messages"], expected,
                    "{}: native payload",
                    native["name"]
                );
            } else {
                let name = native["name"].as_str().expect("name");
                let kind = name.splitn(3, '_').nth(2).expect("assertion kind");
                let valid = name.starts_with("comma_char_")
                    || name.starts_with("apostrophe_char_")
                    || name.starts_with("open_paren_char_")
                    || matches!(
                        kind,
                        "absent"
                            | "empty"
                            | "plain"
                            | "comma"
                            | "computed_quote"
                            | "computed_mixed"
                    );
                assert_eq!(
                    rows[0]["expected_messages"].is_array(),
                    valid,
                    "{name}: message contract"
                );
                if valid {
                    let expected = rows[0]["expected_messages"].as_array().expect("payload");
                    let mut observed = expected.is_empty();
                    // Native can print the same assertion warning in both phases;
                    // the existing fused assembler emits one logical diagnostic.
                    for stage in native["native"].as_array().expect("native stages") {
                        // ld65 V2.19 puts its own prefix before the message.
                        let linker = stage["argv"][0]
                            .as_str()
                            .is_some_and(|tool| tool.ends_with("ld65"));
                        let messages: Vec<_> = stage["stderr"]
                            .as_str()
                            .expect("stderr")
                            .lines()
                            .filter_map(|line| {
                                line.split_once(": Warning: ")
                                    .or_else(|| line.split_once(": Error: "))
                                    .map(|(_, text)| {
                                        let text = if linker {
                                            text.strip_prefix("Assertion failed: ").unwrap_or(text)
                                        } else {
                                            text
                                        };
                                        serde_json::Value::String(text.to_string())
                                    })
                            })
                            .collect();
                        if !messages.is_empty() {
                            assert_eq!(&messages, expected, "{name}: native assertion payload");
                            observed = true;
                        }
                    }
                    assert!(observed, "{name}: no native payload evidence");
                }
            }
        }
    }
    for excluded in facts["excluded_cases"].as_array().expect("exclusions") {
        assert_eq!(excluded["audit"], "text");
        assert_eq!(excluded["surface"], "memory_and_posix");
        assert_eq!(
            posix
                .iter()
                .filter(|c| c["name"] == excluded["name"])
                .count(),
            1
        );
    }
}

#[test]
fn native_filename_values_reach_the_memory_loader_on_every_os() {
    let original: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/plans/2026-10-20-ca65-text-consumers/native.json"
    ))
    .expect("native facts");
    let mut loader = asm198x::source::MemoryLoader::new();
    for (name, text) in original["files"].as_object().expect("files") {
        let text = text.as_str().expect("text");
        loader = loader
            .text(name, text)
            .binary(name, text.as_bytes().to_vec());
    }
    let mut checked = 0;
    for case in original["cases"].as_array().expect("cases") {
        if !matches!(case["consumer"].as_str(), Some("include" | "incbin")) {
            continue;
        }
        checked += 1;
        let name = case["name"].as_str().expect("name");
        let source = case["source"].as_str().expect("source");
        let cfg = case["config"].as_str().expect("config");
        let (result, debug) =
            asm198x::assemble_ca65_files_debug_with_config(source, "input.s", &loader, cfg)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            case["native_hex"].as_str().expect("native image"),
            "{name}"
        );
        let encoded = debug.to_ndjson();
        assert!(!encoded.contains("ca65-string:"));
        assert_eq!(
            debug198x::DebugInfo::read(&encoded).expect("official reader"),
            debug
        );
        let formatted = asm198x::format_ca65(source).expect("format request source");
        assert_eq!(
            asm198x::format_ca65(&formatted).expect("format again"),
            formatted
        );
        let (again, again_debug) =
            asm198x::assemble_ca65_files_debug_with_config(&formatted, "input.s", &loader, cfg)
                .expect("formatted requests");
        assert_eq!(again.bytes, result.bytes, "{name}: formatted image");
        assert_eq!(again_debug.sections, debug.sections, "{name}: sections");
        assert_eq!(again_debug.symbols, debug.symbols, "{name}: symbols");
    }
    assert_eq!(checked, 88);
    let mut flat_checked = 0;
    for case in original["cases"].as_array().expect("cases") {
        if !case["name"].as_str().expect("name").starts_with("direct_")
            || !matches!(case["consumer"].as_str(), Some("include" | "incbin"))
        {
            continue;
        }
        let source = case["source"].as_str().expect("source");
        for (dialect, result) in [
            (
                "65816",
                asm198x::assemble_ca65_816_files(source, "input.s", &loader),
            ),
            (
                "huc6280",
                asm198x::assemble_ca65_huc6280_files(source, "input.s", &loader),
            ),
        ] {
            flat_checked += 1;
            let result = result.unwrap_or_else(|e| panic!("{dialect}/{}: {e:?}", case["name"]));
            let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(
                hex,
                case["native_hex"].as_str().expect("native image"),
                "{dialect}/{}",
                case["name"]
            );
        }
    }
    assert_eq!(flat_checked, 44);
}

fn verify_cli<'a>(
    cases: &[serde_json::Value],
    files: impl IntoIterator<Item = (&'a String, &'a serde_json::Value)>,
    expected_accepted: usize,
) {
    let root = scratch::dir(&format!("ca65-text-consumer-payloads-{}", cases.len()));
    for (name, text) in files {
        std::fs::write(root.join(name), text.as_str().expect("file text")).expect("fixture file");
    }
    let mut accepted = 0;
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        std::fs::write(
            root.join("input.s"),
            case["source"].as_str().expect("source"),
        )
        .expect("source");
        std::fs::write(
            root.join("input.cfg"),
            case["config"].as_str().expect("config"),
        )
        .expect("config");
        let outputs = ["out.bin", "out.debug198x", "out.sym", "out.lst.json"];
        for file in outputs {
            std::fs::write(root.join(file), b"previous output").expect("previous output");
        }
        let assemble = |source| {
            run(
                &root,
                &[
                    "--dialect",
                    "ca65",
                    "-C",
                    "input.cfg",
                    source,
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
        let native = case["native"]
            .as_array()
            .expect("native stages")
            .last()
            .expect("stage")["exit"]
            == 0;
        assert_eq!(
            result.status.code(),
            Some(if native { 0 } else { 1 }),
            "{name}: {result:?}"
        );
        let payload: serde_json::Value =
            serde_json::from_slice(&result.stdout).expect("complete JSON output");
        let diagnostics = if payload.is_object() {
            payload["warnings"].as_array().expect("warnings")
        } else {
            payload.as_array().expect("diagnostics")
        };
        let messages: Vec<_> = diagnostics.iter().map(|d| d["message"].clone()).collect();
        if let Some(expected) = case["expected_messages"].as_array() {
            assert_eq!(&messages, expected, "{name}: payload");
        }
        assert!(!String::from_utf8_lossy(&result.stdout).contains("ca65-string:"));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("ca65-string:"));
        if !native {
            assert!(!diagnostics.is_empty(), "{name}: missing diagnostic");
            for diagnostic in diagnostics {
                assert_eq!(diagnostic["span"]["path"], "input.s", "{name}: path");
                if name.starts_with("macro_body_") {
                    assert!(
                        !diagnostic["span"]["expansion_frames"]
                            .as_array()
                            .expect("frames")
                            .is_empty(),
                        "{name}: macro frames"
                    );
                }
            }
            for file in outputs {
                assert_eq!(
                    std::fs::read(root.join(file)).expect("preserved output"),
                    b"previous output",
                    "{name}: {file}"
                );
            }
            continue;
        }
        accepted += 1;
        let bytes = std::fs::read(root.join("out.bin")).expect("image");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            case["native_hex"].as_str().expect("native image"),
            "{name}"
        );
        let before = debug198x::DebugInfo::read(
            &std::fs::read_to_string(root.join("out.debug198x")).expect("sidecar"),
        )
        .expect("official reader");
        for file in &outputs[1..] {
            assert!(
                !std::fs::read_to_string(root.join(file))
                    .expect("sidecar text")
                    .contains("ca65-string:"),
                "{name}: {file}"
            );
        }
        let formatted = run(&root, &["fmt", "--dialect", "ca65", "input.s"]);
        assert!(formatted.status.success(), "{name}: {formatted:?}");
        std::fs::write(root.join("formatted.s"), &formatted.stdout).expect("formatted source");
        let twice = run(&root, &["fmt", "--dialect", "ca65", "formatted.s"]);
        assert!(twice.status.success(), "{name}: {twice:?}");
        assert_eq!(twice.stdout, formatted.stdout, "{name}: idempotence");
        let again = assemble("formatted.s");
        assert!(again.status.success(), "{name}: {again:?}");
        let after_payload: serde_json::Value =
            serde_json::from_slice(&again.stdout).expect("formatted JSON");
        let after_messages: Vec<_> = after_payload["warnings"]
            .as_array()
            .expect("formatted warnings")
            .iter()
            .map(|d| d["message"].clone())
            .collect();
        assert_eq!(after_messages, messages, "{name}: formatted payload");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("formatted image"),
            bytes,
            "{name}: formatted bytes"
        );
        let after = debug198x::DebugInfo::read(
            &std::fs::read_to_string(root.join("out.debug198x")).expect("formatted debug"),
        )
        .expect("official reader");
        assert_eq!(after.sections, before.sections, "{name}: sections");
        assert_eq!(after.symbols, before.symbols, "{name}: symbols");
    }
    assert_eq!(accepted, expected_accepted);
}

#[test]
fn portable_native_text_and_assertion_payloads_survive_cli_round_trips() {
    let facts = facts();
    verify_cli(
        facts["cases"].as_array().expect("cases"),
        facts["files"].as_object().expect("files"),
        264,
    );
}

#[cfg(unix)]
#[test]
fn posix_native_filename_payloads_survive_actual_filesystem_round_trips() {
    let facts = facts();
    verify_cli(
        facts["posix_cases"].as_array().expect("POSIX cases"),
        facts["files"]
            .as_object()
            .expect("portable files")
            .iter()
            .chain(facts["posix_files"].as_object().expect("POSIX files")),
        48,
    );
}
