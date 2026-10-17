use asm198x::{assemble_ca65_files_with_config, source::MemoryLoader};

#[test]
fn native_4510_selection_and_indirect_z_match_configured_link() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-4510.json")).expect("native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 139);
    let mut loader = MemoryLoader::new();
    for (name, text) in facts["files"].as_object().expect("include fixtures") {
        loader = loader.text(name, text.as_str().expect("include text"));
    }
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let source = case["source"].as_str().expect("source");
        let config = case["config"].as_str().expect("config");
        let stages = case["native"].as_array().expect("stages");
        let result = assemble_ca65_files_with_config(source, "input.s", &loader, config);
        if stages.last().expect("final stage")["exit"] != 0 {
            assert!(result.is_err(), "{name}: native refuses");
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let hex: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            case["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
        let formatted = asm198x::format_ca65(source).expect("format native source");
        assert_eq!(
            asm198x::format_ca65(&formatted).expect("format twice"),
            formatted
        );
        let roundtrip = assemble_ca65_files_with_config(&formatted, "input.s", &loader, config)
            .expect("assemble formatted source");
        assert_eq!(roundtrip.bytes, result.bytes, "{name}: formatted bytes");
        let shape = |result: &asm198x::AssemblyResult| {
            result
                .section_debug
                .iter()
                .map(|s| (s.section.clone(), s.file_offset, s.debug.symbols.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shape(&roundtrip),
            shape(&result),
            "{name}: formatted addresses and symbols"
        );
    }
}

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn configured_cli_preserves_include_locations_and_previous_outputs() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-4510.json")).expect("native facts");
    let fact = facts["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == "include_debug")
        .expect("native include case");
    let source = fact["source"].as_str().expect("source");
    let dir = scratch::dir("ca65-4510-cli");
    std::fs::write(dir.join("input.s"), source).expect("root source");
    std::fs::write(
        dir.join("input.cfg"),
        fact["config"].as_str().expect("config"),
    )
    .expect("config");
    let names = [
        "image.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for mode in ["human", "json"] {
        std::fs::write(
            dir.join("select.inc"),
            facts["files"]["select.inc"].as_str().expect("include"),
        )
        .expect("include source");
        let run = || {
            std::process::Command::new(env!("CARGO_BIN_EXE_asm198x"))
                .current_dir(&dir)
                .args([
                    "--dialect",
                    "ca65",
                    "-C",
                    "input.cfg",
                    "--message-format",
                    mode,
                    "--sym=out.sym",
                    "--debug=out.debug198x",
                    "--listing=out.lst",
                    "--listing-json=out.lst.json",
                    "input.s",
                    "-o",
                    "image.bin",
                ])
                .output()
                .expect("CLI")
        };
        let success = run();
        assert!(success.status.success(), "{success:?}");
        let hex: String = std::fs::read(dir.join("image.bin"))
            .expect("image")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hex, fact["native_hex"].as_str().expect("native bytes"));
        let originals: Vec<_> = names
            .iter()
            .map(|name| std::fs::read(dir.join(name)).expect("output"))
            .collect();
        let invalid = ".setcpu \"4510\"\n.p02\n ora ($07),z\n";
        std::fs::write(dir.join("select.inc"), invalid).expect("failing include");
        let failure = run();
        assert!(!failure.status.success(), "{failure:?}");
        if mode == "json" {
            let errors: serde_json::Value =
                serde_json::from_slice(&failure.stdout).expect("diagnostics");
            assert_eq!(errors[0]["span"]["line"], 3);
            assert!(
                errors[0]["span"]["path"]
                    .as_str()
                    .expect("path")
                    .ends_with("select.inc")
            );
        } else {
            assert!(
                String::from_utf8_lossy(&failure.stderr).contains("select.inc:3"),
                "{failure:?}"
            );
        }
        for (name, original) in names.iter().zip(originals) {
            assert_eq!(
                std::fs::read(dir.join(name)).expect("preserved output"),
                original,
                "{name}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("input.s")).expect("root"),
            source
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("select.inc")).expect("include"),
            invalid
        );
    }
}
