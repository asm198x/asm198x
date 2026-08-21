//! The evidence paragraphs on `/why`, counted from what they describe.
//!
//! That page's argument is that you can check rather than believe. It was
//! making it with hand-typed figures, and two of them had already drifted: it
//! claimed 5,637 recorded verdicts when the corpus held more, and "nine
//! differences" when nine is the number of recorded *cases* across six tracked
//! differences. A trust page that is quietly wrong about its own evidence
//! argues against itself.
//!
//! So the figures come from the corpus, the committed parity data and the same
//! divergence collection the `/divergences` table is built from. The framing
//! prose around the block stays hand-written — what is generated is the part
//! that can rot.

use std::fmt::Write as _;
use std::path::Path;

/// The evidence block for `why.md`.
#[must_use]
pub fn markdown(repo: &Path) -> String {
    let corpus = crate::divergences::corpus_summary(repo);
    let divergences = crate::divergences::collect(repo);
    let cases: usize = divergences.values().map(|d| d.cases).sum();

    let mut out = String::new();

    // Macro Assembler AS is put last on purpose, so the clause naming it reads
    // as prose rather than as a note attached to whichever tool happens to sort
    // there. Its span is the interesting fact: one arbiter covers the CPUs no
    // dedicated assembler does.
    let mut names: Vec<String> = corpus
        .tools
        .iter()
        .filter(|t| *t != "asl")
        .map(|t| tool_name(t).to_string())
        .collect();
    let asl = corpus.cpus_per_tool.get("asl").copied();
    if asl.is_some() {
        names.push(tool_name("asl").to_string());
    }
    let span = match asl {
        Some(n) => format!(", which covers the {n} less-travelled ones"),
        None => String::new(),
    };
    let _ = writeln!(
        out,
        "**Every CPU is arbitrated against a real assembler.** The differential \
         suites assemble the same source with the reference tool and compare \
         bytes. {} tools across {} instruction sets: {}{span}.\n",
        corpus.tools.len(),
        corpus.cpus,
        list(&names)
    );

    let _ = writeln!(
        out,
        "**What they produced is recorded, not remembered.** {} verdicts, each \
         keyed on the reference tool's own version string, committed to this \
         repository. CI replays every one of them on machines with none of \
         those tools installed, so a change that alters our output fails \
         against what the real assembler did — not against a fixture somebody \
         wrote by hand.\n",
        thousands(corpus.verdicts)
    );

    if let Some(parity) = parity(repo) {
        let _ = writeln!(
            out,
            "**The curriculum assembles byte-identically.** {} assembly sources \
             from the Code198x curriculum, across {}, in {} comparisons. Every \
             one matches the reference tool.\n",
            parity.sources,
            list(&parity.machines),
            parity.comparisons
        );
    }

    let _ = writeln!(
        out,
        "**Where we differ is published.** {} tracked difference{} across {} \
         recorded case{}, and [the list](divergences.md) says what each one is. \
         A tracked difference that silently stops being a difference fails the \
         build.",
        divergences.len(),
        plural(divergences.len()),
        cases,
        plural(cases),
    );

    wrap(out.trim_end(), 80)
}

/// Wrap to the width the rest of the book is written at, so a generated block
/// and a hand-written paragraph look the same in the source and a diff of one
/// stays readable.
fn wrap(text: &str, width: usize) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / width);
    for paragraph in text.split('\n') {
        let mut column = 0;
        for word in paragraph.split_whitespace() {
            if column == 0 {
                out.push_str(word);
                column = word.chars().count();
            } else if column + 1 + word.chars().count() > width {
                out.push('\n');
                out.push_str(word);
                column = word.chars().count();
            } else {
                out.push(' ');
                out.push_str(word);
                column += 1 + word.chars().count();
            }
        }
        out.push('\n');
    }
    out
}

/// The tool as a reader knows it, from the binary name the corpus records.
///
/// An unrecognised tool renders as its own binary name rather than a guess —
/// odd-looking once, and never wrong.
fn tool_name(tool: &str) -> &str {
    match tool {
        "acme" => "ACME",
        "rgbasm" => "RGBDS",
        "vasmm68k_mot" => "vasm",
        "asl" => "Macro Assembler AS",
        other => other,
    }
}

/// What the committed parity data says, so the page quotes the CI-gated figure
/// rather than recomputing one against whatever checkout is to hand.
struct Parity {
    sources: u64,
    comparisons: u64,
    machines: Vec<String>,
}

fn parity(repo: &Path) -> Option<Parity> {
    let text = std::fs::read_to_string(crate::parity::data_path(repo)).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let machines = doc
        .get("machines")?
        .as_array()?
        .iter()
        .filter_map(|m| m.get("slug")?.as_str().map(machine_name))
        .collect();
    Some(Parity {
        sources: doc.pointer("/totals/sources")?.as_u64()?,
        comparisons: doc.pointer("/totals/comparisons")?.as_u64()?,
        machines,
    })
}

/// The machine as a reader names it, from the slug the parity data carries.
///
/// An unknown slug is rendered as itself rather than guessed at: a new machine
/// should read oddly once and be named here, not be silently mistitled.
fn machine_name(slug: &str) -> String {
    match slug {
        "commodore-64" => "the C64",
        "commodore-amiga" => "the Amiga",
        "nintendo-entertainment-system" => "the NES",
        "sinclair-zx-spectrum" => "the Spectrum",
        other => other,
    }
    .to_string()
}

/// `a, b and c` — the serial comma is not house style here.
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// `5649` as `5,649`, which is how the prose around it reads.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{list, thousands};

    #[test]
    fn a_list_reads_as_prose() {
        assert_eq!(list(&["a".into()]), "a");
        assert_eq!(list(&["a".into(), "b".into()]), "a and b");
        assert_eq!(list(&["a".into(), "b".into(), "c".into()]), "a, b and c");
    }

    #[test]
    fn wrapping_matches_the_prose_around_it() {
        let wrapped = super::wrap("one two three four five", 9);
        assert_eq!(wrapped, "one two\nthree\nfour five\n");
    }

    #[test]
    fn a_paragraph_break_survives_wrapping() {
        assert_eq!(super::wrap("a\n\nb", 80), "a\n\nb\n");
    }

    #[test]
    fn thousands_are_separated() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(617), "617");
        assert_eq!(thousands(5649), "5,649");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
