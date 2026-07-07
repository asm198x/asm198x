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
use std::collections::BTreeMap;

use super::ca65_flat::{self, DirectiveLine, FlatWalk, WalkDirective};
use super::mos6502::{
    self, BytePrec, assignment_split, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, split_top_level, string_literal,
};
use crate::engine::{AsmError, Expr, Operation};
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
const NES_SEGMENTS: &[(&str, u32, bool)] = &[
    ("ZEROPAGE", 0x0000, false),
    ("OAM", 0x0200, false),
    ("BSS", 0x0300, false),
    ("HEADER", 0x0000, true),
    ("CODE", 0x8000, true),
    ("VECTORS", 0xFFFA, true),
    ("CHARS", 0x0000, true),
];

/// The base address of a segment, and whether it contributes bytes to the ROM.
struct SegInfo {
    base: u32,
    in_file: bool,
}

fn seg_info(seg: &str) -> Option<SegInfo> {
    NES_SEGMENTS
        .iter()
        .find(|(name, _, _)| *name == seg)
        .map(|&(_, base, in_file)| SegInfo { base, in_file })
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
pub(crate) fn assemble(source: &str) -> Result<Vec<u8>, AsmError> {
    assemble_with_debug(source).map(|(rom, _)| rom)
}

/// Assemble + link, also returning the debug [`Capture`] read out of layout
/// (Debug198x U4). One code path: [`assemble`] delegates here, so the bytes
/// with and without capture are identical by construction (AE2).
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble_with_debug(source: &str) -> Result<(Vec<u8>, DebugCapture), AsmError> {
    let (rom, capture) = assemble_program(&parse_program(&isa::mos6502::SET, source)?)?;
    Ok((rom, capture.into_single()))
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
) -> Result<(Vec<u8>, DebugCaptureMulti), AsmError> {
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
) -> Result<(Vec<u8>, DebugCaptureMulti), AsmError> {
    let set = &isa::mos6502::SET;
    // The AST is the single front-end IR: the parse built the source-preserving
    // `Program` (carrying each statement's native `Kind`, `=` constants, and the
    // segment directives); project it to the assembler's `Parsed`. Same
    // bytes as the old direct parse — see
    // `decisions/ast-native-payload-for-multipass-cisc.md`.
    let parsed = parsed_from_program(program)?;

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
        let (resolved, size) = resolve(set, stmt.kind, &size_env, stmt.line)
            .map_err(|e| ca65_flat::stamp_file(e, stmt.file))?;
        *offsets.get_mut(&stmt.seg).expect("segment offset") += size as u32;
        // A line span per byte-emitting statement (address-space-only
        // reservations — ZEROPAGE/BSS `.res` — carry no bytes, so no span; the
        // HEADER segment is iNES file metadata, not CPU-addressed code, so its
        // records would alias CPU $0000 — skipped, per AE3's no-fabrication rule).
        if size > 0 && info.in_file && stmt.seg != "HEADER" {
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
        })
        .collect();

    // Emit pass: turn each placed item into bytes, per segment.
    let mut seg_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (seg, addr, line, file, item) in placed {
        if !seg_info(&seg).expect("seg").in_file {
            continue; // bss/zp segments occupy address space but emit no file bytes
        }
        let buf = seg_bytes.entry(seg).or_default();
        emit(item, addr, &addr_env, buf, line).map_err(|e| ca65_flat::stamp_file(e, file))?;
    }

    let rom = link(&seg_bytes)?;
    Ok((
        rom,
        DebugCaptureMulti {
            sections,
            symbols: dbg_symbols,
            lines: dbg_lines,
        },
    ))
}

/// Lay the file segments into the NROM ROM image.
fn link(seg_bytes: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, AsmError> {
    let empty = Vec::new();
    let get = |s: &str| seg_bytes.get(s).unwrap_or(&empty);

    // iNES header (16 bytes, zero-padded).
    let header = get("HEADER");
    let mut rom = vec![FILL; HEADER_SIZE];
    rom[..header.len().min(HEADER_SIZE)].copy_from_slice(&header[..header.len().min(HEADER_SIZE)]);

    // PRG: CODE at $8000, VECTORS at $FFFA, gap filled.
    let mut prg = vec![FILL; PRG_SIZE];
    let code = get("CODE");
    let vectors = get("VECTORS");
    // CODE reaching the vector table would be silently overwritten by the
    // VECTORS placement below — corrupted code and a debug record describing
    // bytes that did not survive. Reject it, as ld65 does when an area fills.
    if code.len() > (0xFFFA - PRG_BASE) as usize {
        return Err(AsmError::new(
            0,
            format!(
                "segment `CODE` ({} bytes) overlaps `VECTORS` at $FFFA",
                code.len()
            ),
        ));
    }
    place(&mut prg, 0, code, "CODE")?;
    place(&mut prg, (0xFFFA - PRG_BASE) as usize, vectors, "VECTORS")?;
    rom.extend_from_slice(&prg);

    // CHR: CHARS from the start, filled.
    let mut chr = vec![FILL; CHR_SIZE];
    place(&mut chr, 0, get("CHARS"), "CHARS")?;
    rom.extend_from_slice(&chr);

    Ok(rom)
}

fn place(region: &mut [u8], at: usize, bytes: &[u8], name: &str) -> Result<(), AsmError> {
    let end = at + bytes.len();
    if end > region.len() {
        return Err(AsmError::new(
            0,
            format!("segment `{name}` overflows its region"),
        ));
    }
    region[at..end].copy_from_slice(bytes);
    Ok(())
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
}

/// Resolve a parsed statement to an emittable item plus its byte size.
fn resolve(
    set: &'static isa::InstructionSet,
    kind: Kind,
    size_env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(Resolved, usize), AsmError> {
    Ok(match kind {
        Kind::Empty => (Resolved::Nothing, 0),
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

/// Emit one resolved item's bytes at address `addr`.
fn emit(
    item: Resolved,
    addr: u32,
    env: &BTreeMap<String, i64>,
    out: &mut Vec<u8>,
    line_for_errors: usize,
) -> Result<(), AsmError> {
    let pc = i64::from(addr);
    match item {
        Resolved::Nothing => {}
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
                let w = u16::try_from(v & 0xFFFF).expect("masked");
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        Resolved::DBytes(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u16::try_from(v & 0xFFFF).expect("masked");
                out.extend_from_slice(&w.to_be_bytes());
            }
        }
        Resolved::DWords(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u32::try_from(v & 0xFFFF_FFFF).expect("masked");
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

fn to_byte(v: i64, line: usize) -> Result<u8, AsmError> {
    if (-128..=0xFF).contains(&v) {
        Ok((v & 0xFF) as u8)
    } else {
        Err(AsmError::new(
            line,
            format!("value {v} does not fit in a byte"),
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
) -> Result<crate::ast::Program, AsmError> {
    let mut w = Walker::new(set);
    for (i, raw) in source.lines().enumerate() {
        if let Some(d) = w.walk_line(raw, i + 1, FileId(0))? {
            // A `.include`/`.incbin` stays an unresolved item here (KTD1):
            // `--fmt` renders it verbatim without opening the target, and the
            // assembly projection rejects it with a multi-file pointer.
            w.nodes.push(ca65_flat::unresolved_node(d));
        }
    }
    w.finish(source.lines().count() as u32)
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
        self.anons.check()?;
        Ok(Program { nodes: self.nodes })
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

        // `.segment "NAME"` switches the active segment — kept as a source-only
        // node so the formatter reproduces it; the projection reads it back to
        // track the active segment (parse itself needs no segment state).
        if trimmed.starts_with(".segment") {
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
        let kind = parse_op(
            self.set,
            &self.anons,
            &self.current_global,
            &self.consts,
            rest,
            line,
        )?;
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
    use crate::ast::{Item, Operand};
    let mut seg = "CODE".to_string();
    let mut stmts = Vec::new();
    let mut label_seg: BTreeMap<String, String> = BTreeMap::new();
    let mut consts: BTreeMap<String, i64> = BTreeMap::new();

    for node in &program.nodes {
        let line = node.span.line as usize;
        let file = node.span.file;
        match &node.item {
            Some(Item::Equ(Operand::Expr { value, .. })) => {
                if let Some(sym) = node.label.as_ref()
                    && let Ok(v) = fold_const(value, &consts, line)
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
                    .expect("ca65 stores a Kind in every native node")
                    .clone();
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
                if let Some(rest) = node.source.strip_prefix(".segment") {
                    seg = rest.trim().trim_matches('"').to_string();
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
    Ok(Parsed {
        stmts,
        label_seg,
        consts,
    })
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

fn parse_directive(
    anons: &AnonCtx,
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    directive: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let (name, rest) = split_first_word(directive);
    match name.to_ascii_lowercase().as_str() {
        "byte" | "byt" => Ok(Kind::Bytes(parse_data_list(
            anons,
            current_global,
            rest,
            line,
        )?)),
        "word" | "addr" => Ok(Kind::Words(parse_value_list(
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
        "asciiz" => Ok(Kind::Bytes(parse_asciiz(
            anons,
            current_global,
            rest,
            line,
        )?)),
        "res" => parse_res(anons, current_global, consts, rest, line),
        other => Err(AsmError::new(
            line,
            format!("unsupported directive `.{other}`"),
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
            prec: BytePrec::Tight,
            byte_prefix: true,
            caret: mos6502::Caret::BankOrXor,
            at_is_pc: false,
        },
    )
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
        assemble(src).expect("assembles")
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
}
