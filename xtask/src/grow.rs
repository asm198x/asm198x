//! `cargo xtask grow` — re-arbitrate the corpus with the reference tools.
//!
//! Growth needs the reference assemblers, so it runs where they are: a
//! maintainer's machine, not CI. The suites already record what they arbitrate,
//! so this is not new machinery — it is the discoverable front door to it, and
//! the place the follow-up steps are remembered.
//!
//! What it does:
//!
//! 1. refuses to start if an installed reference tool reports a version line
//!    other than the one `ops/arbiter/identities` lists, so a run can only
//!    record the versions the repository says it uses;
//! 2. runs every reference-arbitrated suite live, each verdict replacing the
//!    corpus's answer for its case, and each flush noting in a manifest what it
//!    recorded;
//! 3. on an unfiltered run, prunes what that manifest shows was superseded —
//!    old-version verdicts in a group the run re-recorded, and curriculum
//!    verdicts for a program's previous digest. After a version bump, that is
//!    what finishes regenerating the corpus. Nothing the run did not reach is
//!    touched;
//! 4. refreshes the coverage stamp, so the diff carries both the facts and
//!    what they were worth;
//! 5. shows what changed, because the output of a growth run is a **diff to
//!    review**, not a number to trust.
//!
//! Recording is idempotent, so running it when nothing changed leaves the
//! corpus byte-identical and the diff empty.

use std::path::Path;
use std::process::{Command, ExitCode};

use crate::corpus;

/// Run a growth pass. `filter` narrows to one suite or CPU when given.
pub fn run(repo: &Path, filter: Option<&str>) -> ExitCode {
    match version_mismatches(repo) {
        Ok(wrong) if wrong.is_empty() => {}
        Ok(wrong) => {
            eprintln!(
                "xtask grow: these installed tools are not the versions \
                 ops/arbiter/identities lists, so growth would record (and prune \
                 towards) versions the repository does not say it uses:\n"
            );
            for w in &wrong {
                eprintln!("  {w}");
            }
            eprintln!(
                "\nInstall the listed versions (the arbiter image has them), or bump \
                 the pins and identities together first."
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("xtask grow: {e}");
            return ExitCode::FAILURE;
        }
    }

    let manifest = repo.join("target").join("verdict-growth-manifest.ndjson");
    if let Some(parent) = manifest.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("xtask grow: {}: {e}", parent.display());
        return ExitCode::FAILURE;
    }
    let _ = std::fs::remove_file(&manifest);

    let missing = missing_tools();
    if !missing.is_empty() {
        eprintln!(
            "xtask grow: these reference tools are absent, so whatever they \
             arbitrate cannot grow in this run:\n  {}\n",
            missing.join(" ")
        );
        eprintln!("That is not an error — a partial growth run records what it can.\n");
    }

    let mut cmd = Command::new("cargo");
    cmd.current_dir(repo).args(["test", "--workspace"]);
    if let Some(filter) = filter {
        cmd.arg(filter);
    }
    cmd.args(["--", "--ignored", "--nocapture"]);
    cmd.env(verdict_corpus::MANIFEST_ENV, &manifest);

    println!("arbitrating live — this needs the reference assemblers and takes a while");
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(_) => {
            eprintln!(
                "\nxtask grow: the live suites failed. Verdicts recorded before the \
                 failure are written, but nothing is pruned; fix the failure before \
                 committing, or the diff carries a known-bad state."
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("xtask grow: could not run the suites: {e}");
            return ExitCode::FAILURE;
        }
    }

    // A filtered run may reach only part of a group (several tests record
    // probes for one file and tool), so only an unfiltered run prunes.
    if filter.is_none() {
        let recorded = match verdict_corpus::read_manifest(&manifest) {
            Ok(recorded) => recorded,
            Err(e) => {
                eprintln!("xtask grow: {}: {e}", manifest.display());
                return ExitCode::FAILURE;
            }
        };
        match corpus::prune(repo, &recorded) {
            Ok(removed) => {
                let total: usize = removed.values().sum();
                if total > 0 {
                    println!("\npruned {total} verdict(s) this run superseded:");
                    for (tool, n) in &removed {
                        println!("  {tool}: {n}");
                    }
                }
            }
            Err(e) => {
                eprintln!("xtask grow: could not prune: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    let stamp = Command::new("cargo")
        .current_dir(repo)
        .args(["xtask", "coverage", "--write"])
        .status();
    if !stamp.map(|s| s.success()).unwrap_or(false) {
        eprintln!("xtask grow: could not refresh the coverage stamp");
        return ExitCode::FAILURE;
    }

    println!("\nwhat grew:");
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["diff", "--stat", "--", "crates/asm198x/tests/verdicts"])
        .status();
    println!(
        "\nReview the diff before committing. Every changed line is an observation \
         of a real tool: a moved answer is either our bug or the reference changing \
         behaviour, and the harness or the tool needs explaining, not the line."
    );
    ExitCode::SUCCESS
}

/// The reference executables growth drives.
const TOOLS: &[&str] = &[
    "acme",
    "ca65",
    "ld65",
    "pasmo",
    "sjasmplus",
    "lwasm",
    "rgbasm",
    "rgblink",
    "rgbfix",
    "asl",
    "p2bin",
    "vasmm68k_mot",
];

/// Reference tools not on PATH. Absence is ordinary, not an error — it just
/// bounds what a run can grow.
fn missing_tools() -> Vec<&'static str> {
    TOOLS.iter().filter(|t| !is_present(t)).copied().collect()
}

fn is_present(tool: &str) -> bool {
    Command::new(tool).output().is_ok()
}

/// The version-line probe for a tool, the same one `ops/arbiter/verify.sh`
/// and the live suites use.
fn probe_flag(tool: &str) -> &'static str {
    match tool {
        "asl" | "p2bin" | "pasmo" | "vasmm68k_mot" => "-v",
        _ => "--version",
    }
}

/// Installed tools whose output has no line exactly equal to their entry in
/// `ops/arbiter/identities`. An absent tool is not a mismatch: it records
/// nothing.
fn version_mismatches(repo: &Path) -> Result<Vec<String>, String> {
    let mut wrong = Vec::new();
    for (tool, want) in corpus::current_identities(repo)? {
        let Ok(out) = Command::new(&tool).arg(probe_flag(&tool)).output() else {
            continue;
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !text.lines().any(|line| line.trim() == want) {
            let got = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            wrong.push(format!(
                "{tool}: expected `{want}`, reports `{}`",
                got.trim_end()
            ));
        }
    }
    Ok(wrong)
}
