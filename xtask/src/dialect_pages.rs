//! One generated page per dialect — the v1 bar's per-dialect directive
//! matrices.
//!
//! `decisions/v1-scope.md` puts these on the bar and says why: there are 21
//! dialect front-ends and one dialect reference document, and
//! "source-compatibility is the product's identity, so per-dialect fidelity is
//! what a user most needs written down and it is the least written down".
//! Hand-writing 21 pages is the failure the docs-site plan exists to prevent.
//!
//! # What a page is built from, and what it is not
//!
//! Two accessors, both of which the assembler dispatches from rather than
//! describes: [`asm198x::directives::surfaces`] for the vocabulary and
//! [`asm198x::includes::resolution`] for the multi-file behaviour. A spelling
//! on one of these pages is a spelling the parser accepts, because it is the
//! same list the parser looks a word up in.
//!
//! **The arbitration column is deliberately absent**, and the reason is worth
//! stating rather than leaving as a hole. The verdict corpus records a
//! `dialect` field, but it is a *suite* label and not a `--dialect` name:
//! `asl` covers all twelve asl-syntax chips at once, `vasm-bin` and `vasm-exe`
//! are output legs of one dialect, and `pasmonext` and `z80n` are targets. So
//! "which tool arbitrates this dialect, and over how many verdicts" cannot be
//! joined onto a dialect page without a `--dialect`→CPU mapping that no source
//! owns today. Inventing one here would be the second source of truth these
//! pages exist to avoid. `/compare` carries the same facts keyed on the tool,
//! which is the key the corpus does own.

use std::fmt::Write as _;

use asm198x::dialect_table::{self, Entry};
use asm198x::directives::{Category, DialectSurface, Directive};
use asm198x::includes::{self, Anchor};

/// A page the generator owns entirely.
pub struct Page {
    pub path: String,
    pub body: String,
}

/// The slug a dialect's page lives at.
fn slug(entry: &Entry) -> String {
    entry.name.replace('/', "-")
}

/// Every spelling of an entry, as inline code.
fn spellings(directive: &Directive) -> String {
    directive
        .spellings()
        .iter()
        .map(|s| format!("`{s}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the assembler does with a directive, for a reader.
fn category(category: Category) -> String {
    match category {
        Category::Operation => String::new(),
        Category::Ignored => "Accepted and discarded — it changes no bytes".to_string(),
        Category::ExpressionWord => "Used inside an expression, not as a statement".to_string(),
        Category::KnownUnsupported => "Recognised, and not implemented".to_string(),
        // Not a gap: the reference refuses it for the output we emit, so
        // refusing it is what matching the reference means.
        Category::RefusedByReference(rule) => {
            format!("Refused, as the reference refuses it — {rule}")
        }
    }
}

/// The directive table for one dialect.
fn directives_table(surface: &DialectSurface) -> String {
    let mut rows: Vec<&Directive> = surface.directives.iter().collect();
    rows.sort_by_key(|d| d.id);

    let mut out = String::from("| Directive | Spellings | Notes |\n|---|---|---|\n");
    for directive in rows {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} |",
            directive.id,
            spellings(directive),
            category(directive.category)
        );
    }
    out
}

/// The multi-file paragraph, or the sentence that says there is none.
fn multi_file(name: &str) -> String {
    let Some(row) = includes::resolution()
        .into_iter()
        .find(|r| r.dialect == name)
    else {
        return String::new();
    };

    let mut out = String::from("## Projects in more than one file\n\n");
    if row.anchor == Anchor::None {
        out.push_str(
            "This dialect has no include directive yet, so a project in more than \
             one file does not assemble. It is the one front door where that is \
             true — see [Projects in more than one file](../../guide/multi-file.md).\n",
        );
        return out;
    }

    let _ = writeln!(
        out,
        "A relative request is looked for in **{}**, then in the `-I` search \
         directories in the order they were given.",
        row.anchor.describe().to_lowercase()
    );
    if let Some(ext) = row.default_extension {
        let _ = writeln!(
            out,
            "\nA request with no extension tries `<name>.{ext}` before the exact \
             spelling."
        );
    }
    out.push_str(
        "\nThe anchor is pinned against this dialect's own reference assembler, \
         and the dialects genuinely disagree — \
         [Projects in more than one file](../../guide/multi-file.md) has the \
         comparison.\n",
    );
    out
}

/// One dialect's page.
fn page(entry: &Entry, surface: &DialectSurface) -> Page {
    let mut body = format!("# `{}`\n\n", entry.name);

    let _ = writeln!(
        body,
        "<!-- Generated by `cargo xtask docs` from the assembler's own declared \
         surface. Edits here are overwritten; change the declaration instead. -->\n"
    );

    let _ = writeln!(body, "{}.\n", entry.blurb);

    if !entry.aliases.is_empty() {
        let _ = writeln!(
            body,
            "`--dialect` also accepts {}.\n",
            entry
                .aliases
                .iter()
                .map(|a| format!("`{a}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let operations = surface
        .directives
        .iter()
        .filter(|d| d.category == Category::Operation)
        .count();
    let spelling_count: usize = surface.directives.iter().map(|d| d.spellings().len()).sum();
    let _ = writeln!(
        body,
        "## Directives\n\n{} directives, {} spellings. This is the list the \
         parser looks a word up in, so a spelling here is one the assembler \
         accepts and a spelling missing from it is one it refuses.\n",
        surface.directives.len(),
        spelling_count
    );
    let _ = writeln!(body, "{}", directives_table(surface));
    if operations != surface.directives.len() {
        body.push_str(
            "Instruction mnemonics are not listed here — they come from the \
             instruction-set spec and have their own reference.\n\n",
        );
    }

    let multi = multi_file(entry.name);
    if !multi.is_empty() {
        let _ = writeln!(body, "{multi}");
    }

    body.push_str(
        "## Elsewhere\n\n\
         - [Dialects](../dialects.md) — every dialect and what each is the syntax of\n\
         - [Where we differ](../../divergences.md) — every known difference from a \
         reference assembler\n\
         - [Moving a project across](../../migrate.md) — adopting this on a project \
         that already builds\n",
    );

    Page {
        path: format!("reference/dialects/{}.md", slug(entry)),
        body,
    }
}

/// Every dialect page, in the order the dialect table presents them.
///
/// A dialect with no declared surface is skipped rather than given an empty
/// page: a variant selecting a different **target** — `pasmonext`, the ROM-less
/// MCS-48 parts, the segmented Z8000 — shares its base dialect's vocabulary and
/// would repeat a page rather than add one.
#[must_use]
pub fn pages() -> Vec<Page> {
    let surfaces = asm198x::directives::surfaces();
    dialect_table::DIALECTS
        .iter()
        .filter_map(|entry| {
            surfaces
                .iter()
                .find(|s| s.dialect == entry.name)
                .map(|surface| page(entry, surface))
        })
        .collect()
}

/// The `SUMMARY.md` lines listing the pages under the dialect overview.
#[must_use]
pub fn summary_lines() -> String {
    let surfaces = asm198x::directives::surfaces();
    let mut out = String::from("- [Dialects](reference/dialects.md)\n");
    for entry in dialect_table::DIALECTS {
        if !surfaces.iter().any(|s| s.dialect == entry.name) {
            continue;
        }
        let _ = writeln!(
            out,
            "  - [{}](reference/dialects/{}.md)",
            entry.name,
            slug(entry)
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{pages, summary_lines};

    /// Every page the summary lists is a page that gets written, and the other
    /// way round. The nav gate would catch a mismatch, but catching it here
    /// names the cause rather than the symptom.
    #[test]
    fn the_summary_and_the_pages_agree() {
        let listed: Vec<String> = summary_lines()
            .lines()
            .filter_map(|l| {
                l.split_once("](")
                    .map(|(_, r)| r.trim_end_matches(')').to_string())
            })
            .filter(|p| p.starts_with("reference/dialects/"))
            .collect();
        let written: Vec<String> = pages().into_iter().map(|p| p.path).collect();
        assert_eq!(listed, written);
    }

    /// A dialect that declares a surface gets a page. Twenty-one today.
    #[test]
    fn every_declared_dialect_has_a_page() {
        let written: Vec<String> = pages().into_iter().map(|p| p.path).collect();
        for surface in asm198x::directives::surfaces() {
            let expected = format!(
                "reference/dialects/{}.md",
                surface.dialect.replace('/', "-")
            );
            assert!(
                written.contains(&expected),
                "`{}` declares a surface and has no page",
                surface.dialect
            );
        }
    }

    /// pasmo's missing include is stated on its page rather than left as an
    /// absence, which is the whole reason the declaration carries it.
    #[test]
    fn pasmo_says_it_cannot_include() {
        let page = pages()
            .into_iter()
            .find(|p| p.path.ends_with("pasmo.md"))
            .expect("pasmo has a page");
        assert!(
            page.body.contains("no include directive yet"),
            "pasmo's page should say so"
        );
    }

    /// Every page names its directives, or it is not a matrix.
    #[test]
    fn every_page_carries_a_directive_table() {
        for page in pages() {
            assert!(
                page.body.contains("| Directive | Spellings | Notes |"),
                "{} has no directive table",
                page.path
            );
        }
    }
}
