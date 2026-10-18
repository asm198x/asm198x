use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[path = "support/scratch.rs"]
mod scratch;

fn run(root: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn CLI");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().expect("poll CLI").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill timed-out CLI");
            let output = child.wait_with_output().expect("reap timed-out CLI");
            panic!("CLI failed to complete within five seconds: {args:?}: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().expect("CLI output")
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
fn native_size_overrides_and_malformed_colons_complete_and_match() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-address-size.json"))
            .expect("native captures");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 113);
    let root = scratch::dir("ca65-address-size");
    for (name, text) in facts["files"].as_object().expect("includes") {
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
fn include_refusals_complete_with_locations_and_preserve_previous_outputs() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-address-size.json"))
            .expect("native captures");
    let fact = facts["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == "include_debug")
        .expect("include control");
    let root = scratch::dir("ca65-address-size-preserve");
    let source = fact["source"].as_str().expect("source");
    std::fs::write(root.join("input.s"), source).expect("source");
    std::fs::write(
        root.join("input.cfg"),
        fact["config"].as_str().expect("config"),
    )
    .expect("config");
    let outputs = [
        "out.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for mode in ["human", "json"] {
        for invalid in [
            " nop\n nop\n lda a:$10000\n",
            " nop\n nop\n lda z:$100\n",
            " nop\n nop\n lda q:$12\n",
            " nop\n nop\n .byte foo:\n",
            " nop\n nop\n lda a:\n",
            ".p4510\n nop\n phw a:$10000\n",
        ] {
            std::fs::write(
                root.join("size.inc"),
                facts["files"]["size.inc"].as_str().expect("include"),
            )
            .expect("include");
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
            let success = execute();
            assert!(success.status.success(), "{success:?}");
            let hex: String = std::fs::read(root.join("out.bin"))
                .expect("image")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(hex, fact["native_hex"].as_str().expect("native image"));
            let previous: Vec<_> = outputs
                .iter()
                .map(|name| std::fs::read(root.join(name)).expect("output"))
                .collect();
            std::fs::write(root.join("size.inc"), invalid).expect("failing include");
            let failure = execute();
            assert!(!failure.status.success(), "{failure:?}");
            if mode == "json" {
                let errors: serde_json::Value =
                    serde_json::from_slice(&failure.stdout).expect("JSON diagnostics");
                assert_eq!(errors[0]["span"]["line"], 3);
                assert!(
                    errors[0]["span"]["path"]
                        .as_str()
                        .expect("error path")
                        .ends_with("size.inc")
                );
            } else {
                assert!(
                    String::from_utf8_lossy(&failure.stderr).contains("size.inc:3"),
                    "{failure:?}"
                );
            }
            for (name, previous) in outputs.iter().zip(previous) {
                assert_eq!(
                    std::fs::read(root.join(name)).expect("preserved output"),
                    previous,
                    "{name}"
                );
            }
            assert_eq!(
                std::fs::read_to_string(root.join("input.s")).expect("source"),
                source
            );
            assert_eq!(
                std::fs::read_to_string(root.join("size.inc")).expect("include"),
                invalid
            );
        }
    }
}
