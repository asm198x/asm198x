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
const NOT_YET: &[(&str, &str)] = &[
    ("LOCAL label with a parameter", "LOCAL, in step 2 (scopes)"),
    ("LOCAL label, invoked twice", "LOCAL, in step 2 (scopes)"),
];

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
                    continue;
                };
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
    eprintln!("pasmo verdicts through Lang198x: {checked} checked");
    assert!(checked > 1000, "only {checked} verdicts were checked");
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
const PROGRAMS_NOT_YET: &[(&str, &str)] = &[(
    "org_ignores_rest",
    "a parser rule: PasmoNext never reads past ORG's operand",
)];

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
                    program.files = std::mem::take(&mut program.files).binary(name, bytes);
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

#[test]
fn measured_programs_match_or_say_they_need_three_passes() {
    let programs = programs();
    assert!(programs.len() >= 88, "the fixture has {}", programs.len());
    let mut failures = Vec::new();
    for program in &programs {
        // Programs run with PasmoNext's options, such as `--pass3`, wait for
        // the step that takes those options.
        if !program.options.is_empty() {
            continue;
        }
        let result =
            asm198x::assemble_pasmo_lang198x_files(&program.source, "t.asm", &program.files, true);
        let named = PROGRAMS_NOT_YET
            .iter()
            .any(|(name, _)| *name == program.name);
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
