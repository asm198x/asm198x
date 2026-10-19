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
fn native_single_quotes_and_empty_lists_follow_their_context() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/sjasmplus-packed-single.json"))
            .expect("native facts");
    let cases = facts["cases"].as_array().expect("native cases");
    assert_eq!(cases.len(), 293);
    for case in cases {
        let source = case["source"].as_str().expect("source");
        let name = case["name"].as_str().expect("name");
        let accepted = case["native"][0]["exit"] == 0;
        for assemble in [
            asm198x::assemble_sjasmplus,
            asm198x::assemble_sjasmplus_next,
        ] {
            let result = assemble(source);
            assert_eq!(result.is_ok(), accepted, "{name}: {result:?}");
            if !accepted {
                continue;
            }
            let result = result.expect("native acceptance");
            let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(
                hex,
                case["native_hex"].as_str().expect("native bytes"),
                "{name}"
            );
            let warnings = case["native_warning_count"]
                .as_u64()
                .expect("native warnings") as usize;
            let known_missing = case["known_missing_warning_count"]
                .as_u64()
                .expect("tracked warning gaps") as usize;
            assert!(
                (warnings - known_missing..=warnings).contains(&result.warnings.len()),
                "{name}: warnings {:?}, native {warnings}, tracked gap {known_missing}",
                result.warnings
            );
            let formatted = asm198x::format_sjasmplus(source).expect("format native source");
            if [
                "data_db_2",
                "data_dw_2",
                "data_d24_2",
                "data_dd_2",
                "data_dc_2",
                "scalar_2",
            ]
            .contains(&name)
            {
                assert!(
                    formatted.contains("'FF'"),
                    "{name}: preserve operand spelling: {formatted}"
                );
            }
            assert_eq!(
                asm198x::format_sjasmplus(&formatted).expect("format again"),
                formatted,
                "{name}: idempotence"
            );
            let roundtrip = assemble(&formatted).expect("formatted assembly");
            assert_eq!(roundtrip.bytes, result.bytes, "{name}: formatted bytes");
            assert_eq!(
                roundtrip.symbols, result.symbols,
                "{name}: formatted symbols"
            );
        }
    }
}

#[test]
fn invalid_included_quotes_and_empty_lists_preserve_outputs_and_source() {
    let root = scratch::dir("sjasmplus-packed-single-preserve");
    let source = " org $8000\n include \"definitions.inc\"\n";
    let valid = " ; included context\n dd 'FF'\n";
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/sjasmplus-packed-single.json"))
            .expect("native facts");
    let cases = facts["cases"].as_array().expect("native cases");
    std::fs::write(root.join("input.asm"), source).expect("source");
    let outputs = [
        "out.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for cpu in ["z80", "z80n"] {
        for mode in ["human", "json"] {
            let execute = || {
                run(
                    &root,
                    &[
                        "--dialect",
                        "sjasmplus",
                        "--cpu",
                        cpu,
                        "--message-format",
                        mode,
                        "input.asm",
                        "-o",
                        "out.bin",
                        "--sym=out.sym",
                        "--debug=out.debug198x",
                        "--listing=out.lst",
                        "--listing-json=out.lst.json",
                    ],
                )
            };
            std::fs::write(root.join("definitions.inc"), valid).expect("valid include");
            let success = execute();
            assert!(success.status.success(), "{success:?}");
            assert_eq!(
                std::fs::read(root.join("out.bin")).expect("image"),
                [0x46, 0x46, 0, 0]
            );
            let previous: Vec<_> = outputs
                .iter()
                .map(|name| std::fs::read(root.join(name)).expect("output"))
                .collect();
            for name in [
                "malformed_unclosed",
                "data_db_0",
                "empty_data_0",
                "control_0_dd",
            ] {
                let case = cases
                    .iter()
                    .find(|c| c["name"] == name)
                    .expect("native refusal");
                assert_eq!(case["native"][0]["exit"], 1);
                let invalid = format!(
                    " ; included context\n{}",
                    case["source"].as_str().expect("source")
                );
                std::fs::write(root.join("definitions.inc"), &invalid).expect("invalid include");
                let failure = execute();
                assert_eq!(
                    failure.status.code(),
                    Some(1),
                    "{name}/{cpu}/{mode}: {failure:?}"
                );
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
                    std::fs::read_to_string(root.join("input.asm")).expect("source"),
                    source
                );
                assert_eq!(
                    std::fs::read_to_string(root.join("definitions.inc")).expect("include"),
                    invalid
                );
            }
        }
    }
}
