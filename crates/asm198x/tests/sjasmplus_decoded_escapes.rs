use serde_json::Value;

fn facts() -> Value {
    serde_json::from_str(include_str!("fixtures/sjasmplus-decoded-composition.json"))
        .expect("native facts")
}

#[test]
fn decoded_strings_keep_native_composition_packing_and_refusals() {
    for case in facts()["cases"].as_array().expect("cases") {
        let source = case["source"].as_str().expect("source");
        let name = case["name"].as_str().expect("name");
        let mut loader = asm198x::source::MemoryLoader::new();
        for (name, text) in case["files"].as_object().expect("files") {
            loader = loader.text(name, text.as_str().expect("include text"));
        }
        for assemble in [
            asm198x::assemble_sjasmplus_files,
            asm198x::assemble_sjasmplus_next_files,
        ] {
            let result = assemble(source, "input.asm", &loader);
            if case["exit"] != 0 {
                assert!(result.is_err(), "{name}: native refusal");
                continue;
            }
            let result = result.expect(name);
            let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, case["hex"].as_str().expect("native bytes"), "{name}");
            let formatted = asm198x::format_sjasmplus(source).expect("format");
            assert_eq!(
                asm198x::format_sjasmplus(&formatted).expect("format twice"),
                formatted,
                "{name}"
            );
            assert_eq!(
                assemble(&formatted, "input.asm", &loader)
                    .expect("reassemble")
                    .bytes,
                result.bytes,
                "{name}: formatted bytes"
            );
        }
    }
}

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn cli_decoded_artifacts_match_native_and_refusals_preserve_images() {
    use std::process::Command;
    let dir = scratch::dir("sjasm-decoded-cli");
    for case in facts()["cases"].as_array().expect("cases") {
        let source = case["source"].as_str().expect("source");
        std::fs::write(dir.join("input.asm"), source).expect("source");
        for (name, text) in case["files"].as_object().expect("files") {
            std::fs::write(dir.join(name), text.as_str().expect("include"))
                .expect("include source");
        }
        for cpu in ["z80", "z80n"] {
            for mode in ["human", "json"] {
                std::fs::write(dir.join("image.bin"), b"previous image").expect("previous image");
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
                if case["exit"] != 0 {
                    assert!(!run.status.success(), "{}: {run:?}", case["name"]);
                    assert_eq!(
                        std::fs::read(dir.join("image.bin")).expect("preserved image"),
                        b"previous image"
                    );
                } else {
                    assert!(run.status.success(), "{}: {run:?}", case["name"]);
                    let bytes = std::fs::read(dir.join("image.bin")).expect("image");
                    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                    assert_eq!(
                        hex,
                        case["hex"].as_str().expect("native bytes"),
                        "{}",
                        case["name"]
                    );
                    if mode == "json" {
                        let result: Value =
                            serde_json::from_slice(&run.stdout).expect("JSON result");
                        assert_eq!(result["bytes"], serde_json::json!(bytes));
                    }
                    let listing: Value = serde_json::from_slice(
                        &std::fs::read(dir.join("out.lst.json")).expect("listing"),
                    )
                    .expect("JSON listing");
                    assert_eq!(
                        listing["lines"]
                            .as_array()
                            .expect("lines")
                            .iter()
                            .map(|line| line["bytes"].as_u64().expect("length"))
                            .sum::<u64>(),
                        bytes.len() as u64,
                        "{}: captured byte lengths",
                        case["name"]
                    );
                }
                assert_eq!(
                    std::fs::read_to_string(dir.join("input.asm")).expect("source remains"),
                    source
                );
            }
        }
    }
}
