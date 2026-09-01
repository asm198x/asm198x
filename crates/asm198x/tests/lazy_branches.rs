//! #481 — a chosen branch is read eagerly, understood lazily
//! (`decisions/parse-affecting-directives.md`): the ca65 projection sweep
//! reads a selected branch's lines under the text environment in force at
//! that point, and an untaken branch's lines are never read at all. These are
//! the multi-file consequences; the single-source behaviours are pinned in
//! the dialect's own unit tests. ca65 + ld65 is the ignored differential at
//! the bottom.

use asm198x::source::MemoryLoader;
use asm198x::{assemble_ca65_files, assemble_ca65_files_with_config};

/// Issue point 5: an include inside a selected branch shares the sweep's
/// environment — a definition made inside it flows back out to the rest of
/// the source (probed: `a9 04`).
#[test]
fn an_include_inside_a_taken_branch_shares_the_environment() {
    let loader = MemoryLoader::new().text("defs.inc", ".define d16 4\n");
    let src = ".segment \"CODE\"\n.if 1\n.include \"defs.inc\"\n.endif\n.byte d16\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.bytes[16], 4, "the include's definition flows out");
}

/// The environment also flows *into* an include resolved mid-sweep: a
/// definition made before the block substitutes inside the included file.
#[test]
fn a_definition_flows_into_an_include_inside_a_taken_branch() {
    let loader = MemoryLoader::new().text("use.inc", ".byte six\n");
    let src = ".segment \"CODE\"\n.define six 6\n.if 1\n.include \"use.inc\"\n.endif\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.bytes[16], 6, "the definition reaches the included lines");
}

/// An `.include` whose file name is a text symbol resolves at the moment the
/// sweep reads the directive — after substitution.
#[test]
fn an_include_named_by_a_text_symbol_resolves() {
    let loader = MemoryLoader::new().text("payload.inc", ".byte 7\n");
    let src = ".segment \"CODE\"\n.define F \"payload.inc\"\n.include F\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.bytes[16], 7);
}

/// ca65's sequential reader skips an untaken branch without processing its
/// directives, so an `.include` of a file that does not exist inside one is
/// no error — and now none here either, because the target is only opened
/// when the sweep reads the directive.
#[test]
fn a_missing_include_inside_an_untaken_branch_is_never_opened() {
    let loader = MemoryLoader::new();
    let src = ".segment \"CODE\"\n.if 0\n.include \"does-not-exist.inc\"\n.endif\n.byte 3\n";
    let r = assemble_ca65_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.bytes[16], 3);
}

/// ca65 + ld65 over the #481 behaviours in one source, byte-compared under a
/// flat config. Ignored without the reference tools, exactly like every other
/// differential.
#[test]
#[ignore = "needs ca65 + ld65 on PATH"]
fn lazy_branch_reading_matches_ca65_plus_ld65() {
    let dir = std::env::temp_dir().join(format!("asm198x-lazy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let cfg = r#"
MEMORY { ROM: start = $8000, size = $0100, type = ro, file = %O, fill = yes, fillval = $00; }
SEGMENTS { CODE: load = ROM, type = ro; }
"#;
    // Every accepted probe shape from the #481 session: untaken invisibility,
    // taken flow (top level and across blocks), positional undefine, the
    // per-iteration repeat, and an untaken branch that never needs to parse.
    let src = ".if 0\n.define six 6\nthis is lexable garbage but not a statement\n.endif\n\
.ifdef six\n.byte 6\n.else\n.byte 1\n.endif\n\
.if 1\n.define five 5\n.endif\n.byte five\n\
.if 1\n.undefine five\n.endif\n.ifdef five\n.byte 5\n.else\n.byte 2\n.endif\n\
.repeat 3\n.define val 7\n.byte val\n.undefine val\n.endrepeat\n";
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
    assert_eq!(ours, reference, "byte-identical under lazy branch reading");
}
