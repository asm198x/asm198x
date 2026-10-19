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

#[test]
fn native_reserved_labels_follow_cpu_and_token_context() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-reserved-labels.json"))
            .expect("native captures");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 113);
    let root = scratch::dir("ca65-reserved-labels");
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
            args.extend([input, "-o", "out.bin", "--debug=out.debug198x"]);
            run(&root, &args)
        };
        std::fs::write(root.join("out.bin"), b"previous image").expect("seed image");
        let result = assemble("input.s");
        let accepted = fact["native"]
            .as_array()
            .expect("native stages")
            .last()
            .expect("native result")["exit"]
            == 0;
        assert_eq!(result.status.success(), accepted, "{name}: {result:?}");
        if !accepted {
            assert_eq!(result.status.code(), Some(1), "{name}: {result:?}");
            assert_eq!(
                std::fs::read(root.join("out.bin")).expect("preserved image"),
                b"previous image",
                "{name}"
            );
            continue;
        }
        let shape = || {
            std::fs::read_to_string(root.join("out.debug198x"))
                .expect("debug sidecar")
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("debug record"))
                .filter(|record| record["t"] == "symbol" || record["t"] == "section")
                .collect::<Vec<_>>()
        };
        let before = shape();
        let bytes = std::fs::read(root.join("out.bin")).expect("image");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            fact["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
        if fact["formatter_gap"].is_string() {
            continue;
        }
        let formatted = run(&root, &["fmt", "--dialect", dialect, "input.s"]);
        assert!(formatted.status.success(), "{name}: {formatted:?}");
        std::fs::write(root.join("formatted.s"), &formatted.stdout).expect("formatted source");
        let again = run(&root, &["fmt", "--dialect", dialect, "formatted.s"]);
        assert!(again.status.success(), "{name}: {again:?}");
        assert_eq!(formatted.stdout, again.stdout, "{name}: idempotence");
        let roundtrip = assemble("formatted.s");
        assert!(roundtrip.status.success(), "{name}: {roundtrip:?}");
        assert_eq!(shape(), before, "{name}: formatted symbols and sections");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("formatted bytes"),
            bytes,
            "{name}"
        );
    }
}

#[test]
fn reserved_label_failures_preserve_outputs_and_include_location() {
    let root = scratch::dir("ca65-reserved-labels-preserve");
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-reserved-labels.json"))
            .expect("native captures");
    let config = facts["cases"][0]["config"].as_str().expect("config");
    std::fs::write(root.join("input.cfg"), config).expect("config");
    let source = ".include \"definitions.inc\"\n";
    let invalid = "; file context\nlda:.byte 7\n";
    std::fs::write(root.join("input.s"), source).expect("source");
    let outputs = [
        "out.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for dialect in ["ca65"] {
        for mode in ["human", "json"] {
            let execute = || {
                let mut args = vec!["--dialect", dialect];
                if dialect == "ca65" {
                    args.extend(["-C", "input.cfg"]);
                }
                args.extend([
                    "--message-format",
                    mode,
                    "input.s",
                    "-o",
                    "out.bin",
                    "--sym=out.sym",
                    "--debug=out.debug198x",
                    "--listing=out.lst",
                    "--listing-json=out.lst.json",
                ]);
                run(&root, &args)
            };
            std::fs::write(root.join("definitions.inc"), ".byte 7\n").expect("valid include");
            let success = execute();
            assert!(success.status.success(), "{success:?}");
            assert_eq!(std::fs::read(root.join("out.bin")).expect("image"), [7]);
            let previous: Vec<_> = outputs
                .iter()
                .map(|name| std::fs::read(root.join(name)).expect("output"))
                .collect();
            std::fs::write(root.join("definitions.inc"), invalid).expect("invalid include");
            let failure = execute();
            assert_eq!(failure.status.code(), Some(1), "{failure:?}");
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
}
