//! Debug-record renderings — the `.debug198x` sidecar builder and the `--sym` /
//! `--listing` text renderings (Debug198x plan U3, R6/R8; F2).
//!
//! All three render from the **same** captured record (KTD2: capture once,
//! render three ways): pass 2 fills [`AssemblyResult::debug`] with typed
//! symbols and line→address spans; [`debug_info`] wraps it with header identity
//! for the NDJSON sidecar, and [`render_sym`] / [`render_listing`] are plain
//! text views of it. Hex is a rendering concern here — the record itself keeps
//! decimal JSON integers (KTD3).

use crate::contract::AssemblyResult;

/// Wrap an assembly's captured debug record as a full [`debug198x::DebugInfo`]
/// — the shape the `.debug198x` sidecar serializes and the Emu198x importer
/// reads. The flat engine is a single section (`main`, id 0) based at the load
/// origin (KTD7's degenerate absolute case); `cpu`/`dialect` name the target
/// and syntax for the header, and `source_path` is the file every line span
/// attributes to (v1 is single-file; includes arrive with the language
/// surface).
#[must_use]
pub fn debug_info(
    result: &AssemblyResult,
    cpu: &str,
    dialect: &str,
    source_path: &str,
) -> debug198x::DebugInfo {
    debug198x::DebugInfo {
        header: debug198x::Header {
            tool: "asm198x".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            cpu: cpu.to_string(),
            dialect: dialect.to_string(),
            sources: vec![source_path.to_string()],
            ..debug198x::Header::default()
        },
        sections: vec![debug198x::Section {
            id: 0,
            name: "main".to_string(),
            base: result.origin.map(u64::from),
        }],
        symbols: result.debug.symbols.clone(),
        lines: result
            .debug
            .lines
            .iter()
            .map(|l| debug198x::LineSpan {
                file: source_path.to_string(),
                line: l.line,
                section: 0,
                offset: l.offset,
                length: l.length,
            })
            .collect(),
    }
}

/// A multi-section dialect's debug read-out, before header identity and the
/// source filename are attached: the section table, `(section, offset)`
/// symbols, and `(line, section, offset, length)` spans. The ca65 linker path
/// (U4) and the vasm hunk path (U5) both collect one; [`capture_debug_info`]
/// completes it. Collection is strictly passive — read out beside layout or
/// emission, never branching on it.
pub(crate) struct DebugCapture {
    pub(crate) sections: Vec<debug198x::Section>,
    pub(crate) symbols: Vec<debug198x::Symbol>,
    pub(crate) lines: Vec<(u32, debug198x::SectionId, u64, u64)>,
}

/// Wrap a dialect's [`DebugCapture`] as a full [`debug198x::DebugInfo`] — the
/// multi-section counterpart of [`debug_info`] (Debug198x U4/U5, KTD4).
/// Sections, symbols, and spans come straight from the capture; this adds the
/// header identity and attaches `source_path` to every line span.
#[must_use]
pub(crate) fn capture_debug_info(
    capture: DebugCapture,
    cpu: &str,
    dialect: &str,
    source_path: &str,
) -> debug198x::DebugInfo {
    debug198x::DebugInfo {
        header: debug198x::Header {
            tool: "asm198x".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            cpu: cpu.to_string(),
            dialect: dialect.to_string(),
            sources: vec![source_path.to_string()],
            ..debug198x::Header::default()
        },
        sections: capture.sections,
        symbols: capture.symbols,
        lines: capture
            .lines
            .into_iter()
            .map(|(line, section, offset, length)| debug198x::LineSpan {
                file: source_path.to_string(),
                line,
                section,
                offset,
                length,
            })
            .collect(),
    }
}

/// The `--sym` rendering: one line per symbol, sorted by name. A label whose
/// section has an absolute base renders as `name = $HEX` (base + offset, in
/// the CPU's own address units); a label in a **relocatable** section (a vasm
/// hunk — `base: None`, KTD7) renders section-qualified as
/// `name = <section>+$HEX`, since a bare offset would collide across hunks.
/// Constants render their value. The kind distinction lives in the sidecar,
/// not here. Renders from the [`debug198x::DebugInfo`] record, so the flat and
/// linked (ca65/vasm) paths share it (KTD2).
#[must_use]
pub fn render_sym(info: &debug198x::DebugInfo) -> String {
    let place = |id: debug198x::SectionId, offset: u64| {
        let section = info.sections.iter().find(|s| s.id == id);
        match section.and_then(|s| s.base) {
            Some(base) => format!("${:04X}", base + offset),
            None => {
                let name = section.map_or_else(|| format!("sec{id}"), |s| s.name.clone());
                format!("{name}+${offset:04X}")
            }
        }
    };
    let mut lines: Vec<String> = info
        .symbols
        .iter()
        .map(|s| {
            let value = match s.kind {
                debug198x::SymbolKind::Label {
                    section, offset, ..
                }
                | debug198x::SymbolKind::Entry {
                    section, offset, ..
                } => place(section, offset),
                debug198x::SymbolKind::Const { value } => format!("${value:04X}"),
            };
            format!("{} = {value}", s.name)
        })
        .collect();
    lines.sort();
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// How many emitted bytes a listing row shows before eliding the tail. Data
/// directives can emit hundreds of bytes from one line (`!fill`, `ds`); the
/// listing is a reading aid, not a hex dump, so a long run ends in `..`.
const LISTING_BYTES: usize = 8;

/// The `--listing` rendering: `ADDR  BYTES  SOURCE`, one row per source line.
/// Lines that emit no bytes — `equ`, comments, blanks — keep an empty address
/// and bytes column and show their source verbatim (KTD2: they need no format
/// records; the source text is right there). `addr_unit` is the CPU's bytes
/// per address unit (2 for the word-addressed CP1610, 1 elsewhere): span
/// offsets/lengths are in address units, the bytes column indexes raw bytes.
#[must_use]
pub fn render_listing(source: &str, result: &AssemblyResult, addr_unit: u64) -> String {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    let base = u64::from(result.origin.unwrap_or(0));
    // One span per source-bearing statement (the engine's capture); keyed by
    // line for the row lookup.
    let spans: BTreeMap<u32, &crate::engine::LineRec> =
        result.debug.lines.iter().map(|l| (l.line, l)).collect();

    // The bytes column is sized for `LISTING_BYTES` two-digit bytes with
    // single-space separators; an elided run replaces its tail with `..` at the
    // same width, so the source column always starts at one x-position.
    let bytes_col = LISTING_BYTES * 3 - 1;
    let mut out = String::new();
    for (i, text) in source.lines().enumerate() {
        let line = (i + 1) as u32;
        let row = match spans.get(&line) {
            Some(span) => {
                let addr = base + span.offset;
                let start = usize::try_from(span.offset * addr_unit).unwrap_or(usize::MAX);
                let len = usize::try_from(span.length * addr_unit).unwrap_or(0);
                let emitted = result
                    .bytes
                    .get(start..start.saturating_add(len))
                    .unwrap_or(&[]);
                let hex = if emitted.len() > LISTING_BYTES {
                    let shown: Vec<String> = emitted[..LISTING_BYTES - 1]
                        .iter()
                        .map(|b| format!("{b:02X}"))
                        .collect();
                    format!("{} ..", shown.join(" "))
                } else {
                    let shown: Vec<String> = emitted.iter().map(|b| format!("{b:02X}")).collect();
                    shown.join(" ")
                };
                format!("{addr:04X}  {hex:<bytes_col$}  {text}")
            }
            None => format!("{:4}  {:<bytes_col$}  {text}", "", ""),
        };
        // Trim so no-source rows (blank lines) stay genuinely blank.
        let _ = writeln!(out, "{}", row.trim_end());
    }
    out
}
