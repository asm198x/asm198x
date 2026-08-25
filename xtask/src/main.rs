//! `cargo xtask <command>` — repository automation.
//!
//! Not part of the shipped workspace: no binary anyone installs, excluded from
//! `default-members`, untagged by release-plz and invisible to `dist`. It exists
//! so accounting over the verdict corpus (#61) can be run and checked the same
//! way locally and in CI, without a shell script that drifts from what the
//! corpus actually holds.

mod changelog;
mod compare;
mod coverage;
mod dialect_pages;
mod divergences;
mod docs;
mod evidence;
mod grow;
mod includes;
mod instructions;
mod ledger;
mod machines;
mod nav;
mod parity;
mod search;
mod supersede;
mod surface;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("coverage") => run_coverage(&args[1..]),
        Some("parity") => run_parity(&args[1..]),
        Some("grow") => grow::run(&repo(), args.get(1).map(String::as_str)),
        // The scope form: `--cpu <CPU> --suite <suite>... <reason>`. A listing
        // change strands every verdict keyed on the text it used to emit, and
        // those carry no divergence tag to select on (#214).
        Some("supersede") if args.iter().any(|a| a == "--cpu") => run_supersede_scope(&args[1..]),
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
                     \x20      cargo xtask supersede --cpu <CPU> --suite <suite>... <reason>\n\
                     \n\
                     Each filter must match the verdict's dialect or case, so one\n\
                     issue's divergences can be retired as they close rather than\n\
                     all at once.\n\
                     \n\
                     The --cpu form retires by scope instead, for a change to the\n\
                     text we hand the reference — a listing edit strands every\n\
                     verdict keyed on the old text, and those carry no tag.\n\
                     Suites: form, sweep-chunk, probe, fuzz, curriculum."
                );
                ExitCode::FAILURE
            }
        },
        Some("changelog") => match changelog::check(&repo()) {
            Ok(summary) => {
                println!("{summary}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask changelog: {e}");
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
        Some("surface") => {
            let write = args.iter().any(|a| a == "--write");
            print!("{}", surface::run(&repo(), write));
            ExitCode::SUCCESS
        }
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
     \x20 surface             report how much of each reference's own vocabulary we take\n\
     \x20 surface --write     refresh the surface stamp\n\
     \x20 coverage            report arbitration coverage over the verdict corpus\n\
     \x20 coverage --check    fail if any CPU's coverage fell below the stamp\n\
     \x20 coverage --no-debt  fail if any shortfall is still owed (pre-tag gate)\n\
     \x20 coverage --delta <f>  report movement against a base stamp (never fails)\n\
     \x20 coverage --write    refresh the stamp\n\
     \x20 ledger              print the conformance ledger for this revision\n\
     \x20 grow [filter]       arbitrate what is not yet recorded (needs the tools)\n\
     \x20 supersede <tag> <why>  retire the verdicts carrying a divergence tag\n\
     \x20 changelog           fail if the newest release entry still reads like a draft\n\
     \x20 docs                regenerate the book's generated blocks\n\
     \x20 docs --check        fail if any generated block is stale\n\
     \x20 machines            check the copied CPU→machine mapping against\n\
     \x20                     the umbrella reference library (needs it present)\n"
        .to_string()
}

fn run_docs(args: &[String]) -> ExitCode {
    let check = args.iter().any(|a| a == "--check");
    let repo = repo();

    let report = match docs::run(&repo, check) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("xtask docs: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Read after `docs::run`, not before. SUMMARY.md carries a generated block
    // of its own, so checking the nav first means a page that moved fails the
    // integrity check on the stale block and blocks the very run that would
    // regenerate it.
    //
    // The dead-link gate reads this same parse, so a chapter with no file fails
    // here — where the source lives — rather than in the site build. This is
    // mdBook's `create-missing = false`, kept after mdBook was withdrawn.
    let sections = match nav::read(&repo) {
        Ok(sections) => sections,
        Err(e) => {
            eprintln!("xtask docs: {e}");
            return ExitCode::FAILURE;
        }
    };
    match nav::integrity(&repo, &sections) {
        Ok(problems) if !problems.is_empty() => {
            eprintln!("the documentation nav does not match the pages:");
            for p in &problems {
                eprintln!("  {p}");
            }
            return ExitCode::FAILURE;
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("xtask docs: {e}");
            return ExitCode::FAILURE;
        }
    }

    let nav_path = nav::nav_path(&repo);
    let rendered_nav = nav::render(&sections);
    let nav_stale = std::fs::read_to_string(&nav_path).ok().as_deref() != Some(&rendered_nav);
    if nav_stale
        && !check
        && let Err(e) = std::fs::write(&nav_path, &rendered_nav)
    {
        eprintln!("xtask docs: could not write {}: {e}", nav_path.display());
        return ExitCode::FAILURE;
    }

    // The search index, alongside the nav and for the same reason: mdBook
    // carried search, its withdrawal left no replacement, and a reference of
    // twenty-one generated instruction pages that nobody can search is worse
    // than no reference. Generated here and read there, exactly as the nav is.
    let index_path = crate::search::index_path(&repo);
    let rendered_index = match crate::search::render(&repo) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("xtask docs: {e}");
            return ExitCode::FAILURE;
        }
    };
    let index_stale = std::fs::read_to_string(&index_path).ok().as_deref() != Some(&rendered_index);
    if index_stale
        && !check
        && let Err(e) = std::fs::write(&index_path, &rendered_index)
    {
        eprintln!("xtask docs: could not write {}: {e}", index_path.display());
        return ExitCode::FAILURE;
    }
    if index_stale && check {
        eprintln!(
            "docs/book/search.json is stale.\n\n\
             The site's search reads it, so a heading added here without it \
             reaching the index is a section nobody can find. Run `cargo xtask \
             docs`."
        );
        return ExitCode::FAILURE;
    }

    if nav_stale && check {
        eprintln!(
            "docs/book/nav.json is stale.\n\n\
             The site renders its navigation from it, so a SUMMARY.md change \
             that does not reach it moves the pages without moving the nav. \
             Regenerate with `cargo xtask docs` and commit the result."
        );
        return ExitCode::FAILURE;
    }

    if report.stale.is_empty() && report.stale_pages.is_empty() {
        if nav_stale {
            println!("wrote {}", nav_path.display());
        }
        println!(
            "{} generated block(s) across {} page(s), and {} generated page(s), are current; \
             the nav lists {} page(s), all present",
            report.blocks,
            report.scanned,
            report.pages,
            nav::slugs(&sections).len()
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

    if index_stale && !check {
        println!("wrote {}", index_path.display());
    }
    if nav_stale {
        println!("wrote {}", nav_path.display());
    }
    println!("regenerated {} page(s):", stale.len());
    for page in &stale {
        println!("  {page}");
    }
    ExitCode::SUCCESS
}

/// The repository root: this crate's manifest directory's parent.
/// `supersede --cpu <CPU> --suite <suite>... <reason>`.
fn run_supersede_scope(args: &[String]) -> ExitCode {
    let mut cpu: Option<&str> = None;
    let mut suites: Vec<verdict_corpus::Suite> = Vec::new();
    let mut reason: Option<&str> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--cpu" => cpu = it.next().map(String::as_str),
            "--suite" => match it.next().map(|s| parse_suite(s)) {
                Some(Some(s)) => suites.push(s),
                Some(None) => {
                    eprintln!("xtask supersede: unknown suite");
                    return ExitCode::FAILURE;
                }
                None => break,
            },
            other => reason = Some(other),
        }
    }
    let (Some(cpu), Some(reason)) = (cpu, reason) else {
        eprintln!("usage: cargo xtask supersede --cpu <CPU> --suite <suite>... <reason>");
        return ExitCode::FAILURE;
    };
    if suites.is_empty() {
        eprintln!(
            "xtask supersede: --suite is required.\n\
             \n\
             Without it this would retire the CPU's curriculum and probe verdicts\n\
             too, which no listing change touches — and retiring a true fact\n\
             because it shares a file with a stale one is how a corpus shrinks."
        );
        return ExitCode::FAILURE;
    }
    match supersede::run_by_scope(&repo(), cpu, &suites, reason) {
        Ok(retired) if retired.is_empty() => {
            eprintln!("xtask: no live verdict matches that scope");
            ExitCode::FAILURE
        }
        Ok(retired) => {
            println!("retired {} verdict(s) for {cpu}:", retired.len());
            for case in &retired {
                println!("  {case}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xtask supersede: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_suite(s: &str) -> Option<verdict_corpus::Suite> {
    use verdict_corpus::Suite;
    Some(match s {
        "form" => Suite::Form,
        "sweep-chunk" => Suite::SweepChunk,
        "probe" => Suite::Probe,
        "fuzz" => Suite::Fuzz,
        "curriculum" => Suite::Curriculum,
        _ => return None,
    })
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn run_parity(args: &[String]) -> ExitCode {
    let repo = repo();
    let report = parity::compute(&repo);
    let path = parity::data_path(&repo);

    if args.iter().any(|a| a == "--write") {
        if let Err(e) = std::fs::write(&path, parity::render(&report)) {
            eprintln!("xtask: could not write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", path.display());
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--check") {
        let Ok(existing) = std::fs::read_to_string(&path) else {
            eprintln!(
                "xtask: no parity data at {} — create it with `cargo xtask parity --write`",
                path.display()
            );
            return ExitCode::FAILURE;
        };
        // The pin describes which curriculum the figures below are about, so
        // it is checked first: figures that hold against the wrong revision
        // hold against nothing.
        let pin_ok = match parity::verify_pin(&repo) {
            parity::PinVerdict::Matches => {
                println!("the curriculum checkout is at the recorded pin, on the recorded date");
                true
            }
            parity::PinVerdict::Unverifiable(why) => {
                // Not a failure — a copied tree has no git to ask. But it is
                // said out loud, because a check that goes quiet when it
                // cannot run is indistinguishable from one that passed.
                println!("cannot verify the pin here: {why}");
                true
            }
            parity::PinVerdict::Wrong(lines) => {
                eprintln!("the recorded curriculum pin does not describe this checkout:");
                for line in &lines {
                    eprintln!("  {line}");
                }
                false
            }
        };
        let regressions = parity::regressions(&report, &existing);
        if regressions.is_empty() && pin_ok {
            println!("curriculum parity holds against the committed figures");
            return ExitCode::SUCCESS;
        }
        if !regressions.is_empty() {
            eprintln!("curriculum parity fell:");
            for line in &regressions {
                eprintln!("  {line}");
            }
        }
        return ExitCode::FAILURE;
    }

    print!("{}", parity::render_summary(&report));
    ExitCode::SUCCESS
}

fn run_coverage(args: &[String]) -> ExitCode {
    let repo = repo();

    // The pre-tag release gate. Separate from `--check` because it asks a
    // different question: not "is every shortfall declared" but "is any of them
    // still owed". A release carrying a settled shortfall is fine; one carrying
    // a de-arbitrated form nobody has recovered is what this stops.
    if args.iter().any(|a| a == "--no-debt") {
        let accepted_path = coverage::accepted_path(&repo);
        let accepted = std::fs::read_to_string(&accepted_path).unwrap_or_default();
        let parsed = coverage::parse_accepted(&accepted);
        let owed = coverage::owed(&parsed);
        if owed.is_empty() {
            println!("no arbitration debt is owed — the release may tag");
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "{} shortfall(s) are still owed, so this must not tag:",
            owed.len()
        );
        for (cpu, a) in &owed {
            eprintln!("  {cpu}: {} row(s) — {}", a.rows, a.reason);
        }
        eprintln!(
            "\nThese rows were de-arbitrated and are meant to come back. Recover \
             them with a growth run and drop the entry, or — if they are not \
             coming back — say so, by rewriting the reason as the decision it \
             really is. A release is the deadline for that answer, which is why \
             the entry was allowed to merge in the first place."
        );
        return ExitCode::FAILURE;
    }

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

    // Reporting, never a gate. R12 asks CI to *report* the delta; the gate on a
    // drop is the acknowledgment in `coverage.accepted` and the release ratchet.
    // A base that cannot be read is a shallow clone, not a fault, so this says
    // so and succeeds.
    if let Some(i) = args.iter().position(|a| a == "--delta") {
        let Some(base_path) = args.get(i + 1) else {
            eprintln!("usage: cargo xtask coverage --delta <base coverage.stamp>");
            return ExitCode::FAILURE;
        };
        match std::fs::read_to_string(base_path) {
            Ok(base) => print!(
                "{}",
                coverage::render_delta(&report, &coverage::parse_stamp(&base))
            ),
            Err(e) => {
                println!("arbitration coverage: no base stamp at {base_path} ({e}), so no delta");
            }
        }
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
        // Every shortfall must say why it exists and how big it is, or a form
        // that quietly stopped arbitrating is indistinguishable from one we
        // decided never to reach.
        let accepted_path = coverage::accepted_path(&repo);
        let accepted = std::fs::read_to_string(&accepted_path).unwrap_or_default();
        let parsed = coverage::parse_accepted(&accepted);
        let unaccepted = coverage::unaccepted(&report, &parsed);
        // The status first, so a passing run still shows what it held to.
        print!("{}", coverage::render_status(&report, &parsed));
        let moved = coverage::drift(&report, &coverage::parse_stamp(&existing));
        if moved.is_empty() && unaccepted.is_empty() {
            println!(
                "arbitration coverage holds against the stamp, and every shortfall is declared"
            );
            return ExitCode::SUCCESS;
        }
        if !unaccepted.is_empty() {
            eprintln!(
                "{} shortfall(s) do not match what {} declares:",
                unaccepted.len(),
                accepted_path.display()
            );
            for u in &unaccepted {
                match u {
                    coverage::Unaccepted::Undeclared { cpu, rows } => {
                        eprintln!("  {cpu}: {rows} row(s) unarbitrated, and nothing says why")
                    }
                    coverage::Unaccepted::Wider {
                        cpu,
                        declared,
                        rows,
                    } => eprintln!(
                        "  {cpu}: {rows} row(s) unarbitrated, {declared} declared — {} beyond it",
                        rows - declared
                    ),
                    coverage::Unaccepted::Stale { cpu, declared } => eprintln!(
                        "  {cpu}: arbitrates everything, yet still declares {declared} row(s)"
                    ),
                    coverage::Unaccepted::Unarbitrated { cpu, rows } => eprintln!(
                        "  {cpu}: arbitrates nothing — {rows} row(s), and no reference \
                         has checked one of them"
                    ),
                }
            }
            eprintln!(
                "\nA shortfall is either a decision or a debt, and the number cannot \
                 tell them apart. Declare it — the CPU, the row count, and why it will \
                 never be reached — or recover the rows with a growth run. An entry \
                 that outlived its reason comes out."
            );
            if unaccepted
                .iter()
                .any(|u| matches!(u, coverage::Unaccepted::Unarbitrated { .. }))
            {
                eprintln!(
                    "\nA CPU arbitrating nothing is the one case the file cannot \
                     excuse. Run `cargo xtask grow <CPU>` and land its verdicts with \
                     the spec: a CPU nothing has checked is a compatibility claim \
                     with no evidence behind it, and this project's whole argument \
                     is the evidence."
                );
            }
            if moved.is_empty() {
                return ExitCode::FAILURE;
            }
            eprintln!();
        }
        let (fell, rose): (Vec<_>, Vec<_>) = moved.iter().partition(|m| m.fell());
        let list = |moves: &[&coverage::Move]| {
            for m in moves {
                eprintln!(
                    "  {}: {}.{}% -> {}.{}%",
                    m.cpu,
                    m.was / 10,
                    m.was % 10,
                    m.now / 10,
                    m.now % 10
                );
            }
        };
        if !fell.is_empty() {
            eprintln!(
                "arbitration coverage fell — {} CPU(s) now arbitrate less than the stamp records:",
                fell.len()
            );
            list(&fell);
            eprintln!(
                "\nSomething stopped being arbitrated. Either recover it with a live \
                 recording run, or accept the loss deliberately: `cargo xtask coverage \
                 --write`, and say in the commit which cases went and why. The stamp is \
                 the record of that debt, so it must not move silently."
            );
        }
        if !rose.is_empty() {
            if !fell.is_empty() {
                eprintln!();
            }
            eprintln!(
                "the coverage stamp is behind — {} CPU(s) now arbitrate more than it records:",
                rose.len()
            );
            list(&rose);
            eprintln!(
                "\nRefresh it with `cargo xtask coverage --write`, in the change that \
                 earned the rise. A stamp that lags is a ratchet that has let go: while \
                 it reads the lower number, a regression back down to that number passes \
                 unnoticed. The Z8000's stamp sat at 0 of 271 through three merged pull \
                 requests for exactly this reason."
            );
        }
        return ExitCode::FAILURE;
    }

    print!("{rendered}");
    ExitCode::SUCCESS
}
