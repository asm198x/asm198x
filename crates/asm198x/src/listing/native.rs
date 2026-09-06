//! Native section listings keep CPU placement separate from image placement.
use super::{AssemblyResult, ListingFile, cycle_cell};
use crate::{LineRec, SectionDebug};
use serde_json::json;
use std::fmt::Write as _;

fn cost(section: &SectionDebug, line: &LineRec) -> Option<(u64, u64)> {
    section
        .debug
        .cycles
        .iter()
        .filter(|c| {
            c.file == line.file
                && c.line == line.line
                && c.offset >= line.offset
                && c.offset < line.offset.saturating_add(line.length)
        })
        .map(|c| {
            (
                u64::from(c.base),
                u64::from(c.base) + u64::from(c.page_cross) + u64::from(c.branch_taken),
            )
        })
        .reduce(|a, b| (a.0 + b.0, a.1 + b.1))
}

fn coverage(section: &SectionDebug) -> &'static str {
    match section.debug.cycle_coverage {
        crate::CycleCoverage::Full => "full",
        crate::CycleCoverage::Partial => "partial",
        crate::CycleCoverage::None => "none",
    }
}

pub(super) fn json(input: &str, result: &AssemblyResult, unit: u64) -> String {
    let mut lines = Vec::new();
    let mut labels = Vec::new();
    for s in &result.section_debug {
        for l in &s.debug.lines {
            let mut row = json!({
                "section": s.section.id, "offset": l.offset,
                "address": s.section.base.and_then(|b| b.checked_add(l.offset)),
                "file_offset": s.file_offset.and_then(|b| l.offset.checked_mul(unit)?.checked_add(b)),
                "file": result.files.get(l.file.0 as usize).map(String::as_str).unwrap_or(input),
                "line": l.line, "bytes": l.length.saturating_mul(unit),
            });
            if let Some((min, max)) = cost(s, l) {
                row["cycles"] = json!({"min": min, "max": max});
            }
            lines.push(row);
        }
        for c in crate::cycles::label_costs_debug(&s.debug, unit) {
            labels.push(json!({
                "section": s.section.id, "name": c.name, "offset": c.start,
                "address": s.section.base.and_then(|b| b.checked_add(c.start)),
                "bytes": c.bytes, "cycles": {"min": c.min, "max": c.max},
            }));
        }
    }
    let sections: Vec<_> = result
        .section_debug
        .iter()
        .map(|s| {
            json!({
                "id": s.section.id, "name": s.section.name, "base": s.section.base,
                "file_offset": s.file_offset, "coverage": coverage(s),
            })
        })
        .collect();
    let doc =
        json!({"sections": sections, "lines": lines, "labels": labels, "areas": result.areas});
    format!("{doc}\n")
}

pub(super) fn text(files: &[ListingFile], result: &AssemblyResult, unit: u64) -> String {
    let mut out = String::new();
    for s in &result.section_debug {
        let base = s
            .section
            .base
            .map_or_else(|| "no CPU base".into(), |b| format!("CPU ${b:04X}"));
        let at = s
            .file_offset
            .map_or_else(|| "no file bytes".into(), |b| format!("file +${b:X}"));
        let _ = writeln!(out, "section {} ({base}, {at}):", s.section.name);
        for l in &s.debug.lines {
            let address = s
                .section
                .base
                .and_then(|b| b.checked_add(l.offset))
                .map_or_else(|| format!("+{:04X}", l.offset), |a| format!("{a:04X}"));
            let bytes = s
                .file_offset
                .and_then(|b| l.offset.checked_mul(unit)?.checked_add(b))
                .and_then(|start| {
                    let end = start.checked_add(l.length.checked_mul(unit)?)?;
                    result
                        .bytes
                        .get(usize::try_from(start).ok()?..usize::try_from(end).ok()?)
                });
            let mut hex = String::new();
            if let Some(bytes) = bytes {
                for byte in bytes.iter().take(super::LISTING_BYTES) {
                    let _ = write!(hex, "{byte:02X} ");
                }
                if bytes.len() > super::LISTING_BYTES {
                    hex.push_str("..");
                }
            }
            let cycles = cost(s, l).map(cycle_cell).unwrap_or_default();
            let file = files.get(l.file.0 as usize);
            let path = file.map_or("", |f| f.path.as_str());
            let source = file
                .and_then(|f| f.contents.lines().nth(l.line.saturating_sub(1) as usize))
                .unwrap_or("");
            let _ = writeln!(
                out,
                "{address:>5}  {hex:<26} {cycles:>7}  {path}:{}  {source}",
                l.line
            );
        }
        for c in crate::cycles::label_costs_debug(&s.debug, unit) {
            let _ = writeln!(
                out,
                "  {} (+${:04X}): {} bytes, {} cycles (straight-line)",
                c.name,
                c.start,
                c.bytes,
                cycle_cell((c.min, c.max))
            );
        }
        let _ = writeln!(out, "cycle coverage: {}\n", coverage(s));
    }
    out
}
