use serde_json::Value;
use std::process::Command;

fn notes(output: &asm198x::AssemblyResult) -> Vec<String> {
    output
        .warnings
        .iter()
        .filter(|w| serde_json::to_value(w).expect("warning kind")["kind"] == "Note")
        .map(|w| w.message.clone())
        .collect()
}

#[test]
fn native_display_bytes_and_notes_match_both_targets_and_formatting() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/sjasmplus-display-format.json"))
            .expect("native facts");
    let cases = fixture["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 132);
    for case in cases {
        let source = case["source"].as_str().expect("source");
        let name = case["name"].as_str().expect("name");
        for assemble in [
            asm198x::assemble_sjasmplus,
            asm198x::assemble_sjasmplus_next,
        ] {
            let result = assemble(source);
            if case["exit"] != 0 {
                assert!(result.is_err(), "{name}: native refusal");
                continue;
            }
            let result = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, case["hex"].as_str().expect("native bytes"), "{name}");
            let stderr = case["stderr"].as_str().expect("native text");
            let expected: Vec<String> = if stderr.is_empty() {
                vec![]
            } else {
                stderr
                    .strip_prefix("> ")
                    .expect("native chrome")
                    .strip_suffix('\n')
                    .expect("newline")
                    .split("\n> ")
                    .map(str::to_owned)
                    .collect()
            };
            assert_eq!(notes(&result), expected, "{name}");
            let formatted = asm198x::format_sjasmplus(source).expect("format");
            assert_eq!(
                asm198x::format_sjasmplus(&formatted).expect("format twice"),
                formatted,
                "{name}"
            );
            let roundtrip = assemble(&formatted).expect("formatted assembly");
            assert_eq!(roundtrip.bytes, result.bytes, "{name}");
            assert_eq!(roundtrip.symbols, result.symbols, "{name}");
            assert_eq!(notes(&roundtrip), expected, "{name}: formatted notes");
        }
    }
}

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn cli_display_notes_and_include_failures_preserve_artifacts() {
    let dir = scratch::dir("sjasm-display-format-cli");
    std::fs::write(
        dir.join("input.asm"),
        " org $8000\n include \"values.inc\"\n",
    )
    .expect("root");
    let names = [
        "image.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for cpu in ["z80", "z80n"] {
        for mode in ["human", "json"] {
            let run = || {
                Command::new(env!("CARGO_BIN_EXE_asm198x"))
                    .current_dir(&dir)
                    .args([
                        "--dialect",
                        "sjasmplus",
                        "--cpu",
                        cpu,
                        "--message-format",
                        mode,
                        "--sym=out.sym",
                        "--debug=out.debug198x",
                        "--listing=out.lst",
                        "--listing-json=out.lst.json",
                        "input.asm",
                        "-o",
                        "image.bin",
                    ])
                    .output()
                    .expect("CLI")
            };
            std::fs::write(
                dir.join("values.inc"),
                " DISPLAY \"Code start: \",/H,$8000\n db 1\n",
            )
            .expect("include");
            let success = run();
            assert!(success.status.success(), "{cpu}/{mode}: {success:?}");
            assert_eq!(std::fs::read(dir.join("image.bin")).expect("image"), [1]);
            if mode == "json" {
                let result: Value = serde_json::from_slice(&success.stdout).expect("JSON result");
                assert!(
                    result.to_string().contains("Code start: 0x8000"),
                    "{result}"
                );
            } else {
                assert!(String::from_utf8_lossy(&success.stderr).contains("Code start: 0x8000"));
            }
            let originals: Vec<_> = names
                .iter()
                .map(|n| std::fs::read(dir.join(n)).expect("artifact"))
                .collect();
            for bad in ["/X,1", "/H,", "missing", "1/0", "\"a\"+1"] {
                let source = format!(" db 0\n db 0\n db 0\n db 0\n DISPLAY {bad}\n");
                std::fs::write(dir.join("values.inc"), &source).expect("bad include");
                let failure = run();
                assert!(!failure.status.success(), "{cpu}/{mode}/{bad}");
                if mode == "json" {
                    let result: Value =
                        serde_json::from_slice(&failure.stdout).expect("JSON errors");
                    assert_eq!(result[0]["span"]["line"], 5);
                    assert!(
                        result[0]["span"]["path"]
                            .as_str()
                            .expect("path")
                            .ends_with("values.inc")
                    );
                } else {
                    assert!(
                        String::from_utf8_lossy(&failure.stderr).contains("values.inc:5"),
                        "{failure:?}"
                    );
                }
                for (name, original) in names.iter().zip(&originals) {
                    assert_eq!(
                        &std::fs::read(dir.join(name)).expect("preserved"),
                        original,
                        "{bad}: {name}"
                    );
                }
                assert_eq!(
                    std::fs::read_to_string(dir.join("values.inc")).expect("source"),
                    source
                );
            }
        }
    }
}
