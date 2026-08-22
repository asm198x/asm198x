//! The search index the site queries, generated from the pages it describes.
//!
//! mdBook carried search and its withdrawal left no replacement. The decision
//! record is blunt about why that matters — "21 generated instruction pages
//! today, 45 once the dialect matrices land. Reference without search is worse
//! than no reference."
//!
//! # Why headings rather than full text
//!
//! The question people bring to this reference is *where is `SBC`*, and the
//! generated instruction pages answer it structurally: every mnemonic is a
//! heading, with its one-line description directly underneath. Indexing
//! headings therefore indexes every mnemonic on all twenty-one CPU pages for
//! free, with the text that explains it.
//!
//! It suits the prose pages too, whose headings are the questions they answer
//! ("Where a relative include is looked for", "`--org` is not cosmetic"). A
//! reader hunting the include-path rules matches that heading; full-text
//! ranking would bury it under every page that says "include".
//!
//! And it is small. The book is 400K of markdown, most of it instruction
//! tables that nobody searches by opcode byte; the index is a fraction of that,
//! which is the difference between a search box that works on a phone and one
//! that ships a dictionary first.
//!
//! Full-text search stays available as a later step — Pagefind indexes built
//! output and would sit alongside this rather than replace it — but it is a
//! build dependency on the site, and this needed none.

use std::path::{Path, PathBuf};

/// One searchable place in the book.
struct Entry {
    /// The page's URL path, as the nav spells it.
    slug: String,
    /// The page's own title.
    page: String,
    /// The heading, or `None` for the page itself.
    heading: Option<String>,
    /// The first line of prose under it, where there is one — a mnemonic's
    /// description, or a section's opening sentence.
    context: String,
}

/// Where the index is written.
pub fn index_path(repo: &Path) -> PathBuf {
    repo.join("docs/book/search.json")
}

/// Strip the markdown that would otherwise be searched as punctuation.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' | '*' | '_' => {}
            '[' => {
                // Keep the link text, drop the target.
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    out.push(c);
                }
                if chars.peek() == Some(&'(') {
                    for c in chars.by_ref() {
                        if c == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

/// Whether a line is inside a fenced block, tracked as the walk goes: a `##`
/// inside a shell sample is a comment, not a heading.
fn collect(slug: &str, text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut page_title = slug.to_string();
    let mut fenced = false;
    let mut pending: Option<(Option<String>, usize)> = None;
    let lines: Vec<&str> = text.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let heading = if let Some(rest) = line.strip_prefix("# ") {
            page_title = plain(rest);
            Some(None)
        } else {
            line.strip_prefix("## ")
                .or_else(|| line.strip_prefix("### "))
                .map(|rest| Some(plain(rest)))
        };
        if let Some(heading) = heading {
            pending = Some((heading, i));
        }
        // Close the previous heading at the first line of prose under it.
        if let Some((heading, at)) = pending.clone()
            && i > at
            && !line.trim().is_empty()
            && !line.trim_start().starts_with('|')
            && !line.trim_start().starts_with("<!--")
        {
            out.push(Entry {
                slug: slug.to_string(),
                page: page_title.clone(),
                heading,
                context: plain(line),
            });
            pending = None;
        }
    }
    // A heading with nothing under it still belongs in the index.
    if let Some((heading, _)) = pending {
        out.push(Entry {
            slug: slug.to_string(),
            page: page_title.clone(),
            heading,
            context: String::new(),
        });
    }
    out
}

/// Build the index over every page in the book.
///
/// # Errors
/// An unreadable page.
pub fn render(repo: &Path) -> Result<String, String> {
    let src = crate::docs::book_src(repo);
    let mut entries = Vec::new();
    for path in crate::nav::markdown_files(&src)? {
        let slug = path
            .strip_prefix(&src)
            .unwrap_or(&path)
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        if slug == "SUMMARY" {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        entries.extend(collect(&slug, &text));
    }

    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let mut obj = serde_json::json!({
                "slug": e.slug,
                "page": e.page,
            });
            if let Some(heading) = &e.heading {
                obj["heading"] = serde_json::Value::String(heading.clone());
            }
            if !e.context.is_empty() {
                obj["context"] = serde_json::Value::String(e.context.clone());
            }
            obj
        })
        .collect();

    let doc = serde_json::json!({
        "note": "Generated by `cargo xtask docs` from docs/book/src. The documentation site's search reads this. Do not edit by hand.",
        "entries": items,
    });
    let mut out = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{collect, plain};

    #[test]
    fn markdown_is_stripped_from_what_is_searched() {
        assert_eq!(plain("`--org` is not cosmetic"), "--org is not cosmetic");
        assert_eq!(
            plain("see [the list](divergences.md) for more"),
            "see the list for more"
        );
        assert_eq!(plain("**bold** and _thin_"), "bold and thin");
    }

    /// A `##` inside a fenced sample is a comment, not a heading. The book is
    /// full of shell blocks, so indexing them would fill the index with noise
    /// that looks like sections.
    #[test]
    fn a_hash_inside_a_fence_is_not_a_heading() {
        let page = "# Page\n\nText.\n\n```sh\n## not a heading\n```\n\n## Real\n\nUnder it.\n";
        let found = collect("p", page);
        let headings: Vec<Option<&str>> = found.iter().map(|e| e.heading.as_deref()).collect();
        assert_eq!(headings, vec![None, Some("Real")]);
    }

    /// The instruction pages are the reason this exists: every mnemonic is a
    /// heading with its description directly underneath.
    #[test]
    fn a_mnemonic_carries_its_description() {
        let page = "# MOS 6502\n\nBlurb.\n\n## LDA\n\nLoad accumulator\n\n| Mode |\n|---|\n";
        let found = collect("reference/instructions/mos6502", page);
        let lda = found
            .iter()
            .find(|e| e.heading.as_deref() == Some("LDA"))
            .expect("LDA is indexed");
        assert_eq!(lda.page, "MOS 6502");
        assert_eq!(lda.context, "Load accumulator");
    }

    /// A table row is not prose, so it does not become a heading's context.
    #[test]
    fn a_table_row_is_not_taken_as_context() {
        let page = "# P\n\nB.\n\n## H\n\n| a | b |\n|---|---|\n";
        let found = collect("p", page);
        let h = found
            .iter()
            .find(|e| e.heading.as_deref() == Some("H"))
            .expect("H");
        assert!(h.context.is_empty(), "got {:?}", h.context);
    }
}
