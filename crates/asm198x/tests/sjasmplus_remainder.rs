use serde_json::Value;
use std::process::Command;

#[test]
fn native_remainder_and_binary_prefix_boundaries_match_both_frontends() {
    let facts: Value = serde_json::from_str(include_str!("fixtures/sjasmplus-remainder.json"))
        .expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 236);
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
            let result = result.unwrap_or_else(|error| panic!("{name}: {error:?}"));
            let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, case["hex"].as_str().expect("native bytes"), "{name}");
            let formatted = asm198x::format_sjasmplus(source).expect("format native expression");
            assert_eq!(
                asm198x::format_sjasmplus(&formatted).expect("format again"),
                formatted,
                "{name}"
            );
            let roundtrip = assemble(&formatted).expect("reassemble formatted source");
            assert_eq!(roundtrip.bytes, result.bytes, "{name}: formatted bytes");
            assert_eq!(
                roundtrip.symbols, result.symbols,
                "{name}: formatted symbols"
            );
        }
    }
}

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn remainder_refusals_keep_include_diagnostics_and_previous_artifacts() {
    let facts: Value = serde_json::from_str(include_str!("fixtures/sjasmplus-remainder.json"))
        .expect("native facts");
    let fact = facts["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["name"] == "original")
        .expect("native struct control");
    let source = fact["source"].as_str().expect("source");
    let dir = scratch::dir("sjasm-remainder-cli");
    std::fs::write(
        dir.join("input.asm"),
        " org $8000\n include \"values.inc\"\n",
    )
    .expect("root source");
    let names = [
        "image.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for cpu in ["z80", "z80n"] {
        for mode in ["human", "json"] {
            std::fs::write(dir.join("values.inc"), source).expect("included native source");
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
            let success = run();
            assert!(success.status.success(), "{cpu}/{mode}: {success:?}");
            let image = std::fs::read(dir.join("image.bin")).expect("image");
            let hex: String = image.iter().map(|byte| format!("{byte:02x}")).collect();
            assert_eq!(hex, fact["hex"].as_str().expect("native image"));
            let originals: Vec<_> = names
                .iter()
                .map(|name| std::fs::read(dir.join(name)).expect("artifact"))
                .collect();
            for condition in ["5%0", "5 MOD 0", "1 || (5%0)"] {
                let changed = format!(" db 0\n db 0\n db 0\n db 0\n dd {condition}\n");
                std::fs::write(dir.join("values.inc"), &changed).expect("failing include");
                let failure = run();
                assert!(!failure.status.success(), "{cpu}/{mode}: {condition}");
                if mode == "json" {
                    let diagnostics: Value =
                        serde_json::from_slice(&failure.stdout).expect("diagnostics");
                    assert_eq!(diagnostics[0]["span"]["line"], 5);
                    assert!(
                        diagnostics[0]["span"]["path"]
                            .as_str()
                            .expect("source path")
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
                        &std::fs::read(dir.join(name)).expect("preserved artifact"),
                        original,
                        "{condition}: {name}"
                    );
                }
                assert_eq!(
                    std::fs::read_to_string(dir.join("values.inc")).expect("unchanged source"),
                    changed
                );
            }
        }
    }
}

#[test]
fn pasmo_keeps_its_expression_and_binary_literal_rules() {
    for source in [" db 5%2\n", " db 5 MOD 2\n", " db 5 mod 2\n"] {
        assert!(asm198x::assemble_pasmo(source).is_err());
        assert!(asm198x::assemble_pasmonext(source).is_err());
    }
    assert_eq!(
        asm198x::assemble_pasmo("MOD equ 3\n db MOD,%101\n")
            .expect("Pasmo symbols and binary")
            .bytes,
        [3, 5]
    );
}
