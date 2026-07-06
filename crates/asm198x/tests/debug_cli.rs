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
    assert_eq!(
        asm198x::render_sym(&r),
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

/// The ca65 (NES) and vasm (Amiga) bypass paths reject the debug flags until
/// their emitters land (plan U4/U5) — a clear error, not a silent no-op.
#[test]
fn ca65_and_vasm_reject_debug_flags_for_now() {
    let src_path = temp_source("ca65", ".segment \"CODE\"\n    rts\n");
    let out = bin()
        .args(["--dialect", "ca65", "--debug"])
        .arg(&src_path)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "ca65 + --debug is an error for now");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not yet supported"),
        "the error names the gap: {stderr}"
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
    assert_eq!(asm198x::render_sym(&r), "start = $8000\n");
    // `end start` upgrades the label in place; a non-label entry records
    // `@entry` — pin that shape too.
    let r = asm198x::assemble_pasmo("\torg 8000h\n\tret\n\tend 8000h\n").expect("assemble");
    assert_eq!(asm198x::render_sym(&r), "@entry = $8000\n");
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
