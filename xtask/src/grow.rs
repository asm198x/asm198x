//! `cargo xtask grow` — re-arbitrate the corpus with the reference tools.
//!
//! Growth needs the reference assemblers, so it runs where they are: a
//! maintainer's machine, not CI. The suites already record what they arbitrate,
//! so this is not new machinery — it is the discoverable front door to it, and
//! the place the follow-up steps are remembered.
//!
//! What it does:
//!
//! 1. runs every reference-arbitrated suite live, each verdict replacing the
//!    corpus's answer for its case;
//! 2. on an unfiltered run, prunes verdicts from tool versions no longer in use
//!    (`ops/arbiter/identities`), for the tools this run had — after a version
//!    bump, that is what finishes regenerating the corpus;
//! 3. refreshes the coverage stamp, so the diff carries both the facts and
//!    what they were worth;
//! 4. shows what changed, because the output of a growth run is a **diff to
//!    review**, not a number to trust.
//!
//! Recording is idempotent, so running it when nothing changed leaves the
//! corpus byte-identical and the diff empty.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, ExitCode};

use crate::corpus;

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

    // A filtered run re-records only part of the corpus, so it cannot tell a
    // stale verdict from one it simply did not reach.
    if filter.is_none() {
        match corpus::prune(repo, &present_tools()) {
            Ok(removed) => {
                let total: usize = removed.values().sum();
                if total > 0 {
                    println!("\npruned {total} verdict(s) no current tool re-recorded:");
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
    "asl",
    "p2bin",
    "vasmm68k_mot",
];

/// Reference tools not on PATH. Absence is ordinary, not an error — it just
/// bounds what a run can grow.
fn missing_tools() -> Vec<&'static str> {
    TOOLS.iter().filter(|t| !is_present(t)).copied().collect()
}

/// Reference tools on PATH, by the name verdicts record them under.
pub fn present_tools() -> HashSet<String> {
    TOOLS
        .iter()
        .filter(|t| is_present(t))
        .map(|t| (*t).to_string())
        .collect()
}

fn is_present(tool: &str) -> bool {
    Command::new(tool).output().is_ok()
}
