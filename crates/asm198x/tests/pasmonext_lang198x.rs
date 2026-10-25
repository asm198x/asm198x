//! The migration gate's second path: PasmoNext source read by Lang198x's
//! front end (`lang198x-z80-asm`) and laid out by the PasmoNext walk.
//!
//! Every committed `pasmo` and `pasmonext` verdict is replayed through it, as
//! `verdict_replay.rs` replays them through the shared Z80 core. A verdict may
//! differ only if it is named in [`NOT_YET`] with the plan step that covers
//! it, and a named verdict that passes fails the test, so the list only
//! shrinks. The new path becomes the default once the list is empty (step 6
//! of Lang198x's PasmoNext plan).

mod support;

use std::path::Path;

use support::verdicts::{code_samples_root, corpus_path, parse_curriculum_key, sha256_hex};
use verdict_corpus::{Corpus, Outcome};

/// Verdicts the new path does not match yet, each with what it waits for.
const NOT_YET: &[(&str, &str)] = &[];

/// What the new path made of `source`: its bytes, or `None` if it rejected
/// it.
fn assemble(source: &str, z80n: bool) -> Option<Vec<u8>> {
    asm198x::assemble_pasmo_lang198x(source, z80n)
        .ok()
        .map(|result| result.bytes)
}

#[test]
fn every_pasmo_verdict_matches_through_lang198x() {
    let corpus = Corpus::read(&corpus_path("z80")).expect("the Z80 corpus reads");
    let mut checked = 0;
    // Curriculum verdicts need the Code198x checkout at the recorded source,
    // so they are counted apart: a run that reached none of them must say so.
    let mut curriculum_checked = 0;
    let mut curriculum_skipped = Vec::new();
    let mut failures = Vec::new();
    let mut passed_while_named = Vec::new();
    for verdict in corpus.verdicts() {
        let z80n = match verdict.dialect.as_str() {
            "pasmo" => false,
            "pasmonext" => true,
            _ => continue,
        };
        let (expected, ours) = match &verdict.outcome {
            Outcome::Bytes { .. } => (verdict.outcome.bytes(), assemble(&verdict.source, z80n)),
            Outcome::Rejected { .. } => (None, assemble(&verdict.source, z80n)),
            Outcome::Digest { digest } => {
                let Some(ours) = curriculum(&verdict.source, z80n) else {
                    curriculum_skipped.push(verdict.case.clone());
                    continue;
                };
                curriculum_checked += 1;
                (
                    Some(digest.clone().into_bytes()),
                    ours.map(|b| sha256_hex(&b).into_bytes()),
                )
            }
            Outcome::Divergence { .. } => continue,
        };
        checked += 1;
        let matches = expected == ours;
        let named = NOT_YET.iter().any(|(case, _)| *case == verdict.case);
        match (matches, named) {
            (false, false) => failures.push(verdict.case.clone()),
            (true, true) => passed_while_named.push(verdict.case.clone()),
            _ => {}
        }
    }
    eprintln!(
        "pasmo verdicts through Lang198x: {checked} checked, {curriculum_checked} of them \
         curriculum, {} curriculum not reached",
        curriculum_skipped.len()
    );
    assert!(checked > 1000, "only {checked} verdicts were checked");
    // CI names the checkout, at the pin the verdicts were recorded from, so
    // there every curriculum verdict must be reached. A developer's sibling
    // checkout may have moved on, so there the count is only reported.
    if std::env::var_os("ASM198X_CODE_SAMPLES").is_some() {
        assert!(
            curriculum_checked > 0 && curriculum_skipped.is_empty(),
            "curriculum verdicts not reached with ASM198X_CODE_SAMPLES set: \
             {curriculum_skipped:#?}"
        );
    }
    assert!(
        failures.is_empty(),
        "differ from PasmoNext through Lang198x: {failures:#?}"
    );
    assert!(
        passed_while_named.is_empty(),
        "now match, so take them off NOT_YET: {passed_while_named:#?}"
    );
}

/// A curriculum verdict's program, assembled by the new path, if the
/// curriculum is checked out at the recorded source. `Some(None)` is a
/// rejection.
fn curriculum(key: &str, z80n: bool) -> Option<Option<Vec<u8>>> {
    let root = code_samples_root()?;
    let (relpath, _, source_digest) = parse_curriculum_key(key)?;
    let path = root.join(relpath);
    let source = std::fs::read_to_string(&path).ok()?;
    if sha256_hex(source.as_bytes()) != source_digest {
        return None;
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name()?.to_str()?;
    let loader = asm198x::source::FsLoader::new(dir, Vec::new());
    Some(
        asm198x::assemble_pasmo_lang198x_files(&source, name, &loader, z80n)
            .ok()
            .map(|result| result.bytes),
    )
}

/// Measured programs the new path does not match yet, each with what it
/// waits for. Three-pass programs are not listed: they are found by the
/// fixture's `--- passes 3` and must be reported as such.
const PROGRAMS_NOT_YET: &[(&str, &str)] = &[];

/// Measured programs where PasmoNext 0.1.3 has an obvious bug, which the new
/// path fixes rather than copies, under 198x's
/// `reference-bugs-are-fixed-not-copied.md`: each name, why, and what the new
/// path must do instead, its bytes or `None` for a rejection. One that stops
/// deviating fails the test.
const PROGRAMS_DEVIATING: &[(&str, &str, Option<&str>)] = &[
    (
        "org_ignores_rest",
        "PasmoNext never reads past ORG's operand, so `org 5 junk` drops `junk`",
        None,
    ),
    // The manual: the `$` in a name "are ignored". PasmoNext's `stripdollar`
    // removes the wrong characters, so it reads `ab$c` as `ac`, `ab$` as `a`.
    (
        "name_dollar_trailing",
        "PasmoNext reads `ab$` as `a`, so `jp ab` finds nothing",
        Some("00C30000"),
    ),
    (
        "name_dollar_inside_read_as_manual",
        "PasmoNext reads `ab$c` as `ac`, so `jp abc` finds nothing",
        Some("00C30000"),
    ),
    (
        "name_dollar_inside_pasmonext_reading",
        "PasmoNext reads `ab$c` as `ac`, so `jp ac` reaches it",
        None,
    ),
    // The reserve counter `RB` and `RW` take from.
    (
        "rb_without_rsset",
        "PasmoNext never resets the counter between passes, so pass 2 gives \
         `aa` a new value and ends in a phase error",
        Some("0001"),
    ),
    (
        "rsset_symbol",
        "PasmoNext reads a name after RSSET as 0",
        None,
    ),
    (
        "rsset_expression",
        "PasmoNext reads `2+3` after RSSET as 2",
        None,
    ),
    (
        "rsset_junk_after",
        "PasmoNext drops whatever follows RSSET's number",
        None,
    ),
    (
        "stack_junk_after",
        "PasmoNext drops whatever follows STACK's value",
        None,
    ),
    // Structures.
    (
        "struct_ends_junk",
        "PasmoNext drops whatever follows ENDS",
        None,
    ),
    (
        "struct_name_junk",
        "PasmoNext drops whatever follows STRUCT's name",
        None,
    ),
    (
        "struct_db_trailing_comma",
        "PasmoNext counts the missing item after `db 1,` as a byte",
        None,
    ),
    (
        "struct_db_no_items",
        "PasmoNext counts `fa db`, with no item, as a byte",
        None,
    ),
];

/// One measured program: its source, the files it reads, its options, and
/// what PasmoNext did with it.
struct Program {
    name: String,
    source: String,
    files: asm198x::source::MemoryLoader,
    options: Vec<String>,
    bytes: Option<String>,
    three_passes: bool,
}

fn programs() -> Vec<Program> {
    let text = include_str!("fixtures/pasmonext-0.1.3-programs.txt");
    text.split("\n=== ")
        .skip(1)
        .map(|case| {
            let mut lines = case.lines();
            let mut program = Program {
                name: lines.next().unwrap_or_default().to_owned(),
                source: String::new(),
                files: asm198x::source::MemoryLoader::new(),
                options: Vec::new(),
                bytes: None,
                three_passes: false,
            };
            let mut outcome = false;
            for line in lines {
                if let Some(file) = line.strip_prefix("+++ ") {
                    let (name, hex) = file.split_once(' ').unwrap_or((file, ""));
                    let bytes = verdict_corpus::decode_hex(hex).expect("hex");
                    // A file may be read by INCBIN or INCLUDE, so it is
                    // offered both ways.
                    let mut files = std::mem::take(&mut program.files);
                    if let Ok(text) = String::from_utf8(bytes.clone()) {
                        files = files.text(name, text);
                    }
                    program.files = files.binary(name, bytes);
                } else if let Some(options) = line.strip_prefix("!!! ") {
                    program.options = options.split(' ').map(str::to_owned).collect();
                } else if let Some(hex) = line.strip_prefix("--- bytes ") {
                    program.bytes = Some(hex.to_ascii_uppercase());
                    outcome = true;
                } else if line == "--- passes 3" {
                    program.three_passes = true;
                } else if line.starts_with("---") {
                    outcome = true;
                } else if !outcome {
                    program.source.push_str(line);
                    program.source.push('\n');
                }
            }
            program
        })
        .collect()
}

/// PasmoNext's command-line `options` as the new path takes them, or `None`
/// if one is not taken yet.
fn options(options: &[String]) -> Option<asm198x::PasmoNextOptions> {
    let mut taken = asm198x::PasmoNextOptions::default();
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--nocase" => taken.names_in_any_case = true,
            "--alocal" => taken.automatic_locals = true,
            "--equ" | "-E" => taken.predefined.push(options.next()?.clone()),
            _ => return None,
        }
    }
    Some(taken)
}

#[test]
fn measured_programs_match_or_say_they_need_three_passes() {
    let programs = programs();
    assert!(programs.len() >= 88, "the fixture has {}", programs.len());
    let mut failures = Vec::new();
    for program in &programs {
        // Programs run with options the new path does not take yet, such as
        // `--pass3`, wait for the step that takes them.
        let Some(options) = options(&program.options) else {
            continue;
        };
        let result = asm198x::assemble_pasmo_lang198x_files_with(
            &program.source,
            "t.asm",
            &program.files,
            true,
            &options,
        );
        let named = PROGRAMS_NOT_YET
            .iter()
            .any(|(name, _)| *name == program.name);
        if let Some((_, why, expected)) = PROGRAMS_DEVIATING
            .iter()
            .find(|(name, _, _)| *name == program.name)
        {
            let ours = result.as_ref().ok().map(|r| {
                r.bytes
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<String>()
            });
            if ours.as_deref() != *expected {
                failures.push(format!(
                    "{}: gives {ours:?}, but should deviate to {expected:?}: {why}",
                    program.name
                ));
            }
            continue;
        }
        let ok = if program.three_passes {
            matches!(&result, Err(e) if e.error.message.contains("third pass"))
        } else {
            let ours = result.as_ref().ok().map(|r| {
                r.bytes
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<String>()
            });
            ours == program.bytes
        };
        if ok == named {
            failures.push(format!(
                "{}: {}",
                program.name,
                if named {
                    "matches; take it off PROGRAMS_NOT_YET"
                } else {
                    "differs"
                }
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn includes_resolve_from_the_folder_pasmonext_runs_in() {
    // Laid out like the pattern library's `mistakes/` programs, which
    // include `caller.inc` from the pattern folder they are run from.
    // PasmoNext 0.1.3, run there on `mistakes/broken.asm`, writes 3E 07 00.
    let dir = support::scratch::dir("pasmonext-run-folder");
    std::fs::create_dir_all(dir.join("mistakes")).expect("mistakes folder");
    std::fs::write(dir.join("caller.inc"), " ld a,7\n").expect("caller.inc");
    std::fs::write(dir.join("data.bin"), [0]).expect("data.bin");
    let source = " org 0\n include \"caller.inc\"\n incbin \"data.bin\"\n";
    std::fs::write(dir.join("mistakes/broken.asm"), source).expect("broken.asm");
    let loader = asm198x::source::FsLoader::new(&*dir, Vec::new());
    let result =
        asm198x::assemble_pasmo_lang198x_files(source, "mistakes/broken.asm", &loader, false)
            .expect("assembles");
    assert_eq!(result.bytes, [0x3E, 0x07, 0x00]);
}
