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
    FileId, assemble_acme, assemble_acme_files, assemble_pasmo, assemble_pasmo_files,
    assemble_sjasmplus, assemble_sjasmplus_files,
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
