//! Project-supplied ld65 configurations on the ca65 path (#483, #430,
//! `decisions/layouts-are-data.md`): the bounded `.cfg` reader feeding the
//! shared placement, hermetically here; the byte-for-byte proof against real
//! ca65 + ld65 is the ignored differential at the bottom.

use asm198x::assemble_ca65_files_with_config;
use asm198x::source::MemoryLoader;

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
}
