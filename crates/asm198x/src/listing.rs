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
use crate::span::FileId;

/// Wrap an assembly's captured debug record as a full [`debug198x::DebugInfo`]
/// — the shape the `.debug198x` sidecar serializes and the Emu198x importer
/// reads. The flat engine is a single section (`main`, id 0) based at the load
/// origin (KTD7's degenerate absolute case); `cpu`/`dialect` name the target
/// and syntax for the header.
///
/// A multi-file result (language-surface U9) carries its own `FileId`→path
/// table in [`AssemblyResult::files`]; when present it wins: `Header.sources`
/// is that table in `FileId` order — so `sources[i] ⇔ FileId(i)`, one
/// convention across the contract and the sidecar (KTD2) — and each line span
/// names the file its own record counts within. A single-source result (an
/// empty table) attributes everything to `source_path`, exactly as before
/// multi-file existed.
#[must_use]
pub fn debug_info(
    result: &AssemblyResult,
    cpu: &str,
    dialect: &str,
    source_path: &str,
) -> debug198x::DebugInfo {
    let sources: Vec<String> = if result.files.is_empty() {
        vec![source_path.to_string()]
    } else {
        result.files.clone()
    };
    // An unresolvable id (impossible from the walk, guarded anyway) falls back
    // to the root entry, mirroring `capture_debug_info_multi`.
    let path_of = |file: FileId| {
        sources
            .get(file.0 as usize)
            .or_else(|| sources.first())
            .cloned()
            .unwrap_or_default()
    };
    debug198x::DebugInfo {
        header: debug198x::Header {
            tool: "asm198x".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            cpu: cpu.to_string(),
            dialect: dialect.to_string(),
            sources: sources.clone(),
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
                file: path_of(l.file),
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

/// The multi-file counterpart of [`DebugCapture`] (language-surface U5): each
/// line record carries the [`FileId`] its line counts within, so a linked
/// path's spans can name an included file rather than stamping the root input
/// everywhere. The ca65 assemble+link driver and the vasm multipass core (U6)
/// both collect one; [`capture_debug_info_multi`] completes it against the
/// source map's file table, and the single-source entries collapse it via
/// [`into_single`](Self::into_single).
pub(crate) struct DebugCaptureMulti {
    pub(crate) sections: Vec<debug198x::Section>,
    pub(crate) symbols: Vec<debug198x::Symbol>,
    /// `(file, line, section, offset, length)` — the [`DebugCapture`] span
    /// plus the file the line counts within.
    pub(crate) lines: Vec<(FileId, u32, debug198x::SectionId, u64, u64)>,
}

impl DebugCaptureMulti {
    /// Collapse to the single-file [`DebugCapture`] (every record in the root
    /// input) — the single-source entry points keep their exact pre-multi-file
    /// shape through this.
    pub(crate) fn into_single(self) -> DebugCapture {
        DebugCapture {
            sections: self.sections,
            symbols: self.symbols,
            lines: self
                .lines
                .into_iter()
                .map(|(_, line, section, offset, length)| (line, section, offset, length))
                .collect(),
        }
    }
}

/// Wrap a multi-file [`DebugCaptureMulti`] as a full [`debug198x::DebugInfo`]
/// (language-surface U5). `sources` is the source map's file table in
/// `FileId` order, so `Header.sources[i] ⇔ FileId(i)` — one convention across
/// the contract and the sidecar (KTD2) — and each line span names its own
/// file's path. An unresolvable id (impossible from the walk, guarded anyway)
/// falls back to the root entry.
pub(crate) fn capture_debug_info_multi(
    capture: DebugCaptureMulti,
    cpu: &str,
    dialect: &str,
    sources: Vec<String>,
) -> debug198x::DebugInfo {
    let path_of = |file: FileId| {
        sources
            .get(file.0 as usize)
            .or_else(|| sources.first())
            .cloned()
            .unwrap_or_default()
    };
    let lines = capture
        .lines
        .into_iter()
        .map(
            |(file, line, section, offset, length)| debug198x::LineSpan {
                file: path_of(file),
                line,
                section,
                offset,
                length,
            },
        )
        .collect();
    debug198x::DebugInfo {
        header: debug198x::Header {
            tool: "asm198x".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            cpu: cpu.to_string(),
            dialect: dialect.to_string(),
            sources,
            ..debug198x::Header::default()
        },
        sections: capture.sections,
        symbols: capture.symbols,
        lines,
    }
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
///
/// Single-source: renders the root text only. A multi-file program renders
/// through [`render_listing_files`], which splices each included file's lines
/// in at its include point; this entry keeps its exact pre-multi-file output
/// for the single-file case (it is the one-file degenerate call of the same
/// body).
#[must_use]
pub fn render_listing(source: &str, result: &AssemblyResult, addr_unit: u64) -> String {
    render_listing_files(
        &[ListingFile {
            path: String::new(),
            contents: source.to_string(),
            included_from: None,
        }],
        result,
        addr_unit,
    )
}

/// One source file feeding the multi-file `--listing` rendering
/// (language-surface U9): its file-table path, its full text, and — for an
/// included file — the include point its lines splice in at. Slice index ⇔
/// `FileId` (the [`AssemblyResult::files`] convention, KTD2); entry 0 is the
/// root input.
pub struct ListingFile {
    /// The file-table path (labels the listing's file margin).
    pub path: String,
    /// The file's complete source text.
    pub contents: String,
    /// `(includer, 1-based directive line)` — where this file's lines splice
    /// into the listing. `None` for the root input.
    pub included_from: Option<(FileId, u32)>,
}

/// The multi-file `--listing` rendering (language-surface U9): the root file's
/// rows with each included file's rows **spliced in after its include
/// directive's line**, every row carrying a file margin naming its file (the
/// path's basename; the margin appears only when there is more than one file,
/// so a single-file listing is byte-identical to [`render_listing`]). A
/// binary-inclusion directive's payload is elided to a one-line byte-count row
/// (`.. N bytes` in the bytes column) — the listing is a reading aid, not a
/// hex dump, and the asset's name is right there in the source text.
///
/// A file included more than once assembles more than once but lists once, at
/// its first include point, with its first instantiation's addresses — the
/// listing is a CLI convenience with no stability promise.
#[must_use]
pub fn render_listing_files(
    files: &[ListingFile],
    result: &AssemblyResult,
    addr_unit: u64,
) -> String {
    use std::collections::BTreeMap;

    let base = u64::from(result.origin.unwrap_or(0));
    // One span per source-bearing statement, keyed by (file, line); first one
    // wins so a re-included file lists its first instantiation.
    let mut spans: BTreeMap<(u32, u32), &crate::engine::LineRec> = BTreeMap::new();
    for l in &result.debug.lines {
        spans.entry((l.file.0, l.line)).or_insert(l);
    }
    // Include points: (includer, directive line) → the files spliced there.
    let mut children: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, f) in files.iter().enumerate() {
        if let Some((parent, line)) = f.included_from {
            children.entry((parent.0, line)).or_default().push(i);
        }
    }
    // The file margin: basenames, widened to full paths where basenames
    // collide; a single-file listing carries no margin at all.
    let margins: Vec<String> = if files.len() > 1 {
        let names: Vec<&str> = files
            .iter()
            .map(|f| {
                f.path
                    .rsplit(['/', '\\'])
                    .next()
                    .filter(|n| !n.is_empty())
                    .unwrap_or(f.path.as_str())
            })
            .collect();
        names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                if names.iter().filter(|n| *n == name).count() > 1 {
                    files[i].path.clone()
                } else {
                    (*name).to_string()
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let margin_w = margins.iter().map(String::len).max().unwrap_or(0);

    let mut out = String::new();
    if !files.is_empty() {
        render_one_file(
            0,
            &RenderCx {
                files,
                spans: &spans,
                children: &children,
                result,
                base,
                addr_unit,
                margins: &margins,
                margin_w,
            },
            &mut out,
        );
    }
    out
}

/// The shared rendering context threaded through the splice walk.
struct RenderCx<'a> {
    files: &'a [ListingFile],
    spans: &'a std::collections::BTreeMap<(u32, u32), &'a crate::engine::LineRec>,
    children: &'a std::collections::BTreeMap<(u32, u32), Vec<usize>>,
    result: &'a AssemblyResult,
    base: u64,
    addr_unit: u64,
    margins: &'a [String],
    margin_w: usize,
}

/// Render one file's rows, splicing each included file in after its include
/// directive's line. Recursion is bounded by the include depth (the walk that
/// built the graph caps it), and the graph is a tree by construction — each
/// file records its first includer only.
fn render_one_file(id: usize, cx: &RenderCx<'_>, out: &mut String) {
    use std::fmt::Write as _;

    // The bytes column is sized for `LISTING_BYTES` two-digit bytes with
    // single-space separators; an elided run replaces its tail with `..` at the
    // same width, so the source column always starts at one x-position.
    let bytes_col = LISTING_BYTES * 3 - 1;
    for (i, text) in cx.files[id].contents.lines().enumerate() {
        let line = (i + 1) as u32;
        let row = match cx.spans.get(&(id as u32, line)) {
            Some(span) => {
                let addr = cx.base + span.offset;
                let hex = if is_incbin_line(text) {
                    // The elided binary-payload row: a byte count instead of a
                    // hex dump the reader would never scan.
                    format!(".. {} bytes", span.length * cx.addr_unit)
                } else {
                    let start = usize::try_from(span.offset * cx.addr_unit).unwrap_or(usize::MAX);
                    let len = usize::try_from(span.length * cx.addr_unit).unwrap_or(0);
                    let emitted = cx
                        .result
                        .bytes
                        .get(start..start.saturating_add(len))
                        .unwrap_or(&[]);
                    if emitted.len() > LISTING_BYTES {
                        let shown: Vec<String> = emitted[..LISTING_BYTES - 1]
                            .iter()
                            .map(|b| format!("{b:02X}"))
                            .collect();
                        format!("{} ..", shown.join(" "))
                    } else {
                        let shown: Vec<String> =
                            emitted.iter().map(|b| format!("{b:02X}")).collect();
                        shown.join(" ")
                    }
                };
                format!("{addr:04X}  {hex:<bytes_col$}  {text}")
            }
            None => format!("{:4}  {:<bytes_col$}  {text}", "", ""),
        };
        // Trim so no-source rows (blank lines) stay genuinely blank.
        let margin_w = cx.margin_w;
        let row = if margin_w > 0 {
            format!("{:<margin_w$}  {row}", cx.margins[id])
        } else {
            row
        };
        let _ = writeln!(out, "{}", row.trim_end());
        if let Some(kids) = cx.children.get(&(id as u32, line)) {
            for &k in kids {
                render_one_file(k, cx, out);
            }
        }
    }
}

/// Whether a source line is a binary-inclusion directive, by its first or
/// second whitespace token (a label may lead) matching a dialect's spelling —
/// sjasmplus/pasmo/rgbasm/vasm `incbin`, the ca65 family's `.incbin`, acme's
/// `!bin`/`!binary`, lwasm's `includebin`, asl's `binclude`. A rendering
/// convenience only: it decides the elided byte-count row, never semantics.
fn is_incbin_line(text: &str) -> bool {
    text.split_whitespace().take(2).any(|tok| {
        matches!(
            tok.to_ascii_lowercase().as_str(),
            "incbin" | ".incbin" | "!bin" | "!binary" | "includebin" | "binclude"
        )
    })
}
