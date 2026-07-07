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
use asm198x::{FileId, assemble_sjasmplus, assemble_sjasmplus_files};

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
