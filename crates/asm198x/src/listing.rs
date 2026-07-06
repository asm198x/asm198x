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

/// The `--sym` rendering: one `name = $HEX` line per symbol, sorted by name.
/// Labels and entry points render as absolute addresses (origin + offset, in
/// the CPU's own address units); constants render their value. Both wear the
/// same `$HEX` coat — the kind distinction lives in the sidecar, not here.
#[must_use]
pub fn render_sym(result: &AssemblyResult) -> String {
    let base = u64::from(result.origin.unwrap_or(0));
    let mut lines: Vec<String> = result
        .debug
        .symbols
        .iter()
        .map(|s| {
            let value = match s.kind {
                debug198x::SymbolKind::Label { offset, .. }
                | debug198x::SymbolKind::Entry { offset, .. } => base + offset,
                debug198x::SymbolKind::Const { value } => value,
            };
            format!("{} = ${value:04X}", s.name)
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
