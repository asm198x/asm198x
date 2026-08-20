//! `cargo xtask <command>` — repository automation.
//!
//! Not part of the shipped workspace: no binary anyone installs, excluded from
//! `default-members`, untagged by release-plz and invisible to `dist`. It exists
//! so accounting over the verdict corpus (#61) can be run and checked the same
//! way locally and in CI, without a shell script that drifts from what the
//! corpus actually holds.

mod coverage;
mod docs;
mod grow;
mod instructions;
mod ledger;
mod machines;
mod supersede;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("coverage") => run_coverage(&args[1..]),
        Some("grow") => grow::run(&repo(), args.get(1).map(String::as_str)),
        Some("supersede") => match (args.get(1), args.get(2)) {
            (Some(tag), Some(reason)) => match supersede::run(&repo(), tag, reason, &args[3..]) {
                Ok(retired) if retired.is_empty() => {
                    eprintln!("xtask: no live verdict matches `{tag}`");
                    ExitCode::FAILURE
                }
                Ok(retired) => {
                    println!("retired {} verdict(s) tagged `{tag}`:", retired.len());
                    for case in &retired {
                        println!("  {case}");
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("xtask supersede: {e}");
                    ExitCode::FAILURE
                }
            },
            _ => {
                eprintln!(
                    "usage: cargo xtask supersede <divergence-tag> <reason> [filter...]\n\
                     \n\
                     Each filter must match the verdict's dialect or case, so one\n\
                     issue's divergences can be retired as they close rather than\n\
                     all at once."
                );
                ExitCode::FAILURE
            }
        },
        Some("docs") => run_docs(&args[1..]),
        Some("machines") => match machines::check(&repo()) {
            Ok(differences) if differences.is_empty() => {
                println!("the copied CPU→machine mapping agrees with the library");
                ExitCode::SUCCESS
            }
            Ok(differences) => {
                eprintln!(
                    "{} CPU(s) disagree with the reference library:\n  {}\n\n\
                     The library is the source; update `isa::machines::MACHINES` \
                     to match it.\n\n\
                     This reads the library's working tree, so check which \
                     branch it is on before editing: a wholesale slug \
                     difference usually means the checkout predates a rename \
                     rather than that the copy is wrong.",
                    differences.len(),
                    differences.join("\n  ")
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("xtask machines: {e}");
                ExitCode::FAILURE
            }
        },
        Some("ledger") => {
            print!("{}", ledger::render(&repo()));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`\n\n{}", usage());
            ExitCode::FAILURE
        }
        None => {
            println!("{}", usage());
            ExitCode::SUCCESS
        }
    }
}

fn usage() -> String {
    "cargo xtask — Asm198x repository automation\n\n\
     commands:\n\
     \x20 coverage            report arbitration coverage over the verdict corpus\n\
     \x20 coverage --check    fail if any CPU's coverage fell below the stamp\n\
     \x20 coverage --write    refresh the stamp\n\
     \x20 ledger              print the conformance ledger for this revision\n\
     \x20 grow [filter]       arbitrate what is not yet recorded (needs the tools)\n\
     \x20 supersede <tag> <why>  retire the verdicts carrying a divergence tag\n\
     \x20 docs                regenerate the book's generated blocks\n\
     \x20 docs --check        fail if any generated block is stale\n\
     \x20 machines            check the copied CPU→machine mapping against\n\
     \x20                     the umbrella reference library (needs it present)\n"
        .to_string()
}

fn run_docs(args: &[String]) -> ExitCode {
    let check = args.iter().any(|a| a == "--check");
    let report = match docs::run(&repo(), check) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("xtask docs: {e}");
            return ExitCode::FAILURE;
        }
    };

    if report.stale.is_empty() && report.stale_pages.is_empty() {
        println!(
            "{} generated block(s) across {} page(s), and {} generated page(s), are current",
            report.blocks, report.scanned, report.pages
        );
        return ExitCode::SUCCESS;
    }

    let stale: Vec<String> = report
        .stale_pages
        .iter()
        .cloned()
        .chain(report.stale.iter().cloned())
        .collect();

    if check {
        eprintln!(
            "{} page(s) are stale:\n  {}\n\n\
             The book carries generated data next to prose, and this one no \
             longer matches what the binary produces. Regenerate with `cargo \
             xtask docs` and commit the result — do not edit inside the \
             markers, the next run overwrites it.",
            stale.len(),
            stale.join("\n  ")
        );
        return ExitCode::FAILURE;
    }

    println!("regenerated {} page(s):", stale.len());
    for page in &stale {
        println!("  {page}");
    }
    ExitCode::SUCCESS
}

/// The repository root: this crate's manifest directory's parent.
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn run_coverage(args: &[String]) -> ExitCode {
    let repo = repo();
    let report = coverage::compute(&repo);
    let rendered = coverage::render_stamp(&report);
    let path = coverage::stamp_path(&repo);

    if args.iter().any(|a| a == "--write") {
        if let Err(e) = std::fs::write(&path, &rendered) {
            eprintln!("xtask: could not write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", path.display());
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--check") {
        let Ok(existing) = std::fs::read_to_string(&path) else {
            eprintln!(
                "xtask: no coverage stamp at {} — create it with `cargo xtask coverage --write`",
                path.display()
            );
            return ExitCode::FAILURE;
        };
        let regressions = coverage::regressions(&report, &coverage::parse_stamp(&existing));
        if regressions.is_empty() {
            println!("arbitration coverage holds against the stamp");
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "arbitration coverage fell — {} CPU(s) now arbitrate less than the stamp records:",
            regressions.len()
        );
        for drop in &regressions {
            eprintln!(
                "  {}: {}.{}% -> {}.{}%",
                drop.cpu,
                drop.was / 10,
                drop.was % 10,
                drop.now / 10,
                drop.now % 10
            );
        }
        eprintln!(
            "\nSomething stopped being arbitrated. Either recover it with a live \
             recording run, or accept the loss deliberately: `cargo xtask coverage \
             --write`, and say in the commit which cases went and why. The stamp is \
             the record of that debt, so it must not move silently."
        );
        return ExitCode::FAILURE;
    }

    print!("{rendered}");
    ExitCode::SUCCESS
}
