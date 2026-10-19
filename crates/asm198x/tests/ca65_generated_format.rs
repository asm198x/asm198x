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
fn generated_label_source_gets_canonical_label_placement() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-generated-format.json"))
            .expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 46);
    let root = scratch::dir("ca65-generated-format");
    for (name, text) in facts["files"].as_object().expect("files") {
        std::fs::write(root.join(name), text.as_str().expect("include text")).expect("include");
    }
    let mut known_original_gaps = 0;
    for fact in cases {
        let name = fact["name"].as_str().expect("name");
        let source = fact["source"].as_str().expect("source");
        let formatted = asm198x::format_ca65(source).expect(name);
        for head in fact["generated_heads"].as_array().expect("heads") {
            let label = format!("{}:", head.as_str().expect("head"));
            assert!(
                formatted.lines().any(|line| line == label),
                "{name}: label must have column 0 and its own line: {formatted}"
            );
        }
        assert_eq!(
            asm198x::format_ca65(&formatted).expect(name),
            formatted,
            "{name}: idempotence"
        );
        std::fs::write(root.join("input.s"), source).expect("source");
        std::fs::write(root.join("formatted.s"), &formatted).expect("formatted source");
        std::fs::write(
            root.join("input.cfg"),
            fact["config"].as_str().expect("config"),
        )
        .expect("config");
        let accepted = fact["native"]
            .as_array()
            .expect("stages")
            .last()
            .expect("native result")["exit"]
            == 0;
        let original_gap = fact["original_assembly_gap"].is_number();
        if original_gap {
            known_original_gaps += 1;
        }
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
        let mut original_shape = None;
        for (input, succeeds) in [
            ("input.s", accepted && !original_gap),
            (
                "formatted.s",
                accepted && fact["formatted_assembly_gap"].is_null(),
            ),
        ] {
            std::fs::write(root.join("out.bin"), b"previous image").expect("seed output");
            let result = assemble(input);
            assert_eq!(
                result.status.code(),
                Some(if succeeds { 0 } else { 1 }),
                "{name}/{input}: {result:?}"
            );
            if !succeeds {
                assert_eq!(
                    std::fs::read(root.join("out.bin")).expect("preserved image"),
                    b"previous image",
                    "{name}/{input}"
                );
                continue;
            }
            let hex: String = std::fs::read(root.join("out.bin"))
                .expect("image")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(
                hex,
                fact["native_hex"].as_str().expect("native bytes"),
                "{name}/{input}"
            );
            let debug = debug198x::DebugInfo::read(
                &std::fs::read_to_string(root.join("out.debug198x")).expect("sidecar"),
            )
            .expect("official reader");
            for (symbol, value) in fact["symbol_values"].as_object().expect("symbols") {
                assert_eq!(
                    debug.addr_of(symbol, None),
                    value.as_u64(),
                    "{name}/{input}: {symbol}"
                );
            }
            if input == "input.s" {
                original_shape = Some(shape(&root));
            } else if let Some(before) = &original_shape {
                assert_eq!(&shape(&root), before, "{name}: symbols and sections");
            }
        }
    }
    assert_eq!(known_original_gaps, 17);
}

#[test]
fn included_generated_label_errors_preserve_outputs_and_sources() {
    let root = scratch::dir("ca65-generated-format-errors");
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-generated-format.json"))
            .expect("native facts");
    std::fs::write(
        root.join("input.cfg"),
        facts["cases"][0]["config"].as_str().expect("config"),
    )
    .expect("config");
    let source = ".include \"definitions.inc\"\n";
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
        std::fs::write(root.join("definitions.inc"), ".ident(\"A\"):.byte 7\n")
            .expect("valid include");
        let success = execute();
        assert!(success.status.success(), "{success:?}");
        assert_eq!(std::fs::read(root.join("out.bin")).expect("image"), [7]);
        let previous: Vec<_> = outputs
            .iter()
            .map(|name| std::fs::read(root.join(name)).expect("output"))
            .collect();
        for case in facts["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .filter(|c| {
                c["name"].as_str().expect("name").starts_with("invalid_")
                    || c["name"] == "unclosed_argument"
            })
        {
            let name = case["name"].as_str().expect("name");
            let invalid = format!(
                "; include context\n{}",
                case["source"].as_str().expect("source")
            );
            std::fs::write(root.join("definitions.inc"), &invalid).expect("invalid include");
            let formatted = run(&root, &["fmt", "--dialect", "ca65", "input.s"]);
            assert!(
                formatted.status.success(),
                "{name}: format must not open include: {formatted:?}"
            );
            let failure = execute();
            assert_eq!(failure.status.code(), Some(1), "{name}/{mode}: {failure:?}");
            if mode == "json" {
                let errors: serde_json::Value =
                    serde_json::from_slice(&failure.stdout).expect("diagnostics");
                assert_eq!(errors[0]["span"]["line"], 2, "{name}");
                assert!(
                    errors[0]["span"]["path"]
                        .as_str()
                        .expect("path")
                        .ends_with("definitions.inc")
                );
            } else {
                assert!(
                    String::from_utf8_lossy(&failure.stderr).contains("definitions.inc:2:"),
                    "{name}: {failure:?}"
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
