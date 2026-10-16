//! Project-supplied ld65 configurations on the ca65 path (#483, #430,
//! `decisions/layouts-are-data.md`): the bounded `.cfg` reader feeding the
//! shared placement, hermetically here; the byte-for-byte proof against real
//! ca65 + ld65 is the ignored differential at the bottom.

use asm198x::assemble_ca65_files_with_config;
use asm198x::source::MemoryLoader;

#[test]
fn used_file_extents_match_captured_native_builds() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-used-extents.json"))
            .expect("captured native facts");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 51);
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let result = assemble_ca65_files_with_config(
            case["source"].as_str().expect("source"),
            "input.s",
            &MemoryLoader::new(),
            case["config"].as_str().expect("config"),
        );
        let native = case["native"].as_array().expect("native stages");
        if native.last().expect("native result")["exit"] != 0 {
            assert!(result.is_err(), "{name}: native refused this build");
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let actual: String = result.bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            actual,
            case["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
    }
}

#[test]
fn compact_file_offsets_preserve_cpu_addresses_and_memory_capacity() {
    let cfg = "MEMORY { A:start=$1000,size=$10,file=%O; B:start=$2000,size=$10,file=%O; }\nSEGMENTS { CODE:load=A,type=ro; TAIL:load=B,type=ro; }\n";
    let source = ".segment \"CODE\"\nfirst_label: .byte $a5\n.segment \"TAIL\"\nsecond_label: .word first_label,second_label\n";
    let result = assemble_ca65_files_with_config(source, "input.s", &MemoryLoader::new(), cfg)
        .expect("two compact areas");
    let tail = result
        .section_debug
        .iter()
        .find(|section| section.section.name == "TAIL")
        .expect("TAIL debug section");
    assert_eq!(tail.file_offset, Some(1));
    assert_eq!(tail.section.base, Some(0x2000));
    let label = tail
        .debug
        .symbols
        .iter()
        .find(|symbol| symbol.name == "second_label")
        .expect("section label");
    let debug198x::SymbolKind::Label { offset, .. } = &label.kind else {
        panic!("expected a section-relative label");
    };
    assert_eq!(*offset, 0);
    assert_eq!(&result.bytes[1..], &[0x00, 0x10, 0x00, 0x20]);
    assert_eq!(result.areas[0].size, 16);
    assert_eq!(result.areas[0].free, 15);
    assert_eq!(result.areas[1].size, 16);
    assert_eq!(result.areas[1].free, 12);
    let listing: serde_json::Value =
        serde_json::from_str(&asm198x::render_listing_json("input.s", &result, 1))
            .expect("JSON listing");
    assert_eq!(
        listing["lines"][1]["file_offset"],
        tail.file_offset.expect("file offset")
    );
    assert_eq!(
        listing["lines"][1]["address"],
        tail.section.base.expect("CPU base") + offset
    );
}

/// The layout shape that forced the feature: a floating segment (`RODATA`)
/// placed sequentially after `CODE`, and a segment opening a second file
/// area (`TILES` in CHR). Addresses and file offsets follow ld65's rule.
#[test]
fn a_floating_segment_places_after_its_predecessor() {
    let cfg = r#"
MEMORY {
    ZP:  start = $00,   size = $0100, type = rw, file = "";
    HDR: start = $0000, size = $0004, type = ro, file = %O, fill = yes, fillval = $00;
    PRG: start = $8000, size = $0020, type = ro, file = %O, fill = yes, fillval = $00;
    CHR: start = $0000, size = $0008, type = ro, file = %O, fill = yes, fillval = $00;
}
SEGMENTS {
    ZEROPAGE: load = ZP,  type = zp;
    HEADER:   load = HDR, type = ro;
    CODE:     load = PRG, type = ro, start = $8000;
    RODATA:   load = PRG, type = ro;
    TILES:    load = CHR, type = ro;
}
"#;
    let src = "\
.segment \"HEADER\"\n.byte $4E, $45, $53, $1A\n\
.segment \"CODE\"\nlda table\nrts\n\
.segment \"RODATA\"\ntable: .byte $AA, $BB\n\
.segment \"TILES\"\n.byte $11, $22\n";
    let r = assemble_ca65_files_with_config(src, "main.s", &MemoryLoader::new(), cfg)
        .expect("assembles");
    // Image: 4-byte header + 32-byte PRG + 8-byte CHR.
    assert_eq!(r.bytes.len(), 4 + 0x20 + 8);
    assert_eq!(&r.bytes[0..4], &[0x4E, 0x45, 0x53, 0x1A]);
    // CODE at PRG start: `lda table` where RODATA floats to $8004 (after the
    // 4-byte CODE) — absolute LDA $8004 then RTS.
    assert_eq!(&r.bytes[4..8], &[0xAD, 0x04, 0x80, 0x60]);
    // RODATA lands directly after CODE in the file too.
    assert_eq!(&r.bytes[8..10], &[0xAA, 0xBB]);
    // The rest of PRG is fill; TILES opens the CHR area.
    assert_eq!(&r.bytes[4 + 0x20..4 + 0x20 + 2], &[0x11, 0x22]);
}

/// A segment the config does not define is refused naming the config, not
/// the curriculum — the message follows the layout's provenance.
#[test]
fn an_unknown_segment_names_the_config() {
    let cfg = r#"
MEMORY { PRG: start = $8000, size = $10, type = ro, file = %O, fill = yes, fillval = $00; }
SEGMENTS { CODE: load = PRG, type = ro; }
"#;
    let e = assemble_ca65_files_with_config(
        ".segment \"BSS\"\n.res 1\n",
        "main.s",
        &MemoryLoader::new(),
        cfg,
    )
    .expect_err("BSS is not in this config");
    assert!(
        e.error.message.contains("not in the linker config"),
        "{}",
        e.error.message
    );
    assert!(e.error.message.contains("CODE"), "{}", e.error.message);
}

/// A `start` the cursor has passed is the ld65-shaped refusal, at assembly
/// time, with the real lengths in hand.
#[test]
fn an_overfull_area_is_refused() {
    let cfg = r#"
MEMORY { PRG: start = $8000, size = $10, type = ro, file = %O, fill = yes, fillval = $00; }
SEGMENTS {
    CODE:    load = PRG, type = ro;
    VECTORS: load = PRG, type = ro, start = $8002;
}
"#;
    let src = ".segment \"CODE\"\n.byte 1, 2, 3, 4\n.segment \"VECTORS\"\n.byte 5\n";
    let e = assemble_ca65_files_with_config(src, "main.s", &MemoryLoader::new(), cfg)
        .expect_err("CODE has filled past VECTORS' start");
    assert!(e.error.message.contains("VECTORS"), "{}", e.error.message);
}

/// The acceptance from #430, made a differential: a two-segment program with
/// a floating RODATA, assembled by us under `-C` semantics and by real
/// ca65 + ld65 under the same config, byte-compared. Ignored without the
/// reference tools, exactly like every other differential.
#[test]
#[ignore = "needs ca65 + ld65 on PATH"]
fn project_config_matches_ca65_plus_ld65() {
    let dir = std::env::temp_dir().join(format!("asm198x-ca65-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let cfg = r#"
MEMORY {
    HDR: start = $0000, size = $0010, type = ro, file = %O, fill = yes, fillval = $00;
    PRG: start = $8000, size = $0100, type = ro, file = %O, fill = yes, fillval = $00;
}
SEGMENTS {
    HEADER: load = HDR, type = ro;
    CODE:   load = PRG, type = ro, start = $8000;
    RODATA: load = PRG, type = ro;
}
"#;
    let src = ".segment \"HEADER\"\n.byte $4E, $45, $53, $1A\n\
.segment \"CODE\"\nstart: lda table\nsta $0200\nrts\n\
.segment \"RODATA\"\ntable: .byte $12, $34, $56\n";
    std::fs::write(dir.join("t.cfg"), cfg).expect("cfg");
    std::fs::write(dir.join("t.s"), src).expect("src");
    let run = |cmd: &str, args: &[&str]| {
        let out = std::process::Command::new(cmd)
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap_or_else(|e| panic!("run {cmd}: {e}"));
        assert!(
            out.status.success(),
            "{cmd} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run("ca65", &["t.s", "-o", "t.o"]);
    run("ld65", &["-o", "ref.bin", "-C", "t.cfg", "t.o"]);
    let reference = std::fs::read(dir.join("ref.bin")).expect("reference");
    let ours = assemble_ca65_files_with_config(src, "t.s", &MemoryLoader::new(), cfg)
        .expect("assembles")
        .bytes;
    assert_eq!(ours, reference, "byte-identical under the project config");
    std::fs::remove_dir_all(dir).expect("remove native scratch");
}

// #609: names and the NROM vector address do not impose limits on a supplied
// layout. Expected payloads are independently checked against ca65 + ld65.
fn extent_cases() -> Vec<(&'static str, String, String, Option<Vec<u8>>)> {
    let full = "MEMORY { PRG: start=$8000, size=$8000, file=%O, fill=yes; }";
    let mut cases = Vec::new();
    for name in ["CODE", "PROGRAM"] {
        let cfg = format!("{full}\nSEGMENTS {{ {name}: load=PRG, type=ro; }}");
        cases.push((
            "full_area",
            cfg.clone(),
            format!(".segment \"{name}\"\n.res $8000,$ea\n"),
            Some(vec![0xea; 0x8000]),
        ));
        let mut embedded = vec![0xea; 0x7ffa];
        embedded.extend([0x00, 0x80, 0x01, 0x80, 0x02, 0x80]);
        cases.push((
            "embedded_vectors",
            cfg.clone(),
            format!(".segment \"{name}\"\n.res $7ffa,$ea\n.word $8000,$8001,$8002\n"),
            Some(embedded),
        ));
        cases.push((
            "area_overflow",
            cfg,
            format!(".segment \"{name}\"\n.res $8001,$ea\n"),
            None,
        ));
    }
    let pinned = format!(
        "{full}\nSEGMENTS {{ PROGRAM: load=PRG, type=ro; TABLE: load=PRG, type=ro, start=$fffa; }}"
    );
    cases.push((
        "absent_pinned_segment",
        pinned.clone(),
        ".segment \"PROGRAM\"\n.res $8000,$ea\n".into(),
        Some(vec![0xea; 0x8000]),
    ));
    for suffix in [
        ".segment \"TABLE\"\n",
        ".segment \"TABLE\"\ntable:\n",
        ".segment \"TABLE\"\n.res 0\n",
    ] {
        cases.push((
            "present_empty_pinned_segment",
            pinned.clone(),
            format!(".segment \"PROGRAM\"\n.res $8000,$ea\n{suffix}"),
            None,
        ));
    }
    let floating = "MEMORY { PRG: start=$8000, size=16, file=%O, fill=yes; } SEGMENTS { PROGRAM: load=PRG, type=ro; UNUSED: load=PRG, type=ro, align=16; TAIL: load=PRG, type=ro; }";
    let mut expected = vec![0xea, 0xbb];
    expected.resize(16, 0);
    cases.push((
        "absent_aligned_segment",
        floating.into(),
        ".segment \"PROGRAM\"\n.byte $ea\n.segment \"TAIL\"\n.byte $bb\n".into(),
        Some(expected),
    ));
    let mut split = vec![0xea; 0x7ffa];
    split.extend([0x00, 0x80, 0x01, 0x80, 0x02, 0x80]);
    cases.push((
        "pinned_boundary",
        pinned.clone(),
        ".segment \"PROGRAM\"\n.res $7ffa,$ea\n.segment \"TABLE\"\n.word $8000,$8001,$8002\n"
            .into(),
        Some(split),
    ));
    cases.push((
        "pinned_overlap",
        pinned,
        ".segment \"PROGRAM\"\n.res $7ffb,$ea\n.segment \"TABLE\"\n.word $8000,$8001,$8002\n"
            .into(),
        None,
    ));
    let adjacent = "MEMORY { A: start=$8000, size=4, file=%O, fill=yes; B: start=$9000, size=4, file=%O, fill=yes; } SEGMENTS { CODE: load=A, type=ro; NEXT: load=B, type=ro; }";
    cases.push((
        "area_spill_into_file_neighbor",
        adjacent.into(),
        ".segment \"CODE\"\n.res 5,$ea\n.segment \"NEXT\"\n.byte $aa\n".into(),
        None,
    ));
    let banks = "MEMORY { A: start=$8000, size=4, file=%O, fill=yes; B: start=$8000, size=4, file=%O, fill=yes; } SEGMENTS { FIRST: load=A, type=ro; SECOND: load=B, type=ro; }";
    cases.push((
        "separate_file_banks_share_cpu_addresses",
        banks.into(),
        ".segment \"FIRST\"\n.res 4,$ea\n.segment \"SECOND\"\n.res 4,$bb\n".into(),
        Some(vec![0xea, 0xea, 0xea, 0xea, 0xbb, 0xbb, 0xbb, 0xbb]),
    ));
    let ram = "MEMORY { RAM: start=$200, size=4, file=\"\"; PRG: start=$8000, size=4, file=%O, fill=yes; } SEGMENTS { BSS: load=RAM, type=bss; CODE: load=PRG, type=ro; }";
    cases.push((
        "ram_overflow",
        ram.into(),
        ".segment \"BSS\"\n.res 5\n.segment \"CODE\"\nrts\n".into(),
        None,
    ));
    cases
}

#[test]
fn segment_extents_follow_the_active_layout() {
    for (name, cfg, src, expected) in extent_cases() {
        let result = assemble_ca65_files_with_config(&src, "t.s", &MemoryLoader::new(), &cfg);
        match expected {
            Some(bytes) => assert_eq!(result.expect(name).bytes, bytes, "{name}"),
            None => {
                let error = result.expect_err(name);
                assert!(error.error.message.contains("segment"), "{name}: {error}");
            }
        }
    }
}

#[test]
#[ignore = "needs ca65 + ld65 on PATH"]
fn segment_extents_match_native_linking() {
    let dir = std::env::temp_dir().join(format!("asm198x-ca65-extents-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (name, cfg, src, expected) in extent_cases() {
        std::fs::write(dir.join("t.cfg"), &cfg).expect("cfg");
        std::fs::write(dir.join("t.s"), &src).expect("src");
        let assembled = std::process::Command::new("ca65")
            .args(["t.s", "-o", "t.o"])
            .current_dir(&dir)
            .output()
            .expect("ca65");
        assert!(
            assembled.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&assembled.stderr)
        );
        let linked = std::process::Command::new("ld65")
            .args(["-C", "t.cfg", "t.o", "-o", "ref.bin"])
            .current_dir(&dir)
            .output()
            .expect("ld65");
        assert_eq!(
            linked.status.success(),
            expected.is_some(),
            "{name}: {}",
            String::from_utf8_lossy(&linked.stderr)
        );
        println!(
            "{name}: ca65={}, ld65={}, accepted={}",
            assembled.status,
            linked.status,
            expected.is_some()
        );
        if let Some(bytes) = expected {
            assert_eq!(
                std::fs::read(dir.join("ref.bin")).expect("native bytes"),
                bytes,
                "{name}"
            );
        }
    }
    std::fs::remove_dir_all(dir).expect("remove native scratch");
}

#[test]
fn default_vectors_keep_their_reserved_slot_and_helpful_error() {
    let fits = ".segment \"CODE\"\n.res $7ffa,$ea\n.segment \"VECTORS\"\n.word $8000,$8001,$8002\n";
    let rom = asm198x::assemble_ca65(fits).expect("ends at vector boundary");
    assert_eq!(
        &rom.bytes[16 + 0x7ffa..16 + 0x8000],
        &[0, 0x80, 1, 0x80, 2, 0x80]
    );
    let error = asm198x::assemble_ca65(&fits.replace("$7ffa", "$7ffb")).expect_err("real overlap");
    for detail in ["CODE", "VECTORS", "$FFFA", "PRG"] {
        assert!(error.message.contains(detail), "{error}");
    }
}
