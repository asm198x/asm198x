use asm198x::{
    AssemblyResult, assemble_ca65, render_listing, render_listing_json, source::MemoryLoader,
};
use serde_json::{Value, json};
use std::process::Command;

#[path = "support/scratch.rs"]
mod scratch;

const SOURCE: &str = ".segment \"HEADER\"\n.byte $4e,$45,$53,$1a\n.segment \"CODE\"\nstart: lda #1\n rts\n.segment \"CHARS\"\npattern: .byte $ab\n.segment \"BSS\"\nspace: .res 8\n";

#[test]
fn native_records_keep_cpu_file_and_section_offsets_distinct() {
    let r = assemble_ca65(SOURCE).expect("NES");
    let section = |name| {
        r.section_debug
            .iter()
            .find(|s| s.section.name == name)
            .expect("section")
    };
    let code = section("CODE");
    assert_eq!(code.section.base, Some(0x8000));
    assert_eq!(code.file_offset, Some(16));
    assert_eq!(code.debug.cycles.len(), 2);
    assert_eq!(code.debug.cycles[0].offset, 0);
    assert_eq!(code.debug.cycles[0].base, 2);
    assert_eq!(section("CHARS").section.base, None);
    assert_eq!(section("CHARS").file_offset, Some(16 + 32768));
    assert_eq!(section("BSS").file_offset, None);
    assert!(section("BSS").debug.lines.is_empty());
    assert!(
        r.debug.cycles.is_empty(),
        "native records must not masquerade as flat"
    );
    let v: Value =
        serde_json::from_str(&render_listing_json("main.s", &r, 1)).expect("listing JSON");
    let lines = v["lines"].as_array().expect("lines");
    assert_eq!(lines[0]["address"], 0x8000);
    assert_eq!(lines[0]["file_offset"], 16);
    assert_eq!(lines[0]["cycles"], json!({"min":2,"max":2}));
    let chr = lines.iter().find(|l| l["line"] == 7).expect("CHR line");
    assert!(chr["address"].is_null());
    assert!(chr.get("cycles").is_none());
    let text = render_listing(SOURCE, &r, 1);
    assert!(
        text.contains("section CODE (CPU $8000, file +$10)"),
        "{text}"
    );
    assert!(text.contains("8000  A9 01"), "{text}");
    assert!(text.contains("start (+$0000): 3 bytes, 8 cycles"), "{text}");
    assert!(
        text.contains("+0000  AB"),
        "CHR offset must not become a CPU address: {text}"
    );
}

#[test]
fn section_capture_is_identical_across_debug_and_plain_entries_and_roundtrips() {
    let plain = assemble_ca65(SOURCE).expect("plain");
    let (debug, info) = asm198x::assemble_ca65_debug(SOURCE, "main.s").expect("debug");
    assert_eq!(plain.section_debug, debug.section_debug);
    assert_eq!(
        plain
            .section_debug
            .iter()
            .map(|s| &s.section)
            .collect::<Vec<_>>(),
        info.sections.iter().collect::<Vec<_>>()
    );
    let wire = serde_json::to_value(&plain).expect("encode");
    let decoded: AssemblyResult = serde_json::from_value(wire).expect("decode");
    assert_eq!(plain, decoded);
    let flat = asm198x::assemble_acme("*=$1000\nnop\n").expect("flat");
    let mut old = serde_json::to_value(&flat).expect("old shape");
    assert!(old.get("section_debug").is_none());
    old["future_field"] = json!(true);
    let decoded: AssemblyResult = serde_json::from_value(old).expect("skip unknown");
    assert_eq!(flat, decoded);
}

#[test]
fn repeated_include_instances_have_per_emission_not_per_line_costs() {
    let loader = MemoryLoader::new().text("part.inc", "lda #1\nrts\n");
    let source =
        ".segment \"CODE\"\nfirst:\n.include \"part.inc\"\nsecond:\n.include \"part.inc\"\n";
    let r = asm198x::assemble_ca65_files(source, "main.s", &loader).expect("includes");
    let v: Value = serde_json::from_str(&render_listing_json("main.s", &r, 1)).expect("JSON");
    let lines = v["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0]["cycles"]["max"], 2);
    assert_eq!(lines[2]["cycles"]["max"], 2);
    assert_eq!(lines[2]["address"], 0x8003);
    assert_eq!(lines[2]["file"], "part.inc");

    let r =
        assemble_ca65(".macro body\n lda #1\n rts\n.endmacro\n.segment \"CODE\"\nstart: body\n")
            .expect("macro");
    let v: Value =
        serde_json::from_str(&render_listing_json("macro.s", &r, 1)).expect("macro JSON");
    assert_eq!(
        v["lines"][0]["line"], v["lines"][1]["line"],
        "one invocation line"
    );
    assert_eq!(v["lines"][0]["cycles"]["max"], 2);
    assert_eq!(v["lines"][1]["cycles"]["max"], 6);
}

#[test]
fn cli_lists_configured_sections_in_human_and_json_modes() {
    let dir = scratch::dir("ca65-listings");
    std::fs::write(
        dir.join("main.s"),
        ".segment \"ONE\"\nfirst: lda #1\n.segment \"TWO\"\n.include \"part.inc\"\n",
    )
    .expect("source");
    std::fs::write(dir.join("part.inc"), "second: rts\n").expect("include");
    std::fs::write(dir.join("t.cfg"), "MEMORY { ROM: start=$9000, size=$20, file=%O, fill=yes; }\nSEGMENTS { ONE: load=ROM, type=ro; TWO: load=ROM, type=ro, start=$9010; }").expect("config");
    for json_mode in [false, true] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_asm198x"));
        cmd.current_dir(&dir).args([
            "--dialect",
            "ca65",
            "-C",
            "t.cfg",
            "main.s",
            "-o",
            "out.bin",
            "--listing=out.lst",
            "--listing-json=out.json",
        ]);
        if json_mode {
            cmd.arg("--message-format=json");
        }
        let out = cmd.output().expect("CLI");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = std::fs::read_to_string(dir.join("out.lst")).expect("listing");
        assert!(text.contains("9010  60"), "{text}");
        assert!(text.contains("second: rts"), "included source: {text}");
        let v: Value =
            serde_json::from_slice(&std::fs::read(dir.join("out.json")).expect("JSON listing"))
                .expect("JSON");
        assert_eq!(v["lines"][1]["address"], 0x9010);
        assert_eq!(v["lines"][1]["file_offset"], 16);
        assert_eq!(v["lines"][1]["cycles"]["max"], 6);
        if json_mode {
            let r: AssemblyResult = serde_json::from_slice(&out.stdout).expect("result JSON");
            assert_eq!(r.section_debug[1].section.base, Some(0x9010));
        }
    }
    let out = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(&dir)
        .args([
            "--dialect",
            "ca65",
            "-C",
            "t.cfg",
            "main.s",
            "--listing-json=only.json",
            "--message-format=json",
        ])
        .output()
        .expect("JSON listing alone");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value =
        serde_json::from_slice(&std::fs::read(dir.join("only.json")).expect("standalone listing"))
            .expect("JSON");
    assert_eq!(v["lines"][1]["address"], 0x9010);
}
