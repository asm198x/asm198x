//! The ca65 (NES) dialect, with a bounded ld65-style linker for the one fixed
//! NES configuration the curriculum uses.
//!
//! ca65 is an assembler whose output is linked by ld65 into the final ROM, so
//! producing a byte-identical `.nes` means doing both jobs. The 6502 operand and
//! expression machinery is shared in [`super::mos6502`]; this module adds ca65's
//! surface (`.segment`, `.byte`/`.word`/`.res`, `=` constants, `name:` and
//! `@cheap` labels, `<`/`>` binding tight) and a small linker that places the
//! segments into the standard NROM layout.
//!
//! Every NES unit in the curriculum links with the same `nes.cfg`, so that
//! layout is encoded directly here rather than parsed from a config file —
//! `iNES header (16) + PRG ($8000, 32K, fill $00) + CHR (8K, fill $00)`, with
//! `CODE` at `$8000` and `VECTORS` at `$FFFA`. See `decisions/syntax-stance.md`.
//!
//! `.include`/`.incbin` (language-surface U5) resolve through the shared
//! ca65-flat walk in [`super::ca65_flat`] under
//! [`CA65_SEMANTICS`](super::ca65_flat::CA65_SEMANTICS) — the flat family's
//! probe-pinned ancestor-chain resolution and incbin window, re-confirmed
//! under the ca65+ld65 NES link (they are assembler-side semantics). The
//! parse state threads across boundaries exactly as ca65's textual splice
//! does: `=` constants, cheap-local scope, the anonymous-label stream, and
//! the active segment (a `.segment` switch inside an include persists into
//! the includer — probe-pinned).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use super::ca65_flat::{self, DirectiveLine, FlatWalk, WalkDirective};
use super::macros;
use super::mos6502::{
    self, BytePrec, assignment_split, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, split_top_level, string_literal,
};
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, DiagSeverity, Expr, Operation, Warning, WarningKind};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

// ---------------------------------------------------------------------------
// The fixed NES (NROM) layout
// ---------------------------------------------------------------------------

/// PRG ROM occupies the upper 32K of the CPU address space.
const PRG_BASE: u32 = 0x8000;
const PRG_SIZE: usize = 0x8000;
const CHR_SIZE: usize = 0x2000;
const HEADER_SIZE: usize = 0x10;
const FILL: u8 = 0x00;

/// The segments the fixed NES (NROM) config defines: name, base address, and
/// whether the segment contributes bytes to the ROM file. This is the single
/// source of truth — `seg_info` looks up here, and a rejected `.segment` lists
/// these names. It mirrors the curriculum's `nes.cfg`; a segment outside it
/// (e.g. `RODATA`) is rejected here for the same reason `ld65` rejects it with
/// that config — there is no memory area to place it in.
///
/// The curriculum ships **two** `nes.cfg` variants: the `dash` track carries an
/// `OAM` page at `$0200` and pushes `BSS` to `$0300`, and `meet-the-machine`
/// omits `OAM` so `BSS` starts at `$0200`. This table is the `dash` layout, and
/// a `meet-the-machine` program that places a label in `BSS` would land a page
/// high. Nothing in either track does today, which is why one fixed table has
/// held; the differential's inlined config matches this one deliberately.
const NES_SEGMENTS: &[(&str, u32, Option<usize>)] = &[
    ("ZEROPAGE", 0x0000, None),
    ("OAM", 0x0200, None),
    ("BSS", 0x0300, None),
    ("HEADER", 0x0000, Some(0)),
    ("CODE", 0x8000, Some(HEADER_SIZE)),
    (
        "VECTORS",
        0xFFFA,
        Some(HEADER_SIZE + 0xFFFA - PRG_BASE as usize),
    ),
    ("CHARS", 0x0000, Some(HEADER_SIZE + PRG_SIZE)),
];

/// The base address of a segment, and where in the ROM its bytes land.
///
/// `file_at` is `None` for a segment that occupies address space and
/// contributes no bytes — the zero page, RAM, the OAM shadow.
struct SegInfo {
    base: u32,
    file_at: Option<usize>,
}

impl SegInfo {
    fn in_file(&self) -> bool {
        self.file_at.is_some()
    }
}

fn seg_info(seg: &str) -> Option<SegInfo> {
    NES_SEGMENTS
        .iter()
        .find(|(name, _, _)| *name == seg)
        .map(|&(_, base, file_at)| SegInfo { base, file_at })
}

/// The segment each shorthand switches to. `.code` is `.segment "CODE"`, and
/// so on down; `.data` and `.rodata` name segments the fixed NROM config has no
/// memory area for, and are rejected at placement exactly as `ld65` rejects
/// them ("Missing memory area assignment for segment 'RODATA'").
fn segment_shorthand(word: &str) -> Option<&'static str> {
    Some(match word {
        ".code" => "CODE",
        ".data" => "DATA",
        ".bss" => "BSS",
        ".rodata" => "RODATA",
        ".zeropage" => "ZEROPAGE",
        _ => return None,
    })
}

/// Whether a line is one of the segment-switching directives, which parse into
/// a source-only node rather than an item. `.segment` keeps the prefix test it
/// has always had (its operand may abut the directive); the words that take no
/// operand match whole, so `.codegen` is not `.code`.
fn is_segment_directive(trimmed: &str) -> bool {
    if trimmed.starts_with(".segment") {
        return true;
    }
    let word = trimmed.split_whitespace().next().unwrap_or("");
    word == ".pushseg" || word == ".popseg" || segment_shorthand(word).is_some()
}

/// What a segment-switching node does to the active segment.
enum SegSwitch {
    To(String),
    Push,
    Pop,
}

/// Read a source-only node back as a segment switch. `None` for any other
/// item-less node (a label-only line, a comment flush).
fn segment_switch(source: &str) -> Option<SegSwitch> {
    if let Some(rest) = source.strip_prefix(".segment") {
        return Some(SegSwitch::To(rest.trim().trim_matches('"').to_string()));
    }
    match source.split_whitespace().next().unwrap_or("") {
        ".pushseg" => Some(SegSwitch::Push),
        ".popseg" => Some(SegSwitch::Pop),
        w => segment_shorthand(w).map(|name| SegSwitch::To(name.to_string())),
    }
}

/// The valid segment names, for a rejection message.
fn known_segments() -> String {
    NES_SEGMENTS
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Parsed statements
// ---------------------------------------------------------------------------

// `Clone` so the assembler can project a statement out of the AST node that owns
// it (the assemble+link driver runs on an owned `Vec<Stmt>`; see
// `parsed_from_program`).
#[derive(Clone)]
enum Kind {
    Empty,
    Bytes(Vec<Expr>),
    Words(Vec<Expr>),
    /// `.dbyt` — 16-bit values emitted **big-endian** (high byte first).
    DBytes(Vec<Expr>),
    /// `.dword` — 32-bit values emitted little-endian.
    DWords(Vec<Expr>),
    /// `.res count [, fill]` — `count` bytes of `fill`.
    Res(usize, u8),
    /// A constant a record declaration defines: a field's offset, a member's
    /// value, or a record's own size. It emits nothing and is carried as a
    /// statement so the projection folds it **in source order**, which is where
    /// every other constant here is folded and what makes `.if` and `.res` see
    /// the same file the reference does.
    Constant(String, i64),
    /// `.out`, `.warning`, `.error` and `.fatal` — say something and emit
    /// nothing. The severity decides what happens next: a note or a warning
    /// carries on, an error stops. ca65 reports every `.error` and only the
    /// first `.fatal`, a distinction a single-error result cannot show.
    Message(DiagSeverity, String),
    /// `.export`/`.exportzp`/`.import`/`.importzp`/`.global`/`.globalzp` — a
    /// name this program makes visible to a linker. Assembly and linking are
    /// fused over one translation unit here, so what survives is the part ca65
    /// and ld65 enforce between them: see [`VisRule`].
    Visible {
        rule: VisRule,
        /// The `zp` spellings, which draw ca65's `Symbol 'x' is absolute but
        /// exported zeropage` warning for a label outside the zero page. A
        /// constant never draws it, whatever its value.
        zero_page: bool,
        names: Vec<String>,
        /// `.export name := expr` **defines** `name` as well as exporting it.
        /// `.import` and `.global` refuse the form; only the export spellings
        /// take it, and only for a single name.
        define: Option<Expr>,
    },
    /// `.assert cond, action[, "message"]` — `error` stops, `warning` carries
    /// on. The condition folds against the finished symbol table, so it may
    /// name a label defined below it.
    Assert(Expr, bool, String),
    /// `.align boundary [, fill]` — pad to the next multiple of `boundary`
    /// **within the segment**, not at an absolute address: ca65 emits an
    /// alignment constraint and leaves aligning the segment itself to `ld65`,
    /// which is why `.align 3` in CODE (based at `$8000`, not a multiple of 3)
    /// lands at segment offset 3 and draws ld65's "isn't aligned properly"
    /// warning rather than moving the bytes. The boundary need not be a power
    /// of two.
    Align(i64, u8),
    /// A resolved `.incbin` payload (language-surface U5): raw asset bytes at
    /// the directive's location in the active segment. Never parsed into a
    /// native node — the multi-file walk resolves the directive into a shared
    /// [`Item::Binary`](crate::ast::Item) and the projection carries it here.
    Raw(Vec<u8>),
    Insn {
        operand: mos6502::OperandSyntax,
        mnemonic: String,
    },
}

struct Stmt {
    line: usize,
    /// The file `line` counts within (language-surface U5): the root for a
    /// single-file assemble, an include's `FileId` otherwise. Layout/emit
    /// errors and debug line records are stamped with it.
    file: FileId,
    seg: String,
    label: Option<String>,
    kind: Kind,
}

// The ca65 statement kind is the family-owned native payload carried in the AST
// (`decisions/ast-native-payload-for-multipass-cisc.md`): parse builds it into
// the tree, and the assemble+link driver reads it back. `=` constants use the
// shared `Item::Equ` instead, so no `Kind` reports `inline_label`.
impl crate::ast::NativeItem for Kind {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct Parsed {
    stmts: Vec<Stmt>,
    /// Each label's segment, for the zero-page-vs-absolute decision.
    label_seg: BTreeMap<String, String>,
    /// `=` constants, folded in source order.
    consts: BTreeMap<String, i64>,
}

// ---------------------------------------------------------------------------
// Entry point: assemble + link
// ---------------------------------------------------------------------------

// The debug record read out of layout (Debug198x U4, KTD4) is the shared
// [`DebugCapture`]: per-segment sections, `(section, offset)`-addressed
// symbols, and line spans — all post-link CPU addresses (what a debugger
// needs), never file offsets. A read-out of data layout already computes;
// capturing it cannot change a byte.
use crate::listing::{DebugCapture, DebugCaptureMulti};

/// A segment's section id: its index in [`NES_SEGMENTS`] (the config order).
fn seg_id(seg: &str) -> debug198x::SectionId {
    NES_SEGMENTS
        .iter()
        .position(|(name, _, _)| *name == seg)
        .expect("seg validated against NES_SEGMENTS") as debug198x::SectionId
}

/// Assemble ca65 source and link it into a `.nes` ROM image. Single-source: a
/// `.include`/`.incbin` directive is rejected with a pointer to the multi-file
/// entry (`assemble_multi`).
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble(source: &str) -> Result<(Vec<u8>, Vec<Warning>), AsmError> {
    assemble_with_debug(source).map(|(rom, warnings, _)| (rom, warnings))
}

/// Assemble + link, also returning the debug [`Capture`] read out of layout
/// (Debug198x U4). One code path: [`assemble`] delegates here, so the bytes
/// with and without capture are identical by construction (AE2).
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble_with_debug(
    source: &str,
) -> Result<(Vec<u8>, Vec<Warning>, DebugCapture), AsmError> {
    let (rom, warnings, capture) = assemble_program(&parse_program(
        &isa::mos6502::SET,
        source,
        macros::Expand::Yes,
    )?)?;
    Ok((rom, warnings, capture.into_single()))
}

/// Assemble + link a **multi-file** NES program (language-surface U5): the
/// root is `map`'s `FileId(0)`, `.include`/`.incbin` resolve lazily through
/// `loader` under ca65's probe-pinned semantics
/// ([`CA65_SEMANTICS`](ca65_flat::CA65_SEMANTICS) — the flat family's U4b
/// probes, re-confirmed under the NES link), and the returned capture's line
/// records carry each statement's real file for the debug sidecar.
///
/// # Errors
/// Any per-line parse failure (stamped with its file), a missing target, an
/// include cycle, a bad `.incbin` window, or any layout/link failure.
pub(crate) fn assemble_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<(Vec<u8>, Vec<Warning>, DebugCaptureMulti), AsmError> {
    assemble_program(&parse_program_multi(&isa::mos6502::SET, map, loader)?)
}

/// Assemble + link a parsed [`Program`](crate::ast::Program) — the one body
/// behind the single-source and multi-file entries, so their bytes can never
/// drift. The capture's line records carry each statement's file (U5); the
/// single-source wrapper collapses them back to the root.
///
/// # Errors
/// Returns an [`AsmError`] on any projection, range, or symbol-resolution
/// failure.
fn assemble_program(
    program: &crate::ast::Program,
) -> Result<(Vec<u8>, Vec<Warning>, DebugCaptureMulti), AsmError> {
    let set = &isa::mos6502::SET;
    // The AST is the single front-end IR: the parse built the source-preserving
    // `Program` (carrying each statement's native `Kind`, `=` constants, and the
    // segment directives); project it to the assembler's `Parsed`. Same
    // bytes as the old direct parse — see
    // `decisions/ast-native-payload-for-multipass-cisc.md`.
    let parsed = parsed_from_program(program)?;

    // Scoped label references, answered now that every definition is known:
    // `v` written inside `a::b` is `a::b::v` if that exists, else `a::v`, else
    // `v`. Constants were answered at parse time, positionally, which is what
    // ca65 requires of them; a label may stand anywhere, so it waits for this.
    let mut parsed = parsed;
    {
        let mut refs: BTreeSet<String> = BTreeSet::new();
        for stmt in &parsed.stmts {
            collect_syms(&stmt.kind, &mut refs);
        }
        let known: BTreeSet<&String> = parsed
            .label_seg
            .keys()
            .chain(parsed.consts.keys())
            .collect();
        let scoped: BTreeSet<String> = refs
            .iter()
            .filter(|k| k.starts_with(SCOPE_MARK))
            .cloned()
            .collect();
        let targets = scope_targets(&scoped, &|name| known.contains(&name.to_string()));
        for stmt in &mut parsed.stmts {
            stmt.kind = map_kind_syms(&stmt.kind, &|name| {
                targets.get(name).map(|t| Expr::Sym(t.clone()))
            });
            // `.sizeof` reads a size the declaration stored. A name that
            // declared none has no size to give, which is ca65's `Size of 'V'
            // is unknown` — answered here, where the message can still name
            // what the source wrote rather than the key it became.
            let mut named = BTreeSet::new();
            collect_syms(&stmt.kind, &mut named);
            for key in &named {
                let Some(name) = key.strip_suffix(&format!("{LABEL_SEP}size")) else {
                    continue;
                };
                if !parsed.consts.contains_key(key) {
                    return Err(ca65_flat::stamp_file(
                        AsmError::new(
                            stmt.line,
                            format!("size of `{}` is unknown", display_label(name)),
                        ),
                        stmt.file,
                    ));
                }
            }
        }
    }

    // The address-size environment: constants by value, plus zero-page labels
    // pinned below $100 so the shared mode picker selects the short form.
    let mut size_env = parsed.consts.clone();
    for (name, seg) in &parsed.label_seg {
        if seg == "ZEROPAGE" {
            size_env.insert(name.clone(), 0);
        }
    }

    // Layout pass: resolve each instruction's mode and size, lay statements out
    // within their segment, and record every label's absolute address.
    let mut offsets: BTreeMap<String, u32> = BTreeMap::new();
    // Absolute addresses are `i64` to match the engine's expression evaluator;
    // the NES is 16-bit, so values are masked to a word on emit.
    let mut addr_env: BTreeMap<String, i64> = BTreeMap::new();
    for (name, value) in &parsed.consts {
        addr_env.insert(name.clone(), *value);
    }
    let mut placed: Vec<(String, u32, usize, FileId, Resolved)> = Vec::new(); // (segment, addr, line, file, item)
    // The debug read-out (U4): symbols and line spans fall out of the layout
    // values already in hand — `(section, offset)` is `(seg, addr - base)`.
    let mut dbg_symbols: Vec<debug198x::Symbol> = Vec::new();
    let mut dbg_lines: Vec<(FileId, u32, debug198x::SectionId, u64, u64)> = Vec::new();
    for (name, value) in &parsed.consts {
        dbg_symbols.push(debug198x::Symbol {
            name: name.clone(),
            kind: debug198x::SymbolKind::Const {
                value: *value as u64,
            },
        });
    }
    // Every name an `.import` claims, before any label is placed: ca65 refuses
    // the *definition* of an imported name, and reports it there, so the set
    // has to be known before the label pass rather than gathered by it.
    let imported: BTreeSet<String> = parsed
        .stmts
        .iter()
        .filter_map(|s| match &s.kind {
            Kind::Visible {
                rule: VisRule::MustNotBeDefined,
                names,
                ..
            } => Some(names.clone()),
            _ => None,
        })
        .flatten()
        .collect();

    for stmt in parsed.stmts {
        let info = seg_info(&stmt.seg).ok_or_else(|| {
            // Layout errors are stamped with the statement's file (U5), so a
            // failure inside an included file names that file.
            ca65_flat::stamp_file(
                AsmError::new(
                    stmt.line,
                    format!(
                        "segment `{}` is not in the NES config (valid: {}); this assembler \
                         links the curriculum's fixed NROM layout, which — like `ld65` with \
                         its `nes.cfg` — has no memory area for other segments",
                        stmt.seg,
                        known_segments()
                    ),
                ),
                stmt.file,
            )
        })?;
        let off = *offsets.entry(stmt.seg.clone()).or_insert(0);
        let addr = info.base + off;
        if let Some(label) = &stmt.label {
            // Real ca65 rejects a duplicate symbol; accepting one would also
            // make the debug record lie (the record keeps every definition,
            // the encoder the last — a debugger would disagree with the bytes).
            // `addr_env` was seeded with the `=` constants, so this covers a
            // label colliding with a constant too.
            if imported.contains(label) {
                return Err(ca65_flat::stamp_file(
                    AsmError::new(
                        stmt.line,
                        format!("symbol `{}` is already an import", display_label(label)),
                    ),
                    stmt.file,
                ));
            }
            if addr_env.insert(label.clone(), i64::from(addr)).is_some() {
                return Err(ca65_flat::stamp_file(
                    AsmError::new(
                        stmt.line,
                        format!("duplicate symbol `{}`", display_label(label)),
                    ),
                    stmt.file,
                ));
            }
            // Anonymous (`:`) labels are positional, not names — a debugger
            // cannot look one up, so they stay out of the symbol record. Cheap
            // (`@name`) labels are qualified with a control byte internally;
            // render the source form.
            if !label.starts_with(LABEL_SEP) {
                dbg_symbols.push(debug198x::Symbol {
                    name: display_label(label),
                    kind: debug198x::SymbolKind::Label {
                        section: seg_id(&stmt.seg),
                        offset: u64::from(off),
                        space: None,
                    },
                });
            }
        }
        let (resolved, size) = resolve(set, stmt.kind, &size_env, off, stmt.line)
            .map_err(|e| ca65_flat::stamp_file(e, stmt.file))?;
        *offsets.get_mut(&stmt.seg).expect("segment offset") += size as u32;
        // A line span per byte-emitting statement (address-space-only
        // reservations — ZEROPAGE/BSS `.res` — carry no bytes, so no span; the
        // HEADER segment is iNES file metadata, not CPU-addressed code, so its
        // records would alias CPU $0000 — skipped, per AE3's no-fabrication rule).
        if size > 0 && info.in_file() && stmt.seg != "HEADER" {
            dbg_lines.push((
                stmt.file,
                stmt.line as u32,
                seg_id(&stmt.seg),
                u64::from(off),
                size as u64,
            ));
        }
        if !matches!(resolved, Resolved::Nothing) {
            placed.push((stmt.seg, addr, stmt.line, stmt.file, resolved));
        }
    }

    // The section table: every segment the program touched (placed bytes or
    // just labels/reservations), in config order. CPU-addressed segments carry
    // their absolute base; HEADER (file metadata) and CHARS (PPU address space)
    // are *not* CPU-addressable, so they get `base: None` — the reader's
    // absolute lookups skip them rather than aliasing them onto the zero page,
    // and a PPU-space consumer can supply a `BaseMap` (KTD7). A `Space`
    // qualifier is the eventual richer answer (KTD5, U7).
    let sections: Vec<debug198x::Section> = NES_SEGMENTS
        .iter()
        .enumerate()
        .filter(|(_, (name, _, _))| offsets.contains_key(*name))
        .map(|(id, (name, base, _))| debug198x::Section {
            id: id as debug198x::SectionId,
            name: (*name).to_string(),
            base: (!matches!(*name, "HEADER" | "CHARS")).then_some(u64::from(*base)),
            // The NES mapper's banking is not modelled here; no space is fabricated.
            space: None,
        })
        .collect();

    // Emit pass: turn each placed item into bytes, per segment. Statements run
    // in source order across segments, so `.out` notes come out in the order
    // they were written.
    let mut seg_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut warnings: Vec<Warning> = Vec::new();
    for (seg, addr, line, file, item) in placed {
        // A diagnostic carries no bytes, so it is answered before the segment
        // filter: an `.out` written inside BSS still prints.
        let diagnostic = matches!(item, Resolved::Message(..) | Resolved::Assert(..));
        if !diagnostic && !seg_info(&seg).expect("seg").in_file() {
            continue; // bss/zp segments occupy address space but emit no file bytes
        }
        let buf = seg_bytes.entry(seg).or_default();
        emit(
            item,
            addr,
            &Emit {
                env: &addr_env,
                label_seg: &parsed.label_seg,
                file,
                line,
            },
            buf,
            &mut warnings,
        )
        .map_err(|e| ca65_flat::stamp_file(e, file))?;
    }

    let rom = link(&seg_bytes)?;
    Ok((
        rom,
        warnings,
        DebugCaptureMulti {
            sections,
            symbols: dbg_symbols,
            lines: dbg_lines,
        },
    ))
}

/// Lay the file segments into the NROM ROM image.
fn link(seg_bytes: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, AsmError> {
    // Every segment the source wrote bytes into, as a run: addressed where the
    // CPU sees it, placed where the config puts it in the file. The engine's
    // `lay_out` does the rest, so the NES ROM is built by the same code that
    // places a Game Boy bank or a flat program's single section.
    let runs: Vec<crate::engine::Run> = NES_SEGMENTS
        .iter()
        .filter_map(|(name, base, file_at)| {
            let bytes = seg_bytes.get(*name)?;
            Some(crate::engine::Run {
                name: (*name).to_string(),
                base: i64::from(*base),
                at: crate::engine::Place::At((*file_at)? as i64),
                bytes: bytes.clone(),
            })
        })
        .collect();

    // CODE running into the vector table would be silently overwritten, giving
    // corrupted code and a debug record describing bytes that did not survive.
    // `lay_out` refuses the overlap, but names only the second segment; ld65
    // names the area that filled, and so does this.
    if let Some(code) = seg_bytes.get("CODE")
        && code.len() > (0xFFFA - PRG_BASE) as usize
    {
        return Err(AsmError::new(
            0,
            format!(
                "segment `CODE` ({} bytes) overlaps `VECTORS` at $FFFA",
                code.len()
            ),
        ));
    }

    // The ROM is a fixed shape: 16-byte header, 32K PRG, 8K CHR, `$00` fill.
    let size = HEADER_SIZE + PRG_SIZE + CHR_SIZE;
    let (_, rom) = crate::engine::lay_out(runs, FILL, 1, Some(0), |_| Some(size))?;
    Ok(rom)
}

// ---------------------------------------------------------------------------
// Resolution and emission
// ---------------------------------------------------------------------------

enum Resolved {
    Nothing,
    /// A resolved `.incbin` payload — raw bytes, emitted verbatim (U5).
    Raw(Vec<u8>),
    Bytes(Vec<Expr>),
    Words(Vec<Expr>),
    DBytes(Vec<Expr>),
    DWords(Vec<Expr>),
    Fill(usize, u8),
    Insn {
        form: &'static isa::Form,
        operands: Vec<Expr>,
    },
    /// Something to say, carrying no bytes.
    Message(DiagSeverity, String),
    /// A condition to fold once every symbol is known, and what a failure does.
    Assert(Expr, bool, String),
    /// A visibility claim to check once every symbol is known.
    Visible(VisRule, bool, Vec<String>),
}

/// What a visibility word asks of the name it carries, once assembly and
/// linking are one step over one translation unit.
///
/// Probed against ca65 2.18 + ld65 on 2026-08-24. The three answers do not
/// follow the export/import split the manual's headings suggest: `.global` is
/// in neither camp, and `.forceimport` is in no camp at all — defining its name
/// is `Symbol 'zz' is already an import` and not defining it is an unresolved
/// external, so it is declared `Category::RefusedByReference` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VisRule {
    /// `.export`, `.exportzp`. ca65 itself answers `Exported symbol 'nope' was
    /// never defined`, whether or not anything references it.
    MustBeDefined,
    /// `.import`, `.importzp`. Defining the name is ca65's `Symbol 'zz' is
    /// already an import`. Leaving it undefined is fine until something reads
    /// it, and then it is an unresolved external — which is the ordinary
    /// undefined-symbol refusal here.
    MustNotBeDefined,
    /// `.global`, `.globalzp` — export if the name is defined here, import if
    /// it is not. Both are legal, so nothing is checked beyond the zero-page
    /// warning the `zp` spelling carries.
    EitherWay,
}

/// Resolve a parsed statement to an emittable item plus its byte size.
fn resolve(
    set: &'static isa::InstructionSet,
    kind: Kind,
    size_env: &BTreeMap<String, i64>,
    off: u32,
    line: usize,
) -> Result<(Resolved, usize), AsmError> {
    Ok(match kind {
        Kind::Empty => (Resolved::Nothing, 0),
        // The only statement whose size depends on where it stands: pad to the
        // next multiple of the boundary, measured from the segment's start.
        Kind::Align(boundary, fill) => {
            let pad = (boundary - i64::from(off).rem_euclid(boundary)) % boundary;
            let pad = usize::try_from(pad).expect("non-negative");
            (Resolved::Fill(pad, fill), pad)
        }
        Kind::Raw(v) => {
            let n = v.len();
            (Resolved::Raw(v), n)
        }
        Kind::Bytes(v) => {
            let n = v.len();
            (Resolved::Bytes(v), n)
        }
        Kind::Words(v) => {
            let n = v.len() * 2;
            (Resolved::Words(v), n)
        }
        Kind::DBytes(v) => {
            let n = v.len() * 2;
            (Resolved::DBytes(v), n)
        }
        Kind::DWords(v) => {
            let n = v.len() * 4;
            (Resolved::DWords(v), n)
        }
        Kind::Res(count, fill) => (Resolved::Fill(count, fill), count),
        Kind::Constant(..) => (Resolved::Nothing, 0),
        Kind::Message(severity, text) => (Resolved::Message(severity, text), 0),
        Kind::Visible {
            rule,
            zero_page,
            names,
            // The `:= expr` value is read where constants are collected, so by
            // here the symbol is defined like any other and only the claim is
            // left to check.
            define: _,
        } => (Resolved::Visible(rule, zero_page, names), 0),
        Kind::Assert(cond, fatal, message) => (Resolved::Assert(cond, fatal, message), 0),
        Kind::Insn { operand, mnemonic } => {
            let insn = set
                .instruction(&mnemonic)
                .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
            // ca65 sizes by value (and an explicit `a:` we don't yet need); the
            // ACME-style hex-width rule does not apply.
            let (mode, operand) = mos6502::resolve_mode(insn, operand, size_env, false, line)?;
            let form = insn
                .form(mode)
                .ok_or_else(|| AsmError::new(line, format!("`{mnemonic}` has no {mode} form")))?;
            let operands: Vec<Expr> = operand.into_iter().collect();
            let size = form.len();
            (Resolved::Insn { form, operands }, size)
        }
    })
}

/// Where a statement is being emitted: the finished symbol table, every
/// label's segment (so the `zp` spellings can tell a zero-page label from an
/// absolute one — and both from a constant, which is neither), and the source
/// position a refusal is stamped with.
struct Emit<'a> {
    env: &'a BTreeMap<String, i64>,
    label_seg: &'a BTreeMap<String, String>,
    file: FileId,
    line: usize,
}

/// Emit one resolved item's bytes at address `addr`, appending any diagnostic
/// the source asked for. `env` is the finished symbol table, so an `.assert`
/// here may name a label defined below it.
fn emit(
    item: Resolved,
    addr: u32,
    at: &Emit<'_>,
    out: &mut Vec<u8>,
    warnings: &mut Vec<Warning>,
) -> Result<(), AsmError> {
    let (env, label_seg, file, line_for_errors) = (at.env, at.label_seg, at.file, at.line);
    let pc = i64::from(addr);
    match item {
        Resolved::Nothing => {}
        Resolved::Message(severity, text) => match severity {
            DiagSeverity::Error => return Err(AsmError::new(line_for_errors, text)),
            DiagSeverity::Warning | DiagSeverity::Note => warnings.push(Warning {
                line: line_for_errors,
                message: text,
                kind: if severity == DiagSeverity::Note {
                    WarningKind::Note
                } else {
                    WarningKind::Advisory
                },
                file,
            }),
        },
        Resolved::Visible(rule, zero_page, names) => {
            for name in &names {
                let defined = env.contains_key(name);
                match rule {
                    VisRule::MustBeDefined if !defined => {
                        return Err(AsmError::new(
                            line_for_errors,
                            format!(
                                "exported symbol `{}` was never defined",
                                display_label(name)
                            ),
                        ));
                    }
                    VisRule::MustNotBeDefined if defined => {
                        return Err(AsmError::new(
                            line_for_errors,
                            format!("symbol `{}` is already an import", display_label(name)),
                        ));
                    }
                    _ => {}
                }
                // The zero-page warning is about *labels*: a constant is never
                // "absolute" in the sense ca65 means, whatever its value.
                if zero_page
                    && let Some(segment) = label_seg.get(name)
                    && segment != "ZEROPAGE"
                {
                    warnings.push(Warning {
                        line: line_for_errors,
                        message: format!(
                            "symbol `{}` is absolute but exported zeropage",
                            display_label(name)
                        ),
                        kind: WarningKind::Advisory,
                        file,
                    });
                }
            }
        }
        Resolved::Assert(cond, fatal, message) => {
            if cond.eval(env, pc, line_for_errors)? == 0 {
                if fatal {
                    return Err(AsmError::new(line_for_errors, message));
                }
                warnings.push(Warning {
                    line: line_for_errors,
                    message,
                    kind: WarningKind::Advisory,
                    file,
                });
            }
        }
        Resolved::Raw(bytes) => out.extend_from_slice(&bytes),
        Resolved::Fill(count, fill) => out.extend(std::iter::repeat_n(fill, count)),
        Resolved::Bytes(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                out.push(to_byte(v, line_for_errors)?);
            }
        }
        Resolved::Words(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u16::try_from(to_width(v, 0xFFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        Resolved::DBytes(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u16::try_from(to_width(v, 0xFFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_be_bytes());
            }
        }
        Resolved::DWords(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u32::try_from(to_width(v, 0xFFFF_FFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        Resolved::Insn { form, operands } => {
            let next = pc + form.len() as i64;
            out.extend_from_slice(form.opcode);
            for (slot, e) in form.operands.iter().zip(operands.iter()) {
                let v = e.eval(env, pc, line_for_errors)?;
                match slot.kind {
                    // `ImmediateBe` is Z80N-only; ca65 is 6502/NES, so it never
                    // reaches here, but the match must stay exhaustive.
                    isa::OperandKind::Immediate
                    | isa::OperandKind::ImmediateBe
                    | isa::OperandKind::Address => match slot.bytes {
                        1 => out.push(to_byte(v, line_for_errors)?),
                        2 => out.extend_from_slice(
                            &u16::try_from(v & 0xFFFF).expect("masked").to_le_bytes(),
                        ),
                        other => {
                            return Err(AsmError::new(
                                line_for_errors,
                                format!("unsupported operand width {other}"),
                            ));
                        }
                    },
                    isa::OperandKind::RelativePc => {
                        let offset = v - next;
                        if !(-128..=127).contains(&offset) {
                            return Err(AsmError::new(
                                line_for_errors,
                                format!("branch target out of range ({offset} bytes)"),
                            ));
                        }
                        out.push(offset as i8 as u8);
                    }
                    isa::OperandKind::Displacement => {
                        return Err(AsmError::new(
                            line_for_errors,
                            "displacement operand not valid on 6502",
                        ));
                    }
                }
            }
            out.extend_from_slice(form.suffix);
        }
    }
    Ok(())
}

/// ca65 takes **no negative literal**, at any width: `.byte -1` is `Range
/// error (-1 not in [0..255])`, and `.word -1` and `.dword -1` answer the
/// same way against their own bounds (probed 2026-08-25). It is the only
/// reference here that refuses — the other six read `-1` as its two's
/// complement — so this bound is ca65's, not a house preference.
fn to_byte(v: i64, line: usize) -> Result<u8, AsmError> {
    to_width(v, 0xFF, line).map(|v| v as u8)
}

/// One width's range check, so `.byte`, `.word` and `.dword` cannot drift
/// apart. Before this the word and dword paths simply masked, which accepted
/// `.word -1` and `.word 65536` in silence where ca65 refuses both
/// (asm198x#290).
fn to_width(v: i64, max: i64, line: usize) -> Result<i64, AsmError> {
    if (0..=max).contains(&v) {
        Ok(v)
    } else {
        Err(AsmError::new(
            line,
            format!("range error ({v} not in [0..{max}])"),
        ))
    }
}

// ---------------------------------------------------------------------------
// Anonymous labels (`:` defines, `:-`/`:+` refer)
// ---------------------------------------------------------------------------

/// Anonymous-label state for the parse walk. Definitions are numbered in
/// **evaluation (splice) order** — exactly the one stream real ca65 resolves
/// against across include boundaries (probe-pinned: `bne :-` in the includer
/// after a `.include` resolves to the anon defined *inside* it). The old
/// whole-source line prescan cannot express that (line numbers collide across
/// files), and index arithmetic replaces it losslessly for the single-file
/// case too: backward level *k* is the *k*-th most recent definition, forward
/// level *k* is the *k*-th yet to come — so a forward reference names its
/// synthetic index before the definition arrives, and [`check`](Self::check)
/// reports any that never did once the walk completes.
///
/// Interior mutability because the shared 6502 operand parser threads the
/// value callback as a `&dyn Fn`.
#[derive(Default)]
struct AnonCtx {
    /// Definitions seen so far — also the next definition's index.
    seen: Cell<usize>,
    /// The file the walker is currently parsing, stamped per line so a
    /// deferred forward-reference failure can name its file.
    file: Cell<FileId>,
    /// Unproven forward references: `(required index, sign run length, span)`.
    forward: RefCell<Vec<(usize, usize, crate::ast::Span)>>,
}

impl AnonCtx {
    /// The unique synthetic name of definition `index`. The leading control
    /// char ([`LABEL_SEP`]) can never collide with a real identifier.
    fn name(index: usize) -> String {
        format!("{LABEL_SEP}:#{index}")
    }

    /// Bind the next anonymous definition, returning its synthetic name.
    fn define(&self) -> String {
        let index = self.seen.get();
        self.seen.set(index + 1);
        Self::name(index)
    }

    /// Resolve a `:`-anonymous reference: `sign` is `-` (backward) or `+`
    /// (forward), `level` the run length (`:--` is 2). A backward reference
    /// past the first definition fails now; a forward one is recorded for the
    /// end-of-walk [`check`](Self::check).
    fn refer(&self, sign: char, level: usize, line: usize) -> Result<String, AsmError> {
        let seen = self.seen.get();
        if sign == '-' {
            if level > seen {
                return Err(no_anon(sign, level, line));
            }
            Ok(Self::name(seen - level))
        } else {
            let index = seen + level - 1;
            self.forward.borrow_mut().push((
                index,
                level,
                crate::ast::Span::in_file(self.file.get(), line as u32, 0),
            ));
            Ok(Self::name(index))
        }
    }

    /// Fail on the first forward reference (in parse order) whose definition
    /// never arrived, with the same message an out-of-range backward one gets.
    fn check(&self) -> Result<(), AsmError> {
        let seen = self.seen.get();
        for (index, level, span) in self.forward.borrow().iter() {
            if *index >= seen {
                let mut e = no_anon('+', *level, span.line as usize);
                e = ca65_flat::stamp_file(e, span.file);
                return Err(e);
            }
        }
        Ok(())
    }
}

/// The "no anonymous label `:{run}` in that direction" diagnostic.
fn no_anon(sign: char, level: usize, line: usize) -> AsmError {
    let run: String = std::iter::repeat_n(sign, level).collect();
    AsmError::new(
        line,
        format!("no anonymous label `:{run}` in that direction"),
    )
}

/// A `:`-anonymous reference token (`:-`, `:--`, `:+`, `:++`, …): its sign and
/// run length. `:` alone (no run) is a definition, not a reference.
fn anon_ref(tok: &str) -> Option<(char, usize)> {
    let rest = tok.strip_prefix(':')?;
    let mut chars = rest.chars();
    let first = chars.next()?;
    if (first == '-' || first == '+') && rest.chars().all(|c| c == first) {
        Some((first, rest.len()))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse ca65 (NES) source into the source-preserving semantic
/// [`Program`](crate::ast::Program) — the single front-end IR the assemble+link
/// driver and the `--fmt` formatter both consume. Each line becomes a node
/// carrying its label (the **source form** — `name`, `@cheap`, or empty for an
/// anonymous `:` — for the formatter, and the resolved name for assembly), the
/// verbatim operation source, and comment trivia.
///
/// The ca65 statement [`Kind`] is the family-owned native payload
/// (`decisions/ast-native-payload-for-multipass-cisc.md`); `=` constants are the
/// shared [`Item::Equ`](crate::ast::Item), folded in source order so `.res`
/// counts and the zero-page size decision see earlier definitions; a `.segment`
/// directive is a byte-neutral source-only node the assembly projection reads to
/// track the active segment.
pub(crate) fn parse_program(
    set: &'static isa::InstructionSet,
    source: &str,
    mode: macros::Expand,
) -> Result<crate::ast::Program, AsmError> {
    // Macros expand before parsing (#93), but only for assembly: the formatter
    // asks with `Expand::No`, because laying source out must not replace a
    // definition with its expansions.
    let expanded = ca65_flat::expand_ca65(source, mode)?;
    let text = macros::expanded_text(&expanded, source);
    let origins = macros::line_origins(&expanded);
    let mut w = Walker::new(set);
    // The shared cursor: it groups `.if`/`.repeat` blocks and keeps every
    // include unresolved (KTD1), which is what `--fmt` needs — it renders the
    // directive verbatim and works with a missing target. Macros expanded
    // above, so this parse reads the expanded text.
    ca65_flat::walk_source_expanded(&mut w, text, FileId(0))
        .map_err(|e| macros::remap_lines(e, origins))?;
    let mut program = w.finish(text.lines().count() as u32)?;
    macros::place_nodes(&mut program.nodes, origins);
    Ok(program)
}

/// Parse a multi-file NES ca65 program (language-surface U5, KTD1): the
/// **interleaved, environment-threaded walk** over the source map, resolving
/// `.include`/`.incbin` lazily through `loader` under ca65's probe-pinned
/// semantics ([`CA65_SEMANTICS`](ca65_flat::CA65_SEMANTICS) — ancestor-chain
/// resolution, the negative-size incbin sentinel; re-confirmed under the NES
/// link). Everything the parse accumulates crosses include boundaries in both
/// directions, exactly as ca65's textual splice does (probe-pinned):
/// `=` constants, the cheap-local scope (`current_global`), the
/// anonymous-label stream, and — via the projection reading the spliced node
/// order — the active `.segment` (a switch inside an include persists into
/// the includer afterwards).
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing target, an include cycle, a bad `.incbin` window, or the depth
/// backstop — all at the directive's span.
pub(crate) fn parse_program_multi(
    set: &'static isa::InstructionSet,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<crate::ast::Program, AsmError> {
    let mut w = Walker::new(set);
    let root = map.contents(FileId(0)).unwrap_or_default().to_owned();
    let root_lines = root.lines().count() as u32;
    let mut stack = vec![FileId(0)];
    ca65_flat::walk_file(
        &mut w,
        &root,
        FileId(0),
        map,
        loader,
        &mut stack,
        &ca65_flat::CA65_SEMANTICS,
    )?;
    w.finish(root_lines)
}

/// The per-line parse walk shared by [`parse_program`] (single source) and
/// [`parse_program_multi`] (the include-capable walk). The environment — the
/// `=` constants, the cheap-local scope, the anonymous-label stream, and
/// pending comment trivia — lives here, so in the multi-file walk it threads
/// *through* include boundaries in both directions (KTD1, probe-pinned).
struct Walker {
    set: &'static isa::InstructionSet,
    anons: AnonCtx,
    current_global: String,
    consts: BTreeMap<String, i64>,
    pending_leading: Vec<crate::ast::Comment>,
    nodes: Vec<crate::ast::Node>,
    /// The scopes open at this line, outermost first — what `.proc`/`.scope`
    /// push and their closers pop. Names defined here are qualified with it.
    scopes: Vec<OpenScope>,
    /// Every scope path opened so far, whether or not it is still open. Two
    /// uses: ca65 refuses a second scope of the same name in one parent
    /// (`Duplicate scope 'sa'`), and a `sa::v` reference may only name a scope
    /// already opened above it (`No such scope: 'sa'`) — both positional.
    opened: BTreeSet<String>,
    /// Counts the unnamed `.proc`s, so each gets a path segment of its own.
    unnamed: usize,
    /// The record definitions open here, outermost first. A `.struct` may hold
    /// another, and the inner one allocates its size in the outer.
    records: Vec<RecordBuild>,
}

/// A `.struct`, `.union` or `.enum` being read.
struct RecordBuild {
    kind: RecordKind,
    /// The scope its names are defined in — the path opened by the header.
    path: String,
    /// A struct's next offset, an enum's next value. A union lays every field
    /// at zero, so this stays there and the size is the widest member.
    cursor: i64,
    /// The widest field, for a union.
    widest: i64,
    line: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordKind {
    Struct,
    Union,
    Enum,
}

impl RecordKind {
    /// The word that opens it, and the word that closes it. ca65 pairs them
    /// strictly: `.endstruct` inside an `.enum` is read as a member name, and
    /// answered `Identifier expected`.
    fn words(self) -> (&'static str, &'static str) {
        match self {
            RecordKind::Struct => (".struct", ".endstruct"),
            RecordKind::Union => (".union", ".endunion"),
            RecordKind::Enum => (".enum", ".endenum"),
        }
    }
}

impl RecordBuild {
    /// What `.sizeof` answers for it: how far a struct got, the widest member
    /// of a union.
    fn size(&self) -> i64 {
        match self.kind {
            RecordKind::Union => self.widest,
            _ => self.cursor,
        }
    }
}

/// One open scope.
struct OpenScope {
    /// Its path segment. An unnamed `.proc` gets a synthetic one, which cannot
    /// collide because [`LABEL_SEP`] is not valid in source.
    segment: String,
    /// `.proc` (true) or `.scope` (false). ca65 pairs them by kind: `.endscope`
    /// on an open `.proc` is `No open .SCOPE`, and the reverse is `No open
    /// .PROC`.
    is_proc: bool,
    /// Where it was opened, so an unclosed one can say where to look.
    line: usize,
    /// The cheap-local scope in force outside it, restored when it closes.
    outer_global: String,
}

impl Walker {
    fn new(set: &'static isa::InstructionSet) -> Self {
        Self {
            set,
            anons: AnonCtx::default(),
            current_global: String::new(),
            consts: BTreeMap::new(),
            pending_leading: Vec::new(),
            nodes: Vec::new(),
            scopes: Vec::new(),
            opened: BTreeSet::new(),
            unnamed: 0,
            records: Vec::new(),
        }
    }

    /// Close the walk: flush trailing comments (a trailing block or a
    /// comment-only file), then fail any forward anonymous reference whose
    /// definition never arrived.
    fn finish(mut self, last_line: u32) -> Result<crate::ast::Program, AsmError> {
        use crate::ast::{Node, Program, Span, Trivia};
        if !self.pending_leading.is_empty() {
            self.nodes.push(Node {
                operand_span: None,
                label: None,
                item: None,
                source: String::new(),
                span: Span::at(last_line, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing: None,
                },
            });
        }
        if let Some(open) = self.records.last() {
            let (opener, closer) = open.kind.words();
            return Err(AsmError::new(
                open.line,
                format!("`{opener}` is never closed — `{closer}` is missing"),
            ));
        }
        // ca65: `Local scope was not closed`, reported at the end of the file.
        if let Some(open) = self.scopes.last() {
            let word = if open.is_proc { ".proc" } else { ".scope" };
            return Err(AsmError::new(
                open.line,
                format!("`{word}` is never closed"),
            ));
        }
        self.anons.check()?;
        Ok(Program { nodes: self.nodes })
    }

    /// The path of the scopes open here.
    fn path(&self) -> Vec<String> {
        self.scopes.iter().map(|s| s.segment.clone()).collect()
    }

    /// Recognise `.proc`/`.scope` and their closers, and act on them.
    ///
    /// `Some(label)` is what the line defines: `.proc name` defines `name` as a
    /// label at this address, in the *enclosing* scope, and opens a scope of
    /// that name; `.scope name` opens one and defines nothing. Both closers
    /// define nothing, and are matched by kind.
    fn scope_directive(
        &mut self,
        code: &str,
        line: usize,
    ) -> Result<Option<Option<String>>, AsmError> {
        let (word, rest) = split_first_word(code.trim());
        let rest = rest.trim();
        let word = word.to_ascii_lowercase();
        let closing = match word.as_str() {
            ".endproc" => Some(true),
            ".endscope" => Some(false),
            _ => None,
        };
        if let Some(is_proc) = closing {
            match self.scopes.last() {
                Some(open) if open.is_proc == is_proc => {
                    let open = self.scopes.pop().expect("just matched");
                    self.current_global = open.outer_global;
                    return Ok(Some(None));
                }
                // ca65 pairs the closers by kind, so `.endscope` over an open
                // `.proc` is not a mismatch report but `No open .SCOPE`.
                _ => {
                    let opener = if is_proc { ".proc" } else { ".scope" };
                    return Err(AsmError::new(
                        line,
                        format!("`{word}` with no open `{opener}`"),
                    ));
                }
            }
        }
        let is_proc = match word.as_str() {
            ".proc" => true,
            ".scope" => false,
            _ => return Ok(None),
        };
        // An unnamed `.proc` is real ca65 — deprecated, and it still opens a
        // scope. The synthetic segment cannot be named from source, which is
        // exactly what an unnamed scope means. (ca65 also warns; a parse-time
        // warning has nowhere to go here, and the bytes are what this claims.)
        let name = if rest.is_empty() {
            if !is_proc {
                return Err(AsmError::new(line, "`.scope` needs a name"));
            }
            self.unnamed += 1;
            format!("{LABEL_SEP}proc#{}", self.unnamed)
        } else {
            if !is_ident(rest) {
                return Err(AsmError::new(line, format!("invalid scope name `{rest}`")));
            }
            rest.to_string()
        };
        let path = qualify(&self.path(), &name);
        // ca65: `Duplicate scope 'sa'` — a name may be opened once per parent.
        if !self.opened.insert(path.clone()) {
            return Err(AsmError::new(line, format!("duplicate scope `{name}`")));
        }
        let label = (is_proc && !rest.is_empty()).then(|| qualify(&self.path(), &name));
        let outer_global = std::mem::replace(
            &mut self.current_global,
            label.clone().unwrap_or_else(|| path.clone()),
        );
        self.scopes.push(OpenScope {
            segment: name,
            is_proc,
            line,
            outer_global,
        });
        Ok(Some(label))
    }

    /// Open a scope that is not a `.proc`/`.scope` — a record's name is one
    /// too, which is how `Point::px` is spelled.
    fn push_scope(&mut self, name: &str, line: usize) -> Result<String, AsmError> {
        let path = qualify(&self.path(), name);
        if !self.opened.insert(path.clone()) {
            return Err(AsmError::new(line, format!("duplicate scope `{name}`")));
        }
        let outer_global = std::mem::replace(&mut self.current_global, path.clone());
        self.scopes.push(OpenScope {
            segment: name.to_string(),
            is_proc: false,
            line,
            outer_global,
        });
        Ok(path)
    }

    /// The size a record declares, which `.sizeof` reads back. Stored as an
    /// ordinary constant under a key source cannot spell, so it resolves
    /// through the same scope lookup every other name uses.
    fn record_size_key(path: &str) -> String {
        format!("{path}{LABEL_SEP}size")
    }

    /// The size of the record `name` stands for here, looked up outward
    /// through the open scopes. The *size key* is what is searched for, not the
    /// name: a record declares no constant under its own name, so searching for
    /// that would walk past the scope holding it.
    fn record_size(&self, name: &str) -> Option<i64> {
        let mut segments = self.path();
        loop {
            let key = Self::record_size_key(&qualify(&segments, name));
            if let Some(&size) = self.consts.get(&key) {
                return Some(size);
            }
            segments.pop()?;
        }
    }

    /// `.struct`/`.union`/`.enum` and their closers, and — while one is open —
    /// every line inside it, which declares a field rather than a statement.
    ///
    /// `Some(kind)` is what the line assembles to: nothing at all for a
    /// declaration (they emit no bytes and define constants), which the caller
    /// keeps as a source-only node so the formatter reproduces it.
    fn record_line(&mut self, code: &str, line: usize) -> Result<Option<Vec<Kind>>, AsmError> {
        let (first, rest) = split_first_word(code.trim());
        let word = first.to_ascii_lowercase();
        // A closer, matched by kind.
        for kind in [RecordKind::Struct, RecordKind::Union, RecordKind::Enum] {
            let (_, closer) = kind.words();
            if word != closer {
                continue;
            }
            let Some(open) = self.records.last() else {
                return Err(AsmError::new(line, format!("`{word}` with no open record")));
            };
            if open.kind != kind {
                let (opener, _) = open.kind.words();
                return Err(AsmError::new(
                    line,
                    format!("`{word}` closes a `{opener}` opened at line {}", open.line),
                ));
            }
            let done = self.records.pop().expect("just matched");
            let key = Self::record_size_key(&done.path);
            self.consts.insert(key.clone(), done.size());
            if let Some(open) = self.scopes.pop() {
                self.current_global = open.outer_global;
            }
            // A record declared inside another allocates its size there, the
            // way an unnamed field of that type would.
            if done.kind != RecordKind::Enum {
                self.allocate(done.size());
            }
            return Ok(Some(vec![Kind::Constant(key, done.size())]));
        }
        // An opener, whether or not one is already open.
        let kind = match word.as_str() {
            ".struct" => Some(RecordKind::Struct),
            ".union" => Some(RecordKind::Union),
            ".enum" => Some(RecordKind::Enum),
            _ => None,
        };
        if let Some(kind) = kind {
            let name = rest.trim();
            let name = if name.is_empty() {
                self.unnamed += 1;
                format!("{LABEL_SEP}rec#{}", self.unnamed)
            } else {
                if !is_ident(name) {
                    return Err(AsmError::new(line, format!("invalid record name `{name}`")));
                }
                name.to_string()
            };
            let path = self.push_scope(&name, line)?;
            self.records.push(RecordBuild {
                kind,
                path,
                cursor: 0,
                widest: 0,
                line,
            });
            return Ok(Some(Vec::new()));
        }
        // Not a record word. Inside a record, every other line is a member.
        if self.records.is_empty() {
            return Ok(None);
        }
        let defined = if self.records.last().expect("open").kind == RecordKind::Enum {
            self.enum_member(code.trim(), line)?
        } else {
            self.record_field(code.trim(), line)?
        };
        Ok(Some(defined))
    }

    /// Advance the open record by `size` bytes: a struct moves on, a union
    /// keeps the widest member seen.
    fn allocate(&mut self, size: i64) {
        if let Some(open) = self.records.last_mut() {
            match open.kind {
                RecordKind::Union => open.widest = open.widest.max(size),
                _ => open.cursor += size,
            }
        }
    }

    /// `name .byte`, `name .res 8`, `name .tag Other 3`, or any of those with
    /// the name left out. The name binds to the field's offset — zero for every
    /// member of a union — and the record moves on by the field's size.
    fn record_field(&mut self, text: &str, line: usize) -> Result<Vec<Kind>, AsmError> {
        let (head, tail) = split_first_word(text);
        let (name, allocator, args) = if head.starts_with('.') {
            ("", head, tail.trim())
        } else {
            let (word, rest) = split_first_word(tail.trim());
            (head, word, rest.trim())
        };
        if !allocator.starts_with('.') {
            return Err(AsmError::new(
                line,
                format!("`{allocator}` is not a storage allocator ca65 has"),
            ));
        }
        let count = |text: &str, default: i64| -> Result<i64, AsmError> {
            if text.is_empty() {
                return Ok(default);
            }
            let expr = parse_value(&self.anons, &self.current_global, text, line)?;
            fold_const(&expr, &self.consts, line)
        };
        // `.tag` names another record, whose size this field takes. It is
        // resolved here rather than later because ca65 requires the record to
        // be declared above the field that uses it.
        let (unit, count) = match allocator.to_ascii_lowercase().as_str() {
            ".byte" | ".res" => (1, count(args, 1)?),
            ".word" | ".addr" | ".dbyt" => (2, count(args, 1)?),
            ".faraddr" => (3, count(args, 1)?),
            ".dword" => (4, count(args, 1)?),
            ".tag" => {
                let (record, rest) = split_first_word(args);
                let Some(size) = self.record_size(record) else {
                    return Err(AsmError::new(
                        line,
                        format!("`.tag` names `{record}`, which is not a record declared above it"),
                    ));
                };
                (size, count(rest.trim(), 1)?)
            }
            _ => {
                return Err(AsmError::new(
                    line,
                    format!("`{allocator}` is not a storage allocator ca65 has"),
                ));
            }
        };
        let open = self.records.last().expect("a record is open");
        let offset = match open.kind {
            RecordKind::Union => 0,
            _ => open.cursor,
        };
        let size = unit * count;
        let mut defined = Vec::new();
        if !name.is_empty() {
            if !is_ident(name) {
                return Err(AsmError::new(line, format!("invalid field name `{name}`")));
            }
            let key = qualify(&self.path(), name);
            // The field's own size, so `.sizeof(Outer::field)` answers.
            let size_key = Self::record_size_key(&key);
            self.consts.insert(size_key.clone(), size);
            self.consts.insert(key.clone(), offset);
            defined.push(Kind::Constant(size_key, size));
            defined.push(Kind::Constant(key, offset));
        }
        self.allocate(size);
        Ok(defined)
    }

    /// `name` or `name = expr`. The counter carries on from the last member, so
    /// `a = 5` followed by a bare `b` is 5 then 6.
    fn enum_member(&mut self, text: &str, line: usize) -> Result<Vec<Kind>, AsmError> {
        let (name, value) = match text.split_once('=') {
            Some((name, expr)) => {
                let expr = parse_value(&self.anons, &self.current_global, expr.trim(), line)?;
                (name.trim(), Some(fold_const(&expr, &self.consts, line)?))
            }
            None => (text, None),
        };
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid enum member `{name}`")));
        }
        let open = self.records.last_mut().expect("a record is open");
        let value = value.unwrap_or(open.cursor);
        open.cursor = value + 1;
        let key = qualify(&self.path(), name);
        self.consts.insert(key.clone(), value);
        Ok(vec![Kind::Constant(key, value)])
    }

    /// `.tag Record [count]` outside a record declaration: reserve one
    /// instance's worth of space, the way `.res` does. The label on the line
    /// binds at its start, like any other, so the caller keeps handling that.
    ///
    /// Read here rather than in `parse_directive` because the record's size is
    /// a name, and only the walker knows the scopes it must be looked up
    /// through.
    fn tag_statement(&self, rest: &str, line: usize) -> Result<Option<Kind>, AsmError> {
        let (word, args) = split_first_word(rest.trim());
        if !word.eq_ignore_ascii_case(".tag") {
            return Ok(None);
        }
        let (record, trailing) = split_first_word(args.trim());
        if record.is_empty() {
            return Err(AsmError::new(line, "`.tag` needs a record name"));
        }
        // A *field* may be `.tag Inner 3`; a statement may not. ca65 answers
        // the count here with `Unexpected trailing garbage characters`.
        if !trailing.trim().is_empty() {
            return Err(AsmError::new(
                line,
                format!(
                    "`.tag {record}` takes nothing after the record name here — \
                         a repeat count is only a struct field's to give"
                ),
            ));
        }
        let Some(size) = self.record_size(record) else {
            return Err(AsmError::new(
                line,
                format!("`.tag` names `{record}`, which is not a record declared above it"),
            ));
        };
        let bytes = usize::try_from(size)
            .map_err(|_| AsmError::new(line, "`.tag` reserves a negative amount of space"))?;
        Ok(Some(Kind::Res(bytes, 0)))
    }

    /// A name written here, as the key it is stored or looked up under.
    ///
    /// A constant is answered now: ca65 requires an `=` above the line that
    /// uses it, so the innermost enclosing scope holding one is already known,
    /// and answering here keeps `.if`, `.res` and every other parse-time fold
    /// working inside a scope. A label cannot be, so it becomes a
    /// [`scope_ref`] for [`scope_targets`] to answer once the file is read.
    fn scoped_name(&self, name: &str) -> String {
        let mut segments = self.path();
        loop {
            let candidate = qualify(&segments, name);
            if self.consts.contains_key(&candidate) {
                return candidate;
            }
            if segments.pop().is_none() {
                break;
            }
        }
        scope_ref(&self.path(), name)
    }

    /// `sa::v` — the scope named must already be open or closed above this
    /// line, searched outward from here, which is ca65's `No such scope`.
    fn resolve_qualified(&self, text: &str, line: usize) -> Result<String, AsmError> {
        let (head, rest) = text.split_once("::").expect("caller checked");
        let mut segments = self.path();
        loop {
            let candidate = qualify(&segments, head);
            if self.opened.contains(&candidate) {
                return Ok(format!("{candidate}::{rest}"));
            }
            if segments.pop().is_none() {
                break;
            }
        }
        Err(AsmError::new(line, format!("no such scope `{head}`")))
    }

    /// Rewrite every name in a statement to the key it resolves under. A no-op
    /// outside a scope, except for the `::name` spelling, which means the top
    /// level from anywhere.
    fn scope_syms(&self, kind: &Kind, line: usize) -> Result<Kind, AsmError> {
        let failure: std::cell::RefCell<Option<AsmError>> = std::cell::RefCell::new(None);
        let out = map_kind_syms(kind, &|name: &str| {
            // Anonymous labels, cheap locals and the `.defined` marker are
            // already keys rather than source names.
            if name.starts_with(LABEL_SEP) {
                return None;
            }
            if let Some(global) = name.strip_prefix("::") {
                return Some(Expr::Sym(global.to_string()));
            }
            if name.contains("::") {
                return match self.resolve_qualified(name, line) {
                    Ok(key) => Some(Expr::Sym(key)),
                    Err(e) => {
                        *failure.borrow_mut() = Some(e);
                        None
                    }
                };
            }
            if self.scopes.is_empty() {
                None
            } else {
                Some(Expr::Sym(self.scoped_name(name)))
            }
        });
        match failure.into_inner() {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    /// Recognise a walk-handled `.include`/`.incbin` operation, parsing the
    /// `.incbin` offset/size against the live environment (a forward reference
    /// is ca65's "Constant expression expected" posture, probe-pinned).
    fn walk_directive(&self, rest: &str, line: usize) -> Result<Option<WalkDirective>, AsmError> {
        let (word, args) = split_first_word(rest);
        match word.to_ascii_lowercase().as_str() {
            ".include" => Ok(Some(WalkDirective::Include {
                request: ca65_flat::include_request(args, line, ".include")?,
            })),
            ".incbin" => {
                let fold = |piece: &str| {
                    fold_const(
                        &parse_value(&self.anons, &self.current_global, piece, line)?,
                        &self.consts,
                        line,
                    )
                };
                let (request, offset, size) = ca65_flat::incbin_args(args, line, ".incbin", &fold)?;
                Ok(Some(WalkDirective::Incbin {
                    request,
                    offset,
                    size,
                }))
            }
            _ => Ok(None),
        }
    }
}

impl FlatWalk for Walker {
    /// ca65's block vocabulary, measured against ca65 V2.18 and
    /// case-insensitive: `.if` and its nine sibling heads, `.elseif`, `.else`,
    /// `.endif`, and `.repeat` / `.endrepeat`.
    ///
    /// Every head ca65 has is here. The last two to arrive, `.ifref` and
    /// `.ifnref`, ask whether a symbol has been *used* above the line, which
    /// the projection now records as it goes.
    fn block_keyword(&self, code: &str) -> Option<ca65_flat::BlockKw> {
        use ca65_flat::BlockKw;
        let word = code.split_whitespace().next()?.to_ascii_lowercase();
        Some(match word.as_str() {
            ".if" | ".ifdef" | ".ifndef" | ".ifblank" | ".ifnblank" | ".ifconst" | ".ifnconst"
            | ".ifref" | ".ifnref" => BlockKw::CondOpen,
            // The CPU tests. This leg assembles for one CPU and refuses
            // `.setcpu`, so which of them is true is fixed before the file is
            // read — but they still open a block, and the dead branch still
            // has to be skipped rather than parsed.
            ".ifp02" | ".ifp4510" | ".ifp816" | ".ifpc02" | ".ifpsc02" => BlockKw::CondOpen,
            ".elseif" => BlockKw::ElseIf,
            ".else" => BlockKw::Else,
            ".endif" => BlockKw::CondClose,
            ".repeat" => BlockKw::RepeatOpen,
            ".endrepeat" => BlockKw::RepeatClose,
            _ => return None,
        })
    }

    fn nodes_mut(&mut self) -> &mut Vec<crate::ast::Node> {
        &mut self.nodes
    }

    /// The multi-file walk expands too, or macros would work when a file is
    /// assembled alone and vanish the moment it is included from another.
    fn expand_source(&self, source: &str) -> Result<macros::Expansion, AsmError> {
        ca65_flat::expand_ca65(source, macros::Expand::Yes)
    }

    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        use crate::ast::{Comment, Item, Node, Scope, Span, Symbol, Trivia};
        // Deferred anonymous-reference records need the current file; the
        // parse helpers below only know their line.
        self.anons.file.set(file);
        let (code, comment) = split_comment(raw);
        let trimmed = code.trim();
        if trimmed.is_empty() {
            if let Some(text) = comment {
                self.pending_leading.push(Comment {
                    text: text.to_string(),
                    span: Span::in_file(file, line as u32, 1),
                });
            }
            return Ok(None);
        }
        let trailing = comment.map(|text| Comment {
            text: text.to_string(),
            span: Span::in_file(file, line as u32, (code.len() + 1) as u32),
        });
        let span = Span::in_file(file, line as u32, 1);

        // `.segment "NAME"`, its shorthands (`.code`, `.zeropage`, …) and the
        // `.pushseg`/`.popseg` stack all switch the active segment — kept as
        // source-only nodes so the formatter reproduces them; the projection
        // reads them back to track the active segment (parse itself needs no
        // segment state).
        if is_segment_directive(trimmed) {
            self.nodes.push(Node {
                operand_span: None,
                label: None,
                item: None,
                source: trimmed.to_string(),
                span,
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        // `.struct`/`.union`/`.enum`, their closers, and — while one is open —
        // every line inside, which declares a field rather than a statement.
        // Source-only nodes: a record emits no bytes, it defines constants.
        if let Some(defined) = self.record_line(trimmed, line)? {
            // The line itself, so the formatter reproduces the declaration, and
            // then one node per constant it defined — carried as items so the
            // projection folds them where the source put them.
            self.nodes.push(Node {
                operand_span: None,
                label: None,
                item: None,
                source: trimmed.to_string(),
                span: span.clone(),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            for kind in defined {
                self.nodes.push(Node {
                    operand_span: None,
                    label: None,
                    item: Some(Item::Native(Box::new(kind))),
                    source: String::new(),
                    span: span.clone(),
                    trivia: Trivia::default(),
                });
            }
            return Ok(None);
        }

        // `.proc`/`.scope` and their closers. Kept as source-only nodes like the
        // segment directives, so the formatter reproduces them; `.proc name`
        // carries the label it defines, which the projection places at this
        // address like any label-only line.
        if let Some(label) = self.scope_directive(trimmed, line)? {
            self.nodes.push(Node {
                operand_span: None,
                label: label.map(|qualified| Symbol {
                    name: qualified.rsplit("::").next().unwrap_or("").to_string(),
                    scope: Scope::Global,
                    qualified,
                }),
                item: None,
                source: trimmed.to_string(),
                span,
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        // `NAME = expr` defines a constant — the shared `Item::Equ`, folded in
        // source order (later statements' size decisions see it).
        if let Some(eq) = assignment_split(trimmed) {
            let name = trimmed[..eq].trim();
            if !is_ident(name) {
                return Err(AsmError::new(
                    line,
                    format!("invalid constant name `{name}`"),
                ));
            }
            let expr = parse_value(&self.anons, &self.current_global, &trimmed[eq + 1..], line)?;
            let expr = match self.scope_syms(&Kind::Bytes(vec![expr]), line)? {
                Kind::Bytes(mut es) => es.pop().expect("one expression in, one out"),
                _ => unreachable!("Bytes maps to Bytes"),
            };
            // A constant inside a scope belongs to it: `v` in `.proc pa` is
            // `pa::v`, and `::v` outside is a different symbol.
            let name = qualify(&self.path(), name);
            let name = name.as_str();
            if let Ok(v) = fold_const(&expr, &self.consts, line) {
                self.consts.insert(name.to_string(), v);
            }
            self.nodes.push(Node {
                operand_span: None,
                label: Some(Symbol {
                    qualified: name.to_string(),
                    scope: Scope::Global,
                    name: name.to_string(),
                }),
                item: Some(crate::ast::item_from_operation(Operation::Equ(expr))),
                source: trimmed[eq..].trim().to_string(),
                span,
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        // An optional `name:` / `@cheap:` / `:` label, then an optional operation.
        let (symbol, rest) =
            split_label_symbol(&self.anons, line, &mut self.current_global, trimmed)?;
        // `.include`/`.incbin` are walk-handled, not parsed here: the target
        // must not be opened by the parse (KTD1 — `--fmt` succeeds with a
        // missing target), so hand them back for the driver to resolve (or
        // keep unresolved, in the single-source parse).
        if let Some(kind) = self.walk_directive(rest, line)? {
            return Ok(Some(DirectiveLine {
                kind,
                label: symbol,
                source: rest.trim().to_string(),
                span,
                operand_span: ca65_flat::directive_operand_span(raw, rest, line, file),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            }));
        }
        let kind = match self.tag_statement(rest, line)? {
            Some(kind) => kind,
            None => parse_op(
                self.set,
                &self.anons,
                &self.current_global,
                &self.consts,
                rest,
                line,
            )?,
        };
        // Every name the statement mentions, as the key it resolves under.
        let kind = self.scope_syms(&kind, line)?;
        // A label inside a scope is defined in it, and takes the cheap-local
        // scope with it so `@l` in two procs is two labels.
        let symbol = symbol.map(|mut s| {
            // Only a plain label: a cheap local is already keyed on its global
            // (which is itself qualified, so two procs' `@l` stay apart), and an
            // anonymous one is positional rather than named.
            if !self.scopes.is_empty()
                && matches!(s.scope, crate::ast::Scope::Global)
                && !s.qualified.starts_with(LABEL_SEP)
            {
                s.qualified = qualify(&self.path(), &s.qualified);
                self.current_global = s.qualified.clone();
            }
            s
        });
        let trivia = Trivia {
            leading: std::mem::take(&mut self.pending_leading),
            trailing,
        };
        match (symbol, kind) {
            // A label-less empty line — nothing to place or format (unreachable
            // in practice; a label-less operation never folds to `Empty`).
            (None, Kind::Empty) => self.pending_leading = trivia.leading,
            // A label with no operation: keep the label so the projection places
            // it as an empty statement and records its address.
            (symbol, Kind::Empty) => self.nodes.push(Node {
                operand_span: None,
                label: symbol,
                item: None,
                source: String::new(),
                span,
                trivia,
            }),
            (symbol, kind) => self.nodes.push(Node {
                operand_span: None,
                label: symbol,
                item: Some(Item::Native(Box::new(kind))),
                source: rest.trim().to_string(),
                span,
                trivia,
            }),
        }
        Ok(None)
    }

    fn push_node(&mut self, node: crate::ast::Node) {
        self.nodes.push(node);
    }
}

/// Project the semantic [`Program`](crate::ast::Program) into the assembler's
/// [`Parsed`] — the assemble+link driver runs on an owned `Vec<Stmt>` plus the
/// label→segment and constant maps. Everything is read straight back out of the
/// tree (nothing is re-parsed): a native [`Kind`] payload becomes a placed
/// statement in the segment tracked from the `.segment` nodes, a label-only node
/// becomes an empty placed statement, and an `Item::Equ` node folds into the
/// constant table in source order.
fn parsed_from_program(program: &crate::ast::Program) -> Result<Parsed, AsmError> {
    let mut st = Projection {
        seg: "CODE".to_string(),
        seg_stack: Vec::new(),
        stmts: Vec::new(),
        label_seg: BTreeMap::new(),
        consts: BTreeMap::new(),
        referenced: BTreeSet::new(),
        loop_vars: Vec::new(),
    };
    project_nodes(&program.nodes, &mut st)?;
    Ok(Parsed {
        stmts: st.stmts,
        label_seg: st.label_seg,
        consts: st.consts,
    })
}

/// The projection's running state: everything a later line's fold can see.
struct Projection {
    seg: String,
    /// Segments saved by `.pushseg`, innermost last; `.popseg` restores one.
    seg_stack: Vec<String>,
    stmts: Vec<Stmt>,
    label_seg: BTreeMap<String, String>,
    consts: BTreeMap<String, i64>,
    /// Every name the statements *above this point* have mentioned, which is
    /// what `.ref`/`.referenced` and `.ifref`/`.ifnref` ask about. Dead
    /// branches never reach here, so a use inside one does not count — probed
    /// against V2.18, where `.if 0 / .word L / .endif` leaves `.ref(L)` at 0.
    referenced: BTreeSet<String>,
    /// Loop variables bound by enclosing `.repeat`s, innermost last. Kept apart
    /// from `consts` because ca65 **scopes one to its loop** — `lda #i` after
    /// `.endrepeat` is `Symbol 'i' is undefined`, where acme's `!for` variable
    /// survives its block. Two dialects, two rules.
    loop_vars: Vec<(String, i64)>,
}

impl Projection {
    /// The constants a fold may see here: the file's, plus any enclosing loop
    /// variables shadowing them.
    fn env(&self) -> BTreeMap<String, i64> {
        let mut env = self.consts.clone();
        for (name, value) in &self.loop_vars {
            env.insert(name.clone(), *value);
        }
        env
    }
}

/// Project a run of nodes, folding any conditional or repetition **once, in
/// source order, before layout** — `decisions/conditionals-in-multipass-dialects.md`.
///
/// No layout state is consulted because a ca65 condition cannot reach any: the
/// reference refuses `*` and even a backward label in one, since a ca65 label is
/// relocatable until `ld65` links it and so is never a constant expression.
fn project_nodes(nodes: &[crate::ast::Node], st: &mut Projection) -> Result<(), AsmError> {
    use crate::ast::Item;
    for node in nodes {
        match &node.item {
            Some(Item::Conditional {
                head,
                then_body,
                else_body,
                ..
            }) => {
                if fold_condition(head, st, node.span.line as usize)? {
                    project_nodes(then_body, st)?;
                } else if let Some(body) = else_body {
                    project_nodes(body, st)?;
                }
                continue;
            }
            Some(Item::Repeat { head, body, .. }) => {
                let (count, var) = fold_repeat(head, st, node.span.line as usize)?;
                for i in 0..count {
                    if let Some(name) = &var {
                        st.loop_vars.push((name.clone(), i));
                    }
                    let first = st.stmts.len();
                    let out = project_nodes(body, st);
                    if var.is_some() {
                        // Bake this pass's value into everything the body just
                        // produced, then drop the binding: ca65 scopes a loop
                        // variable to its loop.
                        let vars = st.loop_vars.clone();
                        for stmt in &mut st.stmts[first..] {
                            stmt.kind = map_kind_syms(&stmt.kind, &loop_var_sub(&vars));
                        }
                        st.loop_vars.pop();
                    }
                    out?;
                }
                continue;
            }
            _ => {}
        }
        project_one(node, st)?;
    }
    Ok(())
}

/// Fold a `.if` / `.ifdef` / `.ifndef` / `.elseif` head.
fn fold_condition(head: &str, st: &Projection, line: usize) -> Result<bool, AsmError> {
    let (word, args) = split_first_word(head.trim());
    let word = word.to_ascii_lowercase();
    let args = args.trim();
    let env = st.env();
    match word.as_str() {
        ".ifdef" | ".ifndef" => {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            let defined = env.contains_key(name) || st.label_seg.contains_key(name);
            Ok(if word == ".ifdef" { defined } else { !defined })
        }
        // `.ifblank` asks whether anything follows it on the line — which is
        // how a macro tests an argument it may not have been given, the
        // expansion being textual. ca65 counts tokens, not characters, so
        // whitespace alone is still blank.
        ".ifblank" | ".ifnblank" => Ok((word == ".ifblank") == args.is_empty()),
        // The CPU tests. This leg is a 6502 and refuses `.setcpu`, so no
        // reachable source can make one of the others true.
        // `.ifref` asks the same question `.ref` does, from the same set.
        ".ifref" | ".ifnref" => {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            Ok((word == ".ifref") == st.referenced.contains(name))
        }
        ".ifp02" => Ok(true),
        ".ifp4510" | ".ifp816" | ".ifpc02" | ".ifpsc02" => Ok(false),
        // `.ifconst` is not `.if`: it asks whether the expression *is* a
        // constant, not what it comes to, and answers rather than failing when
        // it is not. ca65's rule is the linker's — probed against V2.18:
        //
        //   `.const(LA - LB)` in one segment is 1, across two is 0;
        //   `.const(LA + LA)` and `.const(LA * 2)` are 0;
        //   `.const(LB - LA)` with `LB` *below* the line is 0.
        //
        // So a label is not constant, a difference of two labels above the line
        // in one segment is, and anything that is not linear in the labels is
        // not. See [`segment_weights`].
        ".ifconst" | ".ifnconst" => {
            if args.is_empty() {
                return Err(AsmError::new(line, format!("`{word}` needs an expression")));
            }
            let expr = parse_value(&AnonCtx::default(), "", args, line)?;
            let expr = map_expr_syms(&expr, &resolve_defined(&st.consts, &st.label_seg));
            Ok((word == ".ifconst") == weighs_nothing(&expr, st))
        }
        ".if" | ".elseif" => {
            if args.is_empty() {
                return Err(AsmError::new(line, format!("`{word}` needs a condition")));
            }
            let expr = parse_value(&AnonCtx::default(), "", args, line)?;
            // `.if .defined(X)` — answered against what stands above this line,
            // which is what makes `.defined` positional in the first place.
            let expr = map_expr_syms(&expr, &resolve_defined(&st.consts, &st.label_seg));
            // ca65: `Constant expression expected` — a condition may not reach
            // forward, and a ca65 label is never constant.
            let value = fold_const(&expr, &env, line).map_err(|_| {
                AsmError::new(
                    line,
                    format!(
                        "`{args}` must be a constant here — ca65 folds a condition against the                          `=` constants above it, and refuses a label or a forward reference"
                    ),
                )
            })?;
            Ok(value != 0)
        }
        _ => Err(AsmError::new(
            line,
            format!("internal error: `{head}` is not a conditional head"),
        )),
    }
}

/// How much of each segment an expression carries — ca65's constancy rule,
/// which is the linker's.
///
/// `Some(weights)` for an expression that is *linear* in the labels: every
/// label above this line counts +1 in its own segment, `*` counts +1 in the
/// active one, and addition and subtraction combine them. All-zero weights
/// mean the expression is a constant. `None` means it is not linear at all — a
/// label multiplied, shifted, masked or fed to a byte extractor — which ca65
/// answers as not constant too, so the caller need not tell the two apart.
///
/// A name this line has not seen yet counts as a label of *no* segment, which
/// cannot cancel against a real one: `.const(LB - LA)` with `LB` below the line
/// is `0` in ca65, and a name defined nowhere is `0` here where ca65
/// additionally reports the undefined symbol.
fn segment_weights(expr: &Expr, st: &Projection) -> Option<BTreeMap<String, i64>> {
    let mut weights = BTreeMap::new();
    walk_weights(expr, 1, st, &mut weights)?;
    Some(weights)
}

/// Whether an expression stands for a value on its own — all weights present
/// and cancelling.
fn weighs_nothing(expr: &Expr, st: &Projection) -> bool {
    segment_weights(expr, st).is_some_and(|w| w.values().all(|&n| n == 0))
}

fn walk_weights(
    expr: &Expr,
    sign: i64,
    st: &Projection,
    weights: &mut BTreeMap<String, i64>,
) -> Option<()> {
    use crate::engine::BinOp as Op;
    let mut carry = |seg: String| *weights.entry(seg).or_insert(0) += sign;
    match expr {
        Expr::Num(_) => Some(()),
        Expr::Pc => {
            carry(st.seg.clone());
            Some(())
        }
        Expr::Sym(name) => {
            if st.consts.contains_key(name) {
                return Some(());
            }
            carry(st.label_seg.get(name).cloned().unwrap_or_default());
            Some(())
        }
        Expr::Bin(Op::Add, a, b) => {
            walk_weights(a, sign, st, weights)?;
            walk_weights(b, sign, st, weights)
        }
        Expr::Bin(Op::Sub, a, b) => {
            walk_weights(a, sign, st, weights)?;
            walk_weights(b, -sign, st, weights)
        }
        // Every other operator is linear only over operands that already stand
        // for values, so each side is weighed on its own and a label anywhere
        // inside ends it.
        Expr::Bin(_, a, b) => (weighs_nothing(a, st) && weighs_nothing(b, st)).then_some(()),
        Expr::Lo(e)
        | Expr::Hi(e)
        | Expr::Bank(e)
        | Expr::Neg(e)
        | Expr::BitNot(e)
        | Expr::LogNot(e) => weighs_nothing(e, st).then_some(()),
    }
}

/// Fold a `.repeat n[, var]` head into its count and optional loop variable.
fn fold_repeat(
    head: &str,
    st: &Projection,
    line: usize,
) -> Result<(i64, Option<String>), AsmError> {
    let (_, args) = split_first_word(head.trim());
    let (count_text, var) = match args.split_once(',') {
        Some((c, v)) => (c.trim(), Some(v.trim().to_string())),
        None => (args.trim(), None),
    };
    if count_text.is_empty() {
        return Err(AsmError::new(line, "`.repeat` needs a count"));
    }
    let expr = parse_value(&AnonCtx::default(), "", count_text, line)?;
    let count = fold_const(&expr, &st.env(), line)?;
    // ca65: a negative count is `Range error`, where zero is no iterations.
    if count < 0 {
        return Err(AsmError::new(
            line,
            format!("`.repeat {count}`: a repetition count may not be negative"),
        ));
    }
    Ok((count, var.filter(|v| !v.is_empty())))
}

/// Substitute a `.repeat`'s loop variables into an expression.
///
/// The value has to be **baked in**, not left as a symbol, for the same reason
/// acme's `!for` variable is: ca65 resolves `Expr::Sym` once, in a later pass,
/// against one table — and a loop variable holds a different value on every
/// iteration, so there is no single entry that table could hold.
fn map_expr_syms(e: &Expr, f: &dyn Fn(&str) -> Option<Expr>) -> Expr {
    match e {
        Expr::Sym(name) => f(name).unwrap_or_else(|| e.clone()),
        Expr::Lo(i) => Expr::Lo(Box::new(map_expr_syms(i, f))),
        Expr::Hi(i) => Expr::Hi(Box::new(map_expr_syms(i, f))),
        Expr::Bank(i) => Expr::Bank(Box::new(map_expr_syms(i, f))),
        Expr::Neg(i) => Expr::Neg(Box::new(map_expr_syms(i, f))),
        Expr::BitNot(i) => Expr::BitNot(Box::new(map_expr_syms(i, f))),
        Expr::LogNot(i) => Expr::LogNot(Box::new(map_expr_syms(i, f))),
        Expr::Bin(op, l, r) => Expr::Bin(
            *op,
            Box::new(map_expr_syms(l, f)),
            Box::new(map_expr_syms(r, f)),
        ),
        Expr::Num(_) | Expr::Pc => e.clone(),
    }
}

/// Substitute the enclosing `.repeat` loop variables. Innermost wins, so a
/// nested `.repeat` may shadow an outer name.
fn loop_var_sub(vars: &[(String, i64)]) -> impl Fn(&str) -> Option<Expr> + '_ {
    move |name| {
        vars.iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| Expr::Num(*v))
    }
}

/// The same, over an operand.
fn map_operand_syms(
    o: &mos6502::OperandSyntax,
    f: &dyn Fn(&str) -> Option<Expr>,
) -> mos6502::OperandSyntax {
    use mos6502::OperandSyntax as O;
    let b = |e: &Expr| map_expr_syms(e, f);
    match o {
        O::None => O::None,
        O::Accumulator => O::Accumulator,
        O::Immediate(e) => O::Immediate(b(e)),
        O::Indirect(e) => O::Indirect(b(e)),
        O::IndexedIndirect(e) => O::IndexedIndirect(b(e)),
        O::IndirectIndexed(e) => O::IndirectIndexed(b(e)),
        O::Indexed(e, i) => O::Indexed(b(e), *i),
        O::Direct(e) => O::Direct(b(e)),
    }
}

/// Every name a statement mentions. Written as a map that answers nothing,
/// because [`map_kind_syms`] already knows where the names in a `Kind` are and
/// a second walk would be a second thing to keep in step with it.
fn collect_syms(kind: &Kind, out: &mut BTreeSet<String>) {
    let seen = std::cell::RefCell::new(Vec::new());
    let _ = map_kind_syms(kind, &|name: &str| {
        seen.borrow_mut().push(name.to_string());
        None
    });
    out.extend(seen.into_inner());
}

/// The same, over a statement kind.
fn map_kind_syms(k: &Kind, f: &dyn Fn(&str) -> Option<Expr>) -> Kind {
    let list = |es: &Vec<Expr>| es.iter().map(|e| map_expr_syms(e, f)).collect();
    match k {
        Kind::Bytes(es) => Kind::Bytes(list(es)),
        Kind::Words(es) => Kind::Words(list(es)),
        Kind::DBytes(es) => Kind::DBytes(list(es)),
        Kind::DWords(es) => Kind::DWords(list(es)),
        Kind::Insn { operand, mnemonic } => Kind::Insn {
            operand: map_operand_syms(operand, f),
            mnemonic: mnemonic.clone(),
        },
        Kind::Empty => Kind::Empty,
        Kind::Res(n, f) => Kind::Res(*n, *f),
        Kind::Constant(name, value) => Kind::Constant(name.clone(), *value),
        Kind::Align(m, f) => Kind::Align(*m, *f),
        Kind::Message(sev, t) => Kind::Message(*sev, t.clone()),
        Kind::Visible {
            rule,
            zero_page,
            names,
            define,
        } => Kind::Visible {
            rule: *rule,
            zero_page: *zero_page,
            names: names.clone(),
            define: define.as_ref().map(|e| map_expr_syms(e, f)),
        },
        Kind::Assert(c, fatal, m) => Kind::Assert(map_expr_syms(c, f), *fatal, m.clone()),
        Kind::Raw(b) => Kind::Raw(b.clone()),
    }
}

/// Project one ordinary node — the body of the original projection loop.
fn project_one(node: &crate::ast::Node, st: &mut Projection) -> Result<(), AsmError> {
    use crate::ast::{Item, Operand};
    let seg = &mut st.seg;
    let seg_stack = &mut st.seg_stack;
    let stmts = &mut st.stmts;
    let label_seg = &mut st.label_seg;
    let consts = &mut st.consts;
    let referenced = &mut st.referenced;
    {
        let line = node.span.line as usize;
        let file = node.span.file;
        match &node.item {
            Some(Item::Equ(Operand::Expr { value, .. })) => {
                if let Some(sym) = node.label.as_ref()
                    && let Ok(v) = fold_const(value, consts, line)
                {
                    consts.insert(sym.qualified.clone(), v);
                }
            }
            // An unresolved include/incbin cannot assemble: it needs a loader,
            // which only the multi-file entry has (U5, KTD1). The single-source
            // API keeps meaning "one file, no includes" — with a pointer, not
            // the old `unsupported directive` rejection.
            Some(Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `.include \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `.incbin \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            // A resolved `.incbin` payload (the multi-file walk's lowering):
            // raw bytes at the directive's location in the active segment,
            // with a label on the directive line binding at the payload start.
            Some(Item::Binary(payload)) => {
                let label = node.label.as_ref().map(|s| s.qualified.clone());
                if let Some(l) = &label {
                    label_seg.insert(l.clone(), seg.clone());
                }
                stmts.push(Stmt {
                    line,
                    file,
                    seg: seg.clone(),
                    label,
                    kind: Kind::Raw(payload.clone()),
                });
            }
            Some(Item::Native(payload)) => {
                let kind = payload
                    .as_any()
                    .downcast_ref::<Kind>()
                    .expect("ca65 stores a Kind in every native node");
                // `.defined` is answered here, in source order, against what
                // this point in the file has seen — which is the question ca65
                // asks.
                let kind = map_kind_syms(kind, &resolve_defined(consts, label_seg));
                // `.ref` is the same question about uses rather than
                // definitions, so it is answered from the same place.
                let kind = map_kind_syms(&kind, &resolve_ref(referenced));
                // Then record what *this* statement mentions, so the next one
                // sees it. Answering first keeps a statement's own names out of
                // its own `.ref`.
                collect_syms(&kind, referenced);
                // A record's constants, folded here so they land in the same
                // map, in the same order, as every `=` in the file.
                if let Kind::Constant(name, value) = &kind {
                    consts.insert(name.clone(), *value);
                }
                // `.export foo := 7` defines `foo` as well as exporting it, so
                // it is collected here with the `=` constants rather than left
                // to the visibility check, which would then find it undefined.
                if let Kind::Visible {
                    names,
                    define: Some(value),
                    ..
                } = &kind
                    && let Some(name) = names.first()
                    && let Ok(v) = fold_const(value, consts, line)
                {
                    consts.insert(name.clone(), v);
                }
                let label = node.label.as_ref().map(|s| s.qualified.clone());
                if let Some(l) = &label {
                    label_seg.insert(l.clone(), seg.clone());
                }
                stmts.push(Stmt {
                    line,
                    file,
                    seg: seg.clone(),
                    label,
                    kind,
                });
            }
            // Item-less nodes: a `.segment` directive (tracked), a label-only line
            // (an empty placed statement), or a comment-only flush node (skipped).
            _ => {
                if let Some(switch) = segment_switch(&node.source) {
                    match switch {
                        SegSwitch::To(name) => *seg = name,
                        SegSwitch::Push => seg_stack.push(seg.clone()),
                        // ca65 pops an empty stack with a diagnostic of its own;
                        // ours is the parse-time refusal, so an unmatched
                        // `.popseg` here leaves the active segment alone.
                        SegSwitch::Pop => {
                            if let Some(prev) = seg_stack.pop() {
                                *seg = prev;
                            }
                        }
                    }
                } else if let Some(sym) = node.label.as_ref() {
                    label_seg.insert(sym.qualified.clone(), seg.clone());
                    stmts.push(Stmt {
                        line,
                        file,
                        seg: seg.clone(),
                        label: Some(sym.qualified.clone()),
                        kind: Kind::Empty,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Split a line into its code and its `;` comment for carrying comments as AST
/// trivia. Defined via [`strip_comment`] so the comment is exactly what it
/// removes — no behaviour change to assembly.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
}

/// Strip a `;` comment, ignoring `;` inside `'c'` or `"..."`.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let (mut in_char, mut in_str) = (false, false);
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b';' if !in_char && !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Split a leading `name:`, `@cheap:`, or bare `:` (anonymous) label into an AST
/// [`Symbol`](crate::ast::Symbol) carrying both the **source form** (`name` /
/// `@cheap` / empty for anonymous — what the formatter re-emits) and the
/// **resolved** name (what assembly uses: the synthetic anonymous key, the
/// `global@cheap` cheap key, or the plain name). Updates `current_global` when a
/// non-cheap named label is defined (cheap locals scope to the preceding global).
fn split_label_symbol<'a>(
    anons: &AnonCtx,
    line: usize,
    current_global: &mut String,
    trimmed: &'a str,
) -> Result<(Option<crate::ast::Symbol>, &'a str), AsmError> {
    use crate::ast::{Scope, Symbol};
    let (word, remainder) = split_first_word(trimmed);
    // A bare `:` is an anonymous label: the empty source name re-emits as a lone
    // `:` (emit appends the colon), while assembly binds the next index in the
    // evaluation-order stream.
    if word == ":" {
        return Ok((
            Some(Symbol {
                name: String::new(),
                scope: Scope::Global,
                qualified: anons.define(),
            }),
            remainder,
        ));
    }
    let Some(name) = word.strip_suffix(':') else {
        return Ok((None, trimmed));
    };
    // `@cheap:` — a cheap local. The `@cheap` source form round-trips; assembly
    // uses the `global@cheap` key.
    if let Some(cheap) = name.strip_prefix('@') {
        if !is_ident(cheap) {
            return Err(AsmError::new(
                line,
                format!("invalid cheap-local label `{name}`"),
            ));
        }
        return Ok((
            Some(Symbol {
                name: name.to_string(),
                scope: Scope::Local {
                    in_global: current_global.clone(),
                },
                qualified: cheap_key(current_global, cheap),
            }),
            remainder,
        ));
    }
    if !is_ident(name) {
        return Err(AsmError::new(line, format!("invalid label `{name}`")));
    }
    *current_global = name.to_string();
    Ok((
        Some(Symbol {
            name: name.to_string(),
            scope: Scope::Global,
            qualified: name.to_string(),
        }),
        remainder,
    ))
}

fn parse_op(
    set: &'static isa::InstructionSet,
    anons: &AnonCtx,
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(Kind::Empty);
    }
    if let Some(directive) = rest.strip_prefix('.') {
        return parse_directive(anons, current_global, consts, directive, line);
    }
    let (mnemonic, operand_text) = split_first_word(rest);
    let mnemonic = mnemonic.to_ascii_uppercase();
    let operand = mos6502::parse_operand(operand_text, line, &|s, l| {
        parse_value(anons, current_global, s, l)
    })?;
    if set.instruction(&mnemonic).is_none() {
        return Err(AsmError::new(
            line,
            format!("unknown instruction `{mnemonic}`"),
        ));
    }
    Ok(Kind::Insn { operand, mnemonic })
}

/// What this dialect accepts beyond the 6502 instruction set.
///
/// The `.` is required: a bare `byte` is a *label definition* in ca65, and real
/// ca65 answers "':' expected" for it. `Sigilled { required: true }` is what
/// keeps that true — see `crate::directives`.
///
/// Dispatch is split, and the declaration covers all of it because it describes
/// the dialect rather than any one parser. The data directives reach
/// [`parse_directive`]; `.include` and `.incbin` are walk-handled; `.segment`
/// is read where segments are assigned; the macro spellings are expanded before
/// parsing.
pub const DIRECTIVES: &[Directive] = &[
    // ca65's expression vocabulary: words that appear *inside* an expression
    // and never begin a line. Declared because the ledger counts what is
    // declared, and leaving these out made eleven working words read as gaps
    // — see `Category::ExpressionWord`.
    //
    // `.max`, `.min` and `.strat` were unreachable until the data-list
    // splitter stopped cutting a call in half at its argument comma; the code
    // behind them was always right.
    // The operator spellings. ca65 writes every operator twice — `&` and
    // `.bitand` are one operator — so the keyword lands on the same token as
    // its symbol twin and inherits its precedence. `.mod` is the exception
    // with no twin: `%` is a binary literal there.
    //
    // `.not` is the one whose place is not the obvious one: it binds looser
    // than everything, so `.not 1 .or 1` is `0`.
    Directive {
        id: "operator",
        pattern: Pattern::Exact(&[
            ".and", ".bitand", ".bitnot", ".bitor", ".bitxor", ".mod", ".not", ".or", ".shl",
            ".shr", ".xor",
        ]),
        category: Category::ExpressionWord,
    },
    Directive {
        id: "expression-word",
        pattern: Pattern::Exact(&[
            ".bankbyte",
            ".def",
            ".defined",
            ".hibyte",
            ".hiword",
            ".lobyte",
            ".loword",
            ".max",
            ".min",
            ".strat",
            ".strlen",
        ]),
        category: Category::ExpressionWord,
    },
    Directive {
        id: "bytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["byte", "byt"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "words",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["word", "addr"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "dbyt",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["dbyt"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "dword",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["dword"],
            required: true,
        },
        category: Category::Operation,
    },
    // `.lobytes`/`.hibytes`/`.bankbytes` take byte 0, 1 and 2 of each value in
    // the list; `.faraddr` emits all three, little-endian. Byte 2 is the bank
    // byte of a 65816 address, which is why the third has that name and the
    // fourth is a `far` address rather than a 24-bit word.
    Directive {
        id: "lobytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["lobytes"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "hibytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["hibytes"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "bankbytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["bankbytes"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "faraddr",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["faraddr"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "asciiz",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["asciiz"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "res",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["res"],
            required: true,
        },
        category: Category::Operation,
    },
    // Dispatched elsewhere: walk-handled, segment-assigned, or macro-expanded
    // before parsing.
    Directive {
        id: "include",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["include"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["incbin"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "segment",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["segment"],
            required: true,
        },
        category: Category::Operation,
    },
    // Walk-handled: the shared cursor reads these into `Item::Conditional` /
    // `Item::Repeat` before `parse_directive` sees a line.
    //
    // Every head ca65 has, `.ifref`/`.ifnref` included: each one folds, so
    // none of them groups a block the projection cannot read.
    Directive {
        id: "conditional",
        pattern: Pattern::Exact(&[
            ".if",
            ".ifdef",
            ".ifndef",
            ".ifblank",
            ".ifnblank",
            ".ifconst",
            ".ifnconst",
            ".ifp02",
            ".ifp4510",
            ".ifp816",
            ".ifpc02",
            ".ifpsc02",
            ".ifref",
            ".ifnref",
            ".elseif",
            ".else",
            ".endif",
        ]),
        category: Category::Operation,
    },
    Directive {
        id: "repeat",
        pattern: Pattern::Exact(&[".repeat", ".endrepeat"]),
        category: Category::Operation,
    },
    Directive {
        id: "macro",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["macro", "mac"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "endmacro",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["endmacro", "endmac"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "local",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["local"],
            required: true,
        },
        category: Category::Operation,
    },
    // -----------------------------------------------------------------------
    // What ca65 has and we do not.
    //
    // Declared rather than left to fall through, because the fall-through
    // answers `unsupported directive` for `.zzqq` as well as for `.export` —
    // and conflating "ca65 has this and we have not implemented it" with "this
    // is not a thing" tells the reader to go looking for a typo. Same call as
    // the asl family's (#87) and vasm's `adda`: the source is valid and the
    // gap is ours, so the message should say so.
    //
    // Ninety-seven spellings, each confirmed in **statement position** against
    // ca65 V2.18 — offered as a lone operation and, where that failed with
    // `Unexpected '.X'`, offered again after a label to catch the label-first
    // forms. The thirty-seven that failed both ways are ca65's
    // pseudo-*functions* — `.lobyte`, `.strlen`, `.max`, `.sizeof` — which
    // live inside expressions and are not directives at all. Declaring those
    // here would name them in a place they never appear.
    // segments and location
    // The segment shorthands, and the stack that saves and restores the active
    // one. Each is `.segment "NAME"` by another name, so they are tracked in
    // the same source-only node and cost nothing beyond the mapping.
    Directive {
        id: "segment-shorthand",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "code", "data", "bss", "rodata", "zeropage", "pushseg", "popseg",
            ],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "align",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["align"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "out",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["out"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "message",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["warning", "error", "fatal"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "assert",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["assert"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "unsupported-segments",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["org", "reloc"],
            required: true,
        },
        category: Category::KnownUnsupported,
    },
    // Symbol visibility, probed against ca65 2.18 + ld65 in the fused
    // assemble+link this dialect performs. See
    // `decisions/symbol-visibility-in-a-fused-assembler.md` and `VisRule`.
    Directive {
        id: "export",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["export", "exportzp"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "import",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["import", "importzp"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "global",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["global", "globalzp"],
            required: true,
        },
        category: Category::Operation,
    },
    // `.forceimport` cannot be satisfied here: defining its name is ca65's
    // `Symbol 'zz' is already an import`, and not defining it is an unresolved
    // external at ld65 even with nothing referencing it — which is what the
    // "force" means. The third dialect to need this category.
    Directive {
        id: "forceimport",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["forceimport"],
            required: true,
        },
        category: Category::RefusedByReference(
            "only usable when a linker resolves it from another module, and asm198x \
             emits a binary",
        ),
    },
    // `.autoimport +`/`-` switches ca65's automatic import of runtime symbols.
    // There is no runtime library to import from here, and it emits nothing.
    Directive {
        id: "autoimport",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["autoimport"],
            required: true,
        },
        category: Category::Ignored,
    },
    // The words that address the listing and the object file's own metadata
    // rather than the program. Probed against V2.18 and ld65: each assembles to
    // the same bytes with the directive present or absent, so accepting and
    // discarding one is what matching the reference means here.
    //
    // `.list off` does not suppress bytes, only listing lines; `.dbg` and
    // `.debuginfo` add records to the object file, which the binary output
    // never carries; `.fileopt`/`.fopt` write an object-file comment. The two
    // option words that are *not* here are the two that change what the parse
    // does: `.linecont +` turns on backslash continuation, and `.case off`
    // makes identifiers case-blind.
    Directive {
        id: "listing",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "list",
                "listbytes",
                "pagelen",
                "pagelength",
                "debuginfo",
                "dbg",
                "fileopt",
                "fopt",
            ],
            required: true,
        },
        category: Category::Ignored,
    },
    Directive {
        id: "unsupported-symbol",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["condes", "constructor", "destructor", "interruptor"],
            required: true,
        },
        category: Category::KnownUnsupported,
    },
    // Walk-handled, like the segment directives: the scope stack is parse
    // state, so these are read before `parse_directive` sees a line.
    Directive {
        id: "scopes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["proc", "endproc", "scope", "endscope"],
            required: true,
        },
        category: Category::Operation,
    },
    // Walk-handled, like the scopes: a record declares constants rather than
    // statements, and the walker is what reads its lines.
    Directive {
        id: "records",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "struct",
                "endstruct",
                "union",
                "endunion",
                "enum",
                "endenum",
                "tag",
            ],
            required: true,
        },
        category: Category::Operation,
    },
    // `.sizeof` is a name rather than a computation here: a record stores its
    // size as a constant when its declaration closes.
    Directive {
        id: "sizeof",
        pattern: Pattern::Exact(&[".sizeof"]),
        category: Category::ExpressionWord,
    },
    Directive {
        id: "unsupported-end",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["end"],
            required: true,
        },
        category: Category::KnownUnsupported,
    },
    // macros
    Directive {
        id: "unsupported-macros",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "define", "delmac", "delmacro", "macpack", "undef", "undefine", "ident", "concat",
                "sprintf", "string", "left", "mid", "right",
            ],
            required: true,
        },
        category: Category::KnownUnsupported,
    },
    // conditionals over the assembler's own state
    Directive {
        id: "unsupported-conditionals",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["ref", "referenced"],
            required: true,
        },
        category: Category::ExpressionWord,
    },
    // CPU selection. `.setcpu "6502"` and `.p02` name the processor this leg
    // already assembles, so they change nothing; every other name selects a
    // different opcode set and is refused by name, the way ACME's `!cpu` is
    // (`decisions/reference-parity-goal.md`, asm198x#302).
    //
    // `.pushcpu`/`.popcpu` save and restore a selection that cannot change
    // here, and `.smart` decides how ca65 tracks the 65816's register widths
    // across `jsr`/`rts` — probed inert on a 6502.
    Directive {
        id: "cpu",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "setcpu", "pushcpu", "popcpu", "smart", "p02", "p4510", "p816", "pc02", "psc02",
            ],
            required: true,
        },
        category: Category::Operation,
    },
    // `.asize`/`.isize` answer the current register widths, which only the
    // 65816 has: ca65 on this target says `Command is only valid in 65816
    // mode`, so refusing them is matching it.
    Directive {
        id: "register-widths",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["asize", "isize"],
            required: true,
        },
        category: Category::RefusedByReference(
            "only valid in 65816 mode, and this leg assembles a 6502",
        ),
    },
    // The option words that change what the parse does, and so cannot be
    // accepted and discarded: `.linecont +` turns on backslash continuation,
    // `.case off` makes identifiers case-blind, `.charmap`/`.localchar`
    // translate characters on the way out, and `.feature` switches syntax
    // rules on one by one.
    Directive {
        id: "unsupported-parse-options",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "linecont",
                "case",
                "charmap",
                "localchar",
                "feature",
                "null",
            ],
            required: true,
        },
        category: Category::KnownUnsupported,
    },
    // A word ca65's tokenizer knows and its parser accepts nowhere. Probed
    // against V2.18 at statement position, after a label, as a struct storage
    // allocator, as an `.import`/`.export` address-size specifier and inside an
    // expanded macro body: every one answers `Unexpected '.FORCEWORD'` (the
    // address-size specifiers there are `zeropage`, `absolute`, `direct` and
    // `near`, plus `far`/`dword` on a CPU wide enough for them). Reachable
    // source cannot contain it, so refusing it is matching the reference.
    Directive {
        id: "forceword",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["forceword"],
            required: true,
        },
        category: Category::RefusedByReference(
            "a word its tokenizer knows and its parser accepts in no position",
        ),
    },
];

fn parse_directive(
    anons: &AnonCtx,
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    directive: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let (name, rest) = split_first_word(directive);
    // Dispatch through the declared surface. The name arrives with the `.`
    // already stripped, so it is put back for the lookup: matching the bare
    // name here would quietly make the sigil optional.
    let sigilled = format!(".{}", name.to_ascii_lowercase());
    let Some(entry) = lookup(DIRECTIVES, &sigilled) else {
        return Err(AsmError::new(
            line,
            format!("`.{name}` is not a directive ca65 has"),
        ));
    };
    // Accepted and discarded: it changes no bytes and cannot fail.
    if entry.category == Category::Ignored {
        return Ok(Kind::Empty);
    }
    if let Category::RefusedByReference(rule) = entry.category {
        return Err(AsmError::new(
            line,
            // Named as the pair, because it is the pair that refuses: ca65
            // rejects the definition and ld65 rejects its absence.
            crate::directives::refused_by_reference("the ca65+ld65 pipeline", &sigilled, rule),
        ));
    }
    if entry.category == Category::ExpressionWord {
        return Err(AsmError::new(
            line,
            crate::directives::not_a_statement(&sigilled),
        ));
    }
    if entry.category == Category::KnownUnsupported {
        return Err(AsmError::new(
            line,
            format!(
                "`.{name}` is a real directive here and asm198x does not implement \
                 it yet — the source is valid and the gap is ours"
            ),
        ));
    }
    match entry.id {
        "bytes" => Ok(Kind::Bytes(parse_data_list(
            anons,
            current_global,
            rest,
            line,
        )?)),
        "words" => Ok(Kind::Words(parse_value_list(
            anons,
            current_global,
            rest,
            line,
        )?)),
        "dbyt" => Ok(Kind::DBytes(parse_value_list(
            anons,
            current_global,
            rest,
            line,
        )?)),
        "dword" => Ok(Kind::DWords(parse_value_list(
            anons,
            current_global,
            rest,
            line,
        )?)),
        // Each extractor is its list of values with a byte selector wrapped
        // round every one, so a forward label still resolves at layout time and
        // the size is the item count (times three for `.faraddr`). ca65 answers
        // a string with a syntax error, which `parse_value_list` does too.
        // `Expr::Bank` is the engine's 65816 `^` node — bits 16-23 — which is
        // byte 2 exactly.
        "lobytes" | "hibytes" | "bankbytes" | "faraddr" => {
            let values = parse_value_list(anons, current_global, rest, line)?;
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                match entry.id {
                    "lobytes" => out.push(Expr::Lo(Box::new(value))),
                    "hibytes" => out.push(Expr::Hi(Box::new(value))),
                    "bankbytes" => out.push(Expr::Bank(Box::new(value))),
                    _ => {
                        out.push(Expr::Lo(Box::new(value.clone())));
                        out.push(Expr::Hi(Box::new(value.clone())));
                        out.push(Expr::Bank(Box::new(value)));
                    }
                }
            }
            Ok(Kind::Bytes(out))
        }
        "asciiz" => Ok(Kind::Bytes(parse_asciiz(
            anons,
            current_global,
            rest,
            line,
        )?)),
        // The processor words. `.setcpu` takes a quoted name; the `.pNN`
        // shorthands are the name themselves.
        "cpu" => {
            let (word, _) = split_first_word(directive);
            let selected = match word.to_ascii_lowercase().as_str() {
                ".setcpu" | "setcpu" => {
                    let text = rest.trim();
                    let Some(name) = string_literal(text) else {
                        return Err(AsmError::new(
                            line,
                            format!("`.setcpu` takes a quoted processor name, not `{text}`"),
                        ));
                    };
                    Some(name.to_ascii_lowercase())
                }
                ".p02" | "p02" => Some("6502".to_string()),
                ".pc02" | "pc02" => Some("65c02".to_string()),
                ".psc02" | "psc02" => Some("65sc02".to_string()),
                ".p816" | "p816" => Some("65816".to_string()),
                ".p4510" | "p4510" => Some("4510".to_string()),
                // `.pushcpu`/`.popcpu` save and restore a selection that
                // cannot change here, and `.smart` is inert on a 6502.
                _ => None,
            };
            // The processors ca65 V2.18 knows, probed by asking it for each:
            // anything else is its own `CPU not supported`, which is a fault in
            // the source rather than a gap here.
            const KNOWN: &[&str] = &[
                "6502", "6502x", "65sc02", "65c02", "65816", "huc6280", "4510", "sweet16", "none",
            ];
            match selected.as_deref() {
                None | Some("6502") => Ok(Kind::Empty),
                Some(name) if KNOWN.contains(&name) => Err(AsmError::new(
                    line,
                    format!(
                        "`{name}` is a different opcode set, and asm198x assembles a 6502 \
                         here — the source is valid and the gap is ours (asm198x#302)"
                    ),
                )),
                Some(name) => Err(AsmError::new(
                    line,
                    format!("`{name}` is not a processor ca65 knows"),
                )),
            }
        }
        "res" => parse_res(anons, current_global, consts, rest, line),
        "align" => parse_align(anons, current_global, consts, rest, line),
        // `.out` is the strict one: ca65 answers a value list with
        // `Unexpected trailing garbage characters`. The other three read a
        // string and ignore whatever follows it on the line — probed, not
        // assumed, because the two shapes read alike in the manual.
        "out" => {
            let text = rest.trim();
            match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                Some(lit) => Ok(Kind::Message(DiagSeverity::Note, lit.to_string())),
                None => Err(AsmError::new(line, "`.out` takes one string")),
            }
        }
        "message" => {
            let severity = if name.eq_ignore_ascii_case("warning") {
                DiagSeverity::Warning
            } else {
                DiagSeverity::Error
            };
            Ok(Kind::Message(severity, leading_string(rest, line)?))
        }
        "assert" => parse_assert(anons, current_global, rest, line),
        "export" | "import" | "global" => parse_visible(
            match entry.id {
                "export" => VisRule::MustBeDefined,
                "import" => VisRule::MustNotBeDefined,
                _ => VisRule::EitherWay,
            },
            name.to_ascii_lowercase().ends_with("zp"),
            anons,
            current_global,
            rest,
            line,
        ),
        // Declared, and dispatched elsewhere: `.include`/`.incbin` are
        // walk-handled, `.segment` is read where segments are assigned, and the
        // macro spellings are expanded before parsing. Reaching here means a
        // path misrouted one, which the original fall-through reported loudly.
        _ => Err(AsmError::new(
            line,
            format!("unsupported directive `.{name}`"),
        )),
    }
}

/// `.res count [, fill]`. `count` must fold to a constant (a literal expression
/// or a `=` constant such as `NUM_ENEMIES`); `fill` defaults to 0.
fn parse_res(
    anons: &AnonCtx,
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let count_src = parts.next().unwrap_or("").trim();
    let count = fold_const(
        &parse_value(anons, current_global, count_src, line)?,
        consts,
        line,
    )
    .map_err(|_| AsmError::new(line, "`.res` count must be a constant"))?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`.res` count must be non-negative"))?;
    let fill = match parts.next() {
        None => 0,
        Some(v) => {
            let n = fold_const(&parse_value(anons, current_global, v, line)?, consts, line)?;
            u8::try_from(n).map_err(|_| AsmError::new(line, "`.res` fill must be a byte"))?
        }
    };
    Ok(Kind::Res(count, fill))
}

/// A visibility operand as the symbol table keys it: a cheap local (`@name`)
/// is scoped to its global the way its definition is, so the check looks the
/// same name up that the label pass stored.
fn vis_name(current_global: &str, raw: &str) -> String {
    match raw.strip_prefix('@') {
        Some(local) => cheap_key(current_global, local),
        None => raw.to_string(),
    }
}

/// `.export name[, name...]`, and the `:= expr` form that defines as it
/// exports. ca65 takes the value form only on the export spellings and only for
/// a single name; `.global bar := 9` and `.import baz := 3` are both
/// `Unexpected trailing garbage characters`.
fn parse_visible(
    rule: VisRule,
    zero_page: bool,
    anons: &AnonCtx,
    current_global: &str,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let parts = split_top_level(rest, ',');
    let mut names = Vec::new();
    let mut define = None;
    for part in &parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((name, value)) = part.split_once(":=") {
            if rule != VisRule::MustBeDefined {
                return Err(AsmError::new(
                    line,
                    "only `.export` and `.exportzp` take `:=`",
                ));
            }
            if parts.len() > 1 {
                return Err(AsmError::new(line, "`:=` exports one name"));
            }
            names.push(vis_name(current_global, name.trim()));
            define = Some(parse_value(anons, current_global, value.trim(), line)?);
        } else {
            names.push(vis_name(current_global, part));
        }
    }
    if names.is_empty() {
        return Err(AsmError::new(line, "a visibility directive needs a name"));
    }
    Ok(Kind::Visible {
        rule,
        zero_page,
        names,
        define,
    })
}

/// The leading string of a `.warning`/`.error`/`.fatal` operand. ca65 reads
/// one string constant and skips the rest of the line: `.warning "a", 5` and
/// `.warning "a" "b"` both say `a` and nothing more.
fn leading_string(rest: &str, line: usize) -> Result<String, AsmError> {
    let rest = rest.trim_start();
    let body = rest
        .strip_prefix('"')
        .ok_or_else(|| AsmError::new(line, "string constant expected"))?;
    let end = body
        .find('"')
        .ok_or_else(|| AsmError::new(line, "unterminated string"))?;
    Ok(body[..end].to_string())
}

/// `.assert cond, action[, "message"]`. The action decides whether a failure
/// stops the assembly: `error` and `lderror` do, `warning` and `ldwarning`
/// note it and carry on. With assembly and linking fused there is no moment
/// between the two `ld` forms and the others.
fn parse_assert(
    anons: &AnonCtx,
    current_global: &str,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let parts = split_top_level(rest, ',');
    let cond = parse_value(
        anons,
        current_global,
        parts.first().copied().unwrap_or(""),
        line,
    )?;
    let action = parts.get(1).map(|a| a.trim().to_ascii_lowercase());
    let fatal = match action.as_deref() {
        Some("error" | "lderror") => true,
        Some("warning" | "ldwarning") => false,
        _ => {
            return Err(AsmError::new(
                line,
                "`.assert` needs an action: `warning`, `error`, `ldwarning` or `lderror`",
            ));
        }
    };
    let message = parts
        .get(2)
        .map(|m| m.trim().trim_matches('"').to_string())
        .unwrap_or_else(|| "Assertion failed".to_string());
    Ok(Kind::Assert(cond, fatal, message))
}

/// `.align boundary [, fill]` — pad to the next multiple of `boundary` within
/// the active segment. The boundary need not be a power of two, and the pad
/// byte defaults to zero.
fn parse_align(
    anons: &AnonCtx,
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let boundary_src = parts.next().unwrap_or("").trim();
    let boundary = fold_const(
        &parse_value(anons, current_global, boundary_src, line)?,
        consts,
        line,
    )
    .map_err(|_| AsmError::new(line, "`.align` boundary must be a constant"))?;
    if boundary < 1 {
        return Err(AsmError::new(line, "`.align` boundary must be positive"));
    }
    let fill = match parts.next() {
        None => 0,
        Some(v) => {
            let n = fold_const(&parse_value(anons, current_global, v, line)?, consts, line)?;
            u8::try_from(n).map_err(|_| AsmError::new(line, "`.align` fill must be a byte"))?
        }
    };
    Ok(Kind::Align(boundary, fill))
}

/// `.byte` list: `"..."` strings expand to raw ASCII bytes; values are bytes.
fn parse_data_list(
    anons: &AnonCtx,
    current_global: &str,
    rest: &str,
    line: usize,
) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "`.byte` needs a value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.bytes().map(|b| Expr::Num(i64::from(b))));
        } else {
            out.push(parse_value(anons, current_global, piece, line)?);
        }
    }
    Ok(out)
}

/// `.asciiz` list: like `.byte` with strings, but a single terminating `$00` is
/// appended after the last item (ca65 emits one NUL for the whole directive).
fn parse_asciiz(
    anons: &AnonCtx,
    current_global: &str,
    rest: &str,
    line: usize,
) -> Result<Vec<Expr>, AsmError> {
    let mut out = parse_data_list(anons, current_global, rest, line)?;
    out.push(Expr::Num(0));
    Ok(out)
}

fn parse_value_list(
    anons: &AnonCtx,
    current_global: &str,
    rest: &str,
    line: usize,
) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "directive needs a value"));
    }
    split_top_level(rest, ',')
        .iter()
        .map(|p| parse_value(anons, current_global, p, line))
        .collect()
}

// ---------------------------------------------------------------------------
// Value parsing over the shared expression core
// ---------------------------------------------------------------------------

/// Parse a ca65 value. A bare `:-`/`:+` run is an anonymous-label reference; a
/// bare `@cheap` operand is a cheap-local reference scoped to the current global;
/// otherwise it is an expression with `<`/`>` binding tight ([`BytePrec::Tight`]).
fn parse_value(
    anons: &AnonCtx,
    current_global: &str,
    raw: &str,
    line: usize,
) -> Result<Expr, AsmError> {
    let t = raw.trim();
    if let Some((sign, level)) = anon_ref(t) {
        return Ok(Expr::Sym(anons.refer(sign, level, line)?));
    }
    if let Some(cheap) = t.strip_prefix('@')
        && is_ident(cheap)
    {
        return Ok(Expr::Sym(cheap_key(current_global, cheap)));
    }
    mos6502::parse_expr(
        t,
        line,
        parse_number,
        mos6502::ExprOpts {
            logical: true,
            scoped_names: true,
            compare: mos6502::Compare {
                eq: true,
                eq_eq: false,
                ne_angle: true,
                ne_bang: false,
                relational: true,
                ordered_eq: true,
                minus_one: false,
            },
            function: Some(expr_function_positional),
            bang_is_or: false,
            prec: BytePrec::Tight,
            byte_prefix: true,
            caret: mos6502::Caret::BankOrXor,
            at_is_pc: false,
        },
    )
}

/// Marks a `.defined(NAME)` the projection has yet to answer. `\u{1}` cannot
/// appear in source, the same property `cheap_key` relies on.
const DEFINED_MARK: &str = "\u{1}defined\u{1}";

/// The same for `.ref(NAME)`/`.referenced(NAME)`, which asks a different
/// question about the same thing: not whether the name is *defined* above this
/// line but whether it has been *used* above it.
const REF_MARK: &str = "\u{1}ref\u{1}";

/// ca65's expression functions **plus** `.defined`/`.def`, which the flat
/// legs cannot have.
///
/// `.defined(X)` is *positional* — `0` before the definition and `1` after,
/// probe-pinned — and nothing is defined yet when the walk parses an
/// expression. So it cannot fold here; it becomes a marker that the
/// projection answers in source order, where the constants and labels seen so
/// far are exactly the set ca65 is asking about.
///
/// The 65816 and HuC6280 legs run on the shared engine, which resolves symbols
/// once at the end and so has no "so far" to consult. They keep refusing
/// `.defined` rather than answering it wrongly.
fn expr_function_positional(
    name: &str,
    args: Vec<mos6502::ExprArg>,
    line: usize,
) -> Result<Expr, AsmError> {
    // `.sizeof(X)` is a name, not a computation: a record's size is stored as a
    // constant when its declaration closes, so this rewrites to that constant
    // and every scope rule applies to it unchanged. (ca65 also answers
    // `.sizeof` for a plain label — the size of what it points at — which needs
    // a size record per label that nothing here keeps.)
    if name.eq_ignore_ascii_case(".sizeof") {
        let [arg]: [_; 1] = args
            .try_into()
            .map_err(|_| AsmError::new(line, format!("`{name}` takes one argument")))?;
        return match arg.value(name, line)? {
            Expr::Sym(sym) => Ok(Expr::Sym(Walker::record_size_key(&sym))),
            _ => Err(AsmError::new(line, format!("`{name}` takes a name"))),
        };
    }
    if matches!(name.to_ascii_lowercase().as_str(), ".ref" | ".referenced") {
        let [arg]: [_; 1] = args
            .try_into()
            .map_err(|_| AsmError::new(line, format!("`{name}` takes one argument")))?;
        return match arg.value(name, line)? {
            Expr::Sym(sym) => Ok(Expr::Sym(format!("{REF_MARK}{sym}"))),
            _ => Err(AsmError::new(line, format!("`{name}` takes a symbol name"))),
        };
    }
    if !matches!(name.to_ascii_lowercase().as_str(), ".defined" | ".def") {
        return ca65_flat::expr_function(name, args, line);
    }
    let [arg]: [_; 1] = args
        .try_into()
        .map_err(|_| AsmError::new(line, format!("`{name}` takes one argument")))?;
    match arg.value(name, line)? {
        Expr::Sym(sym) => Ok(Expr::Sym(format!("{DEFINED_MARK}{sym}"))),
        _ => Err(AsmError::new(line, format!("`{name}` takes a symbol name"))),
    }
}

/// Answer every `.defined` marker in an expression against what is defined so
/// far. Anything else is left alone.
fn resolve_defined<'a>(
    consts: &'a BTreeMap<String, i64>,
    labels: &'a BTreeMap<String, String>,
) -> impl Fn(&str) -> Option<Expr> + 'a {
    move |name| {
        let sym = name.strip_prefix(DEFINED_MARK)?;
        let known = consts.contains_key(sym) || labels.contains_key(sym);
        Some(Expr::Num(i64::from(known)))
    }
}

/// Marks an unqualified reference written inside a scope. ca65 resolves such a
/// name against the innermost enclosing scope that holds it, and only the
/// *whole file* knows which that is, so the parse records the path the
/// reference was written in and [`scope_targets`] answers it once every
/// definition is known. Built from [`LABEL_SEP`], so it cannot collide with a
/// name from source.
const SCOPE_MARK: &str = "\u{1}scope\u{1}";

/// `a::b::name`, or `name` at the top level — the key a definition inside a
/// scope is stored under, and what a reference finally resolves to.
fn qualify(path: &[String], name: &str) -> String {
    if path.is_empty() {
        return name.to_string();
    }
    format!("{}::{name}", path.join("::"))
}

/// The reference key for `name` written inside `path`: the path, then the name.
fn scope_ref(path: &[String], name: &str) -> String {
    format!("{SCOPE_MARK}{}{LABEL_SEP}{name}", path.join("::"))
}

/// Split a reference key back into the path it was written in and the name.
fn split_scope_ref(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix(SCOPE_MARK)?.split_once(LABEL_SEP)
}

/// What each scoped reference resolves to, given everything the file defines.
///
/// ca65 looks a name up in the scope it was written in, then in that scope's
/// parent, and outward to the top — so `v` inside `a::b` is `a::b::v` if that
/// exists, else `a::v`, else `v`. A name that resolves nowhere keeps its bare
/// spelling, so the diagnostic names what the source wrote rather than an
/// internal key.
fn scope_targets(
    refs: &BTreeSet<String>,
    defined: &dyn Fn(&str) -> bool,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for key in refs {
        let Some((path, name)) = split_scope_ref(key) else {
            continue;
        };
        let mut segments: Vec<&str> = if path.is_empty() {
            Vec::new()
        } else {
            path.split("::").collect()
        };
        let target = loop {
            let candidate = if segments.is_empty() {
                name.to_string()
            } else {
                format!("{}::{name}", segments.join("::"))
            };
            if defined(&candidate) {
                break candidate;
            }
            if segments.pop().is_none() {
                break name.to_string();
            }
        };
        out.insert(key.clone(), target);
    }
    out
}

/// Answer every `.ref` marker against what has been *used* so far. The set is
/// the projection's, and it is updated after each statement is answered, so a
/// name a statement mentions is not yet referenced for that statement's own
/// `.ref` — which is what asking "above this line" means.
fn resolve_ref(used: &BTreeSet<String>) -> impl Fn(&str) -> Option<Expr> + '_ {
    move |name| {
        let sym = name.strip_prefix(REF_MARK)?;
        Some(Expr::Num(i64::from(used.contains(sym))))
    }
}

/// A collision-proof symbol key for a cheap local, scoped to its global.
fn cheap_key(global: &str, name: &str) -> String {
    format!("{global}\u{1}{name}")
}

/// The internal separator inside anonymous (`\u{1}:#N`) and cheap
/// (`global\u{1}name`) label keys. Never valid in user source, so keys cannot
/// collide with real names — but it must not leak into user-facing artifacts.
pub(crate) const LABEL_SEP: char = '\u{1}';

/// A label key rendered for a user-facing artifact (the debug record): a cheap
/// key `global\u{1}name` reads back as its source form `global@name`; plain
/// names pass through.
fn display_label(key: &str) -> String {
    key.replace(LABEL_SEP, "@")
}

#[cfg(test)]
mod tests {
    use super::assemble;

    fn rom(src: &str) -> Vec<u8> {
        assemble(src).expect("assembles").0
    }

    #[test]
    fn rom_has_nrom_shape() {
        let r = rom(".segment \"CODE\"\nrts\n");
        assert_eq!(r.len(), 16 + 0x8000 + 0x2000);
    }

    #[test]
    fn segment_outside_the_nes_config_is_rejected_with_help() {
        // `RODATA` has no memory area in the curriculum's `nes.cfg`, so `ld65`
        // rejects it — and so do we. The message names the valid segments
        // rather than a bare "unknown segment".
        let err = assemble(".segment \"RODATA\"\n .byte 1\n").expect_err("rejected");
        let msg = err.to_string();
        assert!(msg.contains("RODATA"), "got `{msg}`");
        assert!(msg.contains("not in the NES config"), "got `{msg}`");
        assert!(
            msg.contains("CODE") && msg.contains("VECTORS"),
            "got `{msg}`"
        );
    }

    /// The expression-function seam: a name followed by `(` is a call, and an
    /// unimplemented `.`-word says so rather than reading as an undefined
    /// symbol.
    #[test]
    fn expression_functions_extract_bytes() {
        let r = rom(".code\nV = $123456\n lda #.lobyte(V)\n lda #.hibyte(V)\n lda #.bankbyte(V)\n");
        assert_eq!(&r[16..22], &[0xA9, 0x56, 0xA9, 0x34, 0xA9, 0x12]);
        // Nested, and over an expression rather than a bare symbol.
        let n = rom(".code\n lda #.lobyte($1234+1)\n lda #.lobyte(.hibyte($123456))\n");
        assert_eq!(&n[16..20], &[0xA9, 0x35, 0xA9, 0x34]);

        // The word extractions, which have no `Expr` node and need none.
        let w = rom(".code\nV = $123456\n .word .loword(V)\n .word .hiword(V)\n");
        assert_eq!(&w[16..20], &[0x56, 0x34, 0x12, 0x00]);

        // Two arguments, and the wrong count refused rather than ignored.
        let m = rom(".code\n lda #.max(3, 7)\n lda #.min(3, 7)\n lda #.min(.max(1,5), 9)\n");
        assert_eq!(&m[16..22], &[0xA9, 0x07, 0xA9, 0x03, 0xA9, 0x05]);
        for (src, why) in [
            (
                ".code\n lda #.max(3)\n",
                "one argument to a two-argument function",
            ),
            (
                ".code\n lda #.lobyte(1, 2)\n",
                "two arguments to a one-argument function",
            ),
        ] {
            assert!(assemble(src).is_err(), "{why}");
        }

        // A string argument yields a number; the wrong argument kind is named.
        let t = rom(".code\n lda #.strlen(\"hello\")\n lda #.strat(\"abc\", 1)\n");
        assert_eq!(&t[16..20], &[0xA9, 0x05, 0xA9, 0x62]);
        for (src, want) in [
            (".code\n lda #.strlen(5)\n", "takes a string, not a value"),
            (
                ".code\n lda #.lobyte(\"hi\")\n",
                "takes a value, not a string",
            ),
        ] {
            let e = assemble(src).expect_err(src).to_string();
            assert!(e.contains(want), "expected `{want}`, got `{e}`");
        }

        // `.defined` is positional — the whole reason it is answered in the
        // projection rather than folded while parsing.
        let d = rom(
            ".code\n lda #.defined(L)\nL = 7\n lda #.defined(L)\n lda #.def(L)\n\
             .if .defined(L)\n lda #$33\n .else\n lda #$44\n .endif\n",
        );
        assert_eq!(
            &d[16..24],
            &[0xA9, 0x00, 0xA9, 0x01, 0xA9, 0x01, 0xA9, 0x33]
        );
        // A label counts as defined, not just an `=` constant.
        let l = rom(".code\nhere: lda #1\n lda #.defined(here)\n");
        assert_eq!(&l[18..20], &[0xA9, 0x01]);

        // A `.`-word we do not implement — with a plain argument, since a
        // string literal fails earlier, in the tokenizer.
        let err = assemble(".code\nV = 1\n lda #.tcount(V)\n").expect_err("not implemented");
        assert!(
            err.to_string().contains("not an expression function"),
            "got `{err}`"
        );
        // `.sizeof` *is* implemented, and refuses a name that declared no size
        // — ca65's `Size of 'V' is unknown`.
        let err = assemble(".code\nV = 1\n lda #.sizeof(V)\n").expect_err("no size");
        assert!(
            err.to_string().contains("size of `V` is unknown"),
            "got `{err}`"
        );
    }

    #[test]
    fn the_segment_shorthands_are_their_spelled_out_segments() {
        // `.code` is `.segment "CODE"`, `.zeropage` is ZEROPAGE, `.bss` is
        // BSS — the same placement, reached by the shorter spelling.
        let long = rom(".segment \"ZEROPAGE\"\npos: .res 1\n\
             .segment \"BSS\"\nbuf: .res 4\n\
             .segment \"CODE\"\n lda pos\n sta buf\n");
        let short = rom(".zeropage\npos: .res 1\n.bss\nbuf: .res 4\n.code\n lda pos\n sta buf\n");
        assert_eq!(long, short);
        // `pos` is zero-page, `buf` is RAM at the config's $0300 (OAM has the
        // page below it).
        assert_eq!(&short[16..21], &[0xA5, 0x00, 0x8D, 0x00, 0x03]);
    }

    /// `.align` pads within the segment, so the boundary is measured from the
    /// segment's start rather than from the CPU address — the distinction only
    /// shows on a boundary the segment base is not itself a multiple of.
    #[test]
    fn align_pads_within_the_segment() {
        // CODE is based at $8000, which is not a multiple of 3; the pad still
        // lands the next byte at segment offset 3.
        let r = rom(".code\n .byte 1\n .align 3\n .byte 2\n");
        assert_eq!(&r[16..20], &[1, 0, 0, 2]);
        let f = rom(".code\n .byte 1\n .align 4, $ff\n .byte 2\n");
        assert_eq!(&f[16..21], &[1, 0xFF, 0xFF, 0xFF, 2]);
        let none = rom(".code\n .byte 1,2,3,4\n .align 4\n .byte 9\n");
        assert_eq!(&none[16..21], &[1, 2, 3, 4, 9]);
    }

    /// A label on the `.align` line binds *before* the pad, not after it —
    /// probe-pinned, and the opposite of what an alignment directive reads like.
    #[test]
    fn a_label_on_an_align_line_binds_before_the_pad() {
        let r = rom(
            ".code\n .byte 1\nhere: .align 4\n .byte 2\n             .segment \"VECTORS\"\n .word here, 0, 0\n",
        );
        // $8001 — where `here` stood, not $8004 where the next byte landed.
        assert_eq!(&r[16 + 0x7FFA..16 + 0x7FFC], &[0x01, 0x80]);
    }

    #[test]
    fn popseg_restores_the_segment_pushseg_saved() {
        // The reservation between the pair happens in ZEROPAGE; the code
        // either side of it stays in CODE, contiguous across the interruption.
        let r = rom(".code\n lda #1\n\
             .pushseg\n.zeropage\ntmp: .res 1\n.popseg\n\
             ldx #2\n");
        assert_eq!(&r[16..20], &[0xA9, 0x01, 0xA2, 0x02]);
    }

    #[test]
    fn a_shorthand_for_a_segment_the_config_lacks_is_rejected_too() {
        // `ca65` accepts `.data`; `ld65` then has no memory area for it. The
        // refusal is the same one the spelled-out form gets.
        let err = assemble(".data\n .byte 1\n").expect_err("rejected");
        assert!(err.to_string().contains("DATA"), "got `{err}`");
    }

    #[test]
    fn header_and_code_and_vectors_place_correctly() {
        let src = "\
.segment \"HEADER\"\n\
    .byte \"NES\", $1A, 2, 1\n\
.segment \"CODE\"\n\
reset:\n\
    sei\n\
nmi:\n\
    rti\n\
irq:\n\
    rti\n\
.segment \"VECTORS\"\n\
    .word nmi, reset, irq\n";
        let r = rom(src);
        // iNES magic.
        assert_eq!(&r[..5], &[0x4E, 0x45, 0x53, 0x1A, 0x02]);
        // CODE at $8000 (file offset 16): sei, rti, rti.
        assert_eq!(&r[16..19], &[0x78, 0x40, 0x40]);
        // reset=$8000, nmi=$8001, irq=$8002. VECTORS at $FFFA (file off 16+0x7FFA).
        let v = 16 + 0x7FFA;
        assert_eq!(&r[v..v + 6], &[0x01, 0x80, 0x00, 0x80, 0x02, 0x80]);
    }

    #[test]
    fn zeropage_label_uses_zp_addressing() {
        let src = "\
.segment \"ZEROPAGE\"\n\
counter: .res 1\n\
.segment \"CODE\"\n\
    sta counter\n";
        let r = rom(src);
        // sta zp = $85 $00 (counter at $00), not abs $8D.
        assert_eq!(&r[16..18], &[0x85, 0x00]);
    }

    #[test]
    fn anonymous_labels_resolve_by_direction() {
        // Byte-for-byte against ca65 + ld65 -t none. CODE at $8000:
        //   ldx #0 / : inx / bne :- / jmp :+ / nop / : rts
        let src = "\
.segment \"CODE\"\n\
    ldx #0\n\
:   inx\n\
    bne :-\n\
    jmp :+\n\
    nop\n\
:   rts\n";
        let r = rom(src);
        assert_eq!(
            &r[16..26],
            &[0xA2, 0x00, 0xE8, 0xD0, 0xFD, 0x4C, 0x09, 0x80, 0xEA, 0x60]
        );
    }

    #[test]
    fn anonymous_label_multi_distance() {
        // `:--` counts two anonymous labels back. ca65 + ld65: ea ea 4c 00 80.
        let src = "\
.segment \"CODE\"\n\
:   nop\n\
:   nop\n\
    jmp :--\n";
        let r = rom(src);
        assert_eq!(&r[16..21], &[0xEA, 0xEA, 0x4C, 0x00, 0x80]);
    }

    #[test]
    fn dword_dbyt_asciiz_match_reference_bytes() {
        // Byte-for-byte against `ca65 --cpu 6502` + `ld65 -t none`:
        //   .dword $12345678 -> 78 56 34 12 (32-bit little-endian)
        //   .dbyt  $1234     -> 12 34       (16-bit big-endian)
        //   .asciiz "hi"     -> 68 69 00    (string + one terminating NUL)
        let r = rom(".segment \"CODE\"\n.dword $12345678\n.dbyt $1234\n.asciiz \"hi\"\n");
        assert_eq!(
            &r[16..25],
            &[0x78, 0x56, 0x34, 0x12, 0x12, 0x34, 0x68, 0x69, 0x00]
        );
    }

    #[test]
    fn cheap_locals_scope_to_global() {
        let src = "\
.segment \"CODE\"\n\
one:\n\
@loop:\n\
    jmp @loop\n\
two:\n\
@loop:\n\
    jmp @loop\n";
        let r = rom(src);
        // one@loop at $8000: jmp $8000. two@loop at $8003: jmp $8003.
        assert_eq!(&r[16..22], &[0x4C, 0x00, 0x80, 0x4C, 0x03, 0x80]);
    }

    // -----------------------------------------------------------------------
    // Conditionals and repetition. Measured against ca65 V2.18; folded once, in
    // source order, before layout — `decisions/conditionals-in-multipass-dialects.md`.
    // -----------------------------------------------------------------------

    fn bytes(src: &str) -> Vec<u8> {
        let rom = crate::assemble_ca65(src).expect(src).bytes;
        // The NES ROM is header + 32K PRG; the code sits at the PRG's start.
        rom[16..].to_vec()
    }

    /// `.out` prints its string and emits nothing. ca65 writes the text bare,
    /// with no file/line prefix, so the note carries the text alone.
    #[test]
    fn out_is_a_note_and_emits_nothing() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.out \"hello there\"\nnop\n")
            .expect("assembles");
        assert_eq!(r.bytes[16], 0xEA);
        let note = r.warnings.first().expect("one note");
        assert_eq!(note.message, "hello there");
        assert_eq!(note.kind, crate::engine::WarningKind::Note);
        assert!(note.to_string().contains("note:"), "got `{note}`");
    }

    /// `.out` takes one string: ca65 answers a value list with `Unexpected
    /// trailing garbage characters` and a bare number with `String constant
    /// expected`. Both are refused here too.
    #[test]
    fn out_refuses_anything_but_one_string() {
        for src in [
            ".segment \"CODE\"\n.out \"a\", 5\n",
            ".segment \"CODE\"\n.out 5\n",
        ] {
            let err = crate::assemble_ca65(src).expect_err(src);
            assert!(err.to_string().contains("one string"), "got `{err}`");
        }
    }

    /// `.export` and `.exportzp` require the name be defined somewhere in the
    /// program — ca65 answers `Exported symbol 'nope' was never defined` with
    /// nothing referencing it, which is what makes this a check and not a
    /// no-op. The name may be defined below the directive.
    #[test]
    fn an_exported_name_must_be_defined() {
        for word in [".export", ".exportzp"] {
            let src = format!(".segment \"CODE\"\n{word} foo\nfoo: .byte 1\n");
            crate::assemble_ca65(&src).expect("defined below");

            let src = format!(".segment \"CODE\"\n{word} nope\n.byte 1\n");
            let err = crate::assemble_ca65(&src).expect_err(word);
            assert!(
                err.to_string().contains("`nope` was never defined"),
                "{word}: got `{err}`"
            );
        }
        // A list, and repeating an export is not an error.
        crate::assemble_ca65(
            ".segment \"CODE\"\n.export foo, bar\n.export foo\nfoo: .byte 1\nbar: .byte 2\n",
        )
        .expect("a list");
    }

    /// `.export name := expr` defines the name as it exports it. Only the
    /// export spellings take the form: ca65 answers `Unexpected trailing
    /// garbage characters` for `.global bar := 9` and `.import baz := 3`.
    #[test]
    fn export_assigns_as_well_as_exports() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.export k := 7\n.byte k\n")
            .expect("defines k");
        assert_eq!(r.bytes[16], 7);
        for word in [".global", ".import"] {
            let src = format!(".segment \"CODE\"\n{word} v := 1\n.byte 1\n");
            let err = crate::assemble_ca65(&src).expect_err(word);
            assert!(err.to_string().contains("`:=`"), "{word}: got `{err}`");
        }
    }

    /// `.import` claims a name defined elsewhere, so defining it here is
    /// ca65's `Symbol 'zz' is already an import` — reported at the definition,
    /// not at the import. Leaving it undefined is fine until something reads
    /// it, and then it is the ordinary undefined-symbol refusal (ld65 calls it
    /// an unresolved external).
    #[test]
    fn an_imported_name_may_not_be_defined_here() {
        for word in [".import", ".importzp"] {
            let src = format!(".segment \"CODE\"\n{word} zz\n.byte 1\n");
            crate::assemble_ca65(&src).expect("unreferenced is fine");

            let src = format!(".segment \"CODE\"\n{word} zz\n.byte 1\nzz: .byte 2\n");
            let err = crate::assemble_ca65(&src).expect_err(word);
            let message = err.to_string();
            assert!(message.contains("already an import"), "{word}: {message}");
            assert!(
                message.contains("line 4"),
                "reported at the definition: {message}"
            );

            let src = format!(".segment \"CODE\"\n{word} zz\n.byte zz\n");
            crate::assemble_ca65(&src).expect_err("referencing it cannot resolve");
        }
    }

    /// `.global` is export-if-defined and import-if-not, so both are legal and
    /// there is nothing to check. The `zp` spelling still warns.
    #[test]
    fn global_checks_nothing_either_way() {
        crate::assemble_ca65(".segment \"CODE\"\n.global g\ng: .byte 1\n").expect("defined");
        crate::assemble_ca65(".segment \"CODE\"\n.global g\n.byte 1\n").expect("not defined");
    }

    /// The `zp` spellings warn for a **label** that is not in the zero page.
    /// A constant never draws the warning, whatever its value — probed with
    /// `K = 7` and `K = $10`, both silent.
    #[test]
    fn exporting_an_absolute_label_as_zeropage_warns() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.exportzp foo\nfoo: .byte 1\n")
            .expect("assembles");
        let w = r.warnings.first().expect("one warning");
        assert!(w.message.contains("absolute but exported zeropage"), "{w}");

        for quiet in [
            ".segment \"ZEROPAGE\"\np: .res 1\n.segment \"CODE\"\n.exportzp p\n.byte 1\n",
            ".segment \"CODE\"\nK = 7\n.exportzp K\n.byte 1\n",
            ".segment \"CODE\"\nK = $10\n.exportzp K\n.byte 1\n",
        ] {
            let r = crate::assemble_ca65(quiet).expect(quiet);
            assert!(r.warnings.is_empty(), "{quiet}: got {:?}", r.warnings);
        }
        // `.globalzp` carries the same warning — it exports when the name is
        // defined here, and this one is.
        let r = crate::assemble_ca65(".segment \"CODE\"\n.globalzp foo\nfoo: .byte 1\n")
            .expect("assembles");
        assert_eq!(r.warnings.len(), 1);
    }

    /// `.forceimport` cannot be satisfied: defining the name is `already an
    /// import`, and not defining it is an unresolved external at ld65 even
    /// with nothing referencing it. Refused rather than counted a gap.
    #[test]
    fn forceimport_is_refused_as_the_reference_refuses_it() {
        let err = crate::assemble_ca65(".segment \"CODE\"\n.forceimport zz\n.byte 1\n")
            .expect_err("cannot be satisfied");
        let message = err.to_string();
        assert!(message.contains("linker resolves it"), "{message}");
        assert!(!message.contains("does not implement"), "{message}");
    }

    /// `.autoimport +`/`-` switches ca65's automatic runtime imports. There is
    /// no runtime library here and it emits nothing.
    #[test]
    fn autoimport_is_accepted_and_discarded() {
        for sign in ["+", "-"] {
            let r =
                crate::assemble_ca65(&format!(".segment \"CODE\"\n.autoimport {sign}\n.byte 1\n"))
                    .expect(sign);
            assert_eq!(r.bytes[16], 1);
        }
    }

    /// `.warning` says its piece and assembles; `.error` and `.fatal` do not.
    /// ca65 prefixes the text with `User warning:`/`User error:` — its own
    /// classification, not the source's words, so it is not reproduced.
    #[test]
    fn the_message_words_carry_their_severity() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.warning \"soft\"\nnop\n")
            .expect("a warning assembles");
        assert_eq!(r.bytes[16], 0xEA);
        let w = r.warnings.first().expect("one warning");
        assert_eq!(w.message, "soft");
        assert_eq!(w.kind, crate::engine::WarningKind::Advisory);

        for word in ["error", "fatal"] {
            let src = format!(".segment \"CODE\"\n.{word} \"hard\"\nnop\n");
            let err = crate::assemble_ca65(&src).expect_err(&src);
            assert!(err.to_string().contains("hard"), "got `{err}`");
        }
    }

    /// The message words read one string and skip the rest of the line, where
    /// `.out` refuses the same shape. Both spellings were probed; the manual
    /// reads alike for the two.
    #[test]
    fn a_message_word_ignores_what_follows_its_string() {
        for src in [
            ".segment \"CODE\"\n.warning \"a\", 5\nnop\n",
            ".segment \"CODE\"\n.warning \"a\" \"b\"\nnop\n",
        ] {
            let r = crate::assemble_ca65(src).expect(src);
            assert_eq!(r.warnings.first().expect("one warning").message, "a");
        }
        let err = crate::assemble_ca65(".segment \"CODE\"\n.warning 5\n").expect_err("no string");
        assert!(err.to_string().contains("string constant"), "got `{err}`");
    }

    /// A message inside an untaken branch is never said: conditionals fold
    /// before layout, so the statement does not reach the emit pass.
    #[test]
    fn a_message_in_an_untaken_branch_stays_quiet() {
        let r = crate::assemble_ca65(
            ".segment \"CODE\"\n.if 0\n.error \"no\"\n.warning \"nor\"\n.endif\nnop\n",
        )
        .expect("assembles");
        assert!(r.warnings.is_empty(), "got `{:?}`", r.warnings);
    }

    /// `.assert` fires only when its condition is zero, and the action decides
    /// whether that stops the assembly. The default message is ca65's own.
    #[test]
    fn assert_fires_on_zero_and_the_action_decides() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.assert 1, error, \"never\"\nnop\n")
            .expect("true assertion is silent");
        assert!(r.warnings.is_empty(), "got `{:?}`", r.warnings);

        let r = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, warning, \"soft\"\nnop\n")
            .expect("a warning assembles anyway");
        assert_eq!(r.bytes[16], 0xEA);
        let w = r.warnings.first().expect("one warning");
        assert_eq!(w.message, "soft");
        assert_eq!(w.kind, crate::engine::WarningKind::Advisory);

        let err = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, error, \"boom\"\nnop\n")
            .expect_err("an error does not");
        assert!(err.to_string().contains("boom"), "got `{err}`");

        let r = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, warning\nnop\n")
            .expect("assembles");
        assert_eq!(
            r.warnings.first().expect("one warning").message,
            "Assertion failed"
        );
    }

    /// The condition folds against the finished symbol table, so it may name a
    /// label defined below it. Real ca65 defers such an assertion to `ld65`;
    /// with assembly and linking fused there is no moment between the two.
    #[test]
    fn assert_sees_a_label_defined_below_it() {
        let src = ".segment \"CODE\"\n.assert later = $8001, error, \"moved\"\nnop\nlater:\n";
        crate::assemble_ca65(src).expect("the label is $8001");
        let src = ".segment \"CODE\"\n.assert later = $1234, error, \"moved\"\nnop\nlater:\n";
        let err = crate::assemble_ca65(src).expect_err("it is not $1234");
        assert!(err.to_string().contains("moved"), "got `{err}`");
    }

    /// The four action words ca65 accepts, and nothing else. `lderror` and
    /// `ldwarning` name the link stage, which is not a separate step here.
    #[test]
    fn assert_takes_the_four_action_words() {
        for (action, fatal) in [
            ("warning", false),
            ("ldwarning", false),
            ("error", true),
            ("lderror", true),
        ] {
            let src = format!(".segment \"CODE\"\n.assert 0, {action}, \"m\"\nnop\n");
            let r = crate::assemble_ca65(&src);
            assert_eq!(r.is_err(), fatal, "{action}");
        }
        let err = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, oops, \"m\"\n")
            .expect_err("bad action");
        assert!(err.to_string().contains("action"), "got `{err}`");
    }

    #[test]
    fn a_conditional_picks_one_branch() {
        assert_eq!(bytes("N=1\n.if N\n nop\n.endif\n rts\n")[..2], [0xEA, 0x60]);
        assert_eq!(bytes(".if 0\n nop\n.else\n rts\n.endif\n")[..1], [0x60]);
        assert_eq!(
            bytes(
                ".if 0\n lda #1\n.elseif 0\n lda #2\n.elseif 1\n lda #3\n.else\n lda #4\n.endif\n"
            )[..2],
            [0xA9, 0x03]
        );
    }

    /// `.ifdef` tests what is defined **above** it. A definition inside an
    /// untaken branch is invisible afterwards, which is ca65's rule and the
    /// shared walk's `emit = false` rule alike.
    #[test]
    fn an_untaken_branch_defines_nothing() {
        assert_eq!(
            bytes(".if 0\nN = 5\n.endif\n.ifdef N\n nop\n.endif\n rts\n")[..1],
            [0x60]
        );
    }

    /// A condition folds against the constants above it and **may not reach
    /// forward** — ca65 answers `Constant expression expected`, and a ca65
    /// label is never constant because `ld65` relocates it.
    #[test]
    fn a_condition_may_not_reach_forward() {
        crate::assemble_ca65(".if later\n nop\n.endif\nlater = 1\n")
            .expect_err("ca65: Constant expression expected");
    }

    #[test]
    fn a_repetition_assembles_its_body_n_times() {
        assert_eq!(
            bytes(".repeat 3\n nop\n.endrepeat\n")[..3],
            [0xEA, 0xEA, 0xEA]
        );
        assert_eq!(bytes(".repeat 0\n nop\n.endrepeat\n rts\n")[..1], [0x60]);
        crate::assemble_ca65(".repeat -1\n nop\n.endrepeat\n").expect_err("ca65: Range error");
    }

    /// The loop variable is **0-based**, and baked into each use — ca65
    /// resolves a symbol once, in a later pass, against one table, and a loop
    /// variable holds a different value on every iteration.
    #[test]
    fn the_loop_variable_is_zero_based_and_live() {
        assert_eq!(
            bytes(".repeat 3, i\n lda #i\n.endrepeat\n")[..6],
            [0xA9, 0x00, 0xA9, 0x01, 0xA9, 0x02]
        );
        assert_eq!(
            bytes(".repeat 4, i\n .byte i*2\n.endrepeat\n")[..4],
            [0, 2, 4, 6]
        );
        // A nested loop shadows, innermost first.
        assert_eq!(
            bytes(".repeat 2, i\n.repeat 2, j\n lda #i*16+j\n.endrepeat\n.endrepeat\n")[..8],
            [0xA9, 0x00, 0xA9, 0x01, 0xA9, 0x10, 0xA9, 0x11]
        );
        // And a condition inside the body sees it.
        assert_eq!(
            bytes(".repeat 3, i\n.if i\n nop\n.endif\n.endrepeat\n rts\n")[..3],
            [0xEA, 0xEA, 0x60]
        );
    }

    /// **Scoped to the loop**, unlike acme's `!for` variable which survives its
    /// block. Two dialects, two rules — which is the point of arbitrating each
    /// against its own reference rather than sharing one.
    #[test]
    fn the_loop_variable_does_not_outlive_the_loop() {
        crate::assemble_ca65(".repeat 2, i\n nop\n.endrepeat\n lda #i\n")
            .expect_err("ca65: Symbol 'i' is undefined");
    }

    /// A directive ca65 has and we do not is refused as *that*, and a word it
    /// does not have is refused as that. The fall-through used to answer
    /// `unsupported directive` for both, which tells a reader with valid
    /// source to go looking for a typo.
    #[test]
    fn a_real_directive_is_told_apart_from_a_typo() {
        let err = |src: &str| super::assemble(src).expect_err(src).to_string();
        // `.condes` rather than `.export`, which became an implemented check
        // once ca65 turned out to enforce one. `.condes` builds an ld65
        // constructor table from linker-config features our fixed layout does
        // not declare, so it stays a gap by decision rather than by schedule
        // (`decisions/reference-parity-goal.md`).
        for d in [
            ".condes foo, 1",
            ".charmap $41, $42",
            ".org $200",
            ".macpack cpu",
        ] {
            let e = err(&format!("\t{d}\n"));
            assert!(
                e.contains("is a real directive here"),
                "`{d}` should name itself a real ca65 directive, got: {e}"
            );
        }
        assert!(
            err("\t.zzqq\n").contains("is not a directive ca65 has"),
            "and a word ca65 does not have should say so"
        );
    }

    /// ca65 spells every operator twice, and both spellings are one operator:
    /// the keyword lands on its symbol twin's token, so it inherits the same
    /// precedence rather than getting a second, nearly-right one.
    #[test]
    fn a_keyword_operator_is_its_symbol_twin() {
        let byte = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n.byte {src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .0
        };
        for (keyword, symbol) in [
            ("4 + 1 .bitand 1", "4 + 1 & 1"),
            ("4 + 2 .bitor 1", "4 + 2 | 1"),
            ("6 .bitxor 3 + 1", "6 ^ 3 + 1"),
            ("1 .shl 2 + 1", "1 << 2 + 1"),
            ("$80 .shr 3", "$80 >> 3"),
            (".bitnot 0 .bitand $FF", "~0 & $FF"),
            ("1 .and 0 .or 1", "1 && 0 || 1"),
            ("0 .or 1 .and 0", "0 || 1 && 0"),
            (".not 1 .or 1", "! 1 || 1"),
        ] {
            assert_eq!(byte(keyword), byte(symbol), "`{keyword}` vs `{symbol}`");
        }
    }

    /// The two things about this layer that are not the obvious reading, both
    /// measured against ca65 V2.18.
    #[test]
    fn the_logical_layer_reads_as_ca65_reads_it() {
        // The value is fenced, because the linked image is padded and a `0`
        // answer would otherwise be indistinguishable from the padding.
        let byte = |src: &str| {
            let image = super::assemble(&format!(".segment \"CODE\"\n.byte $AA,{src},$AA\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .0;
            let at = image
                .windows(3)
                .position(|w| w[0] == 0xAA && w[2] == 0xAA)
                .unwrap_or_else(|| panic!("{src}: no fenced value in the image"));
            vec![image[at + 1]]
        };
        // `.not` binds looser than everything, so the `.or` happens first and
        // the `.not` negates the lot. Read as an ordinary prefix this is 1.
        assert_eq!(byte(".not 1 .or 1"), vec![0]);
        // The logical operators answer 1 or 0 whatever they are given, where
        // the bitwise ones answer the bits.
        assert_eq!(byte("2 .and 3"), vec![1]);
        assert_eq!(byte("2 .bitand 3"), vec![2]);
        // `.mod` sits with `*` and `/`, and takes C's sign rule.
        assert_eq!(byte("7 .mod 4 + 1"), vec![4]);
        assert_eq!(byte("7 .mod (0 - 4)"), vec![3]);
    }

    /// The plural extractors, over the NES path — the differential covers the
    /// flat 65816 dialect, and this dialect has its own directive table.
    #[test]
    fn the_extractors_take_a_byte_from_each_value() {
        // The image is a linked NES ROM — header, then padded segments — so the
        // values are found behind a `$AA,$55` fence rather than at a fixed
        // offset.
        let bytes = |src: &str| {
            let image = super::assemble(&format!(".segment \"CODE\"\n .byte $AA,$55\n{src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .0;
            let at = image
                .windows(2)
                .position(|w| w == [0xAA, 0x55])
                .unwrap_or_else(|| panic!("{src}: no fence in the image"));
            image[at + 2..].to_vec()
        };
        assert_eq!(&bytes(" .lobytes $1234, $5678")[..2], &[0x34, 0x78]);
        assert_eq!(&bytes(" .hibytes $1234, $5678")[..2], &[0x12, 0x56]);
        // Byte 2 of the value, above 24 bits as well as within them.
        assert_eq!(&bytes(" .bankbytes $123456, $12345678")[..2], &[0x12, 0x34]);
        // Three bytes, little-endian, per value.
        assert_eq!(
            &bytes(" .faraddr $112233, $445566")[..6],
            &[0x33, 0x22, 0x11, 0x66, 0x55, 0x44]
        );
        // Masked, not refused: ca65 answers `.hibytes 0-1` with $FF.
        assert_eq!(
            &bytes(" .lobytes 0-1\n .hibytes 0-1\n .bankbytes 0-1")[..3],
            &[0xFF; 3]
        );
    }

    /// `.forceword` is a word ca65's tokenizer knows and its parser takes
    /// nowhere — statement position, after a label, as a struct storage
    /// allocator, as an address-size specifier, inside an expanded macro: every
    /// one answers `Unexpected '.FORCEWORD'`. Refusing it is matching V2.18,
    /// not a gap, so it reads as a refusal rather than as an unimplemented
    /// directive.
    #[test]
    fn a_word_the_reference_never_accepts_is_refused_as_one() {
        let e = super::assemble(".segment \"CODE\"\n .forceword $12\n")
            .expect_err("ca65 refuses it, so we must");
        let text = e.to_string();
        assert!(
            text.contains("accepts in no position"),
            "should name the reference's own refusal, got: {text}"
        );
        assert!(
            !text.contains("the gap is ours"),
            "and must not read as an unimplemented directive: {text}"
        );
    }

    /// The nine conditional heads beyond `.if`/`.ifdef`/`.ifndef`, each folded
    /// the way ca65 V2.18 folds it.
    #[test]
    fn the_conditional_heads_fold_as_ca65_folds_them() {
        // Each case is a head, then the byte the taken branch emits.
        let taken = |head: &str| {
            let src = format!(
                ".segment \"CODE\"\nV = 5\nLA:\nLB:\n{head}\n .byte $11\n .else\n .byte $22\n .endif\nLC: .byte 0\n"
            );
            let image = super::assemble(&src)
                .unwrap_or_else(|e| panic!("{head}: {e}"))
                .0;
            let at = image
                .iter()
                .position(|&b| b == 0x11 || b == 0x22)
                .unwrap_or_else(|| panic!("{head}: neither branch emitted"));
            image[at]
        };
        // `.ifblank` asks whether anything follows it on the line.
        assert_eq!(taken(".ifblank"), 0x11);
        assert_eq!(taken(".ifblank x"), 0x22);
        assert_eq!(taken(".ifnblank"), 0x22);
        assert_eq!(taken(".ifnblank x"), 0x11);
        // `.ifconst` asks whether the expression *is* constant, and answers
        // rather than failing when it is not.
        assert_eq!(taken(".ifconst 1+1"), 0x11);
        assert_eq!(taken(".ifconst V*2"), 0x11);
        assert_eq!(taken(".ifnconst 1"), 0x22);
        // A label is not constant; a difference of two labels above the line in
        // one segment is; a forward one is not, and neither is a label that has
        // been multiplied.
        assert_eq!(taken(".ifconst LB-LA"), 0x11);
        assert_eq!(taken(".ifconst LA"), 0x22);
        // `LC` stands below the line, and a name this line has not seen yet is
        // not constant however it is used.
        assert_eq!(taken(".ifconst LC-LA"), 0x22);
        assert_eq!(taken(".ifconst LA*2"), 0x22);
        assert_eq!(taken(".ifconst .lobyte(LA)"), 0x22);
        // `*` is a label of the active segment, so it cancels against itself
        // and against nothing else.
        assert_eq!(taken(".ifconst *-*"), 0x11);
        assert_eq!(taken(".ifconst *"), 0x22);
        // The CPU tests: this leg is a 6502 and refuses `.setcpu`, so the
        // answer cannot change while a file is read.
        assert_eq!(taken(".ifp02"), 0x11);
        for head in [".ifp816", ".ifpc02", ".ifpsc02", ".ifp4510"] {
            assert_eq!(taken(head), 0x22, "{head}");
        }
    }

    /// What a scope does to a name, measured against ca65 V2.18. The
    /// byte-for-byte agreement is the differential fixture's job; these are the
    /// rules that fixture would not show if it broke.
    #[test]
    fn a_scope_qualifies_the_names_inside_it() {
        // A name in two procs is two symbols, and each `.word` reads its own.
        let image = rom(".segment \"CODE\"\n\
             .proc one\ninner: nop\n.endproc\n\
             .proc two\ninner: nop\n.endproc\n\
             .word one::inner, two::inner, one, two\n");
        assert_eq!(&image[16..20], &[0xEA, 0xEA, 0x00, 0x80]);
        assert_eq!(&image[20..24], &[0x01, 0x80, 0x00, 0x80]);

        // `.scope` opens one and defines nothing, so its name is not a symbol.
        super::assemble(".segment \"CODE\"\n.scope s\n nop\n.endscope\n .word s\n")
            .expect_err("ca65: Symbol 's' is undefined");

        // Lookup walks outward, and `::` is the top level from anywhere.
        let image = rom(".segment \"CODE\"\nv = $11\n.proc p\nv = $22\n .byte v, ::v\n.endproc\n");
        assert_eq!(&image[16..18], &[0x22, 0x11]);

        // A name defined inside is invisible outside it.
        super::assemble(".segment \"CODE\"\n.proc p\ninner: nop\n.endproc\n .word inner\n")
            .expect_err("ca65: Symbol 'inner' is undefined");
    }

    /// The ways a scope can be written wrongly, each answered the way ca65
    /// answers it.
    #[test]
    fn a_scope_has_to_be_opened_and_closed() {
        let err = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n{src}"))
                .expect_err(src)
                .to_string()
        };
        // The closers are paired by kind: ca65 answers `.endscope` over an open
        // `.proc` with `No open .SCOPE`, not with a mismatch report.
        assert!(err(".proc p\n nop\n.endscope\n").contains("no open `.scope`"));
        assert!(err(".scope s\n nop\n.endproc\n").contains("no open `.proc`"));
        assert!(err(".endproc\n").contains("no open `.proc`"));
        // ca65: `Local scope was not closed`.
        assert!(err(".proc p\n nop\n").contains("never closed"));
        // ca65: `Duplicate scope 'sa'` — one name per parent.
        assert!(err(".scope sa\n.endscope\n.scope sa\n.endscope\n").contains("duplicate scope"));
        // A scope reference may only name one opened above it: ca65 answers
        // `No such scope: 'p'` rather than waiting to see whether one arrives.
        assert!(err(" .word p::inner\n.proc p\ninner: nop\n.endproc\n").contains("no such scope"));
    }

    /// Cheap locals belong to the scope they are written in, so the same `@l`
    /// in two procs is two labels rather than a duplicate-symbol error.
    #[test]
    fn a_cheap_local_belongs_to_its_scope() {
        let image = rom(".segment \"CODE\"\n\
             .proc one\n@l: nop\n bne @l\n.endproc\n\
             .proc two\n@l: nop\n bne @l\n.endproc\n");
        // Each branch reaches its own `@l`: nop, bne -3, twice over.
        assert_eq!(&image[16..22], &[0xEA, 0xD0, 0xFD, 0xEA, 0xD0, 0xFD]);
    }

    /// The listing words are accepted and change nothing; the option words that
    /// change the parse are still refused, and say so.
    #[test]
    fn a_listing_word_is_accepted_and_a_parse_option_is_not() {
        let plain = rom(".segment \"CODE\"\n .byte $AA\n");
        let noisy = rom(".segment \"CODE\"\n\
             .debuginfo on\n .fileopt comment, \"x\"\n .fopt author, \"y\"\n\
             .list off\n .listbytes 4\n .pagelen 60\n .pagelength 60\n .dbg line\n\
             .byte $AA\n");
        assert_eq!(plain, noisy, "the listing words change no byte");
        // `.linecont +` and `.case off` change what the parse does, so they are
        // a gap rather than something to discard.
        for word in [".linecont +", ".case off", ".feature labels_without_colons"] {
            let e = super::assemble(&format!(".segment \"CODE\"\n {word}\n"))
                .expect_err(word)
                .to_string();
            assert!(
                e.contains("is a real directive here"),
                "`{word}` should read as our gap, got: {e}"
            );
        }
    }

    /// A record declares constants and emits nothing; `.tag` is what emits.
    #[test]
    fn a_record_declares_a_layout() {
        let image = rom(".segment \"CODE\"\n\
             .struct Point\npx .byte\npy .byte\npw .word\n.endstruct\n\
             .union U\nua .byte\nub .word\n.endunion\n\
             .enum E\nea\neb = 10\nec\n.endenum\n\
             .byte Point::px, Point::py, Point::pw, .sizeof(Point)\n\
             .byte U::ua, U::ub, .sizeof(U)\n\
             .byte E::ea, E::eb, E::ec\n");
        // Struct fields run on; a union lays every member at zero and is as
        // wide as its widest; an enum counts on from an explicit value.
        assert_eq!(&image[16..20], &[0, 1, 2, 4]);
        assert_eq!(&image[20..23], &[0, 0, 2]);
        assert_eq!(&image[23..26], &[0, 10, 11]);

        // `.tag` reserves one instance, and the label binds at its start.
        let image = rom(".segment \"CODE\"\n\
             .struct Point\npx .byte\npy .byte\n.endstruct\n\
             .byte $AA\np: .tag Point\n .word p\n");
        assert_eq!(&image[16..21], &[0xAA, 0, 0, 0x01, 0x80]);
    }

    /// The ways a record can be written wrongly.
    #[test]
    fn a_record_has_to_be_closed_by_its_own_word() {
        let err = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n{src}"))
                .expect_err(src)
                .to_string()
        };
        // ca65 pairs them strictly: inside an `.enum`, `.endstruct` is read as
        // a member name and answered `Identifier expected`.
        assert!(err(".union U\nua .byte\n.endstruct\n").contains("closes a `.union`"));
        assert!(err(".struct S\nsa .byte\n").contains("never closed"));
        assert!(err(".endstruct\n").contains("no open record"));
        // A storage allocator ca65 does not have, and a `.tag` naming nothing.
        assert!(err(".struct S\nsa .zzz\n.endstruct\n").contains("not a storage allocator"));
        assert!(err(".struct S\nsa .tag Nope\n.endstruct\n").contains("not a record declared"));
        // A *field* may repeat (`.tag Inner 3`); a statement may not, which is
        // ca65's `Unexpected trailing garbage characters`.
        assert!(
            err(".struct S\nsa .byte\n.endstruct\nq: .tag S 2\n").contains("takes nothing after")
        );
    }

    /// Naming the processor this leg assembles changes nothing; naming another
    /// is our gap, and naming a word ca65 does not know is a fault in the
    /// source. Three different answers, and the third is ca65's own.
    #[test]
    fn the_processor_words_accept_the_processor_we_assemble() {
        let plain = rom(".segment \"CODE\"\n lda #1\n");
        let named = rom(".segment \"CODE\"\n.setcpu \"6502\"\n.p02\n.smart on\n\
             .pushcpu\n.popcpu\n lda #1\n");
        assert_eq!(plain, named, "naming a 6502 changes no byte");

        let err = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n{src}\n nop\n"))
                .expect_err(src)
                .to_string()
        };
        // ca65's other processors: each a different opcode set, so taking one
        // silently would assemble the wrong instructions.
        for src in [".setcpu \"65C02\"", ".pc02", ".p816", ".p4510", ".psc02"] {
            assert!(
                err(src).contains("the gap is ours"),
                "`{src}` should read as our gap: {}",
                err(src)
            );
        }
        // A name ca65 does not know is its `CPU not supported`, not our gap.
        let e = err(".setcpu \"zz\"");
        assert!(e.contains("not a processor ca65 knows"), "got: {e}");
        // `.asize`/`.isize` are 65816-only, and ca65 refuses them here too.
        let e = err(".asize");
        assert!(e.contains("only valid in 65816 mode"), "got: {e}");
    }

    /// `.ref` asks whether a name has been *used* above the line — a different
    /// question from `.defined`, answered from a different record.
    #[test]
    fn a_reference_is_counted_from_above_the_line() {
        let image = rom(".segment \"CODE\"\n\
             L: nop\n .byte .ref(L)\n .word L\n .byte .ref(L), .referenced(L)\n");
        assert_eq!(&image[16..21], &[0xEA, 0x00, 0x00, 0x80, 0x01]);
        assert_eq!(image[21], 0x01, "`.referenced` is the long spelling");

        // A use inside a branch that is never taken is not a use: the dead leg
        // never reaches the projection, which is where the record is kept.
        let image = rom(".segment \"CODE\"\nM: nop\n.if 0\n .word M\n.endif\n .byte .ref(M)\n");
        assert_eq!(&image[16..18], &[0xEA, 0x00]);

        // A name nothing has mentioned, and one that is never defined at all.
        let image = rom(".segment \"CODE\"\n .byte .ref(ZZ)\n");
        assert_eq!(image[16], 0x00);
    }

    /// `.ifref`/`.ifnref` branch on the same record.
    #[test]
    fn the_reference_conditionals_read_the_same_record() {
        let taken = |src: &str| {
            let image = rom(&format!(
                ".segment \"CODE\"\nL: nop\n{src}\n .byte $11\n .else\n .byte $22\n .endif\n"
            ));
            // The branch byte is found rather than indexed: a `.word L` before
            // the head moves it along.
            *image[16..]
                .iter()
                .find(|&&b| b == 0x11 || b == 0x22)
                .unwrap_or_else(|| panic!("{src}: neither branch emitted"))
        };
        assert_eq!(taken(".ifref L"), 0x22, "nothing has used `L` yet");
        assert_eq!(taken(" .word L\n.ifref L"), 0x11);
        assert_eq!(taken(".ifnref L"), 0x11);
        assert_eq!(taken(" .word L\n.ifnref L"), 0x22);
    }

    /// A pseudo-function is declared as what it is.
    ///
    /// This test used to assert the opposite — that `.lobyte` and `.strlen`
    /// were deliberately absent, because "naming them here would describe them
    /// in a place they never appear". That objection was right about the
    /// model as it stood: every category described a *statement*, so declaring
    /// an expression function claimed it began a line.
    ///
    /// `Category::ExpressionWord` names the place instead, so the objection no
    /// longer holds and the eleven that work stop reading as gaps. What has
    /// not changed is the other half: a function we do **not** implement stays
    /// undeclared, because declaring it would claim we do.
    #[test]
    fn a_pseudo_function_is_declared_as_an_expression_word() {
        let ca65: Vec<crate::directives::Directive> = crate::directives::surfaces()
            .into_iter()
            .filter(|s| s.dialect == "ca65")
            .flat_map(|s| s.directives)
            .collect();
        let kind = |word: &str| {
            ca65.iter()
                .find(|d| d.spellings().iter().any(|s| s == word))
                .map(|d| d.category)
        };
        for f in [".lobyte", ".strlen", ".max", ".defined", ".sizeof"] {
            assert_eq!(
                kind(f),
                Some(crate::directives::Category::ExpressionWord),
                "`{f}` is implemented, so it is declared as what it is"
            );
        }
        for f in [".paramcount", ".tcount"] {
            assert_eq!(
                kind(f),
                None,
                "`{f}` is not implemented, so nothing claims it"
            );
        }
        for d in [".export", ".proc", ".segment", ".byte"] {
            assert!(kind(d).is_some(), "`{d}` should be declared");
        }
    }
}
