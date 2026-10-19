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
fn native_pasmo_character_boundaries_match_both_frontends() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/pasmo-character-literals.json"))
            .expect("native facts");
    let cases = facts["cases"].as_array().expect("native cases");
    assert_eq!(cases.len(), 6);
    for case in cases {
        let source = case["source"].as_str().expect("source");
        let name = case["name"].as_str().expect("name");
        let accepted = case["native"][0]["exit"] == 0;
        for assemble in [asm198x::assemble_pasmo, asm198x::assemble_pasmonext] {
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
            let formatted = asm198x::format_pasmo(source).expect("format native source");
            assert_eq!(
                asm198x::format_pasmo(&formatted).expect("format again"),
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
fn invalid_included_pasmo_literal_preserves_outputs_and_source() {
    let root = scratch::dir("pasmo-character-literal-preserve");
    let source = " include \"definitions.inc\"\n";
    let valid = " dw 'A'\n";
    let invalid = " ; included context\n dw '''\n";
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
                        "pasmo",
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
                [0x41, 0]
            );
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
