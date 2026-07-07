//! U1 (language surface): the multi-file source model. These exercise the
//! loader seam (KTD8) — filesystem + in-memory impls — the `SourceMap`'s
//! `FileId` allocation and dedup-by-canonical-path, and the CLI's repeatable
//! `-I` flag plus the rustc-style `file:line:col` human error rendering. No
//! dialect is include-capable yet (that is U2); everything here drives the
//! foundation directly.

use std::path::PathBuf;
use std::process::Command;

use asm198x::FileId;
use asm198x::source::{FsLoader, MemoryLoader, SourceLoader, SourceMap};

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
