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
//! # Output blocks
//!
//! A source sample can be named after the file it is written as, and a block
//! below it can then claim what the binary prints for it:
//!
//! ```markdown
//! <!-- sample: acme, file: fill.a, refuses -->
//! ```asm
//! * = $c000
//!         !byte $1234
//! ```
//!
//! <!-- output: fill.a, json -->
//! ```json
//! [ … ]
//! ```
//! ```
//!
//! Four modes, each running the real binary in the sample's own directory so a
//! path in the output is the name the page shows:
//!
//! | Mode | Runs | Compares |
//! |---|---|---|
//! | `output` | `asm198x --dialect <d> <file>` | stderr, the human diagnostic |
//! | `json` | the same, `--message-format=json` | stdout, as parsed JSON |
//! | `fmt` | `asm198x fmt --dialect <d> <file>` | stdout |
//! | `disasm` | assembles, then `asm198x disasm` over the bytes | stdout |
//!
//! An output block refers to the **nearest sample above it on the same page**,
//! and names that sample's file — so the reference is what the page already
//! shows a reader, and a block that drifts away from its sample fails rather
//! than silently checking the wrong one.
//!
//! A mode may carry the arguments its output depends on —
//! `<!-- output: fill.a, disasm --org 0xc000 -->` — so the page documenting a
//! flag can show what the flag does.
//!
//! `json` compares the parsed document rather than the text, so the page can
//! show the payload indented while the binary emits it on one line. Every other
//! mode compares text, since layout is the thing being documented.
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
    /// The file name the sample is written as. An output block names it, and
    /// the binary runs against it, so a path the page shows is the path the
    /// output carries.
    file: String,
    /// What the assembler must complain about, if the sample is meant to be
    /// refused at all. `Some("")` asserts only that it fails.
    refuses: Option<String>,
    source: String,
}

/// What the binary must print for the sample above it.
#[derive(Debug)]
struct Expectation {
    page: String,
    line: usize,
    /// The sample's file name, as the marker spells it.
    file: String,
    mode: Mode,
    /// Extra arguments the marker carries, for a mode whose output depends on
    /// one — `disasm --org 0xc000` renders branch targets differently, and the
    /// page documenting `--org` should be able to show that.
    args: Vec<String>,
    body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Mode {
    /// The human diagnostic, on stderr.
    Output,
    /// The `--message-format=json` payload, compared as a parsed document.
    Json,
    /// `asm198x fmt`, on stdout.
    Fmt,
    /// `asm198x disasm` over the assembled bytes, on stdout.
    Disasm,
}

impl Mode {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "output" => Some(Self::Output),
            "json" => Some(Self::Json),
            "fmt" => Some(Self::Fmt),
            "disasm" => Some(Self::Disasm),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Json => "json",
            Self::Fmt => "fmt",
            Self::Disasm => "disasm",
        }
    }

    /// Every mode, so the suite can insist each one is exercised somewhere. A
    /// mode nothing uses is a checker that checks nothing, which is the shape
    /// of green this file exists to refuse.
    const ALL: [Self; 4] = [Self::Output, Self::Json, Self::Fmt, Self::Disasm];
}

/// A block found on a page, in the order the page carries them.
#[derive(Debug)]
enum Block {
    Source(Sample),
    Expect(Expectation),
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
}

fn book_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/book/src")
}

/// The fenced block a marker sits above, and the line its fence opened on.
///
/// A marker that has drifted off its block is an error rather than a skip: left
/// silent it would check nothing while looking like it checked something.
fn fenced(page: &str, lines: &[&str], marker: usize) -> Result<(String, usize), String> {
    let fence = lines
        .get(marker + 1)
        .ok_or_else(|| format!("{page}:{}: marker ends the file", marker + 1))?;
    if !fence.trim_start().starts_with("```") {
        return Err(format!(
            "{page}:{}: a marker must sit directly above its code fence",
            marker + 1
        ));
    }
    let close = lines
        .iter()
        .enumerate()
        .skip(marker + 2)
        .find(|(_, l)| l.trim_start().starts_with("```"))
        .map(|(n, _)| n)
        .ok_or_else(|| format!("{page}:{}: fence is never closed", marker + 2))?;
    Ok((lines[marker + 2..close].join("\n") + "\n", close))
}

/// Read one `sample:` marker's options.
///
/// `refuses` is taken as the rest of the line once seen, because its message
/// may contain commas and is the last thing on the marker. `file:` is what an
/// output block below will name.
fn sample_options(
    page: &str,
    line: usize,
    spec: &str,
) -> Result<(String, String, Option<String>), String> {
    let mut parts = spec.splitn(2, ',');
    let dialect = parts.next().unwrap_or_default().trim().to_string();
    if dialect.is_empty() {
        return Err(format!("{page}:{line}: sample marker names no dialect"));
    }

    let mut file = None;
    let mut refuses = None;
    let mut rest = parts.next().unwrap_or_default().trim().to_string();

    while !rest.is_empty() {
        if rest.starts_with("refuses") {
            // The rest of the line, commas and all.
            refuses = Some(match rest.strip_prefix("refuses:") {
                Some(message) if !message.trim().is_empty() => message.trim().to_string(),
                Some(_) => {
                    return Err(format!(
                        "{page}:{line}: `refuses:` with no message — drop the colon \
                         to assert only that it fails"
                    ));
                }
                None if rest.trim() == "refuses" => String::new(),
                None => {
                    return Err(format!(
                        "{page}:{line}: unknown sample option `{rest}` \
                         (`refuses`, or `refuses: <what it must say>`)"
                    ));
                }
            });
            break;
        }
        let (option, tail) = match rest.split_once(',') {
            Some((o, t)) => (o.trim().to_string(), t.trim().to_string()),
            None => (rest.trim().to_string(), String::new()),
        };
        match option.strip_prefix("file:") {
            Some(name) if !name.trim().is_empty() => file = Some(name.trim().to_string()),
            _ => {
                return Err(format!(
                    "{page}:{line}: unknown sample option `{option}` \
                     (`file: <name>`, `refuses`, or `refuses: <what it must say>`)"
                ));
            }
        }
        rest = tail;
    }

    Ok((dialect, file.unwrap_or_default(), refuses))
}

/// Pull every marked block out of one page, in page order.
fn blocks_in(page: &str, text: &str) -> Result<Vec<Block>, String> {
    const SAMPLE: &str = "<!-- sample:";
    const OUTPUT: &str = "<!-- output:";
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let (marker, is_sample) = match (trimmed.strip_prefix(SAMPLE), trimmed.strip_prefix(OUTPUT))
        {
            (Some(rest), _) => (rest, true),
            (_, Some(rest)) => (rest, false),
            _ => continue,
        };
        let spec = marker
            .trim_end()
            .strip_suffix("-->")
            .ok_or_else(|| format!("{page}:{}: unterminated marker", i + 1))?
            .trim();

        let (body, _) = fenced(page, &lines, i)?;

        if is_sample {
            let (dialect, file, refuses) = sample_options(page, i + 1, spec)?;
            found.push(Block::Source(Sample {
                page: page.to_string(),
                line: i + 1,
                dialect,
                file,
                refuses,
                source: body,
            }));
            continue;
        }

        let (file, mode) = spec.split_once(',').ok_or_else(|| {
            format!(
                "{page}:{}: an output marker names a sample file and a mode \
                 (`<!-- output: fill.a, json -->`)",
                i + 1
            )
        })?;
        let mut words = mode.split_whitespace();
        let named = words.next().unwrap_or_default();
        let mode = Mode::parse(named).ok_or_else(|| {
            format!(
                "{page}:{}: unknown output mode `{named}` (output, json, fmt, disasm)",
                i + 1
            )
        })?;
        found.push(Block::Expect(Expectation {
            page: page.to_string(),
            line: i + 1,
            file: file.trim().to_string(),
            mode,
            args: words.map(str::to_string).collect(),
            body,
        }));
    }
    Ok(found)
}

/// Where one sample is written and run.
///
/// Its own directory, and the binary runs with that as the working directory
/// so the file is named on the command line exactly as the page names it. A
/// path in the output is then the path a reader would see, rather than a
/// temporary directory nobody has.
struct Workspace {
    dir: PathBuf,
    file: String,
}

impl Workspace {
    fn new(sample: &Sample, index: usize) -> Self {
        let dir = std::env::temp_dir().join(format!("asm198x-book-sample-{index}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create sample workspace");
        let file = if sample.file.is_empty() {
            format!("sample-{index}.s")
        } else {
            sample.file.clone()
        };
        std::fs::write(dir.join(&file), &sample.source).expect("write sample");
        Self { dir, file }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        let mut command = bin();
        command.current_dir(&self.dir);
        command.args(args);
        command.output().expect("run asm198x")
    }

    fn binary(&self) -> String {
        format!("{}.bin", self.file)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end().to_string()
}

/// Assemble one sample, returning the assembler's complaint if it failed.
fn assemble(space: &Workspace, dialect: &str) -> Result<(), String> {
    let result = space.run(&["--dialect", dialect, &space.file, "-o", &space.binary()]);
    if result.status.success() {
        Ok(())
    } else {
        Err(text(&result.stderr))
    }
}

/// Run one output block's mode and say how it disagreed, if it did.
fn check(space: &Workspace, sample: &Sample, expected: &Expectation) -> Option<String> {
    let dialect = sample.dialect.as_str();
    let file = space.file.as_str();

    let (got, kind) = match expected.mode {
        Mode::Output => {
            let run = space.run(&["--dialect", dialect, file, "-o", &space.binary()]);
            (text(&run.stderr), "stderr")
        }
        Mode::Json => {
            let run = space.run(&[
                "--dialect",
                dialect,
                "--message-format=json",
                file,
                "-o",
                &space.binary(),
            ]);
            let got = text(&run.stdout);
            // Parsed, not compared as text: the binary emits one line and the
            // page shows the same document indented, which is the readable
            // form and the same claim.
            return match (
                serde_json::from_str::<serde_json::Value>(&got),
                serde_json::from_str::<serde_json::Value>(&expected.body),
            ) {
                (Ok(got_doc), Ok(want)) if got_doc == want => None,
                (Ok(got_doc), Ok(_)) => Some(format!(
                    "the JSON payload differs.\n\
                     \x20      page shows:  {}\n\
                     \x20      binary emits: {got_doc}",
                    expected.body.trim()
                )),
                (Err(e), _) => Some(format!("the binary did not emit JSON: {e}\n{got}")),
                (_, Err(e)) => Some(format!("the page's block is not JSON: {e}")),
            };
        }
        Mode::Fmt => {
            let mut args = vec!["fmt", "--dialect", dialect];
            args.extend(expected.args.iter().map(String::as_str));
            args.push(file);
            let run = space.run(&args);
            (text(&run.stdout), "stdout")
        }
        Mode::Disasm => {
            if let Err(e) = assemble(space, dialect) {
                return Some(format!(
                    "the sample must assemble before disasm reads it: {e}"
                ));
            }
            let binary = space.binary();
            let mut args = vec!["disasm", "--dialect", dialect];
            args.extend(expected.args.iter().map(String::as_str));
            args.push(&binary);
            let run = space.run(&args);
            (text(&run.stdout), "stdout")
        }
    };

    let want = expected.body.trim_end();
    if got == want {
        return None;
    }
    Some(format!(
        "{kind} is not what the page shows.\n\
         \x20      page shows:\n{}\n\
         \x20      binary printed:\n{got}",
        want
    ))
}

#[test]
fn every_book_sample_assembles() {
    let src = book_src();
    // Recursive: the pages sit under `reference/` and `guide/` so that a file's
    // path is the URL it is published at. A flat scan found two pages and no
    // samples at all — the guard at the end of this test is what caught it.
    let pages = verdict_corpus::files::markdown_files(&src)
        .unwrap_or_else(|e| panic!("cannot walk {}: {e}", src.display()));

    let mut blocks = Vec::new();
    for page in &pages {
        // The path from the book root, not just the file name: two pages in
        // different directories can share one, and a failure has to say which.
        let name = page
            .strip_prefix(&src)
            .unwrap_or(page)
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(page).expect("read page");
        blocks.extend(blocks_in(&name, &text).unwrap_or_else(|e| panic!("{e}")));
    }

    let mut failures = Vec::new();
    let mut samples = 0;
    let mut expectations = 0;
    let mut modes_seen: Vec<Mode> = Vec::new();
    // The sample an output block below it refers to, reset at each page so a
    // block can never reach back into the previous one.
    let mut current: Option<(Sample, Workspace)> = None;

    for block in &blocks {
        match block {
            Block::Source(sample) => {
                samples += 1;
                let space = Workspace::new(sample, samples);
                let outcome = assemble(&space, &sample.dialect);
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
                    // Refused as documented, and saying what the page claims it
                    // says. Checking the message matters: a page that quotes a
                    // diagnostic is making a claim about the output too, and
                    // "it failed somehow" is not the claim the reader relies on.
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
                current = Some((
                    Sample {
                        page: sample.page.clone(),
                        line: sample.line,
                        dialect: sample.dialect.clone(),
                        file: sample.file.clone(),
                        refuses: sample.refuses.clone(),
                        source: sample.source.clone(),
                    },
                    space,
                ));
            }
            Block::Expect(expected) => {
                expectations += 1;
                if !modes_seen.contains(&expected.mode) {
                    modes_seen.push(expected.mode);
                }
                let origin = format!(
                    "{}:{} (output: {}, {})",
                    expected.page,
                    expected.line,
                    expected.file,
                    expected.mode.name()
                );
                let Some((sample, space)) = current.as_ref() else {
                    failures.push(format!("{origin} has no sample above it on the page"));
                    continue;
                };
                if sample.page != expected.page {
                    failures.push(format!(
                        "{origin} has no sample above it on the page — the nearest is on {}",
                        sample.page
                    ));
                    continue;
                }
                if sample.file != expected.file {
                    failures.push(format!(
                        "{origin} names `{}`, but the sample above it is `{}` — an \
                         output block describes the sample directly above it",
                        expected.file, sample.file
                    ));
                    continue;
                }
                if let Some(complaint) = check(space, sample, expected) {
                    failures.push(format!("{origin} {complaint}"));
                }
            }
        }
    }

    eprintln!(
        "checked {samples} book sample(s) and {expectations} output block(s) across {} page(s)",
        pages.len()
    );

    assert!(
        failures.is_empty(),
        "{} book block(s) disagree with the assembler:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );

    // A book whose samples all stopped being marked would pass everything above
    // while checking nothing, which is the shape of green that hides a
    // regression. The same assertion guards the verdict corpus, for the same
    // reason.
    assert!(
        samples >= 2,
        "no book samples were found — either the book has none, or the marker \
         convention changed and every lookup now misses"
    );

    // And a mode nothing uses is a checker nobody is checked by. Each one is
    // proven by at least one real page, so growing a mode means using it.
    let missing: Vec<&str> = Mode::ALL
        .iter()
        .filter(|m| !modes_seen.contains(m))
        .map(|m| m.name())
        .collect();
    assert!(
        missing.is_empty(),
        "no page exercises the {} output mode(s): {}",
        missing.len(),
        missing.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::{Block, Mode, blocks_in};

    fn samples(page: &str) -> Vec<super::Sample> {
        blocks_in("t.md", page)
            .expect("parses")
            .into_iter()
            .filter_map(|b| match b {
                Block::Source(s) => Some(s),
                Block::Expect(_) => None,
            })
            .collect()
    }

    #[test]
    fn a_marker_is_read_with_its_options() {
        let page = "<!-- sample: acme -->\n```asm\nrts\n```\n\
                    <!-- sample: pasmo, refuses -->\n```asm\nfrob\n```\n";
        let found = samples(page);
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
        let err = blocks_in("t.md", stray).expect_err("the marker is adrift");
        assert!(err.contains("directly above"), "{err}");
    }

    /// A quoted message travels with the marker, commas and all — it is the
    /// last thing on the line precisely so it can contain them.
    #[test]
    fn a_refusal_may_name_what_it_must_say() {
        let page = "<!-- sample: acme, refuses: value 4660 does not fit, at all -->\n\
                    ```asm\nlda #$1234\n```\n";
        let found = samples(page);
        assert_eq!(
            found[0].refuses.as_deref(),
            Some("value 4660 does not fit, at all")
        );
    }

    #[test]
    fn an_unknown_option_is_refused() {
        let odd = "<!-- sample: acme, sometimes -->\n```asm\nrts\n```\n";
        let err = blocks_in("t.md", odd).expect_err("unknown option");
        assert!(err.contains("sometimes"), "{err}");
    }

    /// `file:` and `refuses:` travel together, in that order, because the
    /// refusal message runs to the end of the line.
    #[test]
    fn a_sample_may_name_its_file_and_its_refusal() {
        let page = "<!-- sample: acme, file: fill.a, refuses: does not fit -->\n\
                    ```asm\nlda #$1234\n```\n";
        let found = samples(page);
        assert_eq!(found[0].file, "fill.a");
        assert_eq!(found[0].refuses.as_deref(), Some("does not fit"));
    }

    #[test]
    fn an_output_block_carries_its_file_and_mode() {
        let page = "<!-- sample: acme, file: fill.a -->\n```asm\nrts\n```\n\
                    <!-- output: fill.a, json -->\n```json\n[]\n```\n";
        let found = blocks_in("t.md", page).expect("parses");
        assert_eq!(found.len(), 2);
        let Block::Expect(expected) = &found[1] else {
            panic!("the second block is an output block");
        };
        assert_eq!(expected.file, "fill.a");
        assert_eq!(expected.mode, Mode::Json);
        assert_eq!(expected.body, "[]\n");
        assert!(expected.args.is_empty());
    }

    /// A mode may carry the arguments its output depends on.
    #[test]
    fn an_output_block_may_pass_arguments() {
        let page = "<!-- sample: acme, file: fill.a -->\n```asm\nrts\n```\n\
                    <!-- output: fill.a, disasm --org 0xc000 -->\n```text\nRTS\n```\n";
        let found = blocks_in("t.md", page).expect("parses");
        let Block::Expect(expected) = &found[1] else {
            panic!("the second block is an output block");
        };
        assert_eq!(expected.mode, Mode::Disasm);
        assert_eq!(expected.args, vec!["--org", "0xc000"]);
    }

    #[test]
    fn an_unknown_output_mode_is_refused() {
        let page = "<!-- output: fill.a, hexdump -->\n```text\n00\n```\n";
        let err = blocks_in("t.md", page).expect_err("unknown mode");
        assert!(err.contains("hexdump"), "{err}");
    }

    /// An output marker with no mode is a marker that would check nothing.
    #[test]
    fn an_output_block_must_name_a_mode() {
        let page = "<!-- output: fill.a -->\n```text\n00\n```\n";
        let err = blocks_in("t.md", page).expect_err("no mode");
        assert!(err.contains("mode"), "{err}");
    }
}
