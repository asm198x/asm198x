use serde_json::Value;

#[test]
fn native_word_operator_results_and_refusals_match_both_frontends() {
    let facts: Value = serde_json::from_str(include_str!("fixtures/sjasmplus-word-operators.json"))
        .expect("native facts");
    for case in facts["cases"].as_array().expect("cases") {
        let source = case["source"].as_str().expect("source");
        let name = case["name"].as_str().expect("name");
        for assemble in [
            asm198x::assemble_sjasmplus,
            asm198x::assemble_sjasmplus_next,
        ] {
            let result = assemble(source);
            if case["exit"] != 0 {
                assert!(result.is_err(), "{name}: native refusal");
            } else {
                let result = result.expect(name);
                let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
                assert_eq!(hex, case["hex"].as_str().expect("native bytes"), "{name}");
                let formatted = asm198x::format_sjasmplus(source).expect("format");
                assert_eq!(
                    asm198x::format_sjasmplus(&formatted).expect("format twice"),
                    formatted,
                    "{name}"
                );
                let roundtrip = assemble(&formatted).expect("reassemble");
                assert_eq!(roundtrip.bytes, result.bytes, "{name}: formatter bytes");
                assert_eq!(
                    roundtrip.symbols, result.symbols,
                    "{name}: formatter symbols"
                );
            }
        }
    }
}

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn included_word_expressions_keep_cli_artifacts_and_failures_consistent() {
    use std::process::Command;
    let dir = scratch::dir("sjasm-words-cli");
    let include = "value equ NOT 0\n dw 1 XOR 2,value AND 2,1 OR 2 XOR 3\n";
    std::fs::write(dir.join("values.inc"), include).expect("include");
    for cpu in ["z80", "z80n"] {
        for mode in ["human", "json"] {
            let source = " org $8000\n include \"values.inc\"\n";
            std::fs::write(dir.join("input.asm"), source).expect("source");
            let run = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                .current_dir(&dir)
                .args([
                    "--dialect",
                    "sjasmplus",
                    "--cpu",
                    cpu,
                    "--message-format",
                    mode,
                    "--debug=out.debug198x",
                    "--sym=out.sym",
                    "--listing=out.lst",
                    "--listing-json=out.lst.json",
                    "input.asm",
                    "-o",
                    "image.bin",
                ])
                .output()
                .expect("CLI");
            assert!(run.status.success(), "{cpu}/{mode}: {run:?}");
            // Components are the captured native XOR, EQU/AND and precedence results.
            let bytes = std::fs::read(dir.join("image.bin")).expect("image");
            assert_eq!(bytes, [3, 0, 2, 0, 1, 0]);
            let listing: Value =
                serde_json::from_slice(&std::fs::read(dir.join("out.lst.json")).expect("listing"))
                    .expect("JSON listing");
            assert!(
                listing["lines"]
                    .as_array()
                    .expect("lines")
                    .iter()
                    .any(|line| line["address"] == 32768 && line["bytes"] == 6)
            );
            assert!(
                std::fs::read_to_string(dir.join("out.sym"))
                    .expect("symbols")
                    .contains(&format!("value = ${:04X}", -1i64))
            );
            std::fs::write(dir.join("input.asm"), " org $8000\n dw 1 xOr 2\n")
                .expect("invalid source");
            let failed = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                .current_dir(&dir)
                .args([
                    "--dialect",
                    "sjasmplus",
                    "--cpu",
                    cpu,
                    "--message-format",
                    mode,
                    "input.asm",
                    "-o",
                    "image.bin",
                ])
                .output()
                .expect("refusal");
            assert!(!failed.status.success(), "{cpu}/{mode}: mixed case refused");
            assert_eq!(
                std::fs::read(dir.join("image.bin")).expect("preserved"),
                bytes
            );
            assert_eq!(
                std::fs::read_to_string(dir.join("values.inc")).expect("unchanged include"),
                include
            );
        }
    }
}

#[test]
fn pasmo_keeps_its_distinct_not_precedence_and_case_rules() {
    let sjasm = asm198x::assemble_sjasmplus(" dw NOT 1,1 OR 2 XOR 3\n").expect("SjASMPlus");
    let pasmo = asm198x::assemble_pasmo(" dw NOT 1,1 OR 2 XOR 3\n").expect("Pasmo");
    assert_eq!(sjasm.bytes, [0, 0, 1, 0]);
    assert_eq!(pasmo.bytes, [254, 255, 0, 0]);
    assert!(asm198x::assemble_sjasmplus(" dw 1 xOr 2\n").is_err());
    assert_eq!(
        asm198x::assemble_pasmo(" dw 1 xOr 2\n")
            .expect("Pasmo mixed case")
            .bytes,
        [3, 0]
    );
}
