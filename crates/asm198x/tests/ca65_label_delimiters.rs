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
fn native_label_delimiters_preserve_bytes_and_addresses() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-label-delimiters.json"))
            .expect("native captures");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 24);
    let root = scratch::dir("ca65-label-delimiters");
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
            fact["native_hex"].as_str().expect("native bytes"),
            "{name}"
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
        assert_eq!(shape(&root), debug, "{name}: addresses and symbols");
    }
}

#[test]
fn attached_label_operand_errors_preserve_outputs_and_report_the_file() {
    let root = scratch::dir("ca65-label-delimiters-preserve");
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-label-delimiters.json"))
            .expect("native captures");
    let config = facts["cases"][0]["config"].as_str().expect("config");
    std::fs::write(root.join("input.cfg"), config).expect("config");
    let source = ".include \"definitions.inc\"\n";
    let invalid = "entry:.byte 256\n";
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
        std::fs::write(root.join("definitions.inc"), "entry:.byte 7\n.word entry\n")
            .expect("valid include");
        let success = execute();
        assert!(success.status.success(), "{success:?}");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("image"),
            [7, 0, 0x10]
        );
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
            assert_eq!(errors[0]["span"]["line"], 1);
            assert!(
                errors[0]["span"]["path"]
                    .as_str()
                    .expect("path")
                    .ends_with("definitions.inc")
            );
        } else {
            assert!(
                String::from_utf8_lossy(&failure.stderr).contains("definitions.inc:1:"),
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

#[test]
fn include_request_columns_point_into_the_authored_line() {
    let root = scratch::dir("ca65-label-delimiter-columns");
    for source in [
        "entry:.include \"nowhere.inc\"\n",
        "   entry:    .include \"nowhere.inc\"\n",
        "        .include \"nowhere.inc\"\n",
    ] {
        std::fs::write(root.join("input.s"), source).expect("source");
        std::fs::write(root.join("out.bin"), b"previous image").expect("seed output");
        let result = run(
            &root,
            &[
                "--dialect",
                "ca65",
                "--message-format",
                "json",
                "input.s",
                "-o",
                "out.bin",
            ],
        );
        assert!(!result.status.success(), "{result:?}");
        let diagnostics: serde_json::Value =
            serde_json::from_slice(&result.stdout).expect("diagnostics");
        let span = &diagnostics[0]["span"];
        assert_eq!(span["line"], 1);
        assert_eq!(
            span["col"],
            source.find('"').expect("request literal") + 1,
            "{source}"
        );
        assert!(span["path"].as_str().expect("path").ends_with("input.s"));
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("preserved output"),
            b"previous image"
        );
    }
}
