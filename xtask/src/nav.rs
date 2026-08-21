//! The documentation nav, and the dead links it would otherwise hide.
//!
//! `SUMMARY.md` is the authored order of the documentation: which pages exist,
//! how they nest, and what the sections are called. mdBook read it directly and
//! refused to build when a listed chapter had no file — `create-missing =
//! false`, a gate that has been in this repo since the book landed.
//!
//! mdBook is withdrawn (`decisions/one-documentation-surface.md`), and both of
//! those jobs have to survive it.
//!
//! # Why the site does not parse `SUMMARY.md`
//!
//! It could. It is thirty lines of predictable markdown. But a second parser in
//! a second language, across a repo boundary, is the shape of drift this
//! project keeps getting bitten by — it is why the CLI reference was wrong for
//! months and why the landing page claimed 80 C64 units when the corpus held
//! 138.
//!
//! So the nav is generated here, committed as `docs/book/nav.json`, and read
//! there — the same arrangement as the parity figures and every generated block
//! in the book. The site renders what this file says and has no opinion of its
//! own about ordering.
//!
//! # The gate
//!
//! `cargo xtask docs --check` fails when `SUMMARY.md` lists a page that does not
//! exist, which is the `create-missing = false` behaviour, in the repo that owns
//! the source rather than in the site build. It also names pages that exist but
//! are listed nowhere: mdBook did not fail on those, but a page the nav cannot
//! reach is one nobody will find, and now that the nav *is* the site's
//! navigation there is no second route to it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The generated nav, committed so the site can read it.
const NAV: &str = "docs/book/nav.json";

/// One page in the nav, with whatever nests under it.
#[derive(Debug, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    /// The page's path without `.md`: `introduction`, `instructions/mos6502`.
    pub slug: String,
    pub children: Vec<Item>,
}

/// A run of pages under one heading. The leading pages have no heading.
#[derive(Debug, PartialEq, Eq)]
pub struct Section {
    pub title: Option<String>,
    pub items: Vec<Item>,
}

/// Parse `SUMMARY.md`.
///
/// The shape mdBook defines and this file uses: `# Heading` opens a section, a
/// bare `[Title](path.md)` is a page outside any list, and `- [Title](path.md)`
/// is a list item whose indentation gives its depth. HTML comments mark the
/// generated block and carry no structure.
#[must_use]
pub fn parse(summary: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    // Depth -> index path into the tree being built.
    let mut stack: Vec<usize> = Vec::new();

    for raw in summary.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("<!--") {
            continue;
        }

        if let Some(title) = trimmed.strip_prefix("# ") {
            // The document's own title is not a section.
            if title.trim() == "Summary" && sections.is_empty() {
                continue;
            }
            sections.push(Section {
                title: Some(title.trim().to_string()),
                items: Vec::new(),
            });
            stack.clear();
            continue;
        }

        let (depth, link) = if let Some(rest) = trimmed.strip_prefix("- ") {
            let indent = line.len() - trimmed.len();
            (indent / 2 + 1, rest)
        } else if trimmed.starts_with('[') {
            (0, trimmed)
        } else {
            continue;
        };

        let Some(item) = link_item(link) else {
            continue;
        };

        if sections.is_empty() {
            sections.push(Section {
                title: None,
                items: Vec::new(),
            });
        }
        let section = sections.last_mut().expect("a section exists");
        stack.truncate(depth.saturating_sub(1));
        push_at(&mut section.items, &stack, item);
        stack.push(last_index(&section.items, &stack));
    }
    sections
}

/// `[Title](path.md)` -> an item, if the line is one.
fn link_item(link: &str) -> Option<Item> {
    let rest = link.strip_prefix('[')?;
    let (title, rest) = rest.split_once("](")?;
    let (path, _) = rest.split_once(')')?;
    let slug = path.strip_suffix(".md").unwrap_or(path);
    Some(Item {
        title: title.to_string(),
        slug: slug.to_string(),
        children: Vec::new(),
    })
}

/// Insert `item` at the position `stack` addresses.
fn push_at(items: &mut Vec<Item>, stack: &[usize], item: Item) {
    match stack.split_first() {
        None => items.push(item),
        Some((head, tail)) => match items.get_mut(*head) {
            Some(parent) => push_at(&mut parent.children, tail, item),
            None => items.push(item),
        },
    }
}

/// The index of the item just inserted at `stack`.
fn last_index(items: &[Item], stack: &[usize]) -> usize {
    match stack.split_first() {
        None => items.len().saturating_sub(1),
        Some((head, tail)) => match items.get(*head) {
            Some(parent) => last_index(&parent.children, tail),
            None => 0,
        },
    }
}

/// Every slug in the nav, in order.
#[must_use]
pub fn slugs(sections: &[Section]) -> Vec<String> {
    fn walk(items: &[Item], out: &mut Vec<String>) {
        for i in items {
            out.push(i.slug.clone());
            walk(&i.children, out);
        }
    }
    let mut out = Vec::new();
    for s in sections {
        walk(&s.items, &mut out);
    }
    out
}

/// Render the nav as the committed JSON.
#[must_use]
pub fn render(sections: &[Section]) -> String {
    fn item(i: &Item) -> serde_json::Value {
        serde_json::json!({
            "title": i.title,
            "slug": i.slug,
            "children": i.children.iter().map(item).collect::<Vec<_>>(),
        })
    }
    let doc = serde_json::json!({
        "note": "Generated by `cargo xtask docs` from docs/book/src/SUMMARY.md. The documentation site renders this. Do not edit by hand.",
        "sections": sections.iter().map(|s| serde_json::json!({
            "title": s.title,
            "items": s.items.iter().map(item).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    let mut out = serde_json::to_string_pretty(&doc).unwrap_or_default();
    out.push('\n');
    out
}

/// Where the generated nav lives under `repo`.
#[must_use]
pub fn nav_path(repo: &Path) -> PathBuf {
    repo.join(NAV)
}

/// Read and parse `SUMMARY.md`.
///
/// # Errors
/// If the file cannot be read.
pub fn read(repo: &Path) -> Result<Vec<Section>, String> {
    let path = crate::docs::book_src(repo).join("SUMMARY.md");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(parse(&text))
}

/// Pages listed with no file, and files listed on no page.
///
/// # Errors
/// If the book source cannot be walked.
pub fn integrity(repo: &Path, sections: &[Section]) -> Result<Vec<String>, String> {
    let src = crate::docs::book_src(repo);
    let listed: BTreeSet<String> = slugs(sections).into_iter().collect();

    let mut problems = Vec::new();
    for slug in &listed {
        if !src.join(format!("{slug}.md")).is_file() {
            problems.push(format!(
                "SUMMARY.md lists `{slug}` but {}.md does not exist",
                src.join(slug).display()
            ));
        }
    }

    let mut on_disk = Vec::new();
    collect_md(&src, &src, &mut on_disk)?;
    on_disk.sort();
    for slug in on_disk {
        if slug != "SUMMARY" && !listed.contains(&slug) {
            problems.push(format!(
                "{slug}.md exists but SUMMARY.md lists it nowhere, so the site cannot reach it"
            ));
        }
    }
    Ok(problems)
}

/// Every `.md` under `dir`, as slugs relative to `root`.
fn collect_md(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_md(root, &path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(rel) = path.strip_prefix(root) {
                let slug = rel.with_extension("");
                out.push(slug.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Summary\n\n[asm198x](introduction.md)\n\n# Using it\n\n\
        - [A first program](first-program.md)\n- [The command line](cli.md)\n\n\
        # Reference\n\n<!-- generated: x -->\n- [Instruction reference](instructions.md)\n\
        \x20 - [MOS 6502](instructions/mos6502.md)\n  - [Zilog Z80](instructions/z80.md)\n\
        <!-- /generated -->\n";

    #[test]
    fn the_document_title_is_not_a_section() {
        let s = parse(SAMPLE);
        assert_eq!(s[0].title, None);
        assert_eq!(s[0].items[0].slug, "introduction");
    }

    #[test]
    fn headings_open_sections() {
        let s = parse(SAMPLE);
        assert_eq!(
            s.iter().map(|x| x.title.clone()).collect::<Vec<_>>(),
            vec![None, Some("Using it".into()), Some("Reference".into())]
        );
    }

    #[test]
    fn indentation_nests() {
        let s = parse(SAMPLE);
        let reference = &s[2].items[0];
        assert_eq!(reference.slug, "instructions");
        assert_eq!(
            reference
                .children
                .iter()
                .map(|c| c.slug.as_str())
                .collect::<Vec<_>>(),
            vec!["instructions/mos6502", "instructions/z80"]
        );
    }

    #[test]
    fn slugs_come_out_in_reading_order() {
        assert_eq!(
            slugs(&parse(SAMPLE)),
            vec![
                "introduction",
                "first-program",
                "cli",
                "instructions",
                "instructions/mos6502",
                "instructions/z80",
            ]
        );
    }

    #[test]
    fn comments_carry_no_structure() {
        // The generated markers sit between the section heading and its items.
        assert_eq!(parse(SAMPLE)[2].items.len(), 1);
    }
}
