use serde_json::Value;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn native_nex_images_and_refusals_match_both_targets_and_formatter() {
    let facts: Value = serde_json::from_str(include_str!("fixtures/sjasmplus-savenex.json"))
        .expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 70);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let source = case["source"].as_str().expect("source");
        for assemble in [
            asm198x::assemble_sjasmplus,
            asm198x::assemble_sjasmplus_next,
        ] {
            let result = assemble(source);
            // V1.3 and appended input files are separate, explicit unsupported
            // boundaries of the approved V1.2 slice.
            if case["exit"] != 0 || name == "open_8" {
                assert!(result.is_err(), "{name}: refusal");
                if name == "open_8" {
                    assert_eq!(
                        result.expect_err("V1.3 boundary").code,
                        asm198x::Code::UnsupportedFeature
                    );
                }
                continue;
            }
            let result = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let native_warnings = case["stderr"]
                .as_str()
                .expect("stderr")
                .lines()
                .filter(|line| line.contains(": warning:"))
                .count();
            assert_eq!(result.warnings.len(), native_warnings, "{name}: warnings");
            let outputs = case["outputs"].as_array().expect("native outputs");
            let expected: Vec<_> = outputs.iter().filter(|o| o["name"] != "raw.bin").collect();
            // Native overwrites repeated names in order. The library retains
            // both requests; the CLI writes the last image to that path.
            if name == "duplicate_name" {
                assert_eq!(result.artifacts.len(), 2);
                let first = cases
                    .iter()
                    .find(|c| c["name"] == "implicit_close")
                    .expect("native first save")["outputs"]
                    .as_array()
                    .expect("outputs")
                    .iter()
                    .find(|o| o["name"] == "out.nex")
                    .expect("first image")["hex"]
                    .as_str()
                    .expect("first bytes");
                assert_eq!(hex(&result.artifacts[0].bytes), first);
                assert_eq!(
                    hex(&result.artifacts[1].bytes),
                    expected[0]["hex"].as_str().expect("last bytes")
                );
            } else {
                assert_eq!(result.artifacts.len(), expected.len(), "{name}");
                for (artifact, native) in result.artifacts.iter().zip(expected) {
                    assert_eq!(
                        artifact.name,
                        native["name"].as_str().expect("name"),
                        "{name}"
                    );
                    assert_eq!(
                        serde_json::to_value(artifact.format).expect("format"),
                        "nex"
                    );
                    assert_eq!(
                        hex(&artifact.bytes),
                        native["hex"].as_str().expect("image"),
                        "{name}: image"
                    );
                }
            }
            let raw = outputs
                .iter()
                .find(|o| o["name"] == "raw.bin")
                .expect("raw");
            assert_eq!(
                hex(&result.bytes),
                raw["hex"].as_str().expect("raw bytes"),
                "{name}: raw"
            );
            let formatted = asm198x::format_sjasmplus(source).expect("format");
            assert_eq!(
                asm198x::format_sjasmplus(&formatted).expect("format twice"),
                formatted,
                "{name}"
            );
            let again = assemble(&formatted).expect("formatted assembly");
            assert_eq!(again.artifacts, result.artifacts, "{name}: artifacts");
            assert_eq!(again.bytes, result.bytes, "{name}: bytes");
            assert_eq!(again.symbols, result.symbols, "{name}: symbols");
        }
    }
}

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn cli_nex_outputs_obey_host_controls_and_preserve_previous_files() {
    use std::process::Command;
    let dir = scratch::dir("sjasm-savenex-cli");
    std::fs::create_dir(dir.join("saved")).expect("prefix");
    std::fs::write(dir.join("input.asm"), " include \"values.inc\"\n").expect("root");
    let good = " DEVICE ZXSPECTRUMNEXT\n org $8000\n db 1,2,3\n SAVENEX OPEN \"out.nex\",$8000,$b000\n SAVENEX CORE 3,0,0\n SAVENEX AUTO\n SAVENEX CLOSE\n";
    let facts: Value =
        serde_json::from_str(include_str!("fixtures/sjasmplus-savenex.json")).expect("facts");
    let native = facts["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == "original_shape")
        .expect("native");
    let expected = native["outputs"]
        .as_array()
        .expect("outputs")
        .iter()
        .find(|o| o["name"] == "out.nex")
        .expect("image")["hex"]
        .as_str()
        .expect("bytes");
    for cpu in ["z80", "z80n"] {
        for mode in ["human", "json"] {
            let run = |extra: &[&str]| {
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
                    .args(extra)
                    .output()
                    .expect("CLI")
            };
            std::fs::write(dir.join("values.inc"), good).expect("include");
            let result = run(&["--outprefix", "saved/"]);
            assert!(result.status.success(), "{cpu}/{mode}: {result:?}");
            assert_eq!(
                hex(&std::fs::read(dir.join("saved/out.nex")).expect("NEX")),
                expected
            );
            assert_eq!(
                std::fs::read(dir.join("image.bin")).expect("raw"),
                [1, 2, 3]
            );
            assert!(!dir.join("out.nex").exists());
            let names = [
                "saved/out.nex",
                "image.bin",
                "out.sym",
                "out.debug198x",
                "out.lst",
                "out.lst.json",
            ];
            let previous: Vec<_> = names
                .iter()
                .map(|n| std::fs::read(dir.join(n)).expect("output"))
                .collect();
            for bad in [" SAVENEX AUTO", " SAVENEX OPEN \"other.nex\"", " ASSERT 0"] {
                let source = format!("{good}{bad}\n");
                std::fs::write(dir.join("values.inc"), &source).expect("bad include");
                // A second OPEN is valid, so make its pending session fail.
                if bad.contains("OPEN") {
                    std::fs::write(
                        dir.join("values.inc"),
                        format!("{source} SAVENEX OPEN \"third.nex\"\n"),
                    )
                    .expect("duplicate open");
                }
                let failure = run(&["--outprefix", "saved/"]);
                assert!(!failure.status.success(), "{cpu}/{mode}: {bad}");
                if mode == "json" {
                    let errors: Value = serde_json::from_slice(&failure.stdout).expect("errors");
                    assert!(
                        errors[0]["span"]["path"]
                            .as_str()
                            .expect("path")
                            .ends_with("values.inc")
                    );
                    assert_eq!(
                        errors[0]["span"]["line"],
                        if bad.contains("OPEN") { 9 } else { 8 }
                    );
                } else {
                    assert!(String::from_utf8_lossy(&failure.stderr).contains("values.inc:"));
                }
                for (n, bytes) in names.iter().zip(&previous) {
                    assert_eq!(
                        &std::fs::read(dir.join(n)).expect("preserved"),
                        bytes,
                        "{bad}: {n}"
                    );
                }
                assert!(!dir.join("saved/other.nex").exists());
            }
            std::fs::write(dir.join("values.inc"), good).expect("good");
            std::fs::write(dir.join("values.inc"), good.replace("out.nex", "image.bin"))
                .expect("collision");
            let before = std::fs::read(dir.join("image.bin")).expect("before");
            assert!(!run(&[]).status.success());
            assert_eq!(std::fs::read(dir.join("image.bin")).expect("after"), before);
            let alias = dir.join("source-alias.nex");
            if alias.exists() {
                std::fs::remove_file(&alias).expect("old alias");
            }
            std::fs::hard_link(dir.join("input.asm"), &alias).expect("source hard link");
            for target in ["values.inc", "input.asm", "source-alias.nex"] {
                let source = good.replace("out.nex", target);
                std::fs::write(dir.join("values.inc"), &source).expect("source collision");
                let root = std::fs::read(dir.join("input.asm")).expect("root");
                assert!(
                    !run(&[]).status.success(),
                    "artifact must not overwrite {target}"
                );
                assert_eq!(
                    std::fs::read_to_string(dir.join("values.inc")).expect("include"),
                    source
                );
                assert_eq!(
                    std::fs::read(dir.join("input.asm")).expect("root preserved"),
                    root
                );
                for (name, bytes) in names.iter().zip(&previous) {
                    assert_eq!(
                        &std::fs::read(dir.join(name)).expect("previous artifact"),
                        bytes,
                        "collision: {name}"
                    );
                }
            }
        }
    }
}

#[test]
fn library_describes_artifacts_without_writing_the_requested_path() {
    let dir = scratch::dir("sjasm-savenex-library");
    let destination = dir.join("not-created.nex");
    let source = format!(
        " DEVICE ZXSPECTRUMNEXT\n SAVENEX OPEN \"{}\"\n SAVENEX CLOSE\n",
        destination.display()
    );
    let result = asm198x::assemble_sjasmplus(&source).expect("library");
    assert_eq!(result.artifacts.len(), 1);
    assert!(!destination.exists());
}
