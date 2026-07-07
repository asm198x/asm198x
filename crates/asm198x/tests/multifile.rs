//! U1 + U2 (language surface): the multi-file source model and the include
//! mechanism. The U1 half exercises the loader seam (KTD8) — filesystem +
//! in-memory impls — the `SourceMap`'s `FileId` allocation and
//! dedup-by-canonical-path, and the CLI's repeatable `-I` flag plus the
//! rustc-style `file:line:col` human error rendering. The U2 half drives the
//! include-capable sjasmplus entry points end-to-end: nested includes, state
//! flowing out of an include (KTD1), cycle/depth/missing-file diagnostics,
//! and the file table surviving failure (KTD2). Every expected byte sequence
//! below is pinned by the sjasmplus probe runs recorded in the U2 report —
//! the reference's behaviour, not an assumption.

use std::path::PathBuf;
use std::process::Command;

use asm198x::source::{FsLoader, MemoryLoader, SourceLoader, SourceMap};
use asm198x::{
    FileId, assemble_1802, assemble_1802_files, assemble_2650, assemble_2650_files, assemble_8039,
    assemble_8039_files, assemble_8048, assemble_8048_files, assemble_acme, assemble_acme_files,
    assemble_ca65, assemble_ca65_816, assemble_ca65_816_files, assemble_ca65_files,
    assemble_ca65_files_debug, assemble_ca65_huc6280, assemble_ca65_huc6280_files,
    assemble_cp1610_files, assemble_f8, assemble_f8_files, assemble_i8080, assemble_i8080_files,
    assemble_lwasm, assemble_lwasm_files, assemble_m6800, assemble_m6800_files, assemble_pasmo,
    assemble_pasmo_files, assemble_pdp11, assemble_pdp11_files, assemble_rgbasm,
    assemble_rgbasm_files, assemble_scmp, assemble_scmp_files, assemble_sjasmplus,
    assemble_sjasmplus_files, assemble_tms7000, assemble_tms7000_files, assemble_tms9900,
    assemble_tms9900_files, assemble_vasm, assemble_vasm_exe_files, assemble_vasm_warned,
    assemble_vasm_warned_files, assemble_z8000, assemble_z8000_files, assemble_z8001,
    assemble_z8001_files,
};

/// The built `asm198x` binary under test.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
}

/// A uniquely-tagged temp directory (so parallel tests never share a tree).
fn temp_tree(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("asm198x-multifile-{tag}"));
    std::fs::create_dir_all(&dir).expect("create temp tree");
    dir
}

/// Write `source` to a uniquely-named temp file and return its path.
fn temp_source(tag: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("asm198x-multifile-{tag}.s"));
    std::fs::write(&path, source).expect("write temp source");
    path
}

// --- SourceMap + filesystem loader ---

/// The same file requested via two spellings (`inc/../inc/defs.inc` vs
/// `inc/defs.inc`) dedups to one `FileId` — the loader canonicalizes, the map
/// keys on the canonical path (KTD2).
#[test]
fn two_spellings_of_one_file_share_a_fileid() {
    let dir = temp_tree("dedup");
    std::fs::create_dir_all(dir.join("inc")).expect("create inc/");
    std::fs::write(dir.join("inc/defs.inc"), "answer equ 42\n").expect("write include");

    let loader = FsLoader::new(&dir, Vec::new());
    let mut map = SourceMap::new("main.s", "        include \"inc/defs.inc\"\n");
    let a = map
        .load(&loader, "inc/../inc/defs.inc", FileId(0), 1)
        .expect("dotted spelling loads");
    let b = map
        .load(&loader, "inc/defs.inc", FileId(0), 2)
        .expect("plain spelling loads");

    assert_eq!(a, b, "both spellings resolve to the one FileId");
    assert_eq!(
        a,
        FileId(1),
        "the first include after the root is FileId(1)"
    );
    assert_eq!(
        map.file_table().len(),
        2,
        "root + one deduped include; no duplicate entry"
    );
    assert_eq!(
        map.contents(a),
        Some("answer equ 42\n"),
        "the include's contents are held by the map"
    );
}

/// `-I` search directories are consulted in order after the input's own
/// directory: a file present only under a search dir resolves through it.
#[test]
fn fs_loader_searches_include_dirs_in_order() {
    let base = temp_tree("search-base");
    let first = temp_tree("search-first");
    let second = temp_tree("search-second");
    // The same name in both search dirs: the first-listed dir must win.
    std::fs::write(first.join("defs.inc"), "first\n").expect("write first");
    std::fs::write(second.join("defs.inc"), "second\n").expect("write second");

    let loader = FsLoader::new(&base, vec![first, second]);
    let mut map = SourceMap::new("main.s", "");
    let id = map
        .load(&loader, "defs.inc", FileId(0), 1)
        .expect("resolves via the search path");
    assert_eq!(
        map.contents(id),
        Some("first\n"),
        "the earlier -I directory wins"
    );
}

// --- in-memory loader (hermetic tests, KTD8) ---

/// A missing file's error names both the request as written and the requesting
/// file, so an include failure is diagnosable without a stack trace.
#[test]
fn memory_loader_missing_file_names_request_and_requester() {
    let loader = MemoryLoader::new();
    let err = loader
        .load_text("defs.inc", Some("main.s"))
        .expect_err("nothing was registered");
    let msg = err.to_string();
    assert!(msg.contains("defs.inc"), "names the request: {msg}");
    assert!(msg.contains("main.s"), "names the requesting file: {msg}");
}

/// A binary load returns bytes through the same seam and mints no `FileId` —
/// spans only ever point into source files (KTD8).
#[test]
fn binary_load_returns_bytes_and_mints_no_fileid() {
    let loader = MemoryLoader::new().binary("sprite.bin", vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let map = SourceMap::new("main.s", "        incbin \"sprite.bin\"\n");

    let bytes = loader
        .load_binary("sprite.bin", Some("main.s"))
        .expect("registered binary loads");
    assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(
        map.file_table(),
        vec!["main.s".to_string()],
        "the file table still holds only the root — no FileId for binary data"
    );
}

/// The include graph records who included whom, at which line: the chain from
/// a nested include walks back to the root, innermost hop first.
#[test]
fn include_graph_records_the_chain_back_to_the_root() {
    let loader = MemoryLoader::new()
        .text("a.inc", "        include \"b.inc\"\n")
        .text("b.inc", "        nop\n");
    let mut map = SourceMap::new("main.s", "        include \"a.inc\"\n");
    let a = map.load(&loader, "a.inc", FileId(0), 3).expect("a loads");
    let b = map.load(&loader, "b.inc", a, 5).expect("b loads");

    assert_eq!(
        map.include_chain(b),
        vec![("a.inc".to_string(), 5), ("main.s".to_string(), 3)],
        "one hop per includer, innermost first"
    );
    assert!(
        map.include_chain(FileId(0)).is_empty(),
        "the root was included from nowhere"
    );
}

// --- CLI: repeatable -I, unknown flags, human rendering ---

/// `-I <dir>` is accepted, repeatable, and (until U2 consumes it) inert: a
/// trivial single-file assemble still succeeds with search dirs given.
#[test]
fn cli_accepts_repeatable_include_dirs() {
    let src = temp_source("cli-inc", "*=$8000\n        rts\n");
    let out = bin()
        .args(["--cpu", "6502", "-I", "somewhere", "-I", "elsewhere"])
        .arg(&src)
        .output()
        .expect("run asm198x");
    assert!(
        out.status.success(),
        "-I parses and does not fail the assemble: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A genuinely unknown flag is still rejected — `-I` did not loosen the
/// unknown-flag guard.
#[test]
fn cli_still_rejects_unknown_flags() {
    let src = temp_source("cli-unknown", "*=$8000\n        rts\n");
    let out = bin()
        .args(["--frobnicate"])
        .arg(&src)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "an unknown flag is an error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown flag"),
        "the rejection names the problem: {stderr}"
    );
}

/// A span-carrying assemble failure renders rustc-style on stderr:
/// `file:line:col: error: message` — the file name, not a bare `line N:`.
#[test]
fn cli_human_error_renders_file_line_col() {
    let src = temp_source("cli-render", "*=$8000\n        lda #$fff\n");
    let out = bin()
        .args(["--cpu", "6502"])
        .arg(&src)
        .output()
        .expect("run asm198x");
    assert!(!out.status.success(), "an oversize immediate fails");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let want = format!("{}:2:13: error:", src.display());
    assert!(
        stderr.contains(&want),
        "rustc-style rendering `{want}` on stderr: {stderr}"
    );
}

// ===========================================================================
// U2 — the include mechanism on sjasmplus (hermetic, MemoryLoader)
// ===========================================================================

/// AE1's mechanism: a two-file program assembles byte-identical to its
/// flattened equivalent, and the result's file table lists both files in
/// `FileId` order (KTD2).
#[test]
fn include_two_files_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("defs.inc", "VAL equ $2b\n");
    let src = "        org $8000\n        include \"defs.inc\"\n        ld a,VAL\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    let flat = assemble_sjasmplus("        org $8000\nVAL equ $2b\n        ld a,VAL\n")
        .expect("flattened equivalent assembles");
    assert_eq!(r.bytes, flat.bytes, "include is transparent to the bytes");
    assert_eq!(
        r.files,
        vec!["main.asm".to_string(), "defs.inc".to_string()],
        "the file table survives into the result (KTD2)"
    );
}

/// Three-deep nesting: main → a → b, with code at every level, in include
/// order.
#[test]
fn include_three_deep_nests() {
    let loader = MemoryLoader::new()
        .text(
            "a.inc",
            "        ld b,2\n        include \"b.inc\"\n        ld d,4\n",
        )
        .text("b.inc", "        ld c,3\n");
    let src = "        org $8000\n        ld a,1\n        include \"a.inc\"\n        ld e,5\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x01, 0x06, 0x02, 0x0E, 0x03, 0x16, 0x04, 0x1E, 0x05],
        "bytes interleave in include order"
    );
    assert_eq!(r.files, vec!["main.asm", "a.inc", "b.inc"]);
}

/// KTD1's driver: an `equ` defined inside the include feeds opcode-embedded
/// operands (`bit`, `rst`) and a `ds` count on the includer's *later* lines.
/// Probe-pinned: sjasmplus emits CB 6F / DF / three zero bytes / 3E 01.
#[test]
fn include_defined_equ_feeds_later_includer_lines() {
    let loader = MemoryLoader::new().text("defs.inc", "BITNUM equ 5\nRSTVEC equ $18\nPAD equ 3\n");
    let src = "        org $8000\n        include \"defs.inc\"\n        bit BITNUM,a\n        rst RSTVEC\n        ds PAD\n        ld a,1\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xCB, 0x6F, 0xDF, 0x00, 0x00, 0x00, 0x3E, 0x01],
        "include-defined constants flow out to later form selection (KTD1)"
    );
}

/// Locals across the boundary, both directions (probe-pinned): the includer's
/// current global scopes a leading-`.` local at the top of the include, the
/// include may reference the includer's locals, and the includer's local
/// *after* the include still sits in the same scope.
#[test]
fn locals_scope_across_the_include_boundary() {
    let loader = MemoryLoader::new().text(
        "loc.inc",
        ".inloc: nop\n        jr .inloc\n        jr .here\n",
    );
    let src = "        org $8000\nstart:\n.here:  nop\n        include \"loc.inc\"\n        jr .after\n.after: nop\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x00, 0x00, 0x18, 0xFD, 0x18, 0xFA, 0x18, 0x00, 0x00],
        "probe-pinned bytes"
    );
    assert_eq!(r.symbols.get("start.here"), Some(&0x8000));
    assert_eq!(r.symbols.get("start.inloc"), Some(&0x8001));
    assert_eq!(r.symbols.get("start.after"), Some(&0x8008));
}

/// A global defined *inside* the include becomes the current global for the
/// includer's subsequent locals (probe-pinned: `mid.tail`, not `start.tail`).
#[test]
fn global_defined_in_include_rescopes_later_includer_locals() {
    let loader = MemoryLoader::new().text("glob.inc", "mid:    nop\n");
    let src = "        org $8000\nstart:  nop\n        include \"glob.inc\"\n.tail:  nop\n        jr .tail\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.symbols.get("mid.tail"), Some(&0x8002), "scope flows out");
    assert!(!r.symbols.contains_key("start.tail"));
}

/// A file included twice (non-cyclically) is processed twice — sjasmplus
/// re-reads it (probe-pinned: two nops) — while the table dedups to one entry.
#[test]
fn file_included_twice_is_processed_twice_with_one_fileid() {
    let loader = MemoryLoader::new().text("body.inc", "        nop\n");
    let src = "        org $8000\n        include \"body.inc\"\n        include \"body.inc\"\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x00, 0x00], "both inclusions emit");
    assert_eq!(r.files, vec!["main.asm", "body.inc"], "one FileId per file");
}

/// A label on the include line binds at the include point's address
/// (probe-pinned: `here` = $8000, `jr here` = 18 FD).
#[test]
fn label_on_the_include_line_binds_at_the_include_point() {
    let loader = MemoryLoader::new().text("body.inc", "        nop\n");
    let src = "        org $8000\nhere:   include \"body.inc\"\n        jr here\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x00, 0x18, 0xFD]);
    assert_eq!(r.symbols.get("here"), Some(&0x8000));
}

/// The `<file>` and bare spellings resolve like the quoted form (probe: all
/// three assemble; our loader applies one search order to every spelling).
#[test]
fn angle_and_bare_include_spellings_resolve() {
    let loader = MemoryLoader::new().text("body.inc", "        nop\n");
    let src = "        org $8000\n        INCLUDE <body.inc>\n        include body.inc\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x00, 0x00]);
}

// --- failure paths: the table survives, diagnostics carry real FileIds ---

/// An error inside an included file names *that* file and line: the span's
/// `FileId` resolves through the failure-path source map (KTD2), and the
/// include graph yields the `included from` chain.
#[test]
fn error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.inc", "        include \"b.inc\"\n")
        .text("b.inc", "        nop\n        frob\n");
    let src = "        org $8000\n        include \"a.inc\"\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.inc, not of main.asm");
    let table = e.source_map.file_table();
    assert_eq!(
        table.get(span.file.0 as usize).map(String::as_str),
        Some("b.inc"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.inc".to_string(), 1), ("main.asm".to_string(), 2)],
        "the include chain walks back to the root"
    );
}

/// A missing include target is a diagnostic at the directive's span — the
/// includer's file:line (and the operand's column) — not a CLI read error.
#[test]
fn missing_include_target_reports_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let src = "        org $8000\n        include \"nothere.inc\"\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("missing target");
    assert!(
        e.error.message.contains("nothere.inc"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 2);
    assert_eq!(span.col, 17, "points at the operand (the file name)");
    assert_eq!(
        e.source_map.file_table().get(span.file.0 as usize),
        Some(&"main.asm".to_string())
    );
}

/// A self-include is a cycle diagnostic listing the chain (better than the
/// reference's depth-overflow error — diagnostics are not byte-compared,
/// KTD5).
#[test]
fn self_include_reports_the_cycle() {
    // The loader serves the root under its own name, so `include "main.asm"`
    // resolves back to FileId(0)'s canonical path.
    let src = "        include \"main.asm\"\n";
    let loader = MemoryLoader::new().text("main.asm", src);
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("cycle");
    assert!(
        e.error.message.contains("include cycle"),
        "names the cycle: {}",
        e.error.message
    );
    assert!(
        e.error.message.contains("main.asm -> main.asm"),
        "lists the chain: {}",
        e.error.message
    );
}

/// An A→B→A cycle lists the full chain in order.
#[test]
fn two_file_cycle_lists_the_chain() {
    let loader = MemoryLoader::new()
        .text("a.inc", "        include \"b.inc\"\n")
        .text("b.inc", "        include \"a.inc\"\n");
    let src = "        include \"a.inc\"\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("cycle");
    assert!(
        e.error
            .message
            .contains("main.asm -> a.inc -> b.inc -> a.inc"),
        "the chain in include order: {}",
        e.error.message
    );
}

/// The depth cap backstops a pathological non-cyclic chain (distinct files
/// nested past the limit).
#[test]
fn depth_cap_fires_on_a_pathological_chain() {
    let mut loader = MemoryLoader::new();
    for i in 0..70 {
        loader = loader.text(
            format!("f{i}.inc"),
            format!("        include \"f{}.inc\"\n", i + 1),
        );
    }
    loader = loader.text("f70.inc", "        nop\n");
    let src = "        include \"f0.inc\"\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("too deep");
    assert!(
        e.error.message.contains("nested"),
        "names the depth problem: {}",
        e.error.message
    );
}

/// The single-source entry points still mean "one file, no includes": an
/// `include` directive there is a clear error, not a silent skip.
#[test]
fn single_source_entry_rejects_include_with_a_clear_error() {
    let e = assemble_sjasmplus("        include \"defs.inc\"\n").expect_err("no loader here");
    assert!(
        e.message.contains("include") && e.message.contains("multi-file"),
        "points at the multi-file entry: {}",
        e.message
    );
}

// --- U2 CLI: -I resolves an include from a search dir; notes on failure ---

/// A two-file assemble through the binary, the include resolving via `-I`
/// from a directory other than the input's own.
#[test]
fn cli_assembles_an_include_via_a_search_dir() {
    let srcdir = temp_tree("u2-cli-src");
    let incdir = temp_tree("u2-cli-inc");
    let main = srcdir.join("main.asm");
    std::fs::write(
        &main,
        "        org $8000\n        include \"defs.inc\"\n        ld a,VAL\n",
    )
    .expect("write main");
    std::fs::write(incdir.join("defs.inc"), "VAL equ $2b\n").expect("write include");
    let out = srcdir.join("main.bin");
    let run = bin()
        .args(["--dialect", "sjasmplus", "-I"])
        .arg(&incdir)
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "assembles: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(&out).expect("output written"),
        vec![0x3E, 0x2B]
    );
}

/// A failure inside an included file renders rustc-style with the included
/// file's name plus an `included from` note naming the includer and line.
#[test]
fn cli_error_in_include_carries_an_included_from_note() {
    let dir = temp_tree("u2-cli-note");
    let main = dir.join("main.asm");
    std::fs::write(&main, "        org $8000\n        include \"bad.inc\"\n").expect("write main");
    std::fs::write(dir.join("bad.inc"), "        frob\n").expect("write include");
    let run = bin()
        .args(["--dialect", "sjasmplus"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success(), "frob fails the assemble");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("bad.inc:1"),
        "names the included file and line: {stderr}"
    );
    assert!(
        stderr.contains("included from") && stderr.contains("main.asm:2"),
        "carries the included-from note: {stderr}"
    );
}

/// JSON failure output stays a bare Diagnostic array; the additive `path`
/// field on the span resolves the file without the success-only table (KTD2).
#[test]
fn cli_json_failure_span_carries_the_included_files_path() {
    let dir = temp_tree("u2-cli-json");
    let main = dir.join("main.asm");
    std::fs::write(&main, "        include \"bad.inc\"\n").expect("write main");
    std::fs::write(dir.join("bad.inc"), "        frob\n").expect("write include");
    let run = bin()
        .args(["--dialect", "sjasmplus", "--message-format=json"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success());
    let diags: Vec<asm198x::Diagnostic> =
        serde_json::from_slice(&run.stdout).expect("a bare Diagnostic array");
    assert_eq!(diags.len(), 1);
    let span = diags[0].span.as_ref().expect("span present");
    assert_eq!(span.line, 1, "line 1 of bad.inc");
    assert!(
        span.path.as_deref().is_some_and(|p| p.ends_with("bad.inc")),
        "the span's path names the included file: {:?}",
        span.path
    );
}

// ===========================================================================
// U3 — the incbin mechanism on the z80 family (hermetic, MemoryLoader).
// Every expected byte sequence and error posture below is pinned by the
// sjasmplus/pasmo probe runs recorded in the U3 report (KTD5).
// ===========================================================================

/// The shared 8-byte probe asset: `10 11 12 13 14 15 16 17`.
fn asset() -> Vec<u8> {
    (0x10..0x18).collect()
}

/// AE2's mechanism: a plain incbin inserts the whole asset at the current
/// location (probe t1: `AA 10..17 BB`), and the binary asset mints no
/// `FileId` — the file table holds only the root (KTD8).
#[test]
fn incbin_inserts_the_asset_at_the_current_location() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "        org $8000\n        db $aa\n        incbin \"data.bin\"\n        db $bb\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    let mut want = vec![0xAA];
    want.extend(asset());
    want.push(0xBB);
    assert_eq!(r.bytes, want, "probe-pinned bytes (t1)");
    assert_eq!(
        r.files,
        vec!["main.asm".to_string()],
        "binary data mints no FileId (KTD8)"
    );
}

/// The offset and offset+length forms slice the asset (probes t2/t3), and
/// both arguments take expressions of `equ` constants (probe t16).
#[test]
fn incbin_offset_and_length_slice_the_asset() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let offset = assemble_sjasmplus_files(
        "        org $8000\n        incbin \"data.bin\",2\n",
        "main.asm",
        &loader,
    )
    .expect("offset form");
    assert_eq!(offset.bytes, vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17]);

    let both = assemble_sjasmplus_files(
        "        org $8000\n        incbin \"data.bin\",2,3\n",
        "main.asm",
        &loader,
    )
    .expect("offset+length form");
    assert_eq!(both.bytes, vec![0x12, 0x13, 0x14]);

    let exprs = assemble_sjasmplus_files(
        "OFF equ 2\nLEN equ 3\n        org $8000\n        incbin \"data.bin\",OFF,LEN\n",
        "main.asm",
        &loader,
    )
    .expect("equ-constant args");
    assert_eq!(exprs.bytes, vec![0x12, 0x13, 0x14]);
}

/// Negative offsets count back from EOF and negative lengths mean "all but
/// the last |n| of the remaining" (probes t8/t9/t11/t12).
#[test]
fn incbin_negative_offset_and_length_count_from_the_end() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let cases: &[(&str, Vec<u8>)] = &[
        ("        incbin \"data.bin\",-2\n", vec![0x16, 0x17]),
        ("        incbin \"data.bin\",-4,2\n", vec![0x14, 0x15]),
        ("        incbin \"data.bin\",2,-3\n", vec![0x12, 0x13, 0x14]),
        (
            "        incbin \"data.bin\",0,-2\n",
            vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15],
        ),
    ];
    for (line, want) in cases {
        let src = format!("        org $8000\n{line}");
        let r = assemble_sjasmplus_files(&src, "main.asm", &loader).expect(line);
        assert_eq!(&r.bytes, want, "probe-pinned bytes for {line}");
    }
}

/// The `<file>` and bare spellings resolve like the quoted form for
/// sjasmplus (probes t14/t15), including a bare name with an offset tail.
#[test]
fn incbin_angle_and_bare_spellings_resolve() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "        org $8000\n        INCBIN <data.bin>\n        incbin data.bin,7\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    let mut want = asset();
    want.push(0x17);
    assert_eq!(r.bytes, want);
}

/// A missing asset is a diagnostic at the directive's span — the operand
/// column, the requesting file — naming the request (probe t10 posture).
#[test]
fn missing_incbin_asset_reports_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let src = "        org $8000\n        incbin \"nothere.bin\"\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("missing asset");
    assert!(
        e.error.message.contains("nothere.bin"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 2);
    assert_eq!(span.col, 16, "points at the operand (the file name)");
    assert_eq!(
        e.source_map.file_table().get(span.file.0 as usize),
        Some(&"main.asm".to_string())
    );
}

/// An out-of-range window — offset beyond EOF, length beyond the remaining
/// bytes, negative forms overshooting — is the reference's error posture
/// (probes t4/t5/t13/t20: `file too short`, exit 1), at the directive's span.
#[test]
fn incbin_window_outside_the_file_is_an_error() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    for line in [
        "        incbin \"data.bin\",100\n",
        "        incbin \"data.bin\",4,100\n",
        "        incbin \"data.bin\",-100\n",
        "        incbin \"data.bin\",2,-10\n",
    ] {
        let src = format!("        org $8000\n{line}");
        let e = assemble_sjasmplus_files(&src, "main.asm", &loader).expect_err(line);
        assert!(
            e.error.message.contains("too short"),
            "the reference's `file too short` posture for {line}: {}",
            e.error.message
        );
        assert_eq!(
            e.error.span.as_ref().map(|s| s.line),
            Some(2),
            "at the directive's span for {line}"
        );
    }
}

/// A zero-length window — an explicit `,N,0` or an offset exactly at EOF —
/// emits nothing and assembles cleanly, with the reference's advisory
/// (probes t6/t7: warning `requested to include no data`, exit 0).
#[test]
fn incbin_zero_length_emits_nothing_and_assembles() {
    let loader = MemoryLoader::new()
        .binary("data.bin", asset())
        .binary("empty.bin", Vec::new());
    for line in [
        "        incbin \"data.bin\",2,0\n",
        "        incbin \"data.bin\",8\n",
        "        incbin \"empty.bin\"\n",
    ] {
        let src = format!("        org $8000\n        db $aa\n{line}        db $bb\n");
        let r = assemble_sjasmplus_files(&src, "main.asm", &loader).expect(line);
        assert_eq!(r.bytes, vec![0xAA, 0xBB], "no payload bytes for {line}");
        assert_eq!(
            r.warnings.len(),
            1,
            "the reference's no-data advisory for {line}"
        );
        assert!(
            r.warnings[0].message.contains("no data"),
            "names the problem: {}",
            r.warnings[0].message
        );
    }
}

/// An incbin pushing the image past 64K fails with the engine cap error
/// carrying the *incbin's* span — proven inside an include, so the file is
/// the include's, not the root's (the U2 mechanism).
#[test]
fn incbin_past_the_64k_cap_carries_the_directive_span() {
    let loader = MemoryLoader::new()
        .text("art.inc", "        nop\n        incbin \"data.bin\"\n")
        .binary("data.bin", asset());
    let src = "        org $fffe\n        include \"art.inc\"\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("past 64K");
    assert!(
        e.error.message.contains("64K"),
        "names the cap: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span present");
    assert_eq!(span.line, 2, "the incbin's line inside art.inc");
    assert_eq!(
        e.source_map.file_table().get(span.file.0 as usize),
        Some(&"art.inc".to_string()),
        "the span names the include that holds the incbin"
    );
}

/// One `LineRec` covers the whole payload at the directive's line and file —
/// here inside an include, so `file` is the include's `FileId`.
#[test]
fn incbin_payload_gets_one_linerec_at_the_directive() {
    let loader = MemoryLoader::new()
        .text("art.inc", "        incbin \"data.bin\",0,4\n")
        .binary("data.bin", asset());
    let src = "        org $8000\n        include \"art.inc\"\n        nop\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x10, 0x11, 0x12, 0x13, 0x00]);
    let rec = r
        .debug
        .lines
        .iter()
        .find(|l| l.length == 4)
        .expect("one record covers the payload");
    assert_eq!(rec.line, 1, "the directive's line inside art.inc");
    assert_eq!(rec.offset, 0);
    assert_eq!(
        r.files.get(rec.file.0 as usize).map(String::as_str),
        Some("art.inc"),
        "the record names the include, not the root"
    );
}

/// A label on the incbin line binds at the payload's start address.
#[test]
fn label_on_the_incbin_line_binds_at_the_payload() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "        org $8000\n        nop\nart:    incbin \"data.bin\",0,2\n        jr art\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x00, 0x10, 0x11, 0x18, 0xFC]);
    assert_eq!(r.symbols.get("art"), Some(&0x8001));
}

/// The single-source entry points still mean "one file": an `incbin` there is
/// a clear pointer to the multi-file entry, not a silent skip.
#[test]
fn single_source_entry_rejects_incbin_with_a_clear_error() {
    for result in [
        assemble_sjasmplus("        incbin \"data.bin\"\n"),
        assemble_pasmo("        incbin \"data.bin\"\n"),
    ] {
        let e = result.expect_err("no loader here");
        assert!(
            e.message.contains("incbin") && e.message.contains("multi-file"),
            "points at the multi-file entry: {}",
            e.message
        );
    }
}

// --- pasmo (U3): the plain form only, probe-pinned ---

/// pasmo's plain `incbin "file"` inserts the whole asset (probe p1); the
/// quoted and bare spellings both resolve (probe p5).
#[test]
fn pasmo_plain_incbin_inserts_the_asset() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "        org $8000\n        db $aa\n        incbin \"data.bin\"\n        incbin data.bin\n        db $bb\n";
    let r = assemble_pasmo_files(src, "main.asm", &loader).expect("assembles");
    let mut want = vec![0xAA];
    want.extend(asset());
    want.extend(asset());
    want.push(0xBB);
    assert_eq!(r.bytes, want);
}

/// pasmo has no offset/length tail (probe p2/p3: `End line expected but
/// ','found`) — the comma is a parse error, at the directive's line.
#[test]
fn pasmo_incbin_rejects_an_offset_tail() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "        org $8000\n        incbin \"data.bin\",2\n";
    let e = assemble_pasmo_files(src, "main.asm", &loader).expect_err("no tail in pasmo");
    assert!(
        e.error.message.contains("only a file name"),
        "names the problem: {}",
        e.error.message
    );
    assert_eq!(e.error.line, 2);
}

/// pasmo's `<file>` is a literal file name, not a quote form (probe p6): the
/// loader is asked for `<data.bin>` verbatim, which fails as not-found.
#[test]
fn pasmo_incbin_angle_brackets_are_literal() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "        org $8000\n        incbin <data.bin>\n";
    let e = assemble_pasmo_files(src, "main.asm", &loader).expect_err("literal <data.bin>");
    assert!(
        e.error.message.contains("<data.bin>"),
        "asked for the verbatim token: {}",
        e.error.message
    );

    // And a file literally so named *does* resolve, matching the reference.
    let odd = MemoryLoader::new().binary("<data.bin>", vec![0x42]);
    let r = assemble_pasmo_files(src, "main.asm", &odd).expect("verbatim name resolves");
    assert_eq!(r.bytes, vec![0x42]);
}

/// pasmo silently accepts an empty asset (probe p7, exit 0, no output) —
/// ours emits nothing; the advisory warning is asm198x's own (diagnostics may
/// exceed the reference, KTD5).
#[test]
fn pasmo_incbin_empty_file_emits_nothing() {
    let loader = MemoryLoader::new().binary("empty.bin", Vec::new());
    let src = "        org $8000\n        db $aa\n        incbin \"empty.bin\"\n        db $bb\n";
    let r = assemble_pasmo_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0xAA, 0xBB]);
}

// --- U3: search order (FsLoader) and the CLI end-to-end ---

/// An incbin inside an included file resolves against *that file's*
/// directory — the same search machinery as includes (probes t17/t18, KTD8).
#[test]
fn incbin_resolves_against_the_requesting_files_directory() {
    let dir = temp_tree("u3-incbin-order");
    let sub = dir.join("art");
    std::fs::create_dir_all(&sub).expect("create art/");
    std::fs::write(sub.join("sprite.bin"), [0xC1, 0xC2]).expect("write asset");
    std::fs::write(sub.join("art.inc"), "        incbin \"sprite.bin\"\n").expect("write include");
    let src = "        org $8000\n        include \"art/art.inc\"\n";
    let root = dir.join("main.asm");
    std::fs::write(&root, src).expect("write main");

    let loader = FsLoader::new(&dir, Vec::new());
    let r = assemble_sjasmplus_files(src, &root.to_string_lossy(), &loader)
        .expect("the include's own dir resolves its asset");
    assert_eq!(r.bytes, vec![0xC1, 0xC2]);

    // The root does NOT see the subdir-local asset without -I (probe t18).
    let miss = "        org $8000\n        incbin \"sprite.bin\"\n";
    assert!(
        assemble_sjasmplus_files(miss, &root.to_string_lossy(), &loader).is_err(),
        "no fallback into a subdirectory"
    );
}

/// End-to-end through the binary: a sjasmplus incbin with an offset tail
/// assembles to the probe-pinned bytes.
#[test]
fn cli_assembles_an_incbin() {
    let dir = temp_tree("u3-cli-incbin");
    let main = dir.join("main.asm");
    std::fs::write(
        &main,
        "        org $8000\n        db $aa\n        incbin \"data.bin\",2,3\n",
    )
    .expect("write main");
    std::fs::write(dir.join("data.bin"), asset()).expect("write asset");
    let out = dir.join("main.bin");
    let run = bin()
        .args(["--dialect", "sjasmplus"])
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "assembles: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(&out).expect("output written"),
        vec![0xAA, 0x12, 0x13, 0x14]
    );
}

// ===========================================================================
// U4 — ACME: `!src`/`!source` and `!bin`/`!binary` resolve inside the
// evaluation walk (hermetic, MemoryLoader). Every expected byte sequence and
// error posture below is pinned by the acme 0.97 probe runs in the U4 report
// (KTD5).
// ===========================================================================

/// AE1's mechanism for acme: a two-file program assembles byte-identical to
/// its flattened equivalent, and the result's file table lists both files in
/// `FileId` order (KTD2).
#[test]
fn acme_include_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("defs.a", "border = $d020\n");
    let src = "* = $1000\n        !src \"defs.a\"\n        sta border\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    let flat = assemble_acme("* = $1000\nborder = $d020\n        sta border\n")
        .expect("flattened equivalent assembles");
    assert_eq!(r.bytes, flat.bytes, "include is transparent to the bytes");
    assert_eq!(
        r.files,
        vec!["main.a".to_string(), "defs.a".to_string()],
        "the file table survives into the result (KTD2)"
    );
}

/// Three-deep nesting: main → a → b, code at every level, in include order;
/// `!source` is the long alias and the spellings are case-insensitive
/// (probe-pinned).
#[test]
fn acme_include_three_deep_nests() {
    let loader = MemoryLoader::new()
        .text(
            "a.a",
            "        lda #2\n        !SRC \"b.a\"\n        lda #4\n",
        )
        .text("b.a", "        lda #3\n");
    let src = "* = $1000\n        lda #1\n        !source \"a.a\"\n        lda #5\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x01, 0xA9, 0x02, 0xA9, 0x03, 0xA9, 0x04, 0xA9, 0x05],
        "bytes interleave in include order"
    );
    assert_eq!(r.files, vec!["main.a", "a.a", "b.a"]);
}

/// KTD1's driver on the 6502: symbols defined inside the include feed the
/// includer's *later* zero-page vs absolute selection — acme picks the
/// addressing mode by value knowledge (probe-pinned: a5 10 / 8d 00 04).
#[test]
fn acme_include_defined_symbols_feed_later_zp_abs_selection() {
    let loader = MemoryLoader::new().text("defs.a", "ptr = $10\naddr = $0400\n");
    let src = "* = $1000\n        !src \"defs.a\"\n        lda ptr\n        sta addr\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA5, 0x10, 0x8D, 0x00, 0x04],
        "zp for ptr, absolute for addr (KTD1)"
    );
}

/// The environment flows *out* of the include: a constant defined inside it
/// drives the includer's later conditional (probe-pinned: a9 ff).
#[test]
fn acme_include_defined_symbol_drives_a_later_conditional() {
    let loader = MemoryLoader::new().text("cfg.a", "DEBUG = 1\n");
    let src = "* = $1000\n        !src \"cfg.a\"\n\
               !if DEBUG = 1 {\n        lda #$ff\n} else {\n        lda #$00\n}\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0xA9, 0xFF], "env threads back out (KTD1)");
}

/// KTD1's proof, testable on ACME today (it has conditionals): a `!src` in an
/// **untaken** branch never loads — the target may not even exist — while the
/// taken branch does load it.
#[test]
fn acme_conditional_guarded_include_loads_only_when_taken() {
    let src = "* = $1000\n\
               !ifdef DEMO {\n        !src \"demo.a\"\n}\n        lda #3\n";
    // Untaken: `demo.a` is not registered anywhere; the walk must not ask for it.
    let untaken = assemble_acme_files(src, "main.a", &MemoryLoader::new())
        .expect("the untaken branch never loads (probe-pinned: acme assembles)");
    assert_eq!(untaken.bytes, vec![0xA9, 0x03]);
    assert_eq!(
        untaken.files,
        vec!["main.a".to_string()],
        "no FileId was minted for the guarded include"
    );

    // Taken: the same source with DEMO defined loads and splices the file.
    let taken_src = format!("DEMO = 1\n{src}");
    let loader = MemoryLoader::new().text("demo.a", "        lda #2\n");
    let taken = assemble_acme_files(&taken_src, "main.a", &loader).expect("taken branch loads");
    assert_eq!(taken.bytes, vec![0xA9, 0x02, 0xA9, 0x03]);
    assert_eq!(taken.files, vec!["main.a", "demo.a"]);
}

/// Anonymous `-`/`+` labels resolve in **spliced evaluation order** across
/// the `!src` boundary, both directions (probe-pinned bytes): the include
/// references the includer's `-`, and the includer's forward `jmp +` lands on
/// the `+` defined inside the include.
#[test]
fn acme_anons_resolve_across_the_include_boundary() {
    let loader = MemoryLoader::new().text("part.a", "+       lda #2\n        beq -\n");
    let src = "* = $1000\n\
               -       lda #1\n\
               \x20       jmp +\n\
               \x20       !src \"part.a\"\n\
               \x20       bne -\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![
            0xA9, 0x01, 0x4C, 0x05, 0x10, 0xA9, 0x02, 0xF0, 0xF7, 0xD0, 0xF5
        ],
        "probe-pinned bytes (c1)"
    );
}

/// A label on the `!src` line binds at the include point (probe-pinned:
/// `here !src …` then `!word here` = 00 10 at origin $1000).
#[test]
fn acme_label_on_the_src_line_binds_at_the_include_point() {
    let loader = MemoryLoader::new().text("body.a", "        lda #7\n");
    let src = "* = $1000\nhere    !src \"body.a\"\n        !word here\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0xA9, 0x07, 0x00, 0x10]);
    assert_eq!(r.symbols.get("here"), Some(&0x1000));
}

/// `!bin "file"[, [size][, [skip]]]` — acme's argument order is size *then*
/// skip, and every window case is probe-pinned: plain, size-only, size+skip,
/// an empty size slot, and the zero-padding postures (size past the data,
/// skip past EOF) that acme pads rather than rejects.
#[test]
fn acme_bin_size_skip_windows_match_the_probes() {
    let loader = || MemoryLoader::new().binary("data.bin", asset());
    let cases: &[(&str, Vec<u8>)] = &[
        // b1: the whole asset.
        ("!bin \"data.bin\"", asset()),
        // b2: size 3 = the first three bytes.
        ("!bin \"data.bin\", 3", vec![0x10, 0x11, 0x12]),
        // b3: size 3, skip 2 — skip first, then take size.
        ("!bin \"data.bin\", 3, 2", vec![0x12, 0x13, 0x14]),
        // b4: size past the data pads with zeroes (never an error).
        ("!bin \"data.bin\", 12", {
            let mut v = asset();
            v.extend([0, 0, 0, 0]);
            v
        }),
        // b6: an empty size slot = skip 2, read to EOF.
        (
            "!bin \"data.bin\", , 2",
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        // b7: skip past EOF with a size = pure zero padding.
        ("!bin \"data.bin\", 2, 20", vec![0x00, 0x00]),
        // b8: skip past EOF without a size = nothing.
        ("!bin \"data.bin\", , 20", vec![]),
        // b9: a negative skip reads from the start.
        ("!bin \"data.bin\", 2, -1", vec![0x10, 0x11]),
        // b10/b17-style: size and skip take constant expressions.
        (
            "SZ = 2\n!bin \"data.bin\", SZ+1, SZ",
            vec![0x12, 0x13, 0x14],
        ),
        // `!binary` is the long alias (b18).
        ("!binary \"data.bin\", 2", vec![0x10, 0x11]),
    ];
    for (line, want) in cases {
        let src = format!("* = $1000\n{line}\n");
        let r = assemble_acme_files(&src, "main.a", &loader()).expect(line);
        assert_eq!(&r.bytes, want, "probe-pinned bytes for {line}");
    }
}

/// A negative `!bin` size is acme's error posture (probe b13, `Negative size
/// argument`), at the directive's span.
#[test]
fn acme_bin_negative_size_is_an_error() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "* = $1000\n        !bin \"data.bin\", -2\n";
    let e = assemble_acme_files(src, "main.a", &loader).expect_err("negative size");
    assert!(
        e.error.message.contains("negative"),
        "names the problem: {}",
        e.error.message
    );
    assert_eq!(e.error.line, 2, "at the directive's line");
}

/// acme requires quotes on the file name (probe b15) and rejects extra
/// arguments (`!src` takes one file, c8; `!bin` at most size and skip, b14).
#[test]
fn acme_malformed_directive_arguments_are_rejected() {
    let loader = MemoryLoader::new()
        .text("body.a", "        lda #7\n")
        .binary("data.bin", asset());
    for (src, needle) in [
        ("* = $1000\n        !src body.a\n", "quoted"),
        ("* = $1000\n        !bin data.bin\n", "quoted"),
        ("* = $1000\n        !src \"body.a\", 2\n", "one file name"),
        ("* = $1000\n        !bin \"data.bin\", 2, 1, 9\n", "at most"),
    ] {
        let e = assemble_acme_files(src, "main.a", &loader).expect_err(src);
        assert!(
            e.error.message.contains(needle),
            "`{src}` names the problem: {}",
            e.error.message
        );
    }
}

/// A label on the `!bin` line binds at the payload's start (probe b16).
#[test]
fn acme_label_on_the_bin_line_binds_at_the_payload() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = "* = $1000\nart     !bin \"data.bin\", 2\n        !word art\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x10, 0x11, 0x00, 0x10]);
    assert_eq!(r.symbols.get("art"), Some(&0x1000));
}

// ===========================================================================
// U7 — ACME `!zone` local-label scoping × the multi-file model. Every byte
// sequence and error posture is pinned by the acme 0.97 probe runs in the U7
// report (probes z1-z20, zh1-zh9, za-zg).
// ===========================================================================

/// AE3 + C3: a `.local` reused across two `!zone`s assembles byte-identical
/// to the flattened reference bytes, and the two locals are two **distinct
/// qualified symbols** in `AssemblyResult.symbols` (KTD4 — qualified-name
/// keys, no new shape).
#[test]
fn acme_zone_local_reuse_yields_two_distinct_qualified_symbols() {
    let src = "* = $1000\n\
               !zone one\n\
               .loop   lda #1\n\
               \x20       bne .loop\n\
               !zone two\n\
               .loop   lda #2\n\
               \x20       bne .loop\n";
    let r = assemble_acme_files(src, "main.a", &MemoryLoader::new()).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x01, 0xD0, 0xFC, 0xA9, 0x02, 0xD0, 0xFC],
        "probe z1 bytes"
    );
    assert_eq!(r.symbols.get("one@1.loop"), Some(&0x1000));
    assert_eq!(r.symbols.get("two@2.loop"), Some(&0x1004));
    assert!(
        !r.symbols.contains_key(".loop"),
        "no unqualified collision key"
    );
}

/// Zone state threads through `!src` like the rest of the environment
/// (probes za/zb/zc): the include inherits the includer's zone (its `beq .x`
/// sees the includer's `.x`), a `!zone` inside the include persists after
/// return (the includer's later `.y` reference resolves in the include's
/// zone), and the includer's pre-include `.x` is then out of scope (zb2).
#[test]
fn acme_zone_state_threads_through_the_include_boundary() {
    let loader = MemoryLoader::new().text("part.a", "        beq .x\n!zone inc\n.y      lda #2\n");
    let src = "* = $1000\n\
               !zone one\n\
               .x      lda #1\n\
               \x20       !src \"part.a\"\n\
               \x20       bne .y\n";
    let r = assemble_acme_files(src, "main.a", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x01, 0xF0, 0xFC, 0xA9, 0x02, 0xD0, 0xFC],
        "inherit + persist (probes za/zb)"
    );

    // zb2: after the include's `!zone`, the includer's `.x` is out of scope.
    let out_of_scope = "* = $1000\n\
                        !zone one\n\
                        .x      lda #1\n\
                        \x20       !src \"part.a\"\n\
                        \x20       bne .x\n";
    let e = assemble_acme_files(out_of_scope, "main.a", &loader).expect_err("out of scope");
    assert!(
        e.error.message.contains("undefined"),
        "probe zb2 posture: {}",
        e.error.message
    );

    // zg: a duplicate `.local` via an include (same zone) errors, naming the
    // *included* file.
    let dup_loader = MemoryLoader::new().text("dup.a", ".x      lda #2\n");
    let dup = "* = $1000\n!zone one\n.x      lda #1\n        !src \"dup.a\"\n";
    let e = assemble_acme_files(dup, "main.a", &dup_loader).expect_err("duplicate");
    let span = e.error.span.as_ref().expect("carries a span");
    assert_eq!(
        e.source_map
            .file_table()
            .get(span.file.0 as usize)
            .map(String::as_str),
        Some("dup.a"),
        "the duplicate is reported in the included file"
    );
}

/// A `.local` before any `!zone` lives in the initial zone under its bare
/// key (probe z4), so zone-free programs keep today's public symbol keys.
#[test]
fn acme_local_before_any_zone_keeps_the_bare_key() {
    let src = "* = $1000\n.early  lda #1\n        bne .early\n";
    let r = assemble_acme_files(src, "main.a", &MemoryLoader::new()).expect("assembles");
    assert_eq!(r.bytes, vec![0xA9, 0x01, 0xD0, 0xFC]);
    assert_eq!(r.symbols.get(".early"), Some(&0x1000));
}

/// An error inside an included file names *that* file and line, and the
/// include graph yields the chain back to the root (KTD2 failure path).
#[test]
fn acme_error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.a", "        !src \"b.a\"\n")
        .text("b.a", "        lda #1\n        frob $10\n");
    let src = "* = $1000\n        !src \"a.a\"\n";
    let e = assemble_acme_files(src, "main.a", &loader).expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.a, not of main.a");
    let table = e.source_map.file_table();
    assert_eq!(
        table.get(span.file.0 as usize).map(String::as_str),
        Some("b.a"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.a".to_string(), 1), ("main.a".to_string(), 2)],
        "the include chain walks back to the root"
    );
}

/// A missing `!src` target is a diagnostic at the directive's span — the
/// operand's column — not a CLI read error; a missing `!bin` asset likewise.
#[test]
fn acme_missing_targets_report_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let src = "* = $1000\n        !src \"nothere.a\"\n";
    let e = assemble_acme_files(src, "main.a", &loader).expect_err("missing target");
    assert!(
        e.error.message.contains("nothere.a"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 2);
    assert_eq!(span.col, 14, "points at the operand (the file name)");

    let bin = "* = $1000\n        !bin \"nothere.bin\"\n";
    let e = assemble_acme_files(bin, "main.a", &loader).expect_err("missing asset");
    assert!(
        e.error.message.contains("nothere.bin"),
        "names the request: {}",
        e.error.message
    );
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(2));
}

/// A self-include is a cycle diagnostic listing the chain (diagnostics may
/// exceed the reference's depth-overflow posture, KTD5).
#[test]
fn acme_self_include_reports_the_cycle() {
    let src = "* = $1000\n        !src \"main.a\"\n";
    let loader = MemoryLoader::new().text("main.a", src);
    let e = assemble_acme_files(src, "main.a", &loader).expect_err("cycle");
    assert!(
        e.error.message.contains("include cycle"),
        "names the cycle: {}",
        e.error.message
    );
    assert!(
        e.error.message.contains("main.a -> main.a"),
        "lists the chain: {}",
        e.error.message
    );
}

/// The single-source entry points still mean "one file": a `!src` or `!bin`
/// there is a clear pointer to the multi-file entry, not a silent skip.
#[test]
fn acme_single_source_entry_rejects_src_and_bin_with_a_pointer() {
    for (src, directive) in [
        ("* = $1000\n        !src \"defs.a\"\n", "!src"),
        ("* = $1000\n        !bin \"data.bin\"\n", "!bin"),
    ] {
        let e = assemble_acme(src).expect_err("no loader here");
        assert!(
            e.message.contains(directive) && e.message.contains("multi-file"),
            "points at the multi-file entry: {}",
            e.message
        );
    }
}

/// End-to-end through the binary: an acme `!src` resolves via `-I` from a
/// directory other than the input's own, and `--prg` still packages the
/// result (the new acme multi-file CLI wiring).
#[test]
fn cli_assembles_an_acme_include_via_a_search_dir() {
    let srcdir = temp_tree("u4-acme-cli-src");
    let incdir = temp_tree("u4-acme-cli-inc");
    let main = srcdir.join("main.a");
    std::fs::write(
        &main,
        "* = $1000\n        !src \"defs.a\"\n        lda #VAL\n",
    )
    .expect("write main");
    std::fs::write(incdir.join("defs.a"), "VAL = $2b\n").expect("write include");
    let out = srcdir.join("main.bin");
    let run = bin()
        .args(["--dialect", "acme", "-I"])
        .arg(&incdir)
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "assembles: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(&out).expect("output written"),
        vec![0xA9, 0x2B]
    );
}

/// A failure inside an acme include renders rustc-style with the included
/// file's name plus an `included from` note naming the includer and line.
#[test]
fn cli_acme_error_in_include_carries_an_included_from_note() {
    let dir = temp_tree("u4-acme-cli-note");
    let main = dir.join("main.a");
    std::fs::write(&main, "* = $1000\n        !src \"bad.a\"\n").expect("write main");
    std::fs::write(dir.join("bad.a"), "        frob $10\n").expect("write include");
    let run = bin()
        .args(["--dialect", "acme"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success(), "frob fails the assemble");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("bad.a:1"),
        "names the included file and line: {stderr}"
    );
    assert!(
        stderr.contains("included from") && stderr.contains("main.a:2"),
        "carries the included-from note: {stderr}"
    );
}

/// End-to-end through the binary for pasmo: the plain form, resolving via
/// the input's own directory (the new pasmo multi-file CLI wiring).
#[test]
fn cli_assembles_a_pasmo_incbin() {
    let dir = temp_tree("u3-cli-pasmo-incbin");
    let main = dir.join("main.asm");
    std::fs::write(&main, "        org $8000\n        incbin \"data.bin\"\n").expect("write main");
    std::fs::write(dir.join("data.bin"), asset()).expect("write asset");
    let out = dir.join("main.bin");
    let run = bin()
        .args(["--dialect", "pasmo"])
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "assembles: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(std::fs::read(&out).expect("output written"), asset());
}

// --- U4: the ca65-syntax flat family (65816, HuC6280) — `.include`/`.incbin`.
// Every expected byte sequence and error posture below is pinned by the ca65
// V2.18 probe runs recorded in the U4 report (the flat816.cfg link recipe),
// not an assumption.

/// A ca65-65816 include is transparent to the bytes, and the file table
/// survives into the result (KTD2).
#[test]
fn ca65_816_include_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("defs.s", "ptr = $10\n");
    let src = " lda #$11\n .include \"defs.s\"\n lda ptr\n";
    let r = assemble_ca65_816_files(src, "main.s", &loader).expect("assembles");
    let flat = assemble_ca65_816("ptr = $10\n lda #$11\n lda ptr\n").expect("flat assembles");
    assert_eq!(r.bytes, flat.bytes, "include is transparent to the bytes");
    assert_eq!(r.bytes, vec![0xA9, 0x11, 0xA5, 0x10], "probe-pinned bytes");
    assert_eq!(r.files, vec!["main.s".to_string(), "defs.s".to_string()]);
}

/// Three-deep nesting, code at every level, in include order; `.INCLUDE` is
/// case-insensitive like every ca65 dot-keyword.
#[test]
fn ca65_816_include_three_deep_nests() {
    let loader = MemoryLoader::new()
        .text("a.s", " lda #$02\n .INCLUDE \"b.s\"\n lda #$04\n")
        .text("b.s", " lda #$03\n");
    let src = " lda #$01\n .include \"a.s\"\n lda #$05\n";
    let r = assemble_ca65_816_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x01, 0xA9, 0x02, 0xA9, 0x03, 0xA9, 0x04, 0xA9, 0x05],
        "bytes interleave in include order"
    );
    assert_eq!(r.files, vec!["main.s", "a.s", "b.s"]);
}

/// KTD1's driver on the 65816: a `.a16`/`.i16` width flip **inside** an
/// include sizes the includer's *later* immediates, and an include-defined
/// constant feeds later zp/abs selection — both probe-pinned
/// (a9 11 / a9 12 00 / a9 34 00, then a5 10).
#[test]
fn ca65_816_width_and_symbols_thread_out_of_the_include() {
    let loader = MemoryLoader::new().text("wide.s", ".a16\n lda #$12\nptr = $10\n");
    let src = " lda #$11\n .include \"wide.s\"\n lda #$34\n .a8\n lda ptr\n";
    let r = assemble_ca65_816_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x11, 0xA9, 0x12, 0x00, 0xA9, 0x34, 0x00, 0xA5, 0x10],
        "the include's .a16 widens the includer's later immediate (probe-pinned)"
    );
}

/// An error inside a nested include names *that* file and line, and the
/// include chain walks back to the root (the shared mechanism, spot-checked
/// on this family).
#[test]
fn ca65_816_error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.s", " .include \"b.s\"\n")
        .text("b.s", " lda #$01\n frob $10\n");
    let src = " .include \"a.s\"\n";
    let e = assemble_ca65_816_files(src, "main.s", &loader).expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.s, not of main.s");
    let table = e.source_map.file_table();
    assert_eq!(
        table.get(span.file.0 as usize).map(String::as_str),
        Some("b.s"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.s".to_string(), 1), ("main.s".to_string(), 1)],
        "the include chain walks back to the root"
    );
}

/// A missing `.include` target and a missing `.incbin` asset are diagnostics
/// at the directive's span (the operand's column), not CLI read errors.
#[test]
fn ca65_816_missing_targets_report_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let src = " lda #$01\n .include \"nothere.s\"\n";
    let e = assemble_ca65_816_files(src, "main.s", &loader).expect_err("missing target");
    assert!(
        e.error.message.contains("nothere.s"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 2);
    assert_eq!(span.col, 11, "points at the operand (the file name)");
    assert_eq!(
        e.source_map.file_table().get(span.file.0 as usize),
        Some(&"main.s".to_string())
    );

    let src = " .incbin \"nothere.bin\"\n";
    let e = assemble_ca65_816_files(src, "main.s", &loader).expect_err("missing asset");
    assert!(
        e.error.message.contains("nothere.bin"),
        "names the request: {}",
        e.error.message
    );
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(1));
}

/// `.incbin "file"[, offset[, size]]` — the probe-pinned window matrix:
/// plain, offset, offset+size, expression arguments, offset at EOF (empty),
/// size 0 (empty), and ca65's negative-size-reads-to-EOF sentinel.
#[test]
fn ca65_816_incbin_windows_match_the_probes() {
    let loader = || MemoryLoader::new().binary("data.bin", asset());
    let cases: &[(&str, Vec<u8>)] = &[
        (" .incbin \"data.bin\"", asset()),
        (
            " .incbin \"data.bin\", 2",
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        (" .incbin \"data.bin\", 2, 3", vec![0x12, 0x13, 0x14]),
        // Constant expressions fold against the live environment.
        (
            "OFF = 2\n .incbin \"data.bin\", OFF, OFF+1",
            vec![0x12, 0x13, 0x14],
        ),
        // Offset at EOF and size 0 are legal and empty (probe-pinned).
        (" .incbin \"data.bin\", 8", vec![]),
        (" .incbin \"data.bin\", 0, 0", vec![]),
        // ca65 reads ANY negative size as "the rest of the file" (probe-pinned:
        // `, 2, -2` on the 8-byte asset emitted all 6 remaining bytes).
        (
            " .incbin \"data.bin\", 2, -2",
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        (" .incbin \"data.bin\", 6, -9", vec![0x16, 0x17]),
    ];
    for (src, expect) in cases {
        let full = format!("{src}\n .byte $bb\n");
        let r = assemble_ca65_816_files(&full, "main.s", &loader())
            .unwrap_or_else(|e| panic!("`{src}` assembles: {e}"));
        let mut want = expect.clone();
        want.push(0xBB);
        assert_eq!(r.bytes, want, "window for `{src}`");
    }
}

/// The probe-pinned `.incbin` error postures: offset past EOF, size past the
/// remaining bytes, a negative offset (ca65: "Range error" / a read error —
/// ours name the numbers), and a forward-referenced argument (ca65:
/// "Constant expression expected"). All at the directive's span.
#[test]
fn ca65_816_incbin_window_errors_match_the_probes() {
    let loader = || MemoryLoader::new().binary("data.bin", asset());
    for (src, what) in [
        (" .incbin \"data.bin\", 9", "offset past EOF"),
        (
            " .incbin \"data.bin\", 6, 4",
            "size past the remaining bytes",
        ),
        (" .incbin \"data.bin\", -2", "negative offset"),
        (
            " .incbin \"data.bin\", 0, SZ\nSZ = 3",
            "forward-referenced size",
        ),
    ] {
        assert!(
            assemble_ca65_816_files(src, "main.s", &loader()).is_err(),
            "`{src}` must fail ({what})"
        );
    }
    // The spans: a window error points at the directive's line.
    let e = assemble_ca65_816_files(" lda #$01\n .incbin \"data.bin\", 9\n", "main.s", &loader())
        .expect_err("offset past EOF");
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(2));
    assert!(
        e.error.message.contains("data.bin"),
        "names the asset: {}",
        e.error.message
    );
}

/// Labels on the `.include`/`.incbin` lines bind at the include point / the
/// payload start (probe-pinned: `here: .include …` then `.word here`).
#[test]
fn ca65_816_labels_on_directive_lines_bind_at_the_point() {
    let loader = MemoryLoader::new()
        .text("body.s", " lda #$07\n")
        .binary("data.bin", asset());
    let src = ".org $1000\nhere: .include \"body.s\"\nart: .incbin \"data.bin\", 2, 2\n .word here\n .word art\n";
    let r = assemble_ca65_816_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x07, 0x12, 0x13, 0x00, 0x10, 0x02, 0x10],
        "here = $1000 (include point), art = $1002 (payload start)"
    );
    assert_eq!(r.symbols.get("here"), Some(&0x1000));
    assert_eq!(r.symbols.get("art"), Some(&0x1002));
}

/// A self-include is a cycle diagnostic listing the chain — ca65 itself has
/// no cycle detection (a self-include dies on the OS open-file limit), so
/// this diagnostic exceeds the reference (KTD5: diagnostics are not
/// byte-compared).
#[test]
fn ca65_816_self_include_reports_the_cycle() {
    let src = " .include \"main.s\"\n";
    let loader = MemoryLoader::new().text("main.s", src);
    let e = assemble_ca65_816_files(src, "main.s", &loader).expect_err("cycle");
    assert!(
        e.error.message.contains("include cycle"),
        "names the cycle: {}",
        e.error.message
    );
    assert!(
        e.error.message.contains("main.s -> main.s"),
        "lists the chain: {}",
        e.error.message
    );
}

/// The single-source entry points still mean "one file": a `.include` or
/// `.incbin` there is a clear pointer to the multi-file entry, not the old
/// `unsupported directive` rejection or a silent skip.
#[test]
fn ca65_816_single_source_entry_rejects_both_directives_with_a_pointer() {
    for src in [" .include \"defs.s\"\n", " .incbin \"data.bin\"\n"] {
        let e = assemble_ca65_816(src).expect_err("no loader here");
        assert!(
            e.message.contains("multi-file"),
            "points at the multi-file entry: {}",
            e.message
        );
    }
}

/// ca65's probe-pinned resolution order walks the **include chain's
/// directories**, innermost → outermost: a nested include's request resolves
/// against its own directory first, then its includer's — never the bare
/// process working directory. (ca65 V2.18: `sub/inc2.s` beat the root-dir
/// copy; with it absent, the root-dir copy was found from inside `sub/`.)
#[test]
fn ca65_flat_include_resolution_walks_the_ancestor_chain() {
    let root = temp_tree("u4-ca65-chain");
    std::fs::create_dir_all(root.join("sub")).expect("create sub/");
    let main = root.join("main.s");
    std::fs::write(&main, " lda #$01\n .include \"sub/mid.s\"\n").expect("write main");
    std::fs::write(
        root.join("sub/mid.s"),
        " lda #$02\n .include \"shared.s\"\n",
    )
    .expect("write mid");
    // Scenario 1: `shared.s` only in the ROOT's directory — found from inside
    // sub/ via the ancestor hop.
    std::fs::remove_file(root.join("sub/shared.s")).ok();
    std::fs::write(root.join("shared.s"), " lda #$03\n").expect("write root shared");
    let source = std::fs::read_to_string(&main).expect("read main");
    let loader = FsLoader::new(&root, Vec::new());
    let r = assemble_ca65_816_files(&source, &main.to_string_lossy(), &loader)
        .expect("resolves via the includer chain");
    assert_eq!(r.bytes, vec![0xA9, 0x01, 0xA9, 0x02, 0xA9, 0x03]);
    assert_eq!(r.files.len(), 3, "root + mid + the chain-resolved shared");

    // Scenario 2: a copy next to the requester (sub/) wins over the root's.
    std::fs::write(root.join("sub/shared.s"), " lda #$04\n").expect("write sub shared");
    let r = assemble_ca65_816_files(&source, &main.to_string_lossy(), &loader)
        .expect("resolves via the requester's own directory");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x01, 0xA9, 0x02, 0xA9, 0x04],
        "the requesting file's directory wins (probe-pinned)"
    );
}

// --- U4: the HuC6280 leg of the family (the shared walk, spot-checked). ---

/// A nested HuC6280 include with an include-defined constant feeding later
/// zp selection, plus the incbin window — probe-pinned bytes
/// (a9 11 / a9 22 / a5 10 / 12 13 14).
#[test]
fn ca65_huc6280_include_and_incbin_match_the_probes() {
    let loader = MemoryLoader::new()
        .text("defs.s", "ptr = $10\n lda #$22\n")
        .binary("data.bin", asset());
    let src = " lda #$11\n .include \"defs.s\"\n lda ptr\n .incbin \"data.bin\", 2, 3\n";
    let r = assemble_ca65_huc6280_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0xA9, 0x11, 0xA9, 0x22, 0xA5, 0x10, 0x12, 0x13, 0x14],
        "probe-pinned (ca65 --cpu huc6280)"
    );
    assert_eq!(r.files, vec!["main.s", "defs.s"]);
}

/// HuC6280-specific opcodes assemble inside an include, and ca65's
/// negative-size sentinel reads to EOF (probe-pinned: 22 16 17).
#[test]
fn ca65_huc6280_extension_ops_and_negative_size_in_include() {
    let loader = MemoryLoader::new()
        .text("outer.s", " .include \"inner.s\"\n")
        .text("inner.s", " sax\n")
        .binary("data.bin", asset());
    let src = " .include \"outer.s\"\n .incbin \"data.bin\", 6, -9\n";
    let r = assemble_ca65_huc6280_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x22, 0x16, 0x17], "probe-pinned");
    assert_eq!(r.files, vec!["main.s", "outer.s", "inner.s"]);
}

/// The HuC6280 leg's error postures: missing targets at the directive span;
/// the single-source entries keep rejecting with the multi-file pointer.
#[test]
fn ca65_huc6280_errors_and_single_source_rejection() {
    let e = assemble_ca65_huc6280_files(" .include \"nope.s\"\n", "main.s", &MemoryLoader::new())
        .expect_err("missing target");
    assert!(e.error.message.contains("nope.s"));
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(1));

    for src in [" .include \"defs.s\"\n", " .incbin \"data.bin\"\n"] {
        let e = assemble_ca65_huc6280(src).expect_err("no loader here");
        assert!(
            e.message.contains("multi-file"),
            "points at the multi-file entry: {}",
            e.message
        );
    }
}

/// A label on the HuC6280 `.incbin` line binds at the payload start.
#[test]
fn ca65_huc6280_label_on_the_incbin_line_binds_at_the_payload() {
    let loader = MemoryLoader::new().binary("data.bin", asset());
    let src = ".org $2000\nart: .incbin \"data.bin\", 0, 2\n .word art\n";
    let r = assemble_ca65_huc6280_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x10, 0x11, 0x00, 0x20]);
    assert_eq!(r.symbols.get("art"), Some(&0x2000));
}

/// End-to-end through the binary: a ca65-65816 `.include` resolves via `-I`
/// from a directory other than the input's own (the new CLI wiring).
#[test]
fn cli_assembles_a_ca65_816_include_via_a_search_dir() {
    let srcdir = temp_tree("u4-816-cli-src");
    let incdir = temp_tree("u4-816-cli-inc");
    let main = srcdir.join("main.s");
    std::fs::write(&main, " .include \"defs.s\"\n lda #VAL\n").expect("write main");
    std::fs::write(incdir.join("defs.s"), "VAL = $2b\n").expect("write include");
    let out = srcdir.join("main.bin");
    let run = bin()
        .args(["--cpu", "65816", "-I"])
        .arg(&incdir)
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "assembles: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(&out).expect("output written"),
        vec![0xA9, 0x2B]
    );
}

/// A failure inside a HuC6280 include renders rustc-style with the included
/// file's name plus an `included from` note (the human CLI path).
#[test]
fn cli_ca65_huc6280_error_in_include_carries_an_included_from_note() {
    let dir = temp_tree("u4-huc-cli-note");
    let main = dir.join("main.s");
    std::fs::write(&main, " lda #$01\n .include \"bad.s\"\n").expect("write main");
    std::fs::write(dir.join("bad.s"), " frob $10\n").expect("write include");
    let run = bin()
        .args(["--cpu", "huc6280"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success(), "frob fails the assemble");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("bad.s:1"),
        "names the included file and line: {stderr}"
    );
    assert!(
        stderr.contains("included from") && stderr.contains("main.s:2"),
        "carries the included-from note: {stderr}"
    );
}

// ===========================================================================
// U4 — rgbasm (SM83): `INCLUDE`/`INCBIN` through the shared walk driver with
// rgbasm's probe-pinned semantics (rgbasm v1.0.1 + rgblink probe runs in the
// U4 report): root-anchored resolution (rgbasm searches the process cwd and
// never the including file's directory — our input's directory stands in),
// `DEF` constants and the `.local` scope threading through the boundary, and
// the no-negatives INCBIN window.
// ===========================================================================

/// An rgbasm include is transparent to the bytes, and the file table survives
/// into the result (KTD2). `include` is case-insensitive like every rgbasm
/// keyword.
#[test]
fn rgbasm_include_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("defs.inc", "DEF VAL EQU $42\n ld c, 3\n");
    let src = "SECTION \"c\", ROM0[$0]\n ld a, 1\n include \"defs.inc\"\n ld a, VAL\n ld b, 2\n";
    let r = assemble_rgbasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x01, 0x0E, 0x03, 0x3E, 0x42, 0x06, 0x02],
        "probe-pinned (rgbasm/rgblink): the include splices at its point and \
         its DEF flows out to the includer"
    );
    assert_eq!(
        r.files,
        vec!["main.asm".to_string(), "defs.inc".to_string()]
    );
}

/// Three-deep nesting, code at every level, in include order (probe-pinned).
#[test]
fn rgbasm_include_three_deep_nests() {
    let loader = MemoryLoader::new()
        .text("a.inc", " ld b, 2\n INCLUDE \"b.inc\"\n ld d, 4\n")
        .text("b.inc", " ld c, 3\n");
    let src = "SECTION \"c\", ROM0[$0]\n ld a, 1\n INCLUDE \"a.inc\"\n ld e, 5\n";
    let r = assemble_rgbasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x01, 0x06, 0x02, 0x0E, 0x03, 0x16, 0x04, 0x1E, 0x05],
        "bytes interleave in include order (probe-pinned)"
    );
    assert_eq!(r.files, vec!["main.asm", "a.inc", "b.inc"]);
}

/// KTD1's driver on rgbasm: `DEF` constants defined **inside** the include
/// feed the includer's *later* opcode-embedded operands (`bit`, `rst`) and a
/// `ds` count — all parse-time consumers (probe-pinned:
/// 3e 01 / 0e 03 / cb 6f / df / 00 00 00 / 06 02).
#[test]
fn rgbasm_def_constants_feed_later_includer_lines() {
    let loader = MemoryLoader::new().text(
        "defs.inc",
        "DEF BITNUM EQU 5\nDEF RSTVEC EQU $18\nDEF PAD EQU 3\n ld c, 3\n",
    );
    let src = "SECTION \"c\", ROM0[$0]\n ld a, 1\n INCLUDE \"defs.inc\"\n bit BITNUM, a\n \
               rst RSTVEC\n ds PAD\n ld b, 2\n";
    let r = assemble_rgbasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![
            0x3E, 0x01, 0x0E, 0x03, 0xCB, 0x6F, 0xDF, 0x00, 0x00, 0x00, 0x06, 0x02
        ],
        "include-defined DEF constants drive bit/rst/ds on later includer lines"
    );
}

/// `.local` labels scope across the include boundary, both directions
/// (probe-pinned: a `.local` at the top of the include scopes under the
/// includer's current global, and the includer's scope continues after the
/// include — bytes 00 00 18 fd 18 fa).
#[test]
fn rgbasm_locals_scope_across_the_include_boundary() {
    let loader = MemoryLoader::new().text("loc.inc", ".inloc:\n nop\n jr .inloc\n");
    let src = "SECTION \"c\", ROM0[$0]\nstart:\n.here:\n nop\n INCLUDE \"loc.inc\"\n jr .here\n";
    let r = assemble_rgbasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x00, 0x00, 0x18, 0xFD, 0x18, 0xFA],
        "probe-pinned (rgbasm/rgblink)"
    );
    assert_eq!(
        r.symbols.get("start.inloc"),
        Some(&0x0001),
        "the include's local qualifies under the includer's global"
    );
}

/// A global defined *inside* the include becomes the current global for the
/// includer's subsequent locals — probe-pinned by reusing one `.local` name
/// in both scopes: rgbasm accepts the duplicate because they are distinct
/// symbols (`start.tail` vs `mid.tail`).
#[test]
fn rgbasm_global_defined_in_include_rescopes_later_includer_locals() {
    let loader = MemoryLoader::new().text("glob.inc", "mid:\n nop\n");
    let src = "SECTION \"c\", ROM0[$0]\nstart:\n.tail:\n nop\n INCLUDE \"glob.inc\"\n.tail:\n \
               nop\n jr .tail\n";
    let r = assemble_rgbasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x00, 0x00, 0x00, 0x18, 0xFD],
        "probe-pinned (rgbasm/rgblink)"
    );
    assert_eq!(r.symbols.get("start.tail"), Some(&0x0000));
    assert_eq!(
        r.symbols.get("mid.tail"),
        Some(&0x0002),
        "the includer's post-include local scopes under the include's global"
    );
}

/// An error inside a nested include names *that* file and line, and the
/// include chain walks back to the root (the shared mechanism, spot-checked
/// on rgbasm).
#[test]
fn rgbasm_error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.inc", " INCLUDE \"b.inc\"\n")
        .text("b.inc", " ld a, 1\n frob $10\n");
    let src = "SECTION \"c\", ROM0[$0]\n INCLUDE \"a.inc\"\n";
    let e = assemble_rgbasm_files(src, "main.asm", &loader).expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.inc, not of main.asm");
    let table = e.source_map.file_table();
    assert_eq!(
        table.get(span.file.0 as usize).map(String::as_str),
        Some("b.inc"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.inc".to_string(), 1), ("main.asm".to_string(), 2)],
        "the include chain walks back to the root"
    );
}

/// A missing `INCLUDE` target and a missing `INCBIN` asset are diagnostics at
/// the directive's span (the operand's column), not CLI read errors.
#[test]
fn rgbasm_missing_targets_report_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let src = "SECTION \"c\", ROM0[$0]\n ld a, 1\n INCLUDE \"nothere.inc\"\n";
    let e = assemble_rgbasm_files(src, "main.asm", &loader).expect_err("missing target");
    assert!(
        e.error.message.contains("nothere.inc"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 3);
    assert_eq!(span.col, 10, "points at the operand (the file name)");

    let src = "SECTION \"c\", ROM0[$0]\n INCBIN \"nothere.bin\"\n";
    let e = assemble_rgbasm_files(src, "main.asm", &loader).expect_err("missing asset");
    assert!(
        e.error.message.contains("nothere.bin"),
        "names the request: {}",
        e.error.message
    );
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(2));
}

/// `INCBIN "file"[, offset[, length]]` — the probe-pinned window matrix:
/// plain, offset, offset+length, `DEF`-constant expression arguments, offset
/// at EOF (empty), and length 0 (empty). rgbasm has no negative sentinel —
/// negatives are errors (the error matrix below).
#[test]
fn rgbasm_incbin_windows_match_the_probes() {
    let loader = || MemoryLoader::new().binary("data.bin", asset());
    let cases: &[(&str, Vec<u8>)] = &[
        (" INCBIN \"data.bin\"", asset()),
        (
            " INCBIN \"data.bin\", 2",
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        (" INCBIN \"data.bin\", 2, 3", vec![0x12, 0x13, 0x14]),
        // Constant expressions fold against the live environment.
        (
            "DEF OFF EQU 2\n INCBIN \"data.bin\", OFF, OFF+1",
            vec![0x12, 0x13, 0x14],
        ),
        // Offset at EOF and length 0 are legal and empty (probe-pinned).
        (" INCBIN \"data.bin\", 8", vec![]),
        (" INCBIN \"data.bin\", 0, 0", vec![]),
    ];
    for (src, expect) in cases {
        let full = format!("SECTION \"c\", ROM0[$0]\n{src}\n db $bb\n");
        let r = assemble_rgbasm_files(&full, "main.asm", &loader())
            .unwrap_or_else(|e| panic!("`{src}` assembles: {e}"));
        let mut want = expect.clone();
        want.push(0xBB);
        assert_eq!(r.bytes, want, "window for `{src}`");
    }
}

/// The probe-pinned `INCBIN` error postures: offset past EOF ("start
/// position is greater than length"), length past the remaining bytes ("out
/// of bounds"), negative offset/length ("Constant must not be negative" —
/// rgbasm has no from-the-end sentinel), and a forward-referenced argument
/// ("Expected constant expression"). All at the directive's span.
#[test]
fn rgbasm_incbin_window_errors_match_the_probes() {
    let loader = || MemoryLoader::new().binary("data.bin", asset());
    for (src, what) in [
        (" INCBIN \"data.bin\", 9", "offset past EOF"),
        (
            " INCBIN \"data.bin\", 2, 7",
            "length past the remaining bytes",
        ),
        (" INCBIN \"data.bin\", -2", "negative offset"),
        (" INCBIN \"data.bin\", 2, -2", "negative length"),
        (
            " INCBIN \"data.bin\", 0, SZ\nDEF SZ EQU 3",
            "forward-referenced length",
        ),
    ] {
        let full = format!("SECTION \"c\", ROM0[$0]\n{src}\n");
        assert!(
            assemble_rgbasm_files(&full, "main.asm", &loader()).is_err(),
            "`{src}` must fail ({what})"
        );
    }
    // The spans: a window error points at the directive's line.
    let e = assemble_rgbasm_files(
        "SECTION \"c\", ROM0[$0]\n ld a, 1\n INCBIN \"data.bin\", 9\n",
        "main.asm",
        &loader(),
    )
    .expect_err("offset past EOF");
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(3));
    assert!(
        e.error.message.contains("data.bin"),
        "names the asset: {}",
        e.error.message
    );
}

/// Labels on the `INCLUDE`/`INCBIN` lines bind at the include point / the
/// payload start (probe-pinned: `here: INCLUDE …` then `dw here`).
#[test]
fn rgbasm_labels_on_directive_lines_bind_at_the_point() {
    let loader = MemoryLoader::new()
        .text("body.inc", " ld a, 7\n")
        .binary("data.bin", asset());
    let src = "SECTION \"c\", ROM0[$0]\nhere: INCLUDE \"body.inc\"\nart: INCBIN \"data.bin\", 2, 2\n \
               dw here\n dw art\n";
    let r = assemble_rgbasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x07, 0x12, 0x13, 0x00, 0x00, 0x02, 0x00],
        "here = $0000 (include point), art = $0002 (payload start) — probe-pinned"
    );
    assert_eq!(r.symbols.get("here"), Some(&0x0000));
    assert_eq!(r.symbols.get("art"), Some(&0x0002));
}

/// A self-include is a cycle diagnostic listing the chain — rgbasm itself
/// stops at its recursion limit of 64 (probe-pinned), so this exact
/// diagnostic exceeds the reference (KTD5: diagnostics are not
/// byte-compared).
#[test]
fn rgbasm_self_include_reports_the_cycle() {
    let src = "SECTION \"c\", ROM0[$0]\n INCLUDE \"main.asm\"\n";
    let loader = MemoryLoader::new().text("main.asm", src);
    let e = assemble_rgbasm_files(src, "main.asm", &loader).expect_err("cycle");
    assert!(
        e.error.message.contains("include cycle"),
        "names the cycle: {}",
        e.error.message
    );
    assert!(
        e.error.message.contains("main.asm -> main.asm"),
        "lists the chain: {}",
        e.error.message
    );
}

/// The single-source entry points still mean "one file": an `INCLUDE` or
/// `INCBIN` there is a clear pointer to the multi-file entry, not an
/// `unknown instruction` rejection or a silent skip.
#[test]
fn rgbasm_single_source_entry_rejects_both_directives_with_a_pointer() {
    for src in [
        "SECTION \"c\", ROM0[$0]\n INCLUDE \"defs.inc\"\n",
        "SECTION \"c\", ROM0[$0]\n INCBIN \"data.bin\"\n",
    ] {
        let e = assemble_rgbasm(src).expect_err("no loader here");
        assert!(
            e.message.contains("multi-file"),
            "points at the multi-file entry: {}",
            e.message
        );
    }
}

/// rgbasm's probe-pinned resolution anchor is the **root input's directory**
/// for every request, however deep the requester (rgbasm v1.0.1 anchors at
/// the process cwd, never the including file's directory; our input's
/// directory stands in for the cwd) — the opposite of lwasm's
/// requester-first rule. A copy next to the requester is *not* consulted.
#[test]
fn rgbasm_include_resolution_anchors_at_the_root() {
    let root = temp_tree("u4-rgbasm-anchor");
    std::fs::create_dir_all(root.join("sub")).expect("create sub/");
    let main = root.join("main.asm");
    std::fs::write(
        &main,
        "SECTION \"c\", ROM0[$0]\n ld a, 1\n INCLUDE \"sub/mid.inc\"\n",
    )
    .expect("write main");
    std::fs::write(
        root.join("sub/mid.inc"),
        " ld b, 2\n INCLUDE \"leaf.inc\"\n",
    )
    .expect("write mid");
    // The root-dir copy resolves — even though the requester lives in sub/ —
    // and a different copy next to the requester is ignored (probe-pinned).
    std::fs::write(root.join("leaf.inc"), " ld c, 3\n").expect("write root leaf");
    std::fs::write(root.join("sub/leaf.inc"), " ld e, 5\n").expect("write sub leaf");
    let source = std::fs::read_to_string(&main).expect("read main");
    let loader = FsLoader::new(&root, Vec::new());
    let r = assemble_rgbasm_files(&source, &main.to_string_lossy(), &loader)
        .expect("resolves at the root anchor");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x01, 0x06, 0x02, 0x0E, 0x03],
        "the ROOT directory's copy wins (probe-pinned: rgbasm never searches \
         the including file's directory)"
    );

    // With the root copy gone, the requester-adjacent copy is still not
    // found: the request falls through to the `-I` dirs and fails without
    // one (probe-pinned).
    std::fs::remove_file(root.join("leaf.inc")).expect("remove root leaf");
    let e = assemble_rgbasm_files(&source, &main.to_string_lossy(), &loader)
        .expect_err("no root copy, no -I: unresolved");
    assert!(
        e.error.message.contains("leaf.inc"),
        "names the request: {}",
        e.error.message
    );
}

/// The CLI assembles an rgbasm include resolved through a `-I` search dir,
/// and `INCBIN` through the same loader (the human path wiring).
#[test]
fn cli_assembles_an_rgbasm_include_via_a_search_dir() {
    let dir = temp_tree("u4-rgbasm-cli");
    let libdir = dir.join("lib");
    std::fs::create_dir_all(&libdir).expect("create lib/");
    let main = dir.join("main.asm");
    std::fs::write(
        &main,
        "SECTION \"c\", ROM0[$0]\n ld a, 1\n INCLUDE \"defs.inc\"\n INCBIN \"art.bin\", 1, 2\n",
    )
    .expect("write main");
    std::fs::write(libdir.join("defs.inc"), " ld c, 3\n").expect("write include");
    std::fs::write(libdir.join("art.bin"), [0xDE, 0xAD, 0xBE, 0xEF]).expect("write asset");
    let out = dir.join("out.bin");
    let run = bin()
        .args(["--cpu", "rgbasm", "-I"])
        .arg(&libdir)
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(&out).expect("read output"),
        vec![0x3E, 0x01, 0x0E, 0x03, 0xAD, 0xBE],
        "include and incbin both resolve through the -I dir"
    );
}

// ===========================================================================
// U4 — lwasm (6809): `include`/`use`/`includebin` through the shared walk
// driver with lwasm's probe-pinned semantics (lwasm 4.24 probe runs in the
// U4 report): requester-directory resolution (then `-I`; no cwd, no root
// fallback), quoted OR bare file names, and the negative-offset-from-EOF
// includebin window.
// ===========================================================================

/// An lwasm include is transparent to the bytes — in the `include` and `use`
/// spellings, quoted and bare — and the file table survives into the result
/// (KTD2). Probe-pinned: all four spellings assemble identically.
#[test]
fn lwasm_include_spellings_match_the_flattened_source() {
    for directive in [
        "include \"body.inc\"",
        "include body.inc",
        "use \"body.inc\"",
        "use body.inc",
        "INCLUDE \"body.inc\"",
    ] {
        let loader = MemoryLoader::new().text("body.inc", "        lda #2\n");
        let src = format!("        {directive}\n        lda #1\n");
        let r = assemble_lwasm_files(&src, "main.asm", &loader)
            .unwrap_or_else(|e| panic!("`{directive}` assembles: {e}"));
        assert_eq!(
            r.bytes,
            vec![0x86, 0x02, 0x86, 0x01],
            "probe-pinned (lwasm --6809 --raw) for `{directive}`"
        );
        assert_eq!(
            r.files,
            vec!["main.asm".to_string(), "body.inc".to_string()]
        );
    }
}

/// Three-deep nesting, code at every level, in include order (probe-pinned:
/// 86 01 / 86 02 / 86 03 / 86 04 / 86 05).
#[test]
fn lwasm_include_three_deep_nests() {
    let loader = MemoryLoader::new()
        .text(
            "a.inc",
            "        lda #2\n        include \"b.inc\"\n        lda #4\n",
        )
        .text("b.inc", "        lda #3\n");
    let src = "        lda #1\n        include \"a.inc\"\n        lda #5\n";
    let r = assemble_lwasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x86, 0x01, 0x86, 0x02, 0x86, 0x03, 0x86, 0x04, 0x86, 0x05]
    );
    assert_eq!(r.files, vec!["main.asm", "a.inc", "b.inc"]);
}

/// KTD1's driver on lwasm: an `equ` defined **inside** the include feeds the
/// includer's *later* direct-vs-extended selection (probe-pinned: `lda ptr`
/// with an include-defined `ptr equ $20` emits the DIRECT form 96 20, not
/// extended B6 00 20).
#[test]
fn lwasm_equ_in_include_feeds_direct_extended_selection() {
    let loader = MemoryLoader::new().text("defs.inc", "ptr     equ $20\n");
    let src = "        include \"defs.inc\"\n        lda ptr\n";
    let r = assemble_lwasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x96, 0x20],
        "the include-defined equ selects the direct form (probe-pinned)"
    );
}

/// An error inside a nested include names *that* file and line, and the
/// include chain walks back to the root (the shared mechanism, spot-checked
/// on lwasm).
#[test]
fn lwasm_error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.inc", "        include \"b.inc\"\n")
        .text("b.inc", "        lda #1\n        frob $10\n");
    let src = "        include \"a.inc\"\n";
    let e = assemble_lwasm_files(src, "main.asm", &loader).expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.inc, not of main.asm");
    let table = e.source_map.file_table();
    assert_eq!(
        table.get(span.file.0 as usize).map(String::as_str),
        Some("b.inc"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.inc".to_string(), 1), ("main.asm".to_string(), 1)],
        "the include chain walks back to the root"
    );
}

/// A missing `include` target and a missing `includebin` asset are
/// diagnostics at the directive's span (the operand's column), not CLI read
/// errors.
#[test]
fn lwasm_missing_targets_report_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let src = "        lda #1\n        include \"nothere.inc\"\n";
    let e = assemble_lwasm_files(src, "main.asm", &loader).expect_err("missing target");
    assert!(
        e.error.message.contains("nothere.inc"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 2);
    assert_eq!(span.col, 17, "points at the operand (the file name)");

    let src = "        includebin \"nothere.bin\"\n";
    let e = assemble_lwasm_files(src, "main.asm", &loader).expect_err("missing asset");
    assert!(
        e.error.message.contains("nothere.bin"),
        "names the request: {}",
        e.error.message
    );
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(1));
}

/// `includebin "file"[,offset[,length]]` — the probe-pinned window matrix:
/// plain (quoted and bare), offset, offset+length, equ-fed expression
/// arguments, offset at EOF (empty), length 0 (empty), and lwasm's
/// **negative offset counts back from EOF** (`-4,2` = two bytes from
/// position len-4; `-8` = the whole file).
#[test]
fn lwasm_includebin_windows_match_the_probes() {
    let loader = || MemoryLoader::new().binary("data.bin", asset());
    let cases: &[(&str, Vec<u8>)] = &[
        ("        includebin \"data.bin\"", asset()),
        ("        includebin data.bin", asset()),
        (
            "        includebin \"data.bin\",2",
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        (
            "        includebin \"data.bin\",2,3",
            vec![0x12, 0x13, 0x14],
        ),
        ("        includebin data.bin,2,3", vec![0x12, 0x13, 0x14]),
        (
            "OFF     equ 2\n        includebin \"data.bin\",OFF,OFF+1",
            vec![0x12, 0x13, 0x14],
        ),
        // Offset at EOF and length 0 are legal and empty (probe-pinned).
        ("        includebin \"data.bin\",8", vec![]),
        ("        includebin \"data.bin\",2,0", vec![]),
        // Negative offsets count back from EOF (probe-pinned).
        ("        includebin \"data.bin\",-4,2", vec![0x14, 0x15]),
        ("        includebin \"data.bin\",-2", vec![0x16, 0x17]),
        ("        includebin \"data.bin\",-8", asset()),
    ];
    for (src, expect) in cases {
        let full = format!("{src}\n        fcb $bb\n");
        let r = assemble_lwasm_files(&full, "main.asm", &loader())
            .unwrap_or_else(|e| panic!("`{src}` assembles: {e}"));
        let mut want = expect.clone();
        want.push(0xBB);
        assert_eq!(r.bytes, want, "window for `{src}`");
    }
}

/// The probe-pinned `includebin` error postures: offset past EOF or before
/// the start ("Start value out of range"), a length past the remaining bytes
/// or negative ("Length value out of range"), and a forward-referenced
/// argument (lwasm misfolds it to an out-of-range 0; ours is a
/// constant-expression diagnostic — diagnostics are not byte-compared,
/// KTD5). All at the directive's span.
#[test]
fn lwasm_includebin_window_errors_match_the_probes() {
    let loader = || MemoryLoader::new().binary("data.bin", asset());
    for (src, what) in [
        ("        includebin \"data.bin\",9", "offset past EOF"),
        (
            "        includebin \"data.bin\",-9",
            "offset before the start",
        ),
        (
            "        includebin \"data.bin\",2,7",
            "length past the remaining bytes",
        ),
        (
            "        includebin \"data.bin\",-2,3",
            "length past EOF after a negative offset",
        ),
        ("        includebin \"data.bin\",2,-3", "negative length"),
        (
            "        includebin \"data.bin\",0,SZ\nSZ      equ 3",
            "forward-referenced length",
        ),
    ] {
        assert!(
            assemble_lwasm_files(src, "main.asm", &loader()).is_err(),
            "`{src}` must fail ({what})"
        );
    }
    // The spans: a window error points at the directive's line.
    let e = assemble_lwasm_files(
        "        lda #1\n        includebin \"data.bin\",9\n",
        "main.asm",
        &loader(),
    )
    .expect_err("offset past EOF");
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(2));
    assert!(
        e.error.message.contains("data.bin"),
        "names the asset: {}",
        e.error.message
    );
}

/// Labels on the `include`/`includebin` lines bind at the include point /
/// the payload start (probe-pinned: `here include …` then `fdb here` — fdb
/// is big-endian).
#[test]
fn lwasm_labels_on_directive_lines_bind_at_the_point() {
    let loader = MemoryLoader::new()
        .text("body.inc", "        lda #7\n")
        .binary("data.bin", asset());
    let src = "        org $1000\nhere    include \"body.inc\"\nart     includebin \"data.bin\",2,2\n        fdb here\n        fdb art\n";
    let r = assemble_lwasm_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x86, 0x07, 0x12, 0x13, 0x10, 0x00, 0x10, 0x02],
        "here = $1000 (include point), art = $1002 (payload start)"
    );
    assert_eq!(r.symbols.get("here"), Some(&0x1000));
    assert_eq!(r.symbols.get("art"), Some(&0x1002));
}

/// A self-include is a cycle diagnostic listing the chain — lwasm itself has
/// no cycle detection (a self-include dies on the OS's open-file limit,
/// probe-pinned), so this diagnostic exceeds the reference (KTD5).
#[test]
fn lwasm_self_include_reports_the_cycle() {
    let src = "        include \"main.asm\"\n";
    let loader = MemoryLoader::new().text("main.asm", src);
    let e = assemble_lwasm_files(src, "main.asm", &loader).expect_err("cycle");
    assert!(
        e.error.message.contains("include cycle"),
        "names the cycle: {}",
        e.error.message
    );
    assert!(
        e.error.message.contains("main.asm -> main.asm"),
        "lists the chain: {}",
        e.error.message
    );
}

/// The single-source entry points still mean "one file": an `include`,
/// `use`, or `includebin` there is a clear pointer to the multi-file entry.
#[test]
fn lwasm_single_source_entry_rejects_directives_with_a_pointer() {
    for src in [
        "        include \"defs.inc\"\n",
        "        use defs.inc\n",
        "        includebin \"data.bin\"\n",
    ] {
        let e = assemble_lwasm(src).expect_err("no loader here");
        assert!(
            e.message.contains("multi-file"),
            "points at the multi-file entry: {}",
            e.message
        );
    }
}

/// lwasm's probe-pinned resolution order: the **requesting file's own
/// directory**, then the `-I` dirs — and *only* those. A root-directory copy
/// is NOT found from inside a subdirectory include (lwasm 4.24 probe-pinned;
/// the opposite of ca65's ancestor chain and rgbasm's root anchor).
#[test]
fn lwasm_include_resolution_is_requester_dir_then_search() {
    let root = temp_tree("u4-lwasm-order");
    std::fs::create_dir_all(root.join("sub")).expect("create sub/");
    let main = root.join("main.asm");
    std::fs::write(
        &main,
        "        lda #1\n        include \"sub/mid.inc\"\n        lda #5\n",
    )
    .expect("write main");
    std::fs::write(
        root.join("sub/mid.inc"),
        "        lda #2\n        include \"leaf.inc\"\n        lda #4\n",
    )
    .expect("write mid");
    // Scenario 1: the requester-adjacent copy resolves (and wins over -I).
    std::fs::write(root.join("sub/leaf.inc"), "        lda #3\n").expect("write sub leaf");
    let source = std::fs::read_to_string(&main).expect("read main");
    let loader = FsLoader::new(&root, Vec::new());
    let r = assemble_lwasm_files(&source, &main.to_string_lossy(), &loader)
        .expect("resolves via the requester's directory");
    assert_eq!(
        r.bytes,
        vec![0x86, 0x01, 0x86, 0x02, 0x86, 0x03, 0x86, 0x04, 0x86, 0x05]
    );

    // Scenario 2: a root-directory copy is NOT found from inside sub/
    // (probe-pinned: lwasm has no root/cwd fallback).
    std::fs::remove_file(root.join("sub/leaf.inc")).expect("remove sub leaf");
    std::fs::write(root.join("leaf.inc"), "        lda #9\n").expect("write root leaf");
    let e = assemble_lwasm_files(&source, &main.to_string_lossy(), &loader)
        .expect_err("the root-dir copy must not resolve");
    assert!(
        e.error.message.contains("leaf.inc"),
        "names the request: {}",
        e.error.message
    );

    // Scenario 3: a `-I` dir resolves it.
    let libdir = root.join("lib");
    std::fs::create_dir_all(&libdir).expect("create lib/");
    std::fs::write(libdir.join("leaf.inc"), "        lda #3\n").expect("write lib leaf");
    let loader = FsLoader::new(&root, vec![libdir]);
    let r =
        assemble_lwasm_files(&source, &main.to_string_lossy(), &loader).expect("resolves via -I");
    assert_eq!(
        r.bytes,
        vec![0x86, 0x01, 0x86, 0x02, 0x86, 0x03, 0x86, 0x04, 0x86, 0x05]
    );
}

/// A failure inside an lwasm include renders rustc-style with the included
/// file's name plus an `included from` note (the human CLI path wiring).
#[test]
fn cli_lwasm_error_in_include_carries_an_included_from_note() {
    let dir = temp_tree("u4-lwasm-cli-note");
    let main = dir.join("main.asm");
    std::fs::write(&main, "        lda #1\n        include \"bad.inc\"\n").expect("write main");
    std::fs::write(dir.join("bad.inc"), "        frob $10\n").expect("write include");
    let run = bin()
        .args(["--cpu", "6809"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success(), "frob fails the assemble");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("bad.inc:1"),
        "names the included file and line: {stderr}"
    );
    assert!(
        stderr.contains("included from") && stderr.contains("main.asm:2"),
        "carries the included-from note: {stderr}"
    );
}

// ===========================================================================
// U4 — the asl-syntax chips (8080 representative + CP1610 decle accounting +
// one smoke per remaining chip). asl's multi-file surface is uniform across
// chips (probe-pinned on the 8080, spot-checked on the TMS9900 and CP1610),
// so the deep coverage rides the family's debut chip and the rest prove
// their wiring. Every expected byte sequence below is pinned by the U4d
// probe runs against asl 1.42 + p2bin.
// ===========================================================================

/// Nested includes on the 8080: main -> a -> sub, code at every level, in
/// include order, with both the quoted and the bare-name spellings.
#[test]
fn i8080_include_three_deep_nests_in_both_spellings() {
    let loader = MemoryLoader::new()
        .text(
            "a.inc",
            "        mvi b,2\n        include sub.inc\n        mvi d,4\n",
        )
        .text("sub.inc", "        mvi c,3\n");
    let src = "        mvi a,1\n        include \"a.inc\"\n        mvi e,5\n";
    let r = assemble_i8080_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x01, 0x06, 0x02, 0x0E, 0x03, 0x16, 0x04, 0x1E, 0x05],
        "bytes interleave in include order"
    );
    assert_eq!(r.files, vec!["main.asm", "a.inc", "sub.inc"]);
}

/// KTD1's driver on the 8080: an `equ` defined inside the include feeds an
/// opcode-embedded operand (`rst`'s vector) and a `ds` count on the
/// includer's *later* lines. Probe-pinned: asl emits 3E 42 / DF / three zero
/// bytes.
#[test]
fn i8080_include_defined_equ_feeds_later_includer_lines() {
    let loader = MemoryLoader::new().text("defs.inc", "CONST equ 42h\nRSTVEC equ 3\nPAD equ 3\n");
    let src =
        "        include \"defs.inc\"\n        mvi a,CONST\n        rst RSTVEC\n        ds PAD\n";
    let r = assemble_i8080_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x42, 0xDF, 0x00, 0x00, 0x00],
        "include-defined constants flow out to later form selection (KTD1)"
    );
}

/// asl's probe-pinned extension default: an extensionless `include` request
/// tries `name.inc` first and the exact spelling second.
#[test]
fn i8080_extensionless_include_tries_inc_then_the_exact_name() {
    // `include defs` with both spellings registered: `.inc` wins.
    let loader = MemoryLoader::new()
        .text("defs.inc", "        mvi a,11h\n")
        .text("defs", "        mvi a,99h\n");
    let src = "        include defs\n";
    let r = assemble_i8080_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x3E, 0x11], "`defs.inc` beats `defs`");

    // Only the exact name registered: the fallback finds it.
    let loader = MemoryLoader::new().text("bare", "        mvi a,77h\n");
    let r = assemble_i8080_files("        include bare\n", "main.asm", &loader)
        .expect("assembles via the exact-name fallback");
    assert_eq!(r.bytes, vec![0x3E, 0x77]);
}

/// An error inside an included file names that file and line, with the
/// include chain walking back to the root (KTD2's failure path).
#[test]
fn i8080_error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.inc", "        include \"b.inc\"\n")
        .text("b.inc", "        nop\n        frob\n");
    let src = "        include \"a.inc\"\n";
    let e = assemble_i8080_files(src, "main.asm", &loader).expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.inc, not of main.asm");
    let table = e.source_map.file_table();
    assert_eq!(
        table.get(span.file.0 as usize).map(String::as_str),
        Some("b.inc"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.inc".to_string(), 1), ("main.asm".to_string(), 1)],
        "the include chain walks back to the root"
    );
}

/// Missing targets — an include and a binclude — report at the directive's
/// span (the operand's column), not as a CLI read error.
#[test]
fn i8080_missing_targets_report_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let e = assemble_i8080_files("        include \"nothere.inc\"\n", "main.asm", &loader)
        .expect_err("missing include target");
    assert!(
        e.error.message.contains("nothere.inc"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!((span.line, span.col), (1, 17), "points at the file name");

    let e = assemble_i8080_files("        binclude \"nothere.bin\"\n", "main.asm", &loader)
        .expect_err("missing binclude asset");
    assert!(
        e.error.message.contains("nothere.bin"),
        "names the request: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 1);
}

/// The probe asset (`10..17`), matching the U4d probe runs — the same shared
/// 8-byte asset [`asset`] builds.
fn asl_asset() -> Vec<u8> {
    asset()
}

/// `binclude`'s legal windows, all probe-pinned: plain, offset, offset+length
/// (as equ-fed expressions), offset at EOF, and length 0 — the empty windows
/// assemble cleanly (with the engine's advisory warning).
#[test]
fn i8080_binclude_windows_match_the_probes() {
    let loader = MemoryLoader::new().binary("data.bin", asl_asset());
    let src = "OFF equ 2\n        db 0aah\n        binclude \"data.bin\"\n        binclude \"data.bin\",2\n        binclude data.bin,OFF,3\n        binclude \"data.bin\",8\n        binclude \"data.bin\",0,0\n        db 0bbh\n";
    let r = assemble_i8080_files(src, "main.asm", &loader).expect("assembles");
    let mut expected = vec![0xAA];
    expected.extend(asl_asset()); // plain
    expected.extend(&asl_asset()[2..]); // offset 2
    expected.extend(&asl_asset()[2..5]); // offset 2 length 3 (equ-fed)
    expected.push(0xBB); // offset-at-EOF and length-0 emit nothing
    assert_eq!(r.bytes, expected);
}

/// `binclude`'s error postures, all probe-pinned strict (asl has no negative
/// sentinels): negative offset, negative length, offset past EOF, and length
/// past the remaining bytes — each at the directive's span.
#[test]
fn i8080_binclude_window_errors_match_the_probes() {
    let loader = MemoryLoader::new().binary("data.bin", asl_asset());
    for (args, what) in [
        (",-2", "negative offset"),
        (",2,-2", "negative length"),
        (",9", "offset past EOF"),
        (",2,7", "length past the remaining bytes"),
    ] {
        let src = format!("        binclude \"data.bin\"{args}\n");
        let e = match assemble_i8080_files(&src, "main.asm", &loader) {
            Ok(_) => panic!("{what} must be an error"),
            Err(e) => e,
        };
        assert!(
            e.error.message.contains("data.bin"),
            "{what} names the asset: {}",
            e.error.message
        );
        let span = e.error.span.as_ref().expect("span at the directive");
        assert_eq!(span.line, 1, "{what} points at the directive line");
    }
}

/// Labels on the `include` and `binclude` lines bind at the include point /
/// payload start (probe-pinned: `here:` reads the include's first byte
/// address, `art:` the payload's).
#[test]
fn i8080_labels_on_directive_lines_bind_at_the_point() {
    let loader = MemoryLoader::new()
        .text("body.inc", "        mvi a,7\n")
        .binary("data.bin", asl_asset());
    let src = "here:   include \"body.inc\"\nart:    binclude \"data.bin\",2,2\n        lxi h,here\n        lxi h,art\n";
    let r = assemble_i8080_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x3E, 0x07, 0x12, 0x13, 0x21, 0x00, 0x00, 0x21, 0x02, 0x00],
        "here = $0000 (the include point), art = $0002 (the payload start)"
    );
}

/// A self-include is a cycle diagnostic listing the chain (asl itself dies on
/// the OS's open-file limit — diagnostics may exceed the reference, KTD5).
#[test]
fn i8080_self_include_reports_the_cycle() {
    let src = "        include \"main.asm\"\n";
    let loader = MemoryLoader::new().text("main.asm", src);
    let e = assemble_i8080_files(src, "main.asm", &loader).expect_err("cycle");
    assert!(
        e.error.message.contains("include cycle"),
        "names the cycle: {}",
        e.error.message
    );
}

/// The single-source entry points stay single-source: both directives are
/// rejected with a pointer to the multi-file API, not a bogus parse error.
#[test]
fn i8080_single_source_entry_rejects_directives_with_a_pointer() {
    let e = assemble_i8080("        include \"defs.inc\"\n").expect_err("include rejected");
    assert!(
        e.message.contains("multi-file"),
        "points at the multi-file entry: {}",
        e.message
    );
    let e = assemble_i8080("        binclude \"data.bin\"\n").expect_err("binclude rejected");
    assert!(
        e.message.contains("multi-file"),
        "points at the multi-file entry: {}",
        e.message
    );
}

// --- CP1610: the decle accounting (the plan's named assumption, resolved) ---

/// The CP1610's probe-pinned `binclude` accounting: offset/length count
/// bytes, and an N-byte window occupies N **decles** — the image carries the
/// N raw file bytes followed by N zero bytes, exactly as asl (`cpu CP-1600`)
/// with p2bin lays it down. An odd byte count is legal: 3 bytes -> 3 decles,
/// and a label after the payload lands at decle N.
#[test]
fn cp1610_binclude_counts_bytes_and_occupies_one_decle_per_byte() {
    let loader = MemoryLoader::new().binary("odd3.bin", vec![0x10, 0x11, 0x12]);
    let src = "        org 0\n        binclude \"odd3.bin\"\nafter:  word after\n";
    let r = assemble_cp1610_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x10, 0x11, 0x12, 0x00, 0x00, 0x00, 0x00, 0x03],
        "3 bytes + 3 zeros = 3 decles; `after` = decle 3 (probe-pinned)"
    );

    // A window slices in bytes first, then tails: `,2,3` -> 12 13 14 + 3 zeros.
    let loader = MemoryLoader::new().binary("data.bin", asl_asset());
    let src = "        org 0\n        binclude \"data.bin\",2,3\nafter:  word after\n";
    let r = assemble_cp1610_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x12, 0x13, 0x14, 0x00, 0x00, 0x00, 0x00, 0x03],
        "the window counts bytes; the tail pads to whole decles"
    );
}

// --- One wiring smoke per remaining asl chip: a two-file assemble through
// the new entry matches the flattened source (hermetic, one assert each). ---

#[test]
fn m6800_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        ldaa #$42\n");
    let r = assemble_m6800_files(
        "        include \"a.inc\"\n        ldaa $42\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_m6800("        ldaa #$42\n        ldaa $42\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn cdp1802_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        inc 3\n");
    let r = assemble_1802_files(
        "        include \"a.inc\"\n        ldn 7\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_1802("        inc 3\n        ldn 7\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn i8048_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        add a,r0\n");
    let src = "        include \"a.inc\"\n        add a,@r1\n";
    let r = assemble_8048_files(src, "main.asm", &loader).expect("assembles");
    let flat = assemble_8048("        add a,r0\n        add a,@r1\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
    // The ROM-less kin shares the walker; its entry wires the same way.
    let r = assemble_8039_files(src, "main.asm", &loader).expect("assembles (8039)");
    let flat = assemble_8039("        add a,r0\n        add a,@r1\n").expect("flat (8039)");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn scmp_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        xpal 1\n");
    let r = assemble_scmp_files(
        "        include \"a.inc\"\n        nop\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_scmp("        xpal 1\n        nop\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn f8_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        lr a,3\n");
    let r = assemble_f8_files(
        "        include \"a.inc\"\n        lr a,ku\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_f8("        lr a,3\n        lr a,ku\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn s2650_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        lodi,r0 $42\n");
    let r = assemble_2650_files(
        "        include \"a.inc\"\n        lodz r1\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_2650("        lodi,r0 $42\n        lodz r1\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn tms7000_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        mov %42h,a\n");
    let r = assemble_tms7000_files(
        "        include \"a.inc\"\n        mov r5,a\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_tms7000("        mov %42h,a\n        mov r5,a\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn pdp11_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        mov #0x1234, r0\n");
    let r = assemble_pdp11_files(
        "        include \"a.inc\"\n        mov (r2)+, -(r3)\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_pdp11("        mov #0x1234, r0\n        mov (r2)+, -(r3)\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn tms9900_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        li r0, 0abcdh\n");
    let r = assemble_tms9900_files(
        "        include \"a.inc\"\n        mov r1, r2\n",
        "main.asm",
        &loader,
    )
    .expect("assembles");
    let flat = assemble_tms9900("        li r0, 0abcdh\n        mov r1, r2\n").expect("flat");
    assert_eq!(r.bytes, flat.bytes);
}

#[test]
fn z8000_include_smoke_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("a.inc", "        add r1, #1234h\n");
    let src = "        include \"a.inc\"\n        ld r1, r2\n";
    let flat_src = "        add r1, #1234h\n        ld r1, r2\n";
    let r = assemble_z8000_files(src, "main.asm", &loader).expect("assembles");
    let flat = assemble_z8000(flat_src).expect("flat");
    assert_eq!(r.bytes, flat.bytes);
    // The segmented Z8001 entry threads the same walker with `seg` set.
    let r = assemble_z8001_files(src, "main.asm", &loader).expect("assembles (z8001)");
    let flat = assemble_z8001(flat_src).expect("flat (z8001)");
    assert_eq!(r.bytes, flat.bytes);
}

// ===========================================================================
// U5: the ca65-NES assemble+link path — `.include`/`.incbin` through the
// `Item::Native` pipeline (its own parse + two-pass + link). Semantics are
// the flat ca65 family's CA65_SEMANTICS, re-probed under the ca65+ld65 NES
// link (they held: resolution and incbin windows are assembler-side); the
// segment/anon/cheap-local threading bytes below are pinned by those probe
// runs (ca65 V2.18 + ld65, the curriculum's nes.cfg).
// ===========================================================================

/// The PRG slice of a linked `.nes` ROM (after the 16-byte iNES header).
fn prg(rom: &[u8]) -> &[u8] {
    &rom[16..16 + 0x8000]
}

/// The CHR slice of a linked `.nes` ROM.
fn chr(rom: &[u8]) -> &[u8] {
    &rom[16 + 0x8000..]
}

/// A NES program split across `.include`d files — PRG code and CHARS data in
/// separate files — links identically to the flattened source, with a
/// ZEROPAGE symbol defined in the root feeding zp selection inside the
/// include and an include-defined constant feeding the root's later lines
/// (KTD1, both directions).
#[test]
fn ca65_nes_program_split_across_includes() {
    let loader = MemoryLoader::new()
        .text("prg.s", "        sta pos\nSPEED = 3\nloop:   jmp loop\n")
        .text("chars.s", ".segment \"CHARS\"\n        .byte $AA, $BB\n");
    let src = "\
.segment \"HEADER\"\n\
        .byte \"NES\", $1A, 2, 1\n\
.segment \"ZEROPAGE\"\n\
pos:    .res 1\n\
.segment \"CODE\"\n\
reset:  lda #SPEED\n\
.include \"prg.s\"\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n\
.include \"chars.s\"\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("links");
    let flat = assemble_ca65(
        &src.replace(
            ".include \"prg.s\"",
            "        sta pos\nSPEED = 3\nloop:   jmp loop",
        )
        .replace(
            ".include \"chars.s\"",
            ".segment \"CHARS\"\n        .byte $AA, $BB",
        ),
    )
    .expect("flat links");
    assert_eq!(r.bytes, flat.bytes, "includes are transparent to the ROM");
    assert_eq!(r.bytes.len(), 16 + 0x8000 + 0x2000, "iNES shape");
    // reset: lda #3 / sta pos (zp) / loop: jmp loop ($8004).
    assert_eq!(
        &prg(&r.bytes)[..7],
        &[0xA9, 0x03, 0x85, 0x00, 0x4C, 0x04, 0x80]
    );
    assert_eq!(&chr(&r.bytes)[..2], &[0xAA, 0xBB]);
    assert_eq!(r.files, vec!["main.s", "prg.s", "chars.s"]);
}

/// Probe-pinned: a `.segment` switch **inside** an included file persists
/// into the includer after the include (ca65 splices text, so segment state
/// is global) — bytes after the include land in the include's segment.
#[test]
fn ca65_nes_segment_switch_inside_include_persists() {
    let loader =
        MemoryLoader::new().text("chars.s", ".segment \"CHARS\"\n        .byte $AA, $BB\n");
    let src = "\
.segment \"CODE\"\n\
reset:  lda #$01\n\
.include \"chars.s\"\n\
        .byte $77\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("links");
    assert_eq!(
        &chr(&r.bytes)[..3],
        &[0xAA, 0xBB, 0x77],
        "the trailing .byte lands in CHARS, not CODE (probe-pinned)"
    );
    assert_eq!(
        &prg(&r.bytes)[..3],
        &[0xA9, 0x01, 0x00],
        "CODE holds only the lda"
    );
}

/// An error inside a nested include names *that* file and line, with the
/// include chain walking back to the root (R3/AE1 on the NES path) — both a
/// parse-time failure (unknown instruction) and a layout-time one (duplicate
/// symbol, stamped by the driver).
#[test]
fn ca65_nes_error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.s", ".include \"b.s\"\n")
        .text("b.s", "        lda #$01\n        frob $10\n");
    let e = assemble_ca65_files(".segment \"CODE\"\n.include \"a.s\"\n", "main.s", &loader)
        .expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.s, not of main.s");
    assert_eq!(
        e.source_map
            .file_table()
            .get(span.file.0 as usize)
            .map(String::as_str),
        Some("b.s"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.s".to_string(), 1), ("main.s".to_string(), 2)],
        "the include chain walks back to the root"
    );

    // The layout pass: a duplicate symbol whose second definition sits inside
    // the include is reported in the include's file.
    let loader = MemoryLoader::new().text("dup.s", "reset:  lda #$02\n");
    let e = assemble_ca65_files(
        ".segment \"CODE\"\nreset:  lda #$01\n.include \"dup.s\"\n",
        "main.s",
        &loader,
    )
    .expect_err("duplicate symbol");
    assert!(
        e.error.message.contains("duplicate symbol `reset`"),
        "names the symbol: {}",
        e.error.message
    );
    let span = e
        .error
        .span
        .as_ref()
        .expect("layout errors carry a span too");
    assert_eq!(span.line, 1, "line 1 of dup.s");
    assert_eq!(
        e.source_map
            .file_table()
            .get(span.file.0 as usize)
            .map(String::as_str),
        Some("dup.s")
    );
}

/// A missing `.include` target and a missing `.incbin` asset are diagnostics
/// at the directive's span (the operand's column), not CLI read errors.
#[test]
fn ca65_nes_missing_targets_report_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let e = assemble_ca65_files(
        ".segment \"CODE\"\n.include \"nothere.s\"\n",
        "main.s",
        &loader,
    )
    .expect_err("missing target");
    assert!(e.error.message.contains("nothere.s"), "{}", e.error.message);
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 2);
    assert_eq!(span.col, 10, "points at the operand (the file name)");

    let e = assemble_ca65_files(
        ".segment \"CHARS\"\n.incbin \"nothere.chr\"\n",
        "main.s",
        &loader,
    )
    .expect_err("missing asset");
    assert!(
        e.error.message.contains("nothere.chr"),
        "{}",
        e.error.message
    );
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(2));
}

/// `.incbin` of CHR data with offset/size windows inside a CHARS-segment
/// include — probe-pinned against ca65+ld65 (full asset, the `2,3` window,
/// then the negative-size rest-of-file sentinel: 8 + 3 + 2 bytes).
#[test]
fn ca65_nes_incbin_chr_windows_match_the_probe() {
    let tiles: Vec<u8> = vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];
    let loader = MemoryLoader::new()
        .text(
            "art.s",
            ".segment \"CHARS\"\n.incbin \"tiles.chr\"\n.incbin \"tiles.chr\", 2, 3\n.incbin \"tiles.chr\", 6, -9\n",
        )
        .binary("tiles.chr", tiles.clone());
    let src = "\
.segment \"CODE\"\n\
reset:  lda #$01\n\
.include \"art.s\"\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("links");
    let mut want = tiles;
    want.extend_from_slice(&[0x12, 0x13, 0x14, 0x16, 0x17]);
    assert_eq!(
        &chr(&r.bytes)[..want.len()],
        want.as_slice(),
        "probe-pinned CHR windows"
    );
    // A window error carries the directive's span in the included file.
    let loader = MemoryLoader::new()
        .text("art.s", ".segment \"CHARS\"\n.incbin \"tiles.chr\", 9\n")
        .binary("tiles.chr", vec![0u8; 8]);
    let e = assemble_ca65_files(".include \"art.s\"\n", "main.s", &loader)
        .expect_err("offset past EOF");
    let span = e.error.span.as_ref().expect("span at the directive");
    assert_eq!(span.line, 2);
    assert_eq!(
        e.source_map
            .file_table()
            .get(span.file.0 as usize)
            .map(String::as_str),
        Some("art.s")
    );
}

/// Anonymous (`:`) labels resolve across the include boundary in evaluation
/// order, and cheap (`@`) locals scope across it — a global inside the
/// include rescopes the includer's later cheap locals. Bytes pinned by the
/// p5/p6 probe runs.
#[test]
fn ca65_nes_anons_and_cheap_locals_cross_the_boundary() {
    // p5: `jmp :+` in the root resolves to the anon inside part.s, and the
    // root's later `bne :-` resolves back to that same in-include anon.
    let loader = MemoryLoader::new().text("part.s", ":       nop\n");
    let src = "\
.segment \"CODE\"\n\
reset:  ldx #0\n\
:       inx\n\
        jmp :+\n\
.include \"part.s\"\n\
        bne :-\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("links");
    assert_eq!(
        &prg(&r.bytes)[..9],
        &[0xA2, 0x00, 0xE8, 0x4C, 0x06, 0x80, 0xEA, 0xD0, 0xFD],
        "probe-pinned: both anon references land on part.s's `:` at $8006"
    );

    // p6: `@loop` at the top of the include scopes to the root's `reset`;
    // `mid:` inside rescopes the root's later `@tail`.
    let loader = MemoryLoader::new().text("sub.s", "@loop:  jmp @loop\nmid:    nop\n");
    let src = "\
.segment \"CODE\"\n\
reset:  lda #$01\n\
.include \"sub.s\"\n\
@tail:  jmp @tail\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("links");
    assert_eq!(
        &prg(&r.bytes)[..9],
        &[0xA9, 0x01, 0x4C, 0x02, 0x80, 0xEA, 0x4C, 0x06, 0x80],
        "probe-pinned: @loop = $8002 under reset, @tail = $8006 under mid"
    );
}

/// The single-source NES entry still means "one file": a `.include` or
/// `.incbin` is a clear pointer to the multi-file entry, not the old
/// `unsupported directive` rejection.
#[test]
fn ca65_nes_single_source_entry_rejects_both_directives_with_a_pointer() {
    for src in [
        ".segment \"CODE\"\n.include \"prg.s\"\n",
        ".segment \"CHARS\"\n.incbin \"tiles.chr\"\n",
    ] {
        let e = assemble_ca65(src).expect_err("no loader here");
        assert!(
            e.message.contains("multi-file"),
            "points at the multi-file entry: {}",
            e.message
        );
    }
}

/// Per-file provenance in the debug record (U5): `Header.sources` is the file
/// table in `FileId` order (KTD2) and each line span names the file its
/// statement was written in — the CHARS bytes attribute to the included art
/// file, the code to the root.
#[test]
fn ca65_nes_debug_line_records_carry_each_statements_file() {
    let loader = MemoryLoader::new()
        .text("art.s", ".segment \"CHARS\"\ntiles:  .byte $AA, $BB\n")
        .binary("tiles.chr", vec![1, 2, 3, 4]);
    let src = "\
.segment \"CODE\"\n\
reset:  lda #$01\n\
.include \"art.s\"\n\
.segment \"VECTORS\"\n\
        .word 0, reset, 0\n";
    let (r, info) = assemble_ca65_files_debug(src, "main.s", &loader).expect("links");
    assert_eq!(r.files, vec!["main.s", "art.s"]);
    assert_eq!(
        info.header.sources, r.files,
        "Header.sources ⇔ AssemblyResult.files, one FileId order (KTD2)"
    );
    let line_for = |file: &str, line: u32| {
        info.lines
            .iter()
            .find(|l| l.file == file && l.line == line)
            .unwrap_or_else(|| panic!("no line span for {file}:{line}"))
    };
    // The root's `lda` is main.s line 2; the include's `.byte` is art.s line 2.
    assert_eq!(line_for("main.s", 2).length, 2);
    let tiles = line_for("art.s", 2);
    assert_eq!(tiles.length, 2);
    // ...and the symbol defined in the include is in the record.
    assert!(
        info.symbols.iter().any(|s| s.name == "tiles"),
        "the include's label reaches the symbol record"
    );
}

/// The CLI's NES route (U5): a ca65 `.include` resolves via a `-I` search
/// dir on the human path and the linked `.nes` is written; the JSON failure
/// path resolves an in-include error's span to the included file's path.
#[test]
fn cli_assembles_a_ca65_nes_include_via_a_search_dir() {
    let srcdir = temp_tree("u5-nes-cli-src");
    let incdir = temp_tree("u5-nes-cli-inc");
    let main = srcdir.join("game.s");
    std::fs::write(
        &main,
        ".segment \"CODE\"\nreset:  lda #VAL\n.include \"defs.s\"\n\
         .segment \"VECTORS\"\n        .word 0, reset, 0\n",
    )
    .expect("write main");
    std::fs::write(incdir.join("defs.s"), "VAL = $2b\n").expect("write include");
    let out = srcdir.join("game.nes");
    let run = bin()
        .args(["--dialect", "ca65", "-I"])
        .arg(&incdir)
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "links: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let rom = std::fs::read(&out).expect("ROM written");
    assert_eq!(rom.len(), 16 + 0x8000 + 0x2000);
    assert_eq!(&rom[16..18], &[0xA9, 0x2B]);
}

/// The CLI's NES JSON failure path (U5): an error inside an included file
/// carries the include's resolved path on the diagnostic's span, and the
/// human path appends the `included from` note.
#[test]
fn cli_ca65_nes_error_in_include_names_that_file() {
    let dir = temp_tree("u5-nes-cli-err");
    let main = dir.join("main.s");
    std::fs::write(
        &main,
        ".segment \"CODE\"\nreset:  lda #$01\n.include \"bad.s\"\n",
    )
    .expect("write main");
    std::fs::write(dir.join("bad.s"), "        frob $10\n").expect("write include");

    // JSON: the span's additive `path` resolves the included file (KTD2).
    let run = bin()
        .args(["--dialect", "ca65", "--message-format=json"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success());
    let diags: Vec<asm198x::Diagnostic> =
        serde_json::from_slice(&run.stdout).expect("a bare Diagnostic array");
    assert_eq!(diags.len(), 1);
    let span = diags[0].span.as_ref().expect("span present");
    assert_eq!(span.line, 1, "line 1 of bad.s");
    assert!(
        span.path.as_deref().is_some_and(|p| p.ends_with("bad.s")),
        "the span's path names the included file: {:?}",
        span.path
    );

    // Human: `bad.s:1: error: …` plus the include-graph note.
    let run = bin()
        .args(["--dialect", "ca65"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("bad.s:1:"),
        "names the included file and line: {stderr}"
    );
    assert!(
        stderr.contains("included from") && stderr.contains("main.s:3"),
        "carries the included-from note: {stderr}"
    );
}

// ===========================================================================
// U6: the vasm (68000) multipass path — `include`/`incbin` through the flat
// (`assemble_vasm_warned_files`) and hunk-executable
// (`assemble_vasm_exe_files`) outputs. Semantics are pinned by the U6 probe
// runs (vasmm68k_mot 2.0b, mot syntax 3.19b): root-anchored resolution (vasm
// searches its cwd and the main source's directory, never the including
// file's), the zero/negative-length "rest of file" incbin sentinel with
// silent over-length truncation, and textual-splice state threading.
// ===========================================================================

/// A two-file 68000 program assembles exactly like the flattened text — the
/// include splices at the directive point (flat output).
#[test]
fn vasm_include_matches_the_flattened_source() {
    let loader = MemoryLoader::new().text("body.inc", "\tmoveq #2,d1\n\tadd.l d1,d0\n");
    let src = "\tmoveq #1,d0\n\tinclude \"body.inc\"\n\trts\n";
    let r = assemble_vasm_warned_files(src, "main.s", &loader).expect("assembles");
    let flat = assemble_vasm_warned("\tmoveq #1,d0\n\tmoveq #2,d1\n\tadd.l d1,d0\n\trts\n")
        .expect("flattened");
    assert_eq!(r.bytes, flat.bytes);
    assert_eq!(r.files, vec!["main.s", "body.inc"]);
}

/// Three-deep nesting, code at every level, spliced in walk order.
#[test]
fn vasm_include_three_deep_nests() {
    let loader = MemoryLoader::new()
        .text(
            "a.inc",
            "\tmoveq #2,d1\n\tinclude \"b.inc\"\n\tmoveq #4,d3\n",
        )
        .text("b.inc", "\tmoveq #3,d2\n");
    let src = "\tmoveq #1,d0\n\tinclude \"a.inc\"\n\tmoveq #5,d4\n";
    let r = assemble_vasm_warned_files(src, "main.s", &loader).expect("assembles");
    let flat = assemble_vasm_warned(
        "\tmoveq #1,d0\n\tmoveq #2,d1\n\tmoveq #3,d2\n\tmoveq #4,d3\n\tmoveq #5,d4\n",
    )
    .expect("flattened");
    assert_eq!(r.bytes, flat.bytes);
    assert_eq!(r.files, vec!["main.s", "a.inc", "b.inc"]);
}

/// An `equ` defined inside an include feeds the includer's later instruction
/// selection — vasm's optimizer consults the constant: `add.l #N` becomes
/// `addq` (5A80), `add.l #BIG,a0` becomes `lea d16(a0),a0` (41E8 1234), and
/// `moveq #N` encodes in range (7205). Bytes probe-pinned (KTD1's outward
/// flow, the vasm shape).
#[test]
fn vasm_equ_in_include_feeds_optimizer_selection() {
    let loader = MemoryLoader::new().text("defs.inc", "N equ 5\nBIG equ $1234\n");
    let src = "\tinclude \"defs.inc\"\n\tadd.l #N,d0\n\tadd.l #BIG,a0\n\tmoveq #N,d1\n";
    let r = assemble_vasm_warned_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![0x5A, 0x80, 0x41, 0xE8, 0x12, 0x34, 0x72, 0x05],
        "addq / lea / moveq selections all saw the include's constants"
    );
}

/// vasm request spellings, all probe-pinned: `"file"`, `'file'`, a bare
/// token, and trailing junk after a quoted name is ignored.
#[test]
fn vasm_request_spellings_match_the_probes() {
    for src in [
        "\tinclude \"body.inc\"\n",
        "\tinclude 'body.inc'\n",
        "\tinclude body.inc\n",
        "\tinclude \"body.inc\" junk\n",
    ] {
        let loader = MemoryLoader::new().text("body.inc", "\tnop\n");
        let r = assemble_vasm_warned_files(src, "main.s", &loader)
            .unwrap_or_else(|e| panic!("{src:?} assembles: {}", e.error));
        assert_eq!(r.bytes, vec![0x4E, 0x71], "{src:?}");
    }
    // The bare form stops at a comma, so the incbin tail still parses.
    let loader = MemoryLoader::new().binary("data.bin", vec![0x10, 0x11, 0x12, 0x13]);
    let r = assemble_vasm_warned_files("\tincbin data.bin,2\n", "main.s", &loader)
        .expect("bare name with a tail");
    assert_eq!(r.bytes, vec![0x12, 0x13]);
}

/// Locals qualify against the enclosing global across the boundary in both
/// directions (textual splice, probe-pinned): a `.local` in the include
/// scopes under the includer's global, and a global defined inside the
/// include rescopes the includer's locals after it.
#[test]
fn vasm_locals_cross_include_boundary() {
    // Probe l.s: main defines `start`, the include defines `.here` under it,
    // and main references `.here` after the include (bytes 4E71 4E71 60FC
    // 4E71 60FC 60F6, probe-pinned).
    let loader = MemoryLoader::new().text("loc.inc", ".here:\tnop\n\tbra.s .here\n");
    let src = "start:\tnop\n\tinclude \"loc.inc\"\n.tail:\tnop\n\tbra.s .tail\n\tbra.s .here\n";
    let r = assemble_vasm_warned_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        vec![
            0x4E, 0x71, 0x4E, 0x71, 0x60, 0xFC, 0x4E, 0x71, 0x60, 0xFC, 0x60, 0xF6
        ]
    );

    // Probe g2.s: a global inside the include rescopes — `.a` after the
    // include resolves under `mid`, which never defined it (vasm: undefined
    // symbol < mid .a>).
    let loader = MemoryLoader::new().text("glob.inc", "mid:\tnop\n");
    let src = "start:\tnop\n.a:\tnop\n\tinclude \"glob.inc\"\n\tbra.s .a\n";
    let e = assemble_vasm_warned_files(src, "main.s", &loader)
        .expect_err("rescoped local is undefined");
    assert!(
        e.error.message.contains("mid.a"),
        "the reference resolved under the include's global: {}",
        e.error.message
    );
}

/// An error inside an included file names that file, its line, and the
/// include chain (R3 / AE1's mechanism on the vasm path).
#[test]
fn vasm_error_in_included_file_names_that_file_with_its_chain() {
    let loader = MemoryLoader::new()
        .text("a.inc", "\tinclude \"b.inc\"\n")
        .text("b.inc", "\tmoveq #1,d0\n\tfrobnicate d0\n");
    let e = assemble_vasm_warned_files("\tinclude \"a.inc\"\n", "main.s", &loader)
        .expect_err("frobnicate is unknown");
    let span = e.error.span.as_ref().expect("the error carries a span");
    assert_eq!(span.line, 2, "line 2 of b.inc, not of main.s");
    assert_eq!(
        e.source_map
            .file_table()
            .get(span.file.0 as usize)
            .map(String::as_str),
        Some("b.inc"),
        "the span names the included file"
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.inc".to_string(), 1), ("main.s".to_string(), 1)],
        "the include chain walks back to the root"
    );
}

/// A warning raised inside an included file is stamped with that file — the
/// CLI prints `w.inc: …`, not the root input's name.
#[test]
fn vasm_warning_in_included_file_names_that_file() {
    let loader = MemoryLoader::new().text("w.inc", "\tmoveq #999,d0\n");
    let r = assemble_vasm_warned_files("\tinclude \"w.inc\"\n\trts\n", "main.s", &loader)
        .expect("warns, not errors");
    assert_eq!(r.warnings.len(), 1);
    assert_eq!(
        r.files
            .get(r.warnings[0].file.0 as usize)
            .map(String::as_str),
        Some("w.inc"),
        "the warning names the include"
    );
}

/// A missing `include` target and a missing `incbin` asset are diagnostics at
/// the directive's span (the operand's column), not CLI read errors.
#[test]
fn vasm_missing_targets_report_at_the_directive_span() {
    let loader = MemoryLoader::new();
    let e = assemble_vasm_warned_files("\tnop\n\tinclude \"nope.inc\"\n", "main.s", &loader)
        .expect_err("missing include");
    assert!(e.error.message.contains("nope.inc"), "{}", e.error.message);
    let span = e.error.span.as_ref().expect("span present");
    assert_eq!((span.line, span.file), (2, FileId(0)));
    assert_ne!(span.col, 0, "points at the operand field");

    let e = assemble_vasm_warned_files("\tincbin \"nope.bin\"\n", "main.s", &loader)
        .expect_err("missing incbin");
    assert!(e.error.message.contains("nope.bin"), "{}", e.error.message);
    assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(1));
}

/// A self-include reports the cycle with its chain.
#[test]
fn vasm_self_include_reports_the_cycle() {
    let loader = MemoryLoader::new().text("loop.inc", "\tinclude \"loop.inc\"\n");
    let e = assemble_vasm_warned_files("\tinclude \"loop.inc\"\n", "main.s", &loader)
        .expect_err("cycle");
    assert!(
        e.error.message.contains("include cycle"),
        "{}",
        e.error.message
    );
}

/// The vasm `incbin` windows, probe-pinned (vasmm68k_mot 2.0b): plain and
/// offset forms; offset at EOF is empty; a length of zero **or** negative is
/// the rest-of-file sentinel; a length past the remaining bytes silently
/// truncates (vasm exits 0 — mirrored so the bytes stay identical).
#[test]
fn vasm_incbin_windows_match_the_probes() {
    let asset = vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];
    let cases: &[(&str, &[u8])] = &[
        (
            "\tdc.b $aa\n\tincbin \"data.bin\"\n\tdc.b $bb\n",
            &[0xAA, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0xBB],
        ),
        (
            "\tincbin \"data.bin\",2\n",
            &[0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        ("\tincbin \"data.bin\",2,3\n", &[0x12, 0x13, 0x14]),
        ("\tincbin \"data.bin\",8\n", &[]),
        // Zero and negative lengths are vasm's "rest of the file" sentinel.
        (
            "\tincbin \"data.bin\",0,0\n",
            &[0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        (
            "\tincbin \"data.bin\",2,-3\n",
            &[0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        ),
        // A length past the remaining bytes silently truncates.
        ("\tincbin \"data.bin\",6,4\n", &[0x16, 0x17]),
        // Offset/length are parse-time constant expressions over the live
        // environment — including constants from an earlier include.
        (
            "\tinclude \"off.inc\"\n\tincbin \"data.bin\",OFF,OFF+1\n",
            &[0x12, 0x13, 0x14],
        ),
    ];
    for (src, want) in cases {
        let loader = MemoryLoader::new()
            .binary("data.bin", asset.clone())
            .text("off.inc", "OFF equ 2\n");
        let r = assemble_vasm_warned_files(src, "main.s", &loader)
            .unwrap_or_else(|e| panic!("{src:?} assembles: {}", e.error));
        assert_eq!(r.bytes, *want, "{src:?}");
        assert!(r.warnings.is_empty(), "no warnings for {src:?}");
    }
}

/// The vasm `incbin` error postures, probe-pinned: a negative offset and an
/// offset past EOF are errors ("bad file-offset argument"); a forward
/// reference in the offset is "expression must be constant" (parse-time
/// folding).
#[test]
fn vasm_incbin_window_errors_match_the_probes() {
    let asset = vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];
    for (src, needle) in [
        ("\tincbin \"data.bin\",-2\n", "negative"),
        ("\tincbin \"data.bin\",9\n", "past the end"),
        ("\tincbin \"data.bin\",OFF\nOFF equ 2\n", "constant"),
    ] {
        let loader = MemoryLoader::new().binary("data.bin", asset.clone());
        let e = assemble_vasm_warned_files(src, "main.s", &loader).expect_err("window error");
        assert!(
            e.error.message.contains(needle),
            "{src:?}: {}",
            e.error.message
        );
        assert_eq!(e.error.span.as_ref().map(|s| s.line), Some(1));
    }
}

/// A label on an `include` line binds at the include point; a label on an
/// `incbin` line binds at the payload start (probe-pinned).
#[test]
fn vasm_labels_on_directive_lines_bind_at_the_point() {
    let loader = MemoryLoader::new()
        .text("body.inc", "\tmoveq #1,d0\n")
        .binary("data.bin", vec![0x10, 0x11]);
    let src = "here:\tinclude \"body.inc\"\nart:\tincbin \"data.bin\"\n\tdc.w here\n\tdc.w art\n";
    let r = assemble_vasm_warned_files(src, "main.s", &loader).expect("assembles");
    // moveq (2) + payload (2) + dc.w 0 (here) + dc.w 2 (art).
    assert_eq!(
        r.bytes,
        vec![0x70, 0x01, 0x10, 0x11, 0x00, 0x00, 0x00, 0x02]
    );
}

/// A `section` switch inside an include persists into the includer after it
/// (textual splice, probe-pinned under the hunk-exe output), and a hunk
/// executable with an included file and an incbin serializes exactly like
/// the flattened source through the single-source exe path.
#[test]
fn vasm_section_switch_inside_include_persists_in_the_exe() {
    let loader = MemoryLoader::new()
        .text("sw.inc", "\tsection two,data\n\tdc.b $01\n")
        .binary(
            "data.bin",
            vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        );
    let src = "\tsection one,code\n\tmoveq #1,d0\n\tinclude \"sw.inc\"\n\tdc.b $02\n\tincbin \"data.bin\",2,3\n";
    let r = assemble_vasm_exe_files(src, "main.s", &loader).expect("exe assembles");
    let flat = asm198x::assemble_vasm_exe(
        "\tsection one,code\n\tmoveq #1,d0\n\tsection two,data\n\tdc.b $01\n\tdc.b $02\n\tdc.b $12,$13,$14\n",
    )
    .expect("flattened exe");
    assert_eq!(
        r.bytes, flat.bytes,
        "the include's section switch owns the includer's later bytes"
    );
    assert_eq!(r.files, vec!["main.s", "sw.inc"]);
}

/// The single-source vasm entries still mean "one file": an `include` or
/// `incbin` is a clear pointer to the multi-file entry, on both the flat and
/// the hunk-exe paths.
#[test]
fn vasm_single_source_entries_reject_both_directives_with_a_pointer() {
    for src in ["\tinclude \"body.inc\"\n", "\tincbin \"data.bin\"\n"] {
        let e = assemble_vasm(src).expect_err("no loader here");
        assert!(
            e.message.contains("multi-file"),
            "flat points at the multi-file entry: {}",
            e.message
        );
        let e = asm198x::assemble_vasm_exe(src).expect_err("no loader here either");
        assert!(
            e.message.contains("multi-file"),
            "exe points at the multi-file entry: {}",
            e.message
        );
    }
}

/// Per-file provenance in the vasm debug record (U6): `Header.sources` is the
/// file table in `FileId` order (KTD2) and each line span names the file its
/// statement was written in — on both the flat and the hunk-exe captures.
#[test]
fn vasm_debug_line_records_carry_each_statements_file() {
    let loader = MemoryLoader::new().text("art.inc", "sprite:\tdc.w $ABCD\n");
    let src = "start:\tmoveq #1,d0\n\tinclude \"art.inc\"\n\trts\n";
    let (r, info) =
        asm198x::assemble_vasm_warned_files_debug(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.files, vec!["main.s", "art.inc"]);
    assert_eq!(
        info.header.sources, r.files,
        "Header.sources ⇔ AssemblyResult.files, one FileId order (KTD2)"
    );
    let line_for = |file: &str, line: u32| {
        info.lines
            .iter()
            .find(|l| l.file == file && l.line == line)
            .unwrap_or_else(|| panic!("no line span for {file}:{line}"))
    };
    assert_eq!(
        line_for("main.s", 1).length,
        2,
        "moveq is the root's line 1"
    );
    assert_eq!(
        line_for("art.inc", 1).length,
        2,
        "the dc.w attributes to the include"
    );
    assert!(
        info.symbols.iter().any(|s| s.name == "sprite"),
        "the include's label reaches the symbol record"
    );

    // The exe capture carries the same per-file records.
    let loader = MemoryLoader::new().text("art.inc", "sprite:\tdc.w $ABCD\n");
    let (_, info) =
        asm198x::assemble_vasm_exe_files_debug(src, "main.s", &loader).expect("exe assembles");
    assert!(
        info.lines
            .iter()
            .any(|l| l.file == "art.inc" && l.line == 1),
        "exe line records name the include too"
    );
}

/// The CLI's vasm route (U6): an `include` resolves via a `-I` search dir on
/// the human path (flat output), and `--exe` writes the hunk executable
/// through the same multi-file entries.
#[test]
fn cli_assembles_a_vasm_include_via_a_search_dir() {
    let srcdir = temp_tree("u6-vasm-cli-src");
    let incdir = temp_tree("u6-vasm-cli-inc");
    let main = srcdir.join("game.s");
    std::fs::write(&main, "\tmoveq #N,d0\n\tinclude \"defs.i\"\n\trts\n").expect("write main");
    std::fs::write(incdir.join("defs.i"), "N equ 3\n").expect("write include");
    let out = srcdir.join("game.bin");
    let run = bin()
        .args(["--dialect", "vasm", "-I"])
        .arg(&incdir)
        .arg(&main)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(
        run.status.success(),
        "assembles: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(&out).expect("bin written"),
        vec![0x70, 0x03, 0x4E, 0x75]
    );
}

/// The CLI's vasm human failure path (U6): an error inside an included file
/// renders `file:line` with the `included from` note.
#[test]
fn cli_vasm_error_in_include_carries_an_included_from_note() {
    let dir = temp_tree("u6-vasm-cli-err");
    let main = dir.join("main.s");
    std::fs::write(&main, "\tmoveq #1,d0\n\tinclude \"bad.i\"\n").expect("write main");
    std::fs::write(dir.join("bad.i"), "\tfrobnicate d0\n").expect("write include");
    let run = bin()
        .args(["--dialect", "vasm"])
        .arg(&main)
        .output()
        .expect("run asm198x");
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("bad.i:1"),
        "names the included file and line: {stderr}"
    );
    assert!(
        stderr.contains("included from") && stderr.contains("main.s:2"),
        "carries the included-from note: {stderr}"
    );
}

// ===========================================================================
// U8 — conditional assembly on sjasmplus meets the include walk. KTD1's
// proof: an include inside an untaken branch never loads. Every byte
// expectation is pinned by the sjasmplus 1.21.0 probe runs (u8-probes).
// ===========================================================================

/// KTD1's proof for a keyword dialect: `IF 0 / INCLUDE "missing.inc" / ENDIF`
/// assembles cleanly — the loader is never asked for the untaken target
/// (probe p14: the reference skips it too).
#[test]
fn guarded_include_never_loads_when_untaken() {
    let loader = MemoryLoader::new(); // deliberately empty: nothing may load
    let src = "        org $8000\n\
               \x20       IF 0\n\
               \x20       include \"missing.inc\"\n\
               \x20       ENDIF\n\
               \x20       ld a,1\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("untaken include skipped");
    assert_eq!(r.bytes, vec![0x3E, 0x01]);
    assert_eq!(
        r.files,
        vec!["main.asm".to_string()],
        "the untaken target never entered the file table"
    );
}

/// The taken counterpart: a guarded include loads, and what it defines (an
/// `equ` feeding a later opcode-embedded form) flows out — the environment
/// threads through the conditional *and* the include boundary.
#[test]
fn guarded_include_loads_when_taken() {
    let loader = MemoryLoader::new().text("defs.inc", "BITN equ 5\n");
    let src = "        org $8000\n\
               \x20       IF 1\n\
               \x20       include \"defs.inc\"\n\
               \x20       ENDIF\n\
               \x20       bit BITN,a\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("taken include loads");
    assert_eq!(r.bytes, vec![0xCB, 0x6F]);
    assert_eq!(
        r.files,
        vec!["main.asm".to_string(), "defs.inc".to_string()]
    );
}

/// A guarded `incbin` behaves the same way: the untaken asset is never
/// requested from the loader (the include mechanism's KTD1, on the binary
/// path).
#[test]
fn guarded_incbin_never_loads_when_untaken() {
    let loader = MemoryLoader::new();
    let src = "        org $8000\n\
               \x20       IF 0\n\
               \x20       incbin \"missing.bin\"\n\
               \x20       ENDIF\n\
               \x20       ld a,1\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("untaken incbin skipped");
    assert_eq!(r.bytes, vec![0x3E, 0x01]);
}

/// DEFINEs thread through the include boundary in both directions (probe
/// p36): the includer's `DEFINE WANT` guards code *inside* the include, and
/// the include's `DEFINE FROMINC` guards code *after* the include point.
#[test]
fn defines_thread_through_includes_both_ways() {
    let loader = MemoryLoader::new().text(
        "guard.inc",
        "        IFDEF WANT\n        ld a,9\n        ENDIF\n        DEFINE FROMINC\n",
    );
    let src = "        org $8000\n\
               \x20       DEFINE WANT\n\
               \x20       include \"guard.inc\"\n\
               \x20       IFDEF FROMINC\n\
               \x20       ld b,1\n\
               \x20       ENDIF\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x3E, 0x09, 0x06, 0x01]);
}

/// A conditional block cannot span an include boundary, direction one: the
/// `IF` sits in the includer and the `ENDIF` inside the include. The
/// includer's own structure parse rejects it before the include is even
/// consulted (the reference rejects both halves — probe p12).
#[test]
fn cross_file_endif_in_include_is_rejected() {
    let loader = MemoryLoader::new().text("tail.inc", "        ld a,9\n        ENDIF\n");
    let src = "        org $8000\n\
               \x20       IF 1\n\
               \x20       include \"tail.inc\"\n\
               \x20       ld b,1\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("unterminated IF");
    assert!(
        e.error.message.contains("no matching `ENDIF`"),
        "names the unterminated block: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span at the IF head");
    assert_eq!(span.line, 2, "points at the `IF` line");
    assert_eq!(
        e.source_map.file_table().first().map(String::as_str),
        Some("main.asm")
    );
}

/// Direction two: the `IF` opens inside the include and the `ENDIF` sits in
/// the includer. The include's structure parse rejects the unterminated
/// block, naming the include's file (probe p13's posture).
#[test]
fn cross_file_if_in_include_is_rejected() {
    let loader = MemoryLoader::new().text("frag.inc", "        IF 1\n        ld a,9\n");
    let src = "        org $8000\n        include \"frag.inc\"\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("unterminated IF");
    assert!(
        e.error.message.contains("no matching `ENDIF`"),
        "names the unterminated block: {}",
        e.error.message
    );
    let span = e.error.span.as_ref().expect("span inside the include");
    assert_eq!(span.line, 1, "the `IF` line of frag.inc");
    assert_eq!(
        e.source_map
            .file_table()
            .get(span.file.0 as usize)
            .map(String::as_str),
        Some("frag.inc"),
        "the diagnostic names the include, not the root"
    );
}

/// A stray `ENDIF` at the top level of the root (the other half of probe
/// p13's fixture) is rejected by the root's own parse.
#[test]
fn stray_endif_in_the_root_is_rejected() {
    let loader = MemoryLoader::new().text("frag.inc", "        nop\n");
    let src = "        org $8000\n        include \"frag.inc\"\n        ENDIF\n";
    let e = assemble_sjasmplus_files(src, "main.asm", &loader).expect_err("stray ENDIF");
    assert!(
        e.error.message.contains("`ENDIF` without a matching `IF`"),
        "unexpected: {}",
        e.error.message
    );
}

/// A conditional inside an include evaluates with the environment live at the
/// include point, and locals keep scoping across the boundary (the U2
/// boundary scenario, now under a conditional).
#[test]
fn conditional_inside_include_sees_the_includers_environment() {
    let loader = MemoryLoader::new().text(
        "body.inc",
        "        IF MODE = 2\n.here:  nop\n        jr .here\n        ELSE\n        ld a,0\n        ENDIF\n",
    );
    let src = "        org $8000\n\
               MODE    equ 2\n\
               start:\n\
               \x20       include \"body.inc\"\n";
    let r = assemble_sjasmplus_files(src, "main.asm", &loader).expect("assembles");
    assert_eq!(r.bytes, vec![0x00, 0x18, 0xFD]);
    assert_eq!(
        r.symbols.get("start.here"),
        Some(&0x8000),
        "the include's local scoped under the includer's global"
    );
}
