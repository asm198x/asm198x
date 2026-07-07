//! Debug198x U3 — the CLI debug-artifact surface (`--debug`, `--sym`,
//! `--listing`) and the text renderings behind it. Rendering goldens pin the
//! `--sym` / `--listing` formats; the CLI tests cover default naming, container
//! coexistence (`--prg`), and AE2's byte-identity promise.

use std::path::PathBuf;
use std::process::Command;

/// The built `asm198x` binary under test.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
}

/// Write `source` to a uniquely-named temp file and return its path (so
/// parallel tests never share an input).
fn temp_source(tag: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("asm198x-debug-cli-{tag}.s"));
    std::fs::write(&path, source).expect("write temp source");
    path
}

/// A Z80 program exercising a label, a constant, a comment, and a data run
/// longer than the listing's byte column (so the `..` elision is pinned).
const Z80_SRC: &str = "\
\torg 8000h\n\
start:\n\
\tld a,5\n\
five\tequ 5\n\
; setup done\n\
\tret\n\
data:\tdb 1,2,3,4,5,6,7,8,9,10\n";

/// The `--sym` rendering: `name = $HEX`, sorted by name; labels absolute,
/// constants by value (golden).
#[test]
fn sym_rendering_golden() {
    let r = asm198x::assemble_pasmo(Z80_SRC).expect("assemble");
    let info = asm198x::debug_info(&r, "z80", "pasmo", "test.z80");
    assert_eq!(
        asm198x::render_sym(&info),
        "data = $8003\nfive = $0005\nstart = $8000\n"
    );
}

/// The `--listing` rendering: `ADDR  BYTES  SOURCE` rows; `equ` and comment
/// lines keep an empty address/bytes column; a data run longer than the byte
/// column elides with `..` (golden).
#[test]
fn listing_rendering_golden() {
    let r = asm198x::assemble_pasmo(Z80_SRC).expect("assemble");
    let listing = asm198x::render_listing(Z80_SRC, &r, 1);
    // An empty address/bytes column is 31 spaces: 4 (addr) + 2 + 23 (bytes) + 2.
    let expected = [
        "                               \torg 8000h",
        "                               start:",
        "8000  3E 05                    \tld a,5",
        "                               five\tequ 5",
        "                               ; setup done",
        "8002  C9                       \tret",
        "8003  01 02 03 04 05 06 07 ..  data:\tdb 1,2,3,4,5,6,7,8,9,10",
        "",
    ]
    .join("\n");
    assert_eq!(listing, expected);
}

/// Each flag produces its artifact at the default input-derived path, for a
/// Z80 and a 6502 program.
#[test]
fn flags_produce_artifacts_for_z80_and_6502() {
    let cases: [(&str, &[&str], &str); 2] = [
        ("z80", &["--dialect", "pasmo"], Z80_SRC),
        (
            "6502",
            &["--cpu", "6502"],
            "* = $c000\nstart:\n    lda #$01\n    rts\n",
        ),
    ];
    for (tag, args, src) in cases {
        let src_path = temp_source(&format!("flags-{tag}"), src);
        let out_bin = src_path.with_extension("bin");
        let status = bin()
            .args(args)
            .args(["--debug", "--sym", "--listing"])
            .arg(&src_path)
            .arg("-o")
            .arg(&out_bin)
            .status()
            .expect("run asm198x");
        assert!(status.success(), "{tag}: assemble succeeds");
        for ext in ["debug198x", "sym", "lst"] {
            let artifact = src_path.with_extension(ext);
            assert!(
                artifact.exists(),
                "{tag}: default-named .{ext} artifact exists"
            );
        }
    }
}

/// The sidecar is a readable `debug198x` file: the reader parses it, the
/// header carries the CPU/dialect identity, and a label resolves via
/// `addr_of` (the AE1 lookup mechanism, exercised at the CLI boundary).
#[test]
fn sidecar_reads_back_and_resolves_symbols() {
    let src_path = temp_source("sidecar", Z80_SRC);
    let status = bin()
        .args(["--dialect", "pasmo", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("bin"))
        .status()
        .expect("run asm198x");
    assert!(status.success());

    let ndjson =
        std::fs::read_to_string(src_path.with_extension("debug198x")).expect("read sidecar");
    let info = asm198x::debug198x::DebugInfo::read(&ndjson).expect("sidecar parses");
    assert_eq!(info.header.cpu, "z80");
    assert_eq!(info.header.dialect, "pasmo");
    assert_eq!(info.addr_of("start", None), Some(0x8000), "label resolves");
    assert_eq!(
        info.line_at(0x8000, None).map(|l| l.line),
        Some(3),
        "the first instruction's address maps back to its source line"
    );
}

/// `--debug` alongside `--prg` emits both artifacts (the container and the
/// sidecar), and an explicit `--debug=path` wins over the default name.
#[test]
fn debug_coexists_with_prg_container() {
    let src_path = temp_source("prg", "* = $0801\nstart:\n    lda #$01\n    rts\n");
    let prg = src_path.with_extension("prg");
    let sidecar = std::env::temp_dir().join("asm198x-debug-cli-explicit.debug198x");
    let status = bin()
        .args(["--cpu", "6502", "--prg"])
        .arg(format!("--debug={}", sidecar.display()))
        .arg(&src_path)
        .arg("-o")
        .arg(&prg)
        .status()
        .expect("run asm198x");
    assert!(status.success());
    assert!(prg.exists(), "the .prg container is written");
    assert!(sidecar.exists(), "the explicit-path sidecar is written");
}

/// AE2 (R7): the image bytes with `--debug --sym --listing` are identical to
/// the bytes without any debug flag.
#[test]
fn debug_flags_never_change_the_image() {
    let plain_src = temp_source("ae2-plain", Z80_SRC);
    let flagged_src = temp_source("ae2-flagged", Z80_SRC);
    let plain_bin = plain_src.with_extension("bin");
    let flagged_bin = flagged_src.with_extension("bin");

    let status = bin()
        .args(["--dialect", "pasmo"])
        .arg(&plain_src)
        .arg("-o")
        .arg(&plain_bin)
        .status()
        .expect("run asm198x");
    assert!(status.success());
    let status = bin()
        .args(["--dialect", "pasmo", "--debug", "--sym", "--listing"])
        .arg(&flagged_src)
        .arg("-o")
        .arg(&flagged_bin)
        .status()
        .expect("run asm198x");
    assert!(status.success());

    let plain = std::fs::read(&plain_bin).expect("read plain image");
    let flagged = std::fs::read(&flagged_bin).expect("read flagged image");
    assert_eq!(plain, flagged, "debug flags never change an emitted byte");
}

/// `--listing` stays rejected on the linked paths (ca65/vasm) — it needs a
/// per-section byte map; the record-backed `--debug`/`--sym` are live on both
/// (U4/U5).
#[test]
fn vasm_listing_still_rejected() {
    let src_path = temp_source("vasm-lst", "\tmoveq #0,d0\n\trts\n");
    let out = bin()
        .args(["--dialect", "vasm", "--listing"])
        .arg(&src_path)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "vasm + --listing is an error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("`--debug` and `--sym` are"),
        "the error names what works: {stderr}"
    );
}

/// `--debug` alongside `--sna` emits both the snapshot and the sidecar (the
/// container test's Spectrum half; needs `end <addr>` for the entry point).
#[test]
fn debug_coexists_with_sna_container() {
    let src_path = temp_source("sna", "\torg 8000h\nstart:\n\tret\n\tend start\n");
    let sna = src_path.with_extension("sna");
    let status = bin()
        .args(["--dialect", "pasmo", "--sna", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(&sna)
        .status()
        .expect("run asm198x");
    assert!(status.success());
    assert!(sna.exists(), "the .sna snapshot is written");
    assert!(
        src_path.with_extension("debug198x").exists(),
        "the sidecar is written alongside the snapshot"
    );
}

/// A failed container run leaves no debug artifacts: `--sna` on a non-Z80
/// dialect errors before anything (image or sidecar) is written.
#[test]
fn failed_container_run_writes_no_artifacts() {
    let src_path = temp_source("sna-wrong", "* = $0801\n    rts\n");
    let out = bin()
        .args(["--cpu", "6502", "--sna", "--debug", "--sym"])
        .arg(&src_path)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "--sna on acme is an error");
    for ext in ["debug198x", "sym", "sna"] {
        assert!(
            !src_path.with_extension(ext).exists(),
            "no .{ext} artifact outlives the failed run"
        );
    }
}

/// The debug flags reject `--fmt` and `--disasm` — there is no assembly record
/// to render, so the combination errors rather than silently doing nothing.
#[test]
fn debug_flags_reject_fmt_and_disasm() {
    let src_path = temp_source("fmt", "\tnop\n");
    let out = bin()
        .args(["--dialect", "pasmo", "--fmt", "--listing"])
        .arg(&src_path)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "--fmt + --listing is an error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("apply to an assembly run"),
        "the error explains the combination: {stderr}"
    );
}

/// `--message-format=json --debug`: the sidecar is written to disk while
/// stdout stays JSON-only (the machine contract holds with artifacts on).
#[test]
fn json_mode_writes_sidecar_and_keeps_stdout_json() {
    let src_path = temp_source("json", "\torg 8000h\nstart:\n\tld a,5\n\tret\n");
    let out = bin()
        .args(["--dialect", "pasmo", "--message-format=json", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("bin"))
        .output()
        .expect("run asm198x");
    assert!(out.status.success());
    let _: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is exactly one JSON value");
    assert!(
        src_path.with_extension("debug198x").exists(),
        "the sidecar is written in JSON mode too"
    );
}

/// An input already named `*.debug198x` must not be clobbered by the default
/// sidecar path — the CLI refuses and asks for an explicit path.
#[test]
fn default_sidecar_path_never_clobbers_the_input() {
    let src_path = std::env::temp_dir().join("asm198x-debug-cli-clobber.debug198x");
    std::fs::write(&src_path, "\torg 8000h\n\tret\n").expect("write source");
    let out = bin()
        .args(["--dialect", "pasmo", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("bin"))
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "clobbering the input is refused");
    let source = std::fs::read_to_string(&src_path).expect("input still readable");
    assert!(
        source.contains("org 8000h"),
        "the input source survives untouched"
    );
}

/// The `--sym` rendering includes an `Entry` symbol (from `end <addr>`) at its
/// absolute address, alongside labels and constants.
#[test]
fn sym_rendering_includes_entry_symbols() {
    let r = asm198x::assemble_pasmo("\torg 8000h\nstart:\n\tret\n\tend start\n").expect("assemble");
    let info = asm198x::debug_info(&r, "z80", "pasmo", "test.z80");
    assert_eq!(asm198x::render_sym(&info), "start = $8000\n");
    // `end start` upgrades the label in place; a non-label entry records
    // `@entry` — pin that shape too.
    let r = asm198x::assemble_pasmo("\torg 8000h\n\tret\n\tend 8000h\n").expect("assemble");
    let info = asm198x::debug_info(&r, "z80", "pasmo", "test.z80");
    assert_eq!(asm198x::render_sym(&info), "@entry = $8000\n");
}

/// The CP1610 listing (the one word-addressed CPU): addresses are decles, the
/// bytes column shows each decle's two raw bytes (`addr_unit = 2`).
#[test]
fn cp1610_listing_indexes_bytes_by_decle() {
    let src = "\torg 5000h\nstart:\tmovr r0, r1\n\tnop\n";
    let r = asm198x::assemble_cp1610(src).expect("assemble");
    let listing = asm198x::render_listing(src, &r, 2);
    let expected = [
        "                               \torg 5000h",
        "5000  00 81                    start:\tmovr r0, r1",
        "5001  00 34                    \tnop",
        "",
    ]
    .join("\n");
    assert_eq!(listing, expected);
}

// --- U4: the ca65 (NES) linker path ---

/// A two-segment NES program with a zero-page variable and a `=` constant —
/// the AE6 shape.
const NES_SRC: &str = "\
SPEED = 3\n\
.segment \"ZEROPAGE\"\n\
pos:    .res 1\n\
.segment \"CODE\"\n\
reset:  lda #SPEED\n\
        sta pos\n\
loop:   jmp loop\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";

/// AE6: the ca65 sidecar lists every used segment, a linker-placed label
/// resolves to its post-link ROM address in both the symbol table and the
/// line map, and `=` constants carry the constant kind.
#[test]
fn ca65_sidecar_covers_segments_symbols_and_lines() {
    let src_path = temp_source("nes-ae6", NES_SRC);
    let status = bin()
        .args(["--dialect", "ca65", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("nes"))
        .status()
        .expect("run asm198x");
    assert!(status.success());

    let ndjson =
        std::fs::read_to_string(src_path.with_extension("debug198x")).expect("read sidecar");
    let info = asm198x::debug198x::DebugInfo::read(&ndjson).expect("sidecar parses");
    assert_eq!(info.header.cpu, "6502");
    assert_eq!(info.header.dialect, "ca65");

    // Both code-bearing segments (and the zero-page one) are listed with their
    // absolute bases.
    let section = |name: &str| info.sections.iter().find(|s| s.name == name);
    assert_eq!(section("CODE").and_then(|s| s.base), Some(0x8000));
    assert_eq!(section("VECTORS").and_then(|s| s.base), Some(0xFFFA));
    assert_eq!(section("ZEROPAGE").and_then(|s| s.base), Some(0));

    // A linker-placed label resolves to its post-link CPU address in the
    // symbol table and maps back through the line map (AE6, AE1 mechanism).
    assert_eq!(info.addr_of("reset", None), Some(0x8000));
    assert_eq!(info.addr_of("loop", None), Some(0x8004));
    assert_eq!(
        info.line_at(0x8004, None).map(|l| l.line),
        Some(7),
        "`jmp loop`'s address maps to its source line"
    );

    // The zero-page variable and the `=` constant carry their kinds.
    assert_eq!(info.addr_of("pos", None), Some(0));
    let speed = info
        .symbols
        .iter()
        .find(|s| s.name == "SPEED")
        .expect("SPEED present");
    assert_eq!(
        speed.kind,
        asm198x::debug198x::SymbolKind::Const { value: 3 },
        "a `=` binding is a constant, not an address"
    );
}

/// AE2 for the ca65 path: the `.nes` ROM bytes with `--debug --sym` are
/// identical to the bytes without any debug flag.
#[test]
fn ca65_debug_flags_never_change_the_rom() {
    let plain_src = temp_source("nes-plain", NES_SRC);
    let flagged_src = temp_source("nes-flagged", NES_SRC);
    for (src, flags) in [
        (&plain_src, &[][..]),
        (&flagged_src, &["--debug", "--sym"][..]),
    ] {
        let status = bin()
            .args(["--dialect", "ca65"])
            .args(flags)
            .arg(src)
            .arg("-o")
            .arg(src.with_extension("nes"))
            .status()
            .expect("run asm198x");
        assert!(status.success());
    }
    let plain = std::fs::read(plain_src.with_extension("nes")).expect("plain ROM");
    let flagged = std::fs::read(flagged_src.with_extension("nes")).expect("flagged ROM");
    assert_eq!(plain, flagged, "debug flags never change a linked byte");
}

/// The ca65 `--sym` rendering resolves labels through their section bases —
/// post-link absolutes, not segment offsets.
#[test]
fn ca65_sym_renders_post_link_addresses() {
    let src_path = temp_source("nes-sym", NES_SRC);
    let status = bin()
        .args(["--dialect", "ca65", "--sym"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("nes"))
        .status()
        .expect("run asm198x");
    assert!(status.success());
    let sym = std::fs::read_to_string(src_path.with_extension("sym")).expect("read sym");
    assert_eq!(
        sym,
        "SPEED = $0003\nloop = $8004\npos = $0000\nreset = $8000\n"
    );
}

/// `--listing` stays rejected for ca65 (it needs a per-section byte map);
/// the error says which artifacts are available.
#[test]
fn ca65_listing_still_rejected() {
    let src_path = temp_source("nes-lst", NES_SRC);
    let out = bin()
        .args(["--dialect", "ca65", "--listing"])
        .arg(&src_path)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("`--debug` and `--sym` are"),
        "the error names what works: {stderr}"
    );
}

/// JSON mode + ca65 + `--debug`: the sidecar writes while stdout stays a
/// single JSON value (the linked-image result).
#[test]
fn ca65_json_mode_writes_sidecar() {
    let src_path = temp_source("nes-json", NES_SRC);
    let out = bin()
        .args(["--dialect", "ca65", "--message-format=json", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("nes"))
        .output()
        .expect("run asm198x");
    assert!(out.status.success());
    let _: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is exactly one JSON value");
    assert!(src_path.with_extension("debug198x").exists());
}

/// HEADER (iNES metadata) and CHARS (PPU space) are not CPU-addressable:
/// their sections carry no base, HEADER contributes no line spans, and CPU
/// zero-page lookups never alias onto them.
#[test]
fn ca65_non_cpu_segments_never_alias_the_zero_page() {
    let src = "\
.segment \"HEADER\"\n\
        .byte $4E, $45, $53, $1A\n\
.segment \"ZEROPAGE\"\n\
pos:    .res 1\n\
.segment \"CODE\"\n\
reset:  sta pos\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n\
.segment \"CHARS\"\n\
tiles:  .byte $FF, $00\n";
    let src_path = temp_source("nes-alias", src);
    let status = bin()
        .args(["--dialect", "ca65", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("nes"))
        .status()
        .expect("run asm198x");
    assert!(status.success());
    let ndjson =
        std::fs::read_to_string(src_path.with_extension("debug198x")).expect("read sidecar");
    let info = asm198x::debug198x::DebugInfo::read(&ndjson).expect("sidecar parses");

    let section = |name: &str| info.sections.iter().find(|s| s.name == name);
    assert_eq!(
        section("HEADER").map(|s| s.base),
        Some(None),
        "HEADER is file metadata, not a CPU address"
    );
    assert_eq!(
        section("CHARS").map(|s| s.base),
        Some(None),
        "CHARS is PPU space; a consumer supplies a BaseMap"
    );
    // A CPU zero-page lookup resolves the ZEROPAGE variable, never the iNES
    // header bytes or CHR data that share the raw value 0.
    assert_eq!(info.addr_of("pos", None), Some(0));
    assert!(
        info.line_at(0, None).is_none(),
        "no HEADER/CHR line span answers a CPU zero-page lookup"
    );
    assert!(
        info.addr_of("tiles", None).is_none(),
        "a PPU-space label has no CPU address without a BaseMap"
    );
}

/// Cheap (`@name`) labels render their source form in the record — the
/// internal control-byte key never leaks — and anonymous (`:`) labels stay
/// out of the symbol table entirely.
#[test]
fn ca65_cheap_and_anon_labels_render_cleanly() {
    let src = "\
.segment \"CODE\"\n\
reset:  lda #1\n\
@wait:  bne @wait\n\
:       jmp :-\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";
    let src_path = temp_source("nes-cheap", src);
    let status = bin()
        .args(["--dialect", "ca65", "--sym"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("nes"))
        .status()
        .expect("run asm198x");
    assert!(status.success());
    let sym = std::fs::read_to_string(src_path.with_extension("sym")).expect("read sym");
    assert_eq!(
        sym, "reset = $8000\nreset@wait = $8002\n",
        "cheap label renders as source form; anonymous labels are positional, not symbols"
    );
}

/// A duplicate symbol is rejected (as real ca65 rejects it) — accepting it
/// would leave a debug record disagreeing with the emitted bytes.
#[test]
fn ca65_duplicate_symbol_is_rejected() {
    let src = "\
.segment \"CODE\"\n\
reset:  lda #1\n\
reset:  lda #2\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";
    let src_path = temp_source("nes-dup", src);
    let out = bin()
        .args(["--dialect", "ca65"])
        .arg(&src_path)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "a duplicate label is an error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("duplicate symbol `reset`"),
        "the error names the symbol: {stderr}"
    );
}

/// An artifact path colliding with the output image is refused — the sidecar
/// must never clobber the just-written ROM.
#[test]
fn artifact_path_never_clobbers_the_output_image() {
    let src_path = temp_source("nes-clobber-out", NES_SRC);
    let rom = src_path.with_extension("nes");
    let out = bin()
        .args(["--dialect", "ca65"])
        .arg(format!("--debug={}", rom.display()))
        .arg(&src_path)
        .arg("-o")
        .arg(&rom)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "artifact onto the ROM is refused");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("refusing to overwrite the output image"),
        "the error explains the collision: {stderr}"
    );
}

// --- U5: the vasm (Amiga hunk) path ---

/// A two-section Amiga program with a cross-section reference and an `even`
/// pad — the AE4 shape.
const VASM_SRC: &str = "\
\tsection code,code\n\
start:\tlea data(pc),a0\n\
\tmoveq #5,d0\n\
loop:\tdbf d0,loop\n\
\trts\n\
\tsection data,data\n\
data:\tdc.w 1,2,3\n\
msg:\tdc.b \"hi\",0\n\
\teven\n\
tail:\tdc.l msg\n";

/// AE4: the vasm sidecar lists both sections (relocatable — no base), byte
/// ranges attribute to the right section, a cross-section label resolves once
/// a `BaseMap` supplies load addresses, and the hidden `even` pad byte belongs
/// to no line (the padding rule).
#[test]
fn vasm_sidecar_covers_sections_and_relocatable_lookups() {
    let src_path = temp_source("vasm-ae4", VASM_SRC);
    let exe = src_path.with_extension("exe");
    let status = bin()
        .args(["--dialect", "vasm", "--exe", "--debug"])
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .status()
        .expect("run asm198x");
    assert!(status.success());

    let ndjson =
        std::fs::read_to_string(src_path.with_extension("debug198x")).expect("read sidecar");
    let info = asm198x::debug198x::DebugInfo::read(&ndjson).expect("sidecar parses");
    assert_eq!(info.header.cpu, "68000");
    assert_eq!(info.header.dialect, "vasm");

    // Both hunks are listed, relocatable: no fabricated absolute bases.
    assert_eq!(
        info.sections.iter().map(|s| s.base).collect::<Vec<_>>(),
        vec![None, None],
        "hunks carry no base — the loader assigns addresses"
    );

    // Without a BaseMap nothing resolves absolutely; with one (the loaded hunk
    // addresses), a label in section 1 referenced from section 0 resolves.
    assert_eq!(info.addr_of("data", None), None, "no base map, no absolute");
    let bases: asm198x::debug198x::BaseMap = [(0, 0x2000), (1, 0x8000)].into_iter().collect();
    assert_eq!(info.addr_of("start", Some(&bases)), Some(0x2000));
    assert_eq!(info.addr_of("data", Some(&bases)), Some(0x8000));
    assert_eq!(
        info.line_at(0x2006, Some(&bases)).map(|l| l.line),
        Some(4),
        "`dbf` at code+6 maps to its source line"
    );
    // The `even` pad byte (data+9, between `msg` and `tail`) belongs to no line.
    assert!(
        info.line_at(0x8009, Some(&bases)).is_none(),
        "alignment fill is attributed to no source line"
    );
    assert_eq!(
        info.line_at(0x800A, Some(&bases)).map(|l| l.line),
        Some(10),
        "`tail:` data starts on the next even offset"
    );
}

/// AE2 for the vasm path: hunk-exe and flat outputs are byte-identical with
/// and without the debug flags.
#[test]
fn vasm_debug_flags_never_change_the_output() {
    // Exe (multi-section) half.
    let plain = temp_source("vasm-plain", VASM_SRC);
    let flagged = temp_source("vasm-flagged", VASM_SRC);
    for (src, flags) in [(&plain, &[][..]), (&flagged, &["--debug", "--sym"][..])] {
        let status = bin()
            .args(["--dialect", "vasm", "--exe"])
            .args(flags)
            .arg(src)
            .arg("-o")
            .arg(src.with_extension("exe"))
            .status()
            .expect("run asm198x");
        assert!(status.success());
    }
    assert_eq!(
        std::fs::read(plain.with_extension("exe")).expect("plain exe"),
        std::fs::read(flagged.with_extension("exe")).expect("flagged exe"),
        "debug flags never change a hunk-exe byte"
    );

    // Flat (single-section) half.
    let flat_src = "\tmoveq #7,d0\nspin:\tdbf d0,spin\n\trts\n";
    let plain = temp_source("vasm-flat-plain", flat_src);
    let flagged = temp_source("vasm-flat-flagged", flat_src);
    for (src, flags) in [(&plain, &[][..]), (&flagged, &["--debug", "--sym"][..])] {
        let status = bin()
            .args(["--dialect", "vasm"])
            .args(flags)
            .arg(src)
            .arg("-o")
            .arg(src.with_extension("bin"))
            .status()
            .expect("run asm198x");
        assert!(status.success());
    }
    assert_eq!(
        std::fs::read(plain.with_extension("bin")).expect("plain bin"),
        std::fs::read(flagged.with_extension("bin")).expect("flagged bin"),
        "debug flags never change a flat byte"
    );
}

/// The flat vasm record: one section, `equ` constants carry the constant
/// kind, and labels are section-relative.
#[test]
fn vasm_flat_record_carries_constants_and_labels() {
    let src = "SIZE\tequ 40\nstart:\tmove.l #SIZE,d1\n\trts\n";
    let src_path = temp_source("vasm-flat-rec", src);
    let status = bin()
        .args(["--dialect", "vasm", "--debug", "--sym"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("bin"))
        .status()
        .expect("run asm198x");
    assert!(status.success());
    let ndjson =
        std::fs::read_to_string(src_path.with_extension("debug198x")).expect("read sidecar");
    let info = asm198x::debug198x::DebugInfo::read(&ndjson).expect("sidecar parses");
    let size = info
        .symbols
        .iter()
        .find(|s| s.name == "SIZE")
        .expect("SIZE present");
    assert_eq!(
        size.kind,
        asm198x::debug198x::SymbolKind::Const { value: 40 },
        "an `equ` is a constant, not an address"
    );
    let sym = std::fs::read_to_string(src_path.with_extension("sym")).expect("read sym");
    assert_eq!(sym, "SIZE = $0028\nstart = $0000\n");
}

/// JSON mode + vasm + `--debug`: the sidecar writes while stdout stays a
/// single JSON value.
#[test]
fn vasm_json_mode_writes_sidecar() {
    let src_path = temp_source("vasm-json", VASM_SRC);
    let out = bin()
        .args([
            "--dialect",
            "vasm",
            "--exe",
            "--message-format=json",
            "--debug",
        ])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("exe"))
        .output()
        .expect("run asm198x");
    assert!(out.status.success());
    let _: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is exactly one JSON value");
    assert!(src_path.with_extension("debug198x").exists());
}

/// A multi-hunk exe's `--sym` renders section-qualified offsets — bare
/// offsets would collide across hunks (`start` and `data` are both offset 0).
#[test]
fn vasm_exe_sym_is_section_qualified() {
    let src_path = temp_source("vasm-exe-sym", VASM_SRC);
    let status = bin()
        .args(["--dialect", "vasm", "--exe", "--sym"])
        .arg(&src_path)
        .arg("-o")
        .arg(src_path.with_extension("exe"))
        .status()
        .expect("run asm198x");
    assert!(status.success());
    let sym = std::fs::read_to_string(src_path.with_extension("sym")).expect("read sym");
    assert_eq!(
        sym,
        "data = data+$0000\nloop = code+$0006\nmsg = data+$0006\nstart = code+$0000\ntail = data+$000A\n"
    );
}

/// `--debug` on a multi-file NES program (language-surface U5): the sidecar's
/// `Header.sources` lists every file in `FileId` order and each line record
/// names the file its bytes were written in — the CHARS data attributes to
/// the included art file, the code to the root input.
#[test]
fn ca65_multifile_debug_line_records_name_the_included_file() {
    let dir = std::env::temp_dir().join("asm198x-debug-cli-nes-multi");
    std::fs::create_dir_all(&dir).expect("temp tree");
    let main = dir.join("main.s");
    std::fs::write(
        &main,
        ".segment \"CODE\"\nreset:  lda #$01\n.include \"art.s\"\n\
         .segment \"VECTORS\"\n        .word 0, reset, 0\n",
    )
    .expect("write main");
    std::fs::write(
        dir.join("art.s"),
        ".segment \"CHARS\"\ntiles:  .byte $AA, $BB\n",
    )
    .expect("write art");
    let status = bin()
        .args(["--dialect", "ca65", "--debug"])
        .arg(&main)
        .arg("-o")
        .arg(main.with_extension("nes"))
        .status()
        .expect("run asm198x");
    assert!(status.success());
    let ndjson = std::fs::read_to_string(main.with_extension("debug198x")).expect("read sidecar");
    let info = asm198x::debug198x::DebugInfo::read(&ndjson).expect("sidecar parses");

    assert_eq!(info.header.sources.len(), 2, "root + the include");
    assert!(
        info.header.sources[1].ends_with("art.s"),
        "sources[1] is the include (FileId order): {:?}",
        info.header.sources
    );
    let code = info
        .lines
        .iter()
        .find(|l| l.line == 2 && l.file.ends_with("main.s"))
        .expect("the root's lda has a span in main.s");
    assert_eq!(code.length, 2);
    let tiles = info
        .lines
        .iter()
        .find(|l| l.line == 2 && l.file.ends_with("art.s"))
        .expect("the include's .byte has a span in art.s");
    assert_eq!(tiles.length, 2, "the CHARS bytes attribute to art.s");
}
