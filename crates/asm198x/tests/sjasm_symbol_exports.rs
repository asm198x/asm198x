use asm198x::{ArtifactFormat, assemble_sjasmplus};
use std::process::Command;

#[path = "support/scratch.rs"]
mod scratch;

const BANKED: &str = " DEVICE ZXSPECTRUM128\n ORG $C010\n PAGE 1\ndraw: db 1\n PAGE 3\n ORG $C010\nmusic: db 3\nanswer EQU $C010\n CSPECTMAP \"native.map\"\n LABELSLIST \"native.lbl\"\n END draw\n";

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("text artifact")
}
fn sorted(text: &str) -> Vec<&str> {
    let mut rows: Vec<_> = text.lines().collect();
    rows.sort_unstable();
    rows
}

#[test]
fn source_requests_use_final_symbols_without_writing_files_or_changing_bytes() {
    let result = assemble_sjasmplus(BANKED).expect("assembly");
    assert_eq!(result.bytes, [1, 3]);
    assert_eq!(result.artifacts.len(), 2);
    assert_eq!(result.artifacts[0].format, ArtifactFormat::CspectMap);
    assert_eq!(
        text(&result.artifacts[0].bytes),
        "0000C010 00000010 01 ANSWER\n0000C010 00004010 00 DRAW\n0000C010 0000C010 00 MUSIC\n"
    );
    assert_eq!(
        text(&result.artifacts[1].bytes),
        "03:0010 answer\n01:0010 draw\n03:0010 music\n"
    );
}

#[test]
fn last_request_wins_and_virtual_labels_keep_logical_addresses() {
    let result = assemble_sjasmplus(" DEVICE ZXSPECTRUM48\n CSPECTMAP \"old.map\"\n LABELSLIST \"old.lbl\"\n ORG $8000\ncode: nop\n CSPECTMAP \"new.map\"\n LABELSLIST \"new.lbl\",1\n").expect("assembly");
    assert_eq!(
        result
            .artifacts
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["new.map", "new.lbl"]
    );
    assert_eq!(text(&result.artifacts[1].bytes), ":8000 code\n");
    assert!(assemble_sjasmplus(" CSPECTMAP \"bad.map\"\n").is_err());
    assert!(assemble_sjasmplus(" DEVICE ZXSPECTRUM48\n LABELSLIST\n").is_err());
    assert!(
        assemble_sjasmplus(" DEVICE ZXSPECTRUM48\n LABELSLIST \"x\",future\nfuture EQU 1\n")
            .is_err()
    );
}

#[test]
fn includes_default_names_conditionals_and_formatting_preserve_requests() {
    let loader = asm198x::source::MemoryLoader::new().text("maps.inc", " CSPECTMAP\n");
    let source = " DEVICE ZXSPECTRUM128\n ORG $C010\nentry: nop\n INCLUDE \"maps.inc\"\n IF 0\n LABELSLIST\n ENDIF\n";
    let result = asm198x::assemble_sjasmplus_files(source, "root.asm", &loader).expect("include");
    assert_eq!(result.artifacts.len(), 1);
    assert_eq!(result.artifacts[0].name, "maps.inc.map");
    let formatted = asm198x::format_sjasmplus(BANKED).expect("format");
    assert_eq!(asm198x::format_sjasmplus(&formatted).unwrap(), formatted);
    let reformatted = assemble_sjasmplus(&formatted).expect("reassemble");
    let original = assemble_sjasmplus(BANKED).unwrap();
    assert_eq!(reformatted.bytes, original.bytes);
    assert_eq!(reformatted.artifacts, original.artifacts);
}

#[test]
fn cli_writes_source_maps_under_outprefix_and_json_describes_them() {
    let dir = scratch::dir("sjasm-symbol-cli");
    let prefix = dir.join("exports");
    std::fs::create_dir(&prefix).unwrap();
    std::fs::write(dir.join("input.asm"), BANKED).unwrap();
    // The library is pure even with an absolute artifact name.
    let absolute = dir.join("not-written.map");
    assemble_sjasmplus(&BANKED.replace("native.map", absolute.to_str().unwrap())).unwrap();
    assert!(!absolute.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(&dir)
        .args([
            "--dialect",
            "sjasmplus",
            "input.asm",
            "-o",
            "code.bin",
            "--outprefix",
        ])
        .arg(&prefix)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = assemble_sjasmplus(BANKED).unwrap();
    for artifact in &expected.artifacts {
        assert_eq!(
            std::fs::read(prefix.join(&artifact.name)).unwrap(),
            artifact.bytes
        );
    }
    assert_eq!(std::fs::read(dir.join("code.bin")).unwrap(), expected.bytes);
    let json = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(&dir)
        .args([
            "--dialect",
            "sjasmplus",
            "input.asm",
            "--message-format=json",
        ])
        .output()
        .unwrap();
    assert!(json.status.success());
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["artifacts"][0]["format"], "cspectmap");
    assert_eq!(value["artifacts"][1]["format"], "labelslist");
    assert!(!dir.join("native.map").exists());
    assert!(!dir.join("native.lbl").exists());
}

#[test]
#[ignore = "requires SjASMPlus 1.21.0"]
fn maps_match_native_for_pages_constants_structures_and_options() {
    let cases = [
        BANKED.to_string(),
        " DEVICE ZXSPECTRUM48\n ORG $8000\ncode: nop\nvalue EQU 7\n CSPECTMAP <native.map>\n LABELSLIST <native.lbl>,1\n".into(),
        " DEVICE ZXSPECTRUMNEXT\n ORG $E010\n PAGE 10\nParent: db 1\n.local: db 2\nvalue EQU $E010\nsmall EQU 7\nlarge EQU $123456\nnegative EQU -1\n PAGE 20\n CSPECTMAP \"native.map\"\n LABELSLIST \"native.lbl\"\n".into(),
        " DEVICE ZXSPECTRUM48\n ORG $8000\ncode: nop\nvalue EQU 7\n CSPECTMAP\n LABELSLIST bare.lbl,1\n".into(),
        " DEVICE ZXSPECTRUM128\n STRUCT Pair\nfirst BYTE 1\nsecond BYTE 2\n ENDS\n ORG $C010\ninstance Pair\n CSPECTMAP \"native.map\"\n LABELSLIST \"native.lbl\"\n".into(),
        " ORG $8000\nflat: nop\n DEVICE ZXSPECTRUMNEXT\n CSPECTMAP \"old.map\"\n LABELSLIST \"old.lbl\"\nnext: nop\n DEVICE ZXSPECTRUM48\n CSPECTMAP \"native.map\"\n LABELSLIST \"native.lbl\"\n".into(),
    ];
    for (case, source) in cases.iter().enumerate() {
        let dir = scratch::dir(&format!("sjasm-symbols-{case}"));
        std::fs::write(dir.join("input.asm"), source).expect("source");
        let native = Command::new("sjasmplus")
            .current_dir(&dir)
            .args(["--nologo", "--raw=native.bin", "input.asm"])
            .output()
            .expect("native");
        assert!(
            native.status.success(),
            "case {case}: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let ours = assemble_sjasmplus(source).unwrap_or_else(|e| panic!("case {case}: {e}"));
        assert_eq!(
            ours.bytes,
            std::fs::read(dir.join("native.bin")).expect("bytes"),
            "case {case}"
        );
        for artifact in ours.artifacts {
            let native = std::fs::read_to_string(dir.join(&artifact.name))
                .unwrap_or_else(|e| panic!("case {case}: {}: {e}", artifact.name));
            assert_eq!(
                sorted(text(&artifact.bytes)),
                sorted(&native),
                "case {case}: {}",
                artifact.name
            );
        }
    }
}

#[test]
#[ignore = "requires CSPECT_SYMBOL_PROBE (see support/cspect)"]
fn cspect_consumes_our_generated_map() {
    let dir = scratch::dir("cspect-symbols");
    let ours = assemble_sjasmplus(BANKED).expect("assembly");
    let map = dir.join("ours.map");
    std::fs::write(&map, &ours.artifacts[0].bytes).expect("map");
    let result = Command::new("dotnet")
        .arg(std::env::var_os("CSPECT_SYMBOL_PROBE").expect("probe DLL"))
        .arg(std::env::var_os("CSPECT_EXE").expect("CSPECT_EXE"))
        .arg(map)
        .output()
        .expect("consumer");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "requires UNREAL_SYMBOL_PROBE (see docs/sjasmplus-symbol-exports.md)"]
fn unrealspeccy_consumes_our_generated_labels() {
    let dir = scratch::dir("unreal-symbols");
    let ours = assemble_sjasmplus(BANKED).expect("assembly");
    let path = dir.join("ours.lbl");
    std::fs::write(&path, &ours.artifacts[1].bytes).expect("labels");
    let result = Command::new(std::env::var_os("UNREAL_SYMBOL_PROBE").expect("probe"))
        .arg(path)
        .output()
        .expect("consumer");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}
