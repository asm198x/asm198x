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
fn native_quoted_admission_checks_tokens_and_error_order() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-quoted-admission.json"))
            .expect("native captures");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 549);
    let root = scratch::dir("ca65-quoted-admission");
    for (name, text) in facts["files"].as_object().expect("includes") {
        std::fs::create_dir_all(root.join(name).parent().expect("include parent"))
            .expect("include directories");
        std::fs::write(root.join(name), text.as_str().expect("include text")).expect("include");
    }
    for fact in cases {
        let name = fact["name"].as_str().expect("name");
        let dialect = fact["dialect"].as_str().expect("dialect");
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
        let comma_gap = fact["assembly_gaps"] == serde_json::json!([900]);
        if comma_gap {
            assert_eq!(name, "ascii_44_direct");
        } else {
            assert!(fact["assembly_gaps"].is_null(), "{name}: untracked gap");
        }
        let accepted = !comma_gap
            && fact["native"]
                .as_array()
                .expect("native stages")
                .last()
                .expect("native result")["exit"]
                == 0;
        assert_eq!(result.status.success(), accepted, "{name}: {result:?}");
        if !accepted {
            assert_eq!(result.status.code(), Some(1), "{name}: {result:?}");
            for output in outputs {
                assert_eq!(
                    std::fs::read(root.join(output)).expect("preserved output"),
                    b"previous image",
                    "{name}: {output}"
                );
            }
            let diagnostics: serde_json::Value =
                serde_json::from_slice(&result.stdout).expect("JSON diagnostics");
            assert_eq!(diagnostics[0]["span"]["path"], "input.s", "{name}");
            if !comma_gap {
                assert_eq!(
                    diagnostics[0]["span"]["line"], fact["native_error_line"],
                    "{name}"
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
        // Independently reproduced formatter gaps are tracked by issue.
        if fact["formatter_gap"].is_string() {
            continue;
        }
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
        assert_eq!(shape(&root), debug, "{name}: addresses and symbols");
    }
}

#[test]
fn quoted_capture_errors_keep_included_source_locations() {
    let root = scratch::dir("ca65-quoted-admission-includes");
    std::fs::write(root.join("input.s"), ".include \"definitions.inc\"\n").expect("root source");
    for dialect in ["ca65", "65816", "huc6280"] {
        for token in ["'AB'", "\"open"] {
            std::fs::write(
                root.join("definitions.inc"),
                format!("; before capture\n.macro unused\n.byte {token}\n.endmacro\n"),
            )
            .expect("included source");
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
                serde_json::from_slice(&result.stdout).expect("JSON diagnostic");
            let path = diagnostics[0]["span"]["path"]
                .as_str()
                .expect("include path");
            assert!(
                Path::new(path).ends_with("definitions.inc"),
                "{dialect}: {path}"
            );
            assert_ne!(diagnostics[0]["span"]["file"], 0);
            assert_eq!(diagnostics[0]["span"]["line"], 3);
            for output in outputs {
                assert_eq!(
                    std::fs::read(root.join(output)).expect("preserved output"),
                    b"previous output",
                    "{dialect}: {token}: {output}"
                );
            }
        }
    }
}
