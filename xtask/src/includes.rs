//! The multi-file table on the migration page, read from the declared surfaces.
//!
//! Someone moving a project across needs one fact before anything else: whether
//! this assembler reads their `include` at all, and how their dialect spells
//! it. That table was hand-written, listing five dialects out of twenty-one —
//! so a reader with a 6809 or a TMS9900 project learned nothing, and there was
//! nothing to notice when a dialect gained an include.
//!
//! It comes from [`asm198x::directives::surfaces`] instead, which is what the
//! declared surface was built for. Generating it also found the surfaces
//! wrong: the file directives are read by each dialect's multi-file walk
//! rather than its `parse_op`, and fourteen dialects had therefore never
//! declared them — a table generated before that fix would have said they
//! cannot include a file, which is the opposite of true.

use std::fmt::Write as _;

use asm198x::dialect_table;
use asm198x::directives::{self, Directive};
use asm198x::includes::{self, Anchor};

/// Every spelling of one entry, as inline code, or a dash if the dialect has
/// no such entry.
fn spellings(directives: &[Directive], id: &str) -> String {
    let Some(entry) = directives.iter().find(|d| d.id == id) else {
        return "—".to_string();
    };
    entry
        .spellings()
        .iter()
        .map(|s| format!("`{s}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The table, ordered as the dialect table orders `--dialect` itself.
///
/// Variants that select a **target** rather than a syntax — `pasmonext`, the
/// ROM-less MCS-48 parts, the segmented Z8000 — share their base dialect's
/// vocabulary and are left out; listing them would repeat a row rather than
/// add one.
pub fn markdown() -> String {
    let surfaces = directives::surfaces();
    let mut out = String::from("| Dialect | Source file | Binary file |\n|---|---|---|\n");
    for entry in dialect_table::DIALECTS {
        let Some(surface) = surfaces.iter().find(|s| s.dialect == entry.name) else {
            continue;
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} |",
            entry.name,
            spellings(&surface.directives, "include"),
            spellings(&surface.directives, "incbin"),
        );
    }
    out
}

/// Where each dialect looks for a relative include.
///
/// The rows come from [`asm198x::includes::resolution`], which most dialects
/// answer straight off the semantics const their multi-file walk runs on — so
/// the table and the behaviour are one fact rather than two that agree today.
pub fn anchors() -> String {
    let mut out =
        String::from("| Dialect | Looked for in | A request with no extension |\n|---|---|---|\n");
    let rows = includes::resolution();
    for entry in dialect_table::DIALECTS {
        let Some(row) = rows.iter().find(|r| r.dialect == entry.name) else {
            continue;
        };
        let extensionless = match (row.anchor, row.default_extension) {
            (Anchor::None, _) => "—".to_string(),
            (_, Some(ext)) => format!("`defs` tries `defs.{ext}` first"),
            (_, None) => "taken as spelled".to_string(),
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} |",
            entry.name,
            row.anchor.describe(),
            extensionless
        );
    }
    out
}
