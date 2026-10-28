//! The gate for ca65's walk over Lang198x's front end
//! (`docs/plans/2026-10-27-ca65-walk/plan.md`).
//!
//! Every committed `ca65` verdict for the 6502, from the form audit, the
//! differential fuzz and the probes, is assembled through the walk and
//! checked against what ca65 V2.19 and ld65 did: the same bytes, or a
//! refusal where ca65 refused. Each case the walk does not match yet is
//! named here with the step that covers it, and the list may only shrink.
//!
//! The NES curriculum's programs are checked the same way, linked as the
//! curriculum links them: every `ca65-nes` verdict's ROM, by its digest,
//! and each program's debug record and memory map against the old
//! reader's. Their source is Code198x's, found through
//! `ASM198X_CODE_SAMPLES` at the pinned revision; without it, that half has
//! nothing to check.

mod support;

use support::verdicts::{code_samples_root, corpus_path, parse_curriculum_key, sha256_hex};
use verdict_corpus::{Corpus, Outcome, Suite};

/// The cases the walk does not match yet: a case's name, and the step of
/// the plan that covers it.
const NOT_YET: &[(&str, &str)] = &[];

#[test]
fn the_walk_gives_ca65s_verdicts() {
    let corpus = Corpus::read(&corpus_path("6502")).expect("the 6502 corpus");
    let mut checked = 0;
    let mut differ: Vec<(String, String)> = Vec::new();
    for verdict in corpus.verdicts() {
        if verdict.dialect != "ca65" || verdict.suite == Suite::Curriculum {
            continue;
        }
        let key = verdict.key();
        let ours = asm198x::assemble_ca65_lang198x(&key.source).map(|r| r.bytes);
        let matches = match &verdict.outcome {
            Outcome::Rejected { .. } => ours.is_err(),
            outcome => ours.as_ref().ok() == outcome.bytes().as_ref(),
        };
        checked += 1;
        if !matches {
            let ours = match &ours {
                Ok(bytes) => format!("{bytes:02X?}"),
                Err(e) => format!("refused: {e}"),
            };
            differ.push((
                key.case.clone(),
                format!(
                    "{} [{:?}]: ours {ours}, ca65 {:?}\n{}",
                    key.case, verdict.suite, verdict.outcome, key.source
                ),
            ));
        }
    }
    eprintln!(
        "ca65 walk: {checked} verdicts checked, {} differ",
        differ.len()
    );
    assert!(checked > 300, "the corpus lost its ca65 verdicts");
    let unnamed: Vec<&String> = differ
        .iter()
        .filter(|(case, _)| !NOT_YET.iter().any(|(named, _)| named == case))
        .map(|(_, detail)| detail)
        .collect();
    let stale: Vec<&str> = NOT_YET
        .iter()
        .filter(|(named, _)| !differ.iter().any(|(case, _)| case == named))
        .map(|(named, _)| *named)
        .collect();
    assert!(
        unnamed.is_empty(),
        "{} differences are not named:\n{}",
        unnamed.len(),
        unnamed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        stale.is_empty(),
        "these now match ca65, so take them off the list: {stale:?}"
    );
}

/// The curriculum programs the walk does not build as ca65 and ld65 did
/// yet: a program's path, and the step of the plan that covers it.
const CURRICULUM_NOT_YET: &[(&str, &str)] = &[];

#[test]
fn the_walk_builds_the_nes_curriculum_as_ca65_did() {
    let Some(root) = code_samples_root() else {
        eprintln!("no curriculum checkout: set ASM198X_CODE_SAMPLES to check it");
        return;
    };
    let corpus = Corpus::read(&corpus_path("6502")).expect("the 6502 corpus");
    let mut checked = 0;
    let mut differ: Vec<(String, String)> = Vec::new();
    for verdict in corpus.verdicts() {
        if verdict.dialect != "ca65-nes" || verdict.suite != Suite::Curriculum {
            continue;
        }
        let key = verdict.key();
        let Outcome::Digest { digest } = &verdict.outcome else {
            continue;
        };
        let (relpath, _, source_digest) =
            parse_curriculum_key(&key.source).expect("a curriculum key");
        let source = std::fs::read_to_string(root.join(relpath))
            .unwrap_or_else(|e| panic!("{relpath}: {e}"));
        assert_eq!(
            sha256_hex(source.as_bytes()),
            source_digest,
            "{relpath} is not at the pinned revision"
        );
        checked += 1;
        same_outputs(relpath, &source, &asm198x::source::MemoryLoader::new());
        let ours = asm198x::assemble_ca65_nes_lang198x(&source).map(|r| r.bytes);
        let matches = matches!(&ours, Ok(bytes) if sha256_hex(bytes) == *digest);
        if !matches {
            let ours = match &ours {
                Ok(bytes) => format!("digest {}", sha256_hex(bytes)),
                Err(e) => format!("refused: {e}"),
            };
            differ.push((relpath.to_owned(), format!("{relpath}: ours {ours}")));
        }
    }
    eprintln!(
        "ca65 walk: {checked} curriculum programs checked, {} differ",
        differ.len()
    );
    assert!(checked >= 51, "the corpus lost its ca65-nes verdicts");
    let unnamed: Vec<&String> = differ
        .iter()
        .filter(|(path, _)| !CURRICULUM_NOT_YET.iter().any(|(named, _)| named == path))
        .map(|(_, detail)| detail)
        .collect();
    let stale: Vec<&str> = CURRICULUM_NOT_YET
        .iter()
        .filter(|(named, _)| !differ.iter().any(|(path, _)| path == named))
        .map(|(named, _)| *named)
        .collect();
    assert!(
        unnamed.is_empty(),
        "{} programs differ and are not named:\n{}",
        unnamed.len(),
        unnamed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        stale.is_empty(),
        "these now match ca65, so take them off the list: {stale:?}"
    );
}

/// The verdicts compare bytes, so the walk's messages are checked here: a
/// warning or a failed `warning` assertion carries on and says so, as the
/// old reader does, and an error stops with its text.
#[test]
fn the_walk_says_what_messages_say() {
    let warned = |source: &str| {
        let result = asm198x::assemble_ca65_lang198x(source).expect(source);
        result
            .warnings
            .iter()
            .map(|w| w.message.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(warned(".warning \"soft\"\nnop\n"), ["soft"]);
    assert_eq!(warned(".assert 0, warning\nnop\n"), ["Assertion failed"]);
    assert_eq!(warned(".assert 0, ldwarning, \"x\"\nnop\n"), ["x"]);
    assert!(warned(".assert 1, warning, \"x\"\nnop\n").is_empty());
    for (source, text) in [
        (".error \"hard\"\nnop\n", "hard"),
        (".fatal \"stop\"\nnop\n", "stop"),
        (".assert 0, error, \"boom\"\nnop\n", "boom"),
    ] {
        let error = asm198x::assemble_ca65_lang198x(source).expect_err(source);
        assert!(error.to_string().contains(text), "{source:?}: {error}");
    }
}

/// ca65 V2.19 keeps eight CPUs on its stack. It refuses a ninth
/// `.pushcpu` and a `.popcpu` with nothing kept, and warns at the last line
/// it read when CPUs are still kept at the end. Measured on V2.19.
#[test]
fn the_walk_keeps_ca65s_cpu_stack() {
    let warned = |source: &str| {
        let result = asm198x::assemble_ca65_lang198x(source).expect(source);
        result
            .warnings
            .iter()
            .map(|w| (w.line, w.message.clone()))
            .collect::<Vec<_>>()
    };
    let left = |line| vec![(line, "CPU stack is not empty".to_owned())];
    assert_eq!(warned("nop\n.pushcpu\nnop\n\n"), left(4));
    assert_eq!(warned(".pushcpu\n.pushcpu\n.popcpu\nnop"), left(4));
    assert_eq!(warned(".repeat 8\n.pushcpu\n.endrepeat\nnop\n"), left(4));
    assert!(warned(".pushcpu\n.popcpu\nnop\n").is_empty());
    for (source, text) in [
        (".repeat 9\n.pushcpu\n.endrepeat\n", "CPU stack overflow"),
        ("nop\n.popcpu\n", "CPU stack is empty"),
    ] {
        let error = asm198x::assemble_ca65_lang198x(source).expect_err(source);
        assert!(error.to_string().contains(text), "{source:?}: {error}");
    }
}

/// The walk's multi-file entry: `.include` and `.incbin` read through the
/// loader, the bytes interleave in include order, and the file table and
/// include chain come back with the result. ca65 measures these through the
/// live multi-file probes; these hold the walk to them without the tools.
#[test]
fn the_walk_reads_included_files() {
    use asm198x::source::MemoryLoader;
    let loader = MemoryLoader::new()
        .text(
            "a.s",
            " lda #$02\n .INCLUDE \"b.s\"\n lda #$04\nptr = $10\n",
        )
        .text("b.s", " lda #$03\n")
        .binary(
            "data.bin",
            vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        );
    let src = " lda #$01\n .include \"a.s\"\n lda #$05\n lda ptr\n\
               .incbin \"data.bin\", 2, 3\n .incbin \"data.bin\", 6\n";
    let r = asm198x::assemble_ca65_lang198x_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(
        r.bytes,
        [
            0xA9, 0x01, 0xA9, 0x02, 0xA9, 0x03, 0xA9, 0x04, 0xA9, 0x05, 0xA5, 0x10, 0x12, 0x13,
            0x14, 0x16, 0x17,
        ]
    );
    assert_eq!(r.files, ["main.s", "a.s", "b.s"]);

    // A macro and a scope cross a file boundary, as ca65's text does.
    let loader = MemoryLoader::new().text("uses.s", " two\n.endscope\n");
    let src = ".macro two\n nop\n nop\n.endmacro\n.scope s\n .include \"uses.s\"\n";
    let r = asm198x::assemble_ca65_lang198x_files(src, "main.s", &loader).expect("assembles");
    assert_eq!(r.bytes, [0xEA, 0xEA]);
}

#[test]
fn the_walk_names_the_file_a_problem_is_in() {
    use asm198x::source::MemoryLoader;
    let loader = MemoryLoader::new()
        .text("a.s", " .include \"b.s\"\n")
        .text("b.s", " lda #$01\n frob $10\n");
    let e = asm198x::assemble_ca65_lang198x_files(" .include \"a.s\"\n", "main.s", &loader)
        .expect_err("frob is unknown");
    let span = e.error.span.as_ref().expect("a span");
    assert_eq!(span.line, 2, "line 2 of b.s");
    let table = e.source_map.file_table();
    assert_eq!(
        table.get(span.file.0 as usize).map(String::as_str),
        Some("b.s")
    );
    assert_eq!(
        e.source_map.include_chain(span.file),
        vec![("a.s".to_string(), 1), ("main.s".to_string(), 1)]
    );

    // A missing file, and a window outside the data, are refused at the
    // line that asked.
    let loader = MemoryLoader::new().binary("data.bin", vec![1, 2]);
    for (src, line) in [
        (" nop\n .include \"nothere.s\"\n", 2),
        (" nop\n .incbin \"nothere.bin\"\n", 2),
        (" .incbin \"data.bin\", 3\n", 1),
        (" .incbin \"data.bin\", 1, 2\n", 1),
    ] {
        let e = asm198x::assemble_ca65_lang198x_files(src, "main.s", &loader).expect_err(src);
        let span = e.error.span.as_ref().expect("a span");
        assert_eq!(span.line, line, "{src:?}: {}", e.error);
    }

    // One file reads no others, and says where to go instead.
    let e = asm198x::assemble_ca65_lang198x(".incbin \"data.bin\"\n").expect_err("one file");
    assert!(e.to_string().contains("multi-file"), "{e}");
}

/// Asserts that the walk and the old reader give `source` the same image,
/// debug record, memory map and warnings, linked as the NES curriculum is.
fn same_outputs(name: &str, source: &str, loader: &asm198x::source::MemoryLoader) {
    let old = asm198x::assemble_ca65_files_debug(source, name, loader);
    let walk = asm198x::assemble_ca65_nes_lang198x_files_debug(source, name, loader);
    let ((old, old_info), (walk, walk_info)) = match (old, walk) {
        (Ok(old), Ok(walk)) => (old, walk),
        (old, walk) => panic!(
            "{name}: one refused\nold: {:?}\nwalk: {:?}",
            old.err().map(|e| e.error),
            walk.err().map(|e| e.error)
        ),
    };
    assert_eq!(walk.bytes, old.bytes, "{name}: bytes");
    assert_eq!(walk.origin, None, "{name}: a linked image has no origin");
    assert_eq!(walk.areas, old.areas, "{name}: memory map");
    assert_eq!(walk.warnings, old.warnings, "{name}: warnings");
    assert_eq!(walk.section_debug, old.section_debug, "{name}: sections");
    assert_eq!(
        walk_info.to_ndjson(),
        old_info.to_ndjson(),
        "{name}: debug record"
    );
}

/// The walk's debug record, memory map and image match the old reader's,
/// which the Debug198x fixtures pin, and the walk writes those fixtures'
/// sidecars exactly.
#[test]
fn the_walk_describes_its_link_as_the_old_reader_does() {
    use asm198x::source::MemoryLoader;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/debug198x");
    let read = |name: &str| std::fs::read_to_string(format!("{dir}/{name}")).expect(name);
    let loader = MemoryLoader::new().text("6502-nes-multifile.inc", read("6502-nes-multifile.inc"));
    // Each tool version is the same, so the files compare whole.
    let normal = |text: &str| {
        let at = text.find("\"tool_version\":\"").expect("a tool version") + 16;
        let end = at + text[at..].find('"').expect("its end");
        format!("{}*{}", &text[..at], &text[end..])
    };
    for name in ["6502-nes", "6502-nes-multifile"] {
        let source = read(&format!("{name}.s"));
        same_outputs(name, &source, &loader);
        let (_, info) =
            asm198x::assemble_ca65_nes_lang198x_files_debug(&source, &format!("{name}.s"), &loader)
                .expect(name);
        assert_eq!(
            normal(&info.to_ndjson()),
            normal(&read(&format!("{name}.debug198x"))),
            "{name}: the committed sidecar"
        );
    }

    let none = MemoryLoader::new();
    for (name, source) in [
        // A segment the program names but puts nothing in is not part of
        // the record, and `CODE` is not either when nothing goes in it.
        (
            "unused segments",
            ".segment \"BSS\"\n.segment \"ZEROPAGE\"\nv: .res 2\n.segment \"VECTORS\"\n.word 0, 0, 0\n",
        ),
        // `.res` with a value and `.align` make lines; the header has none.
        (
            "fill",
            ".segment \"HEADER\"\n.byte \"NES\", $1a\n.segment \"CODE\"\nnop\n.align 4\n.res 3, $ea\n.res 2\n",
        ),
        // A macro's lines are its call's; a repeat's are its own.
        (
            "expansions",
            ".macro two\n nop\n lda #1\n.endmacro\n.segment \"CODE\"\nstart: two\n.repeat 2\n inx\n.endrepeat\n",
        ),
        // `.org` gives a section an address of its own, which carries on
        // across lines. ca65 keeps it across a change of segment, which
        // the walk does not do yet (6e).
        (
            "org",
            ".segment \"VECTORS\"\n.word 0, 0, 0\n.segment \"CODE\"\nfirst: nop\n.org $3000\nthere: nop\nnext: lda there\n",
        ),
        (
            "org twice",
            ".segment \"CODE\"\n.org $3000\nfirst: nop\n.org $4000\nsecond: nop\n.org $4001\nthird: nop\n",
        ),
        (
            "org in RAM",
            ".segment \"BSS\"\n.org $0400\nbuffer: .res 16\n",
        ),
        // Labels in each scope, cheap locals and constants.
        (
            "names",
            "SPEED = 3\n.segment \"CODE\"\nreset: lda #SPEED\n@wait: bne @wait\n: jmp :-\n.proc inner\nhere: rts\n.endproc\n",
        ),
    ] {
        same_outputs(name, source, &none);
    }
}

/// The walk checks `asm198x:` budgets against its link: an area's free
/// space, and a routine's cycles within its own segment.
#[test]
fn the_walk_checks_budgets_against_its_link() {
    use asm198x::source::MemoryLoader;
    let none = MemoryLoader::new();
    let walk = |source: &str| asm198x::assemble_ca65_nes_lang198x_files(source, "main.s", &none);
    assert!(walk("; asm198x: free(PRG) >= $E0\n.segment \"CODE\"\n.res 16\n").is_ok());
    let e =
        walk("; asm198x: free(PRG) >= $8000\n.segment \"CODE\"\n.res 16\n").expect_err("no room");
    assert!(e.error.to_string().contains("space budget"), "{}", e.error);
    let e = walk("; asm198x: free(ROM) >= 1\nnop\n").expect_err("no such area");
    assert!(e.error.to_string().contains("PRG"), "{}", e.error);
    let routine = ".segment \"CODE\"\nwork: lda #1\n rts\n.segment \"VECTORS\"\n.word 1\n";
    let fits = walk(&format!("; asm198x: cycles(work) <= 8\n{routine}"));
    assert!(fits.is_ok(), "{:?}", fits.err().map(|e| e.error));
    let e = walk(&format!("; asm198x: cycles(work) <= 7\n{routine}")).expect_err("too slow");
    assert!(e.error.to_string().contains("cycle budget"), "{}", e.error);

    // A budget in an included file is named as that file's.
    let loader = MemoryLoader::new().text("part.inc", "; asm198x: cycles(work) <= 1\n");
    let e = asm198x::assemble_ca65_nes_lang198x_files(
        &format!(".include \"part.inc\"\n{routine}"),
        "main.s",
        &loader,
    )
    .expect_err("too slow");
    let file = e.error.span.as_ref().expect("a span").file;
    assert_eq!(e.source_map.file_table()[file.0 as usize], "part.inc");
}

/// The walk links under a project's own configuration, as the old reader
/// does, with the same record and map.
#[test]
fn the_walk_links_under_a_configuration() {
    use asm198x::source::MemoryLoader;
    let cfg = "MEMORY { ZP: start=$0, size=$100, type=rw, file=\"\"; \
               MAIN: start=$1000, size=$1000, file=%O; }\n\
               SEGMENTS { ZEROPAGE: load=ZP, type=zp; CODE: load=MAIN, type=ro; \
               RODATA: load=MAIN, type=ro; }\n";
    let source = ".segment \"ZEROPAGE\"\nv: .res 1\n.segment \"RODATA\"\ntable: .byte 1, 2\n\
                  .segment \"CODE\"\nstart: lda table\n sta v\n.org $2000\nfar: nop\n";
    let none = MemoryLoader::new();
    let (old, old_info) =
        asm198x::assemble_ca65_files_debug_with_config(source, "main.s", &none, cfg).expect("old");
    let (walk, walk_info) =
        asm198x::assemble_ca65_lang198x_files_debug_with_config(source, "main.s", &none, cfg)
            .expect("walk");
    assert_eq!(walk.bytes, old.bytes);
    assert_eq!(walk.areas, old.areas);
    assert_eq!(walk.section_debug, old.section_debug);
    assert_eq!(walk_info.to_ndjson(), old_info.to_ndjson());
    let plain = asm198x::assemble_ca65_lang198x_files_with_config(source, "main.s", &none, cfg)
        .expect("walk");
    assert_eq!(plain, walk);
}

/// The walk warns where ca65 V2.19 and ld65 warn, as measured: of a name
/// exported to the zero page whose address size is absolute, at whichever
/// of its declaration and definition comes second, and of a `bss` or `zp`
/// segment that holds a byte other than zero, once, at the first line that
/// wrote one.
#[test]
fn the_walk_warns_as_ca65_and_ld65_do() {
    let warned = |source: &str| {
        let result = asm198x::assemble_ca65_nes_lang198x(source).expect(source);
        result
            .warnings
            .iter()
            .map(|w| (w.line, w.message.clone()))
            .collect::<Vec<_>>()
    };
    let zp = |name: &str| format!("Symbol `{name}` is absolute but exported zeropage");
    let code = ".segment \"CODE\"\n";
    for (source, expected) in [
        (".exportzp foo\nfoo: .byte 1\n", vec![(3, zp("foo"))]),
        ("foo: .byte 1\n.exportzp foo\n", vec![(3, zp("foo"))]),
        (".globalzp foo\nfoo: .byte 1\n", vec![(3, zp("foo"))]),
        (
            ".exportzp foo, bar\nfoo: .byte 1\nbar: .byte 2\n",
            vec![(3, zp("foo")), (4, zp("bar"))],
        ),
        // A constant warns by its size, as a label does.
        ("K = $1234\n.exportzp K\n", vec![(3, zp("K"))]),
        ("K = 7\n.exportzp K\n", vec![]),
        (".org $10\n.exportzp foo\nfoo: .byte 1\n", vec![]),
        (".global foo\nfoo: .byte 1\n", vec![]),
    ] {
        assert_eq!(warned(&format!("{code}{source}")), expected, "{source}");
    }
    assert!(
        warned(".segment \"ZEROPAGE\"\nfoo: .res 1\n.segment \"CODE\"\n.exportzp foo\n").is_empty()
    );

    let bss = |name: &str| format!("Segment `{name}` with type `bss` contains initialized data");
    for (source, expected) in [
        (
            ".segment \"BSS\"\n.byte 0\n.byte 2\n.byte 3\n",
            vec![(3, bss("BSS"))],
        ),
        (".segment \"BSS\"\n.byte 0\n.res 2\n.align 4\n", vec![]),
        (
            ".segment \"ZEROPAGE\"\nv: .byte 5\n.segment \"BSS\"\n.res 2, 1\n",
            vec![(2, bss("ZEROPAGE")), (4, bss("BSS"))],
        ),
        (".segment \"BSS\"\n.word w\nw:\n", vec![(2, bss("BSS"))]),
    ] {
        assert_eq!(warned(source), expected, "{source}");
    }

    // A warning in an included file names that file.
    let loader = asm198x::source::MemoryLoader::new().text("part.inc", ".byte 0\n.byte 1\n");
    let r = asm198x::assemble_ca65_nes_lang198x_files(
        ".segment \"BSS\"\n.include \"part.inc\"\n",
        "main.s",
        &loader,
    )
    .expect("assembles");
    let warning = r.warnings.first().expect("a warning");
    assert_eq!(warning.line, 2);
    assert_eq!(r.files[warning.file.0 as usize], "part.inc");
}

/// In ca65 V2.19, `.org` stays in force across a change of segment until
/// `.reloc`, counting every segment's bytes. Measured with ca65 and ld65
/// under this configuration.
#[test]
fn the_walk_keeps_org_across_segments() {
    use asm198x::source::MemoryLoader;
    let cfg = "MEMORY { ZP: start=$0000, size=$100, type=rw, file=\"\"; \
               MAIN: start=$1000, size=$1000, fill=no, file=%O; }\n\
               SEGMENTS { ZEROPAGE: load=ZP, type=zp; CODE: load=MAIN, type=ro; \
               RODATA: load=MAIN, type=ro; }\n";
    let none = MemoryLoader::new();
    for (source, bytes) in [
        (
            ".org $2000\n.segment \"RODATA\"\n.byte 1\n.segment \"CODE\"\n.word *\n",
            vec![0x01, 0x20, 0x01],
        ),
        (
            ".segment \"CODE\"\n.org $2000\nnop\n.segment \"RODATA\"\nl: .word l\n\
             .segment \"CODE\"\n.word *\n",
            vec![0xEA, 0x03, 0x20, 0x01, 0x20],
        ),
        (
            ".segment \"CODE\"\n.org $2000\nnop\n.reloc\n.segment \"RODATA\"\nl: .word l\n",
            vec![0xEA, 0x01, 0x10],
        ),
    ] {
        let r = asm198x::assemble_ca65_lang198x_files_with_config(source, "main.s", &none, cfg)
            .unwrap_or_else(|e| panic!("{source}: {}", e.error));
        assert_eq!(r.bytes, bytes, "{source}");
    }
}

/// ca65 V2.19 reports an error and carries on with the next line, then, if
/// it found none, each name used but never defined, and then, if those are
/// sound, each one-byte value that does not fit. A line it cannot read is
/// abandoned, so it gets one error. Measured on V2.19.
#[test]
fn the_walk_reports_every_error() {
    let errors = |source: &str| {
        let error = asm198x::assemble_ca65_lang198x(source).expect_err(source);
        error
            .all()
            .map(|e| (e.line, e.message.clone()))
            .collect::<Vec<_>>()
    };
    let lines = |source: &str| {
        errors(source)
            .iter()
            .map(|(line, _)| *line)
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(".byte 1,\n.byte 2\nnop\nlda (\n"), [1, 4, 4]);
    assert_eq!(lines("foo: nop\nfoo: nop\nbar: nop\nbar: nop\n"), [2, 4]);
    assert_eq!(
        lines(".struct P\nf1 .byte 0\nf2 .res 0\n.endstruct\n"),
        [2, 3]
    );
    assert_eq!(
        errors(".byte nosuch\n.byte other\n.byte nosuch\n"),
        [
            (1, "Symbol `nosuch` is undefined".to_owned()),
            (2, "Symbol `other` is undefined".to_owned()),
        ]
    );
    assert_eq!(lines("lab: .byte lab\n.byte lab+1\n"), [1, 2]);
    // A line's error stops the checks at the end, and an undefined name
    // stops the check of the data.
    assert_eq!(lines(".byte 1,\n.byte nosuch\n"), [1]);
    assert_eq!(lines("foo: nop\nfoo: nop\nlab: .byte lab\n"), [2]);
    assert_eq!(lines(".byte nosuch\nlab: .byte lab\n"), [1]);
}
