//! Every assembly sample in the book is assembled by the real binary.
//!
//! Docs-site plan R2. A sample that stops assembling fails the build rather
//! than sitting on the site telling people something untrue — which is the
//! failure mode documentation has by default, and the one this repo has already
//! been bitten by: the CLI reference drifted from the binary in two ways for
//! months because nothing executed it.
//!
//! A sample is marked in the source, using the same comment idiom the generated
//! blocks use:
//!
//! ```markdown
//! <!-- sample: acme -->
//! ```asm
//! * = $c000
//!         rts
//! ```
//! ```
//!
//! `<!-- sample: acme, refuses -->` inverts it: the block **must not**
//! assemble. Documentation that shows a diagnostic is making a claim about the
//! assembler too, and an example of a failure that quietly started working
//! would be as wrong as one that stopped.
//!
//! This lives in `tests/` rather than in the docs xtask so it runs on every
//! `cargo test`, not only in the job that builds the book. A broken sample is
//! then a local failure, seconds after it is written.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One marked sample, with where it came from so a failure can be found.
#[derive(Debug)]
struct Sample {
    page: String,
    line: usize,
    dialect: String,
    /// What the assembler must complain about, if the sample is meant to be
    /// refused at all. `Some("")` asserts only that it fails.
    refuses: Option<String>,
    source: String,
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
}

fn book_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/book/src")
}

/// Pull every marked sample out of one page.
fn samples_in(page: &str, text: &str) -> Result<Vec<Sample>, String> {
    const MARKER: &str = "<!-- sample:";
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix(MARKER) else {
            continue;
        };
        let spec = rest
            .trim_end()
            .strip_suffix("-->")
            .ok_or_else(|| format!("{page}:{}: unterminated sample marker", i + 1))?
            .trim();

        // Split once: a `refuses:` message may itself contain commas, and it
        // is the last thing on the line.
        let (dialect, options) = match spec.split_once(',') {
            Some((d, rest)) => (d.trim(), rest.trim()),
            None => (spec, ""),
        };
        if dialect.is_empty() {
            return Err(format!("{page}:{}: sample marker names no dialect", i + 1));
        }
        let refuses = match options {
            "" => None,
            "refuses" => Some(String::new()),
            other => match other.strip_prefix("refuses:") {
                Some(message) if !message.trim().is_empty() => Some(message.trim().to_string()),
                Some(_) => {
                    return Err(format!(
                        "{page}:{}: `refuses:` with no message — drop the colon to \
                         assert only that it fails",
                        i + 1
                    ));
                }
                None => {
                    return Err(format!(
                        "{page}:{}: unknown sample option `{other}` \
                         (`refuses`, or `refuses: <what it must say>`)",
                        i + 1
                    ));
                }
            },
        };

        // The fence must open on the next line: anything else means the marker
        // has drifted from the block it describes, and a marker pointing at
        // nothing would silently check nothing.
        let fence = lines
            .get(i + 1)
            .ok_or_else(|| format!("{page}:{}: sample marker ends the file", i + 1))?;
        if !fence.trim_start().starts_with("```") {
            return Err(format!(
                "{page}:{}: a sample marker must sit directly above its code fence",
                i + 1
            ));
        }
        let close = lines
            .iter()
            .enumerate()
            .skip(i + 2)
            .find(|(_, l)| l.trim_start().starts_with("```"))
            .map(|(n, _)| n)
            .ok_or_else(|| format!("{page}:{}: sample fence is never closed", i + 2))?;

        found.push(Sample {
            page: page.to_string(),
            line: i + 1,
            dialect: dialect.to_string(),
            refuses,
            source: lines[i + 2..close].join("\n") + "\n",
        });
    }
    Ok(found)
}

/// Assemble one sample, returning the assembler's complaint if it failed.
fn assemble(sample: &Sample, index: usize) -> Result<(), String> {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("asm198x-book-sample-{index}.s"));
    let out = dir.join(format!("asm198x-book-sample-{index}.bin"));
    std::fs::write(&src, &sample.source).expect("write sample");

    let result = bin()
        .args(["--dialect", &sample.dialect])
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out);

    if result.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).trim().to_string())
    }
}

#[test]
fn every_book_sample_assembles() {
    let src = book_src();
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", src.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    pages.sort();

    let mut samples = Vec::new();
    for page in &pages {
        let name = page
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = std::fs::read_to_string(page).expect("read page");
        samples.extend(samples_in(&name, &text).unwrap_or_else(|e| panic!("{e}")));
    }

    let mut failures = Vec::new();
    for (index, sample) in samples.iter().enumerate() {
        let outcome = assemble(sample, index);
        let origin = format!("{}:{} ({})", sample.page, sample.line, sample.dialect);
        match (sample.refuses.as_deref(), outcome) {
            // Assembles, and was meant to.
            (None, Ok(())) => {}
            (None, Err(complaint)) => {
                failures.push(format!("{origin} no longer assembles: {complaint}"));
            }
            (Some(_), Ok(())) => failures.push(format!(
                "{origin} is documented as refused, but assembled — the page \
                 shows a diagnostic the assembler no longer produces"
            )),
            // Refused as documented, and saying what the page claims it says.
            // Checking the message matters: a page that quotes a diagnostic is
            // making a claim about the output too, and "it failed somehow" is
            // not the claim the reader is relying on.
            (Some(""), Err(_)) => {}
            (Some(expected), Err(complaint)) => {
                if !complaint.contains(expected) {
                    failures.push(format!(
                        "{origin} is refused, but not the way the page says.\n\
                         \x20      page expects: {expected}\n\
                         \x20      assembler said: {complaint}"
                    ));
                }
            }
        }
    }

    eprintln!(
        "checked {} book sample(s) across {} page(s)",
        samples.len(),
        pages.len()
    );

    assert!(
        failures.is_empty(),
        "{} book sample(s) disagree with the assembler:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );

    // A book whose samples all stopped being marked would pass everything above
    // while checking nothing, which is the shape of green that hides a
    // regression. The same assertion guards the verdict corpus, for the same
    // reason.
    assert!(
        samples.len() >= 2,
        "no book samples were found — either the book has none, or the marker \
         convention changed and every lookup now misses"
    );
}

#[cfg(test)]
mod tests {
    use super::samples_in;

    #[test]
    fn a_marker_is_read_with_its_options() {
        let page = "<!-- sample: acme -->\n```asm\nrts\n```\n\
                    <!-- sample: pasmo, refuses -->\n```asm\nfrob\n```\n";
        let found = samples_in("t.md", page).expect("parses");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].dialect, "acme");
        assert_eq!(found[0].refuses, None);
        assert_eq!(found[0].source, "rts\n");
        assert_eq!(found[1].dialect, "pasmo");
        assert_eq!(found[1].refuses.as_deref(), Some(""));
    }

    /// A marker that has drifted off its block is an error, not a skip. Left
    /// silent it would check nothing while looking like it checked something.
    #[test]
    fn a_marker_must_sit_on_its_fence() {
        let stray = "<!-- sample: acme -->\n\nSome prose.\n\n```asm\nrts\n```\n";
        let err = samples_in("t.md", stray).expect_err("the marker is adrift");
        assert!(err.contains("directly above"), "{err}");
    }

    /// A quoted message travels with the marker, commas and all — it is the
    /// last thing on the line precisely so it can contain them.
    #[test]
    fn a_refusal_may_name_what_it_must_say() {
        let page = "<!-- sample: acme, refuses: value 4660 does not fit, at all -->\n\
                    ```asm\nlda #$1234\n```\n";
        let found = samples_in("t.md", page).expect("parses");
        assert_eq!(
            found[0].refuses.as_deref(),
            Some("value 4660 does not fit, at all")
        );
    }

    #[test]
    fn an_unknown_option_is_refused() {
        let odd = "<!-- sample: acme, sometimes -->\n```asm\nrts\n```\n";
        let err = samples_in("t.md", odd).expect_err("unknown option");
        assert!(err.contains("sometimes"), "{err}");
    }
}
