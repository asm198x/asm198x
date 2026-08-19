//! `cargo xtask grow` — one command to arbitrate what is not yet recorded.
//!
//! Growth needs the reference assemblers, so it runs where they are: a
//! maintainer's machine, not CI. The suites already record what they arbitrate,
//! so this is not new machinery — it is the discoverable front door to it, and
//! the place the follow-up steps are remembered.
//!
//! What it does:
//!
//! 1. runs every reference-arbitrated suite live, which appends any verdict the
//!    corpus does not already hold;
//! 2. refreshes the coverage stamp, so the diff carries both the new facts and
//!    what they were worth;
//! 3. shows what changed, because the output of a growth run is a **diff to
//!    review**, not a number to trust.
//!
//! Recording is idempotent, so running it when nothing is new leaves the corpus
//! byte-identical and the diff empty. That makes it safe to run habitually,
//! which is the only way a corpus stays current.

use std::path::Path;
use std::process::{Command, ExitCode};

/// Run a growth pass. `filter` narrows to one suite or CPU when given.
pub fn run(repo: &Path, filter: Option<&str>) -> ExitCode {
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

    println!("arbitrating live — this needs the reference assemblers and takes a while");
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(_) => {
            eprintln!(
                "\nxtask grow: the live suites failed. Verdicts already recorded are \
                 kept — the corpus is append-only — but fix the failure before \
                 committing, or the diff carries a known-bad state."
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("xtask grow: could not run the suites: {e}");
            return ExitCode::FAILURE;
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
        "\nReview the diff before committing. Every added line is an observation \
         of a real tool; if one looks wrong, it is the tool or the harness that \
         needs explaining, not the line that needs editing."
    );
    ExitCode::SUCCESS
}

/// Reference tools not on PATH. Absence is ordinary, not an error — it just
/// bounds what a run can grow.
fn missing_tools() -> Vec<&'static str> {
    const TOOLS: &[&str] = &[
        "acme",
        "ca65",
        "ld65",
        "pasmo",
        "sjasmplus",
        "lwasm",
        "rgbasm",
        "rgblink",
        "asl",
        "p2bin",
        "vasmm68k_mot",
    ];
    TOOLS
        .iter()
        .filter(|t| Command::new(t).output().is_err())
        .copied()
        .collect()
}
