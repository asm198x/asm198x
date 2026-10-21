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

use super::ca65_flat::{self, DirectiveLine, FlatWalk, WalkDirective, parse_number};
use super::ca65_layout;
use super::macros::{self, MacroSyntax};
use super::mos6502::{
    self, BytePrec, assignment_split, fold_const, is_ident, split_first_word, split_top_level,
    string_literal,
};
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, DiagSeverity, Expr, Operation, Warning, WarningKind};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

mod deferred;
mod format;
mod link;
mod project;
pub(super) mod string_tokens;
pub(crate) use link::VisRule;
use link::{Emit, Resolved, area_usage, emit, link, resolve};
use project::{Sweep, collect_syms, map_expr_syms, map_kind_syms, parsed_from_program};

// ---------------------------------------------------------------------------
// The fixed NES (NROM) layout
// ---------------------------------------------------------------------------

/// What an assemble hands back: the linked image, its warnings, the debug
/// capture, and the layout's memory account (#499).
type Assembled<Capture> = Result<
    (
        Vec<u8>,
        Vec<Warning>,
        Capture,
        Vec<crate::engine::AreaUsage>,
    ),
    AsmError,
>;

// The segments the assembler places are the active layout's
// (`super::ca65_layout`, #483): resolved names, base addresses, and file
// positions. The curriculum's fixed NROM table lives on as
// `Layout::nes_default` — the same rows it always resolved to, now as a value
// a project config replaces through `-C`.

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

// ---------------------------------------------------------------------------
// Parsed statements
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum AddressSize {
    ZeroPage,
    Absolute,
}

// `Clone` so the assembler can project a statement out of the AST node that owns
// it (the assemble+link driver runs on an owned `Vec<Stmt>`; see
// `parsed_from_program`).
#[derive(Clone)]
enum Kind {
    Empty,
    /// A line the structural parse (#481) carried verbatim, to be read by the
    /// projection sweep under the text environment in force when the sweep
    /// reaches it — `decisions/parse-affecting-directives.md`. The node's
    /// `source` holds the whole raw line; nothing else is filled in.
    Unread,
    Bytes(Vec<Expr>),
    Words(Vec<Expr>),
    /// `.dbyt` — 16-bit values emitted **big-endian** (high byte first).
    DBytes(Vec<Expr>),
    /// `.dword` — 32-bit values emitted little-endian.
    DWords(Vec<Expr>),
    /// `.res count [, fill]` — `count` bytes of `fill`.
    Res(usize, Option<u8>),
    /// `.org expr` — the address a following label reads, without moving where
    /// its bytes land. `.reloc` ends it.
    Org(Expr),
    Reloc,
    /// A constant a record declaration defines: a field's offset or size,
    /// or a record's own size. It emits nothing and is carried as a
    /// statement so the projection folds it **in source order**, which is where
    /// every other constant here is folded and what makes `.if` and `.res` see
    /// the same file the reference does.
    Constant(String, i64),
    /// An enum member retains its expression until the existing Equ projection.
    EnumValue(String, Expr),
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
    Align(i64, Option<u8>),
    /// A resolved `.incbin` payload (language-surface U5): raw asset bytes at
    /// the directive's location in the active segment. Never parsed into a
    /// native node — the multi-file walk resolves the directive into a shared
    /// [`Item::Binary`](crate::ast::Item) and the projection carries it here.
    Raw(Vec<u8>),
    BitBranch {
        form: &'static isa::Form,
        zero_page: Expr,
        target: Expr,
    },
    Insn {
        operand: mos6502::OperandSyntax,
        address_size: Option<AddressSize>,
        mnemonic: String,
        extension: Option<&'static isa::InstructionSet>,
    },
}

#[derive(Clone)]
struct BoundInsn {
    form: &'static isa::Form,
    operands: Vec<Expr>,
    unsigned_word: bool,
}

struct Stmt {
    line: usize,
    /// The file `line` counts within (language-surface U5): the root for a
    /// single-file assemble, an include's `FileId` otherwise. Layout/emit
    /// errors and debug line records are stamped with it.
    file: FileId,
    expansion_frames: Vec<crate::span::ExpansionFrame>,
    seg: String,
    label: Option<String>,
    kind: Kind,
    bound_insn: Option<BoundInsn>,
}

struct Placed {
    seg: String,
    addr: u32,
    line: usize,
    file: FileId,
    expansion_frames: Vec<crate::span::ExpansionFrame>,
    item: Resolved,
    /// Default reservation/alignment fill is not initialized BSS content.
    has_initializer: bool,
}

/// Ask the shared evaluator which unresolved numeric values are actually
/// demanded after ready aliases have values. Logical branches stay lazy.
fn demanded_values(placed: &[Placed], values: &BTreeMap<String, i64>) -> BTreeSet<String> {
    let demanded = RefCell::new(BTreeSet::new());
    let lookup = |name: &str| {
        values.get(name).copied().or_else(|| {
            demanded.borrow_mut().insert(name.to_string());
            None
        })
    };
    for at in placed {
        match &at.item {
            Resolved::Bytes(exprs)
            | Resolved::Words(exprs)
            | Resolved::DBytes(exprs)
            | Resolved::DWords(exprs)
            | Resolved::Insn {
                operands: exprs, ..
            } => {
                let width = match &at.item {
                    Resolved::Bytes(_) => 1,
                    Resolved::Words(_) | Resolved::DBytes(_) => 2,
                    Resolved::DWords(_) => 4,
                    _ => 0, // Instruction operands retain their statement PC.
                };
                let mut pc = i64::from(at.addr);
                for value in exprs {
                    let _ = value.eval_with(&lookup, Some(pc), at.line);
                    pc += width;
                }
            }
            Resolved::Assert(value, ..) => {
                let _ = value.eval_with(&lookup, Some(i64::from(at.addr)), at.line);
            }
            Resolved::Visible(rule, _, names) if *rule != VisRule::MustNotBeDefined => {
                demanded.borrow_mut().extend(names.iter().cloned());
            }
            _ => {}
        }
    }
    demanded.into_inner()
}

fn statement_error(
    error: AsmError,
    file: FileId,
    frames: &[crate::span::ExpansionFrame],
) -> AsmError {
    let mut error = ca65_flat::stamp_file(error, file);
    if let Some(span) = error.span.as_mut()
        && span.expansion_frames.is_empty()
    {
        span.expansion_frames.extend_from_slice(frames);
    }
    error
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
    /// Explicit segment selections, including empty segments, in taken source.
    declared_segments: BTreeSet<String>,
    stmts: Vec<Stmt>,
    /// Each label's segment, for the zero-page-vs-absolute decision.
    label_seg: BTreeMap<String, String>,
    /// `=` constants, folded in source order.
    consts: BTreeMap<String, i64>,
    /// Authored constants and stable auxiliary names for debug output;
    /// canonical keys remain private to expression resolution.
    const_names: BTreeMap<String, String>,
    deferred: deferred::Deferred,
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

/// Assemble to bytes alone — the round-trip and unit tests' entry.
#[cfg(test)]
pub(crate) fn assemble(source: &str) -> Result<(Vec<u8>, Vec<Warning>), AsmError> {
    assemble_with_debug(source).map(|(rom, warnings, _, _)| (rom, warnings))
}

/// Assemble ca65 source and link it into a `.nes` ROM image. Single-source: a
/// `.include`/`.incbin` directive is rejected with a pointer to the multi-file
/// entry (`assemble_multi`).
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
/// Assemble + link, also returning the debug [`Capture`] read out of layout
/// (Debug198x U4). One code path: [`assemble`] delegates here, so the bytes
/// with and without capture are identical by construction (AE2).
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble_with_debug(source: &str) -> Assembled<DebugCapture> {
    let (rom, warnings, capture, areas) = assemble_program(
        &parse_program(&isa::mos6502::SET, source, macros::Expand::Yes)?,
        &super::ca65_layout::Layout::nes_default(),
        Sweep::single(&isa::mos6502::SET, source),
    )?;
    crate::cycles::check_budgets(
        &[("input", source)],
        &capture.cycle_costs(),
        crate::engine::CycleCoverage::Full,
        &areas,
    )?;
    Ok((rom, warnings, capture.into_single(), areas))
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
) -> Assembled<DebugCaptureMulti> {
    assemble_multi_with(map, loader, &super::ca65_layout::Layout::nes_default())
}

/// [`assemble_multi`] under a caller-supplied layout — the `-C` path (#483):
/// the same assembly, placed by the project's own configuration.
///
/// # Errors
/// As [`assemble_multi`], plus the layout's own resolution failures (a
/// pinned `start` behind its area's fill).
pub(crate) fn assemble_multi_with(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    layout: &super::ca65_layout::Layout,
) -> Assembled<DebugCaptureMulti> {
    let program = parse_program_multi(&isa::mos6502::SET, map, loader)?;
    let out = assemble_program(
        &program,
        layout,
        Sweep::multi(&isa::mos6502::SET, map, loader),
    )?;
    let sources: Vec<(&str, &str)> = (0..)
        .map_while(|i| {
            let id = crate::span::FileId(i);
            Some((map.path(id)?, map.contents(id)?))
        })
        .collect();
    let costs = out.2.cycle_costs();
    // Check each file separately so the diagnostic retains its actual FileId.
    for (i, source) in sources.iter().enumerate() {
        crate::cycles::check_budgets(
            &[*source],
            &costs,
            crate::engine::CycleCoverage::Full,
            &out.3,
        )
        .map_err(|e| ca65_flat::stamp_file(e, FileId(i as u32)))?;
    }
    Ok(out)
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
    layout_def: &super::ca65_layout::Layout,
    sweep: Sweep<'_>,
) -> Assembled<DebugCaptureMulti> {
    let set = &isa::mos6502::SET;
    // The AST is the single front-end IR: the parse built the source-preserving
    // `Program` (carrying each statement's native `Kind`, `=` constants, and the
    // segment directives); project it to the assembler's `Parsed`. Same
    // bytes as the old direct parse — see
    // `decisions/ast-native-payload-for-multipass-cisc.md`.
    let parsed = parsed_from_program(program, sweep)?;

    // Scoped label references, answered now that every definition is known:
    // `v` written inside `a::b` is `a::b::v` if that exists, else `a::v`, else
    // `v`. Constants were answered at parse time, positionally, which is what
    // ca65 requires of them; a label may stand anywhere, so it waits for this.
    let mut parsed = parsed;
    {
        let mut refs: BTreeSet<String> = BTreeSet::new();
        for stmt in &parsed.stmts {
            collect_syms(&stmt.kind, &mut refs);
            if let Kind::Visible { rule, names, .. } = &stmt.kind
                && *rule != VisRule::MustNotBeDefined
            {
                refs.extend(names.iter().cloned());
            }
        }
        for definition in parsed.deferred.definitions.values() {
            collect_syms(&Kind::Bytes(vec![definition.value.clone()]), &mut refs);
        }
        refs.extend(
            parsed
                .deferred
                .source_reads
                .iter()
                .map(|(name, _)| name.clone()),
        );
        let known: BTreeSet<&String> = parsed
            .label_seg
            .keys()
            .chain(parsed.consts.keys())
            .chain(parsed.deferred.definitions.keys())
            .collect();
        let scoped: BTreeSet<String> = refs
            .iter()
            .filter(|k| k.starts_with(SCOPE_MARK))
            .cloned()
            .collect();
        let targets = scope_targets(&scoped, &|name| known.contains(&name.to_string()));
        for (name, span) in &parsed.deferred.source_reads {
            let target = targets.get(name).unwrap_or(name);
            if !known.contains(target) {
                return Err(AsmError::at(
                    span.clone(),
                    format!("undefined symbol `{}`", display_label(target)),
                ));
            }
        }
        for definition in parsed.deferred.definitions.values_mut() {
            definition.value = map_expr_syms(&definition.value, &|name| {
                targets.get(name).map(|target| Expr::Sym(target.clone()))
            });
        }
        for stmt in &mut parsed.stmts {
            stmt.kind = map_kind_syms(&stmt.kind, &|name| {
                targets.get(name).map(|t| Expr::Sym(t.clone()))
            });
            if let Kind::Visible { rule, names, .. } = &mut stmt.kind
                && *rule != VisRule::MustNotBeDefined
            {
                for name in names {
                    if let Some(target) = targets.get(name) {
                        *name = target.clone();
                    }
                }
            }
            if let Some(bound) = &mut stmt.bound_insn {
                for operand in &mut bound.operands {
                    *operand = map_expr_syms(operand, &|name| {
                        targets.get(name).map(|t| Expr::Sym(t.clone()))
                    });
                }
            }
            // `.sizeof` reads a size the declaration stored. A name that
            // declared none has no size to give, which is ca65's `Size of 'V'
            // is unknown` — answered here, where the message can still name
            // what the source wrote rather than the key it became.
            let mut named = BTreeSet::new();
            collect_syms(&stmt.kind, &mut named);
            for key in &named {
                let Some(name) = key.strip_suffix(SIZE_SUFFIX) else {
                    continue;
                };
                if !parsed.consts.contains_key(key) {
                    return Err(statement_error(
                        AsmError::new(
                            stmt.line,
                            format!("size of `{}` is unknown", display_label(name)),
                        ),
                        stmt.file,
                        &stmt.expansion_frames,
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

    // Where a segment floats behind another in its area (a project config's
    // RODATA after CODE), its base is a function of the earlier segments'
    // lengths — so size everything first, with the same `resolve` the layout
    // pass uses, and feed the lengths to the layout's resolution. No
    // statement's size depends on its absolute address (`.align` measures
    // from the segment's start), so this pass cannot disagree with the real
    // one. The default layout floats nothing and skips this entirely.
    let seg_lengths = if layout_def.from_config || layout_def.needs_lengths() {
        // Config resolution also needs presence: an explicitly selected empty
        // segment exists for ld65, while an absent declaration reserves nothing.
        let mut lengths: BTreeMap<String, u32> = parsed
            .declared_segments
            .iter()
            .map(|name| (name.clone(), 0))
            .collect();
        for stmt in &parsed.stmts {
            let off = *lengths.entry(stmt.seg.clone()).or_insert(0);
            let (_, size) = resolve(
                set,
                stmt.kind.clone(),
                &size_env,
                off,
                stmt.line,
                layout_def.fill,
                stmt.bound_insn.clone(),
            )
            .map_err(|e| statement_error(e, stmt.file, &stmt.expansion_frames))?;
            *lengths.get_mut(&stmt.seg).expect("segment length") += size as u32;
        }
        lengths
    } else {
        BTreeMap::new()
    };
    let layout = &layout_def.resolve(&seg_lengths)?;

    // Layout pass: resolve each instruction's mode and size, lay statements out
    // within their segment, and record every label's absolute address.
    let mut offsets: BTreeMap<String, u32> = BTreeMap::new();
    // Absolute addresses are `i64` to match the engine's expression evaluator;
    // the NES is 16-bit, so values are masked to a word on emit.
    let mut addr_env: BTreeMap<String, i64> = BTreeMap::new();
    for (name, value) in &parsed.consts {
        addr_env.insert(name.clone(), *value);
    }
    // Native default absolute mode advances one cursor across segment changes.
    // Physical segment offsets still decide where the bytes are placed.
    let mut absolute_pc: Option<i64> = None;
    let mut placed: Vec<Placed> = Vec::new();
    // The debug read-out (U4): symbols and line spans fall out of the layout
    // values already in hand — `(section, offset)` is `(seg, addr - base)`.
    let mut dbg_symbols: Vec<debug198x::Symbol> = Vec::new();
    let mut dbg_lines: Vec<(FileId, u32, debug198x::SectionId, u64, u64)> = Vec::new();
    let mut dbg_cycles = Vec::new();
    let mut dbg_spans = DebugSpans::default();
    for (name, value) in &parsed.consts {
        dbg_symbols.push(debug198x::Symbol {
            name: parsed.const_names.get(name).unwrap_or(name).clone(),
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

    let mut definition_positions = BTreeMap::new();
    let needed_positions: BTreeSet<usize> = parsed
        .deferred
        .definitions
        .values()
        .map(|definition| definition.position)
        .collect();
    for (position, stmt) in parsed.stmts.into_iter().enumerate() {
        let info = layout.seg(&stmt.seg).ok_or_else(|| {
            // Layout errors are stamped with the statement's file (U5), so a
            // failure inside an included file names that file.
            statement_error(
                AsmError::new(
                    stmt.line,
                    if layout.from_config {
                        format!(
                            "segment `{}` is not in the linker config (valid: {}); like \
                             `ld65`, a segment outside SEGMENTS has no memory area to \
                             place it in",
                            stmt.seg,
                            layout.known()
                        )
                    } else {
                        format!(
                            "segment `{}` is not in the NES config (valid: {}); this assembler \
                             links the curriculum's fixed NROM layout, which — like `ld65` with \
                             its `nes.cfg` — has no memory area for other segments",
                            stmt.seg,
                            layout.known()
                        )
                    },
                ),
                stmt.file,
                &stmt.expansion_frames,
            )
        })?;
        let off = *offsets.entry(stmt.seg.clone()).or_insert(0);
        let addr = info.base + off;
        let prior_pc = absolute_pc.unwrap_or(i64::from(addr));
        if needed_positions.contains(&position) {
            definition_positions.insert(position, prior_pc);
        }
        if let Some(label) = &stmt.label {
            // Real ca65 rejects a duplicate symbol; accepting one would also
            // make the debug record lie (the record keeps every definition,
            // the encoder the last — a debugger would disagree with the bytes).
            // `addr_env` was seeded with the `=` constants, so this covers a
            // label colliding with a constant too.
            if parsed.deferred.definitions.contains_key(label) {
                return Err(statement_error(
                    AsmError::new(
                        stmt.line,
                        format!("duplicate symbol `{}`", display_label(label)),
                    ),
                    stmt.file,
                    &stmt.expansion_frames,
                ));
            }
            if imported.contains(label) {
                return Err(statement_error(
                    AsmError::new(
                        stmt.line,
                        format!("symbol `{}` is already an import", display_label(label)),
                    ),
                    stmt.file,
                    &stmt.expansion_frames,
                ));
            }
            if addr_env.insert(label.clone(), prior_pc).is_some() {
                return Err(statement_error(
                    AsmError::new(
                        stmt.line,
                        format!("duplicate symbol `{}`", display_label(label)),
                    ),
                    stmt.file,
                    &stmt.expansion_frames,
                ));
            }
            // Anonymous (`:`) labels are positional, not names — a debugger
            // cannot look one up, so they stay out of the symbol record. Cheap
            // (`@name`) labels are qualified with a control byte internally;
            // render the source form.
            if !label.starts_with(LABEL_SEP) {
                let (section, offset) = dbg_spans.location(
                    layout.seg_id(&stmt.seg),
                    layout.segs.len(),
                    info,
                    off,
                    prior_pc,
                );
                dbg_symbols.push(debug198x::Symbol {
                    name: display_label(label),
                    kind: debug198x::SymbolKind::Label {
                        section,
                        offset,
                        space: None,
                    },
                });
            }
        }
        // Native defines the label before the directive changes the PC/mode.
        match &stmt.kind {
            Kind::Org(e) => {
                let target = e
                    .eval(&addr_env, prior_pc, stmt.line)
                    .map_err(|e| statement_error(e, stmt.file, &stmt.expansion_frames))?;
                // Native DoOrg validates the 24-bit origin before changing mode.
                if !(0..=0xFF_FFFF).contains(&target) {
                    return Err(statement_error(
                        AsmError::new(stmt.line, "origin is outside the 24-bit range"),
                        stmt.file,
                        &stmt.expansion_frames,
                    ));
                }
                absolute_pc = Some(target);
            }
            Kind::Reloc => {
                absolute_pc = None;
            }
            _ => {}
        }
        // A constant-only bit-branch target has no segment base to cancel
        // ca65's relocatable PC. Native ca65 refuses it before linking; `.org`
        // makes the PC absolute even when it matches the linked address.
        if absolute_pc.is_none()
            && let Kind::BitBranch { target, .. } = &stmt.kind
            && fold_const(target, &parsed.consts, stmt.line).is_ok()
        {
            return Err(statement_error(
                AsmError::new(
                    stmt.line,
                    "constant bit-branch target requires absolute mode (`.org`)",
                ),
                stmt.file,
                &stmt.expansion_frames,
            ));
        }
        let has_initializer = !matches!(stmt.kind, Kind::Res(_, None) | Kind::Align(_, None));
        let (resolved, size) = resolve(
            set,
            stmt.kind,
            &size_env,
            off,
            stmt.line,
            layout_def.fill,
            stmt.bound_insn,
        )
        .map_err(|e| statement_error(e, stmt.file, &stmt.expansion_frames))?;
        *offsets.get_mut(&stmt.seg).expect("segment offset") += size as u32;
        // A line span per byte-emitting statement (address-space-only
        // reservations — ZEROPAGE/BSS `.res` — carry no bytes, so no span; the
        // HEADER segment is iNES file metadata, not CPU-addressed code, so its
        // records would alias CPU $0000 — skipped, per AE3's no-fabrication rule).
        if size > 0 && info.file_at.is_some() && stmt.seg != "HEADER" {
            let (section, offset) = dbg_spans.location(
                layout.seg_id(&stmt.seg),
                layout.segs.len(),
                info,
                off,
                absolute_pc.unwrap_or(i64::from(addr)),
            );
            dbg_lines.push((stmt.file, stmt.line as u32, section, offset, size as u64));
            if let Resolved::Insn { form, .. } = &resolved {
                dbg_cycles.push((
                    section,
                    crate::engine::CycleRec {
                        file: stmt.file,
                        line: stmt.line as u32,
                        offset,
                        base: form.cycles.base,
                        page_cross: form.cycles.page_cross,
                        branch_taken: form.cycles.branch_taken,
                        bounds: None,
                    },
                ));
            }
        }
        if !matches!(resolved, Resolved::Nothing) {
            // The address the statement *reports* — what `*` reads and what a
            // relative branch measures from. Where its bytes go is the
            // segment buffer's business, and that has not moved.
            let reported = u32::try_from(absolute_pc.unwrap_or(i64::from(addr))).map_err(|_| {
                statement_error(
                    AsmError::new(
                        stmt.line,
                        "`.org` puts this statement outside the address space",
                    ),
                    stmt.file,
                    &stmt.expansion_frames,
                )
            })?;
            placed.push(Placed {
                seg: stmt.seg,
                addr: reported,
                line: stmt.line,
                file: stmt.file,
                expansion_frames: stmt.expansion_frames,
                has_initializer,
                item: resolved,
            });
        }
        if let Some(pc) = &mut absolute_pc {
            *pc += size as i64;
        }
    }

    for (name, definition) in &parsed.deferred.definitions {
        if imported.contains(name) {
            return Err(AsmError::at(
                definition.span.clone(),
                format!("symbol `{}` is already an import", display_label(name)),
            ));
        }
    }
    parsed
        .deferred
        .finish(&mut addr_env, &definition_positions, &|values| {
            demanded_values(&placed, values)
        })?;
    for name in parsed.deferred.definitions.keys() {
        if !parsed.consts.contains_key(name)
            && let Some(value) = addr_env.get(name)
        {
            dbg_symbols.push(debug198x::Symbol {
                name: parsed.const_names.get(name).unwrap_or(name).clone(),
                kind: debug198x::SymbolKind::Const {
                    value: *value as u64,
                },
            });
        }
    }

    let mut actual_lengths = offsets.clone();
    for name in &parsed.declared_segments {
        actual_lengths.entry(name.clone()).or_insert(0);
    }
    layout_def.validate_extents(layout, &actual_lengths)?;

    // The section table: every segment the program touched (placed bytes or
    // just labels/reservations), in config order. CPU-addressed segments carry
    // their absolute base; HEADER (file metadata) and CHARS (PPU address space)
    // are *not* CPU-addressable, so they get `base: None` — the reader's
    // absolute lookups skip them rather than aliasing them onto the zero page,
    // and a PPU-space consumer can supply a `BaseMap` (KTD7). A `Space`
    // qualifier is the eventual richer answer (KTD5, U7).
    let mut sections: Vec<debug198x::Section> = layout
        .segs
        .iter()
        .enumerate()
        .filter(|(_, s)| offsets.contains_key(&s.name))
        .map(|(id, s)| debug198x::Section {
            id: id as debug198x::SectionId,
            name: s.name.clone(),
            base: s.cpu_addressable.then_some(u64::from(s.base)),
            // The NES mapper's banking is not modelled here; no space is fabricated.
            space: None,
        })
        .collect();

    // Emit pass: turn each placed item into bytes, per segment. Statements run
    // in source order across segments, so `.out` notes come out in the order
    // they were written.
    let mut seg_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let uninitialized: BTreeSet<&str> = layout_def
        .segments
        .iter()
        .filter(|s| !s.initialized)
        .map(|s| s.name.as_str())
        .collect();
    let mut warned_segments = BTreeSet::new();
    let mut exports: BTreeMap<String, String> = BTreeMap::new();
    for Placed {
        seg,
        addr,
        line,
        file,
        expansion_frames,
        item,
        has_initializer,
    } in placed
    {
        if let Resolved::Visible(rule, _, names) = &item
            && *rule != VisRule::MustNotBeDefined
        {
            for name in names.iter().filter(|name| addr_env.contains_key(*name)) {
                let external = name.rsplit("::").next().unwrap_or(name).to_string();
                if let Some(previous) = exports.insert(external.clone(), name.clone())
                    && previous != *name
                {
                    return Err(statement_error(
                        AsmError::new(line, format!("duplicate external identifier `{external}`")),
                        file,
                        &expansion_frames,
                    ));
                }
            }
        }
        // A diagnostic carries no bytes, so it is answered before the segment
        // filter: an `.out` written inside BSS still prints.
        let diagnostic = matches!(item, Resolved::Message(..) | Resolved::Assert(..));
        if !diagnostic && layout.seg(&seg).expect("seg").file_at.is_none() {
            // ld65 ignores reservations and all-zero initialized content.
            // Validate omitted expressions before inspecting their actual bytes.
            let check_content = has_initializer && uninitialized.contains(seg.as_str());
            let nonzero = match &item {
                Resolved::Nothing => false,
                Resolved::Raw(bytes) => check_content && bytes.iter().any(|&b| b != 0),
                Resolved::Fill(count, fill) => check_content && *count != 0 && *fill != 0,
                _ => {
                    let mut bytes = Vec::new();
                    emit(
                        item,
                        addr,
                        &Emit {
                            env: &addr_env,
                            label_seg: &parsed.label_seg,
                            file,
                            line,
                        },
                        &mut bytes,
                        &mut warnings,
                    )
                    .map_err(|e| statement_error(e, file, &expansion_frames))?;
                    check_content && bytes.iter().any(|&b| b != 0)
                }
            };
            if nonzero && warned_segments.insert(seg.clone()) {
                warnings.push(Warning {
                    file,
                    line,
                    message: format!("segment `{seg}` with type bss contains initialized data"),
                    kind: WarningKind::Advisory,
                });
            }
            continue;
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
        .map_err(|e| statement_error(e, file, &expansion_frames))?;
    }

    let mut file_offsets: BTreeMap<_, _> = layout
        .segs
        .iter()
        .enumerate()
        .filter_map(|(id, s)| Some((id as debug198x::SectionId, s.file_at? as u64)))
        .collect();
    for (section, file_offset, _, _) in dbg_spans.spans {
        if let Some(at) = file_offset {
            file_offsets.insert(section.id, at);
        }
        sections.push(section);
    }
    let rom = link(&seg_bytes, layout)?;
    let areas = area_usage(layout_def, layout, &offsets);
    Ok((
        rom,
        warnings,
        DebugCaptureMulti {
            cycle_coverage: crate::engine::CycleCoverage::Full,
            file_offsets,
            sections,
            symbols: dbg_symbols,
            lines: dbg_lines,
            cycles: dbg_cycles,
        },
        areas,
    ))
}

/// Logical debug coordinates retain a separate physical mapping for each span.
#[derive(Default)]
struct DebugSpans {
    spans: Vec<(debug198x::Section, Option<u64>, u32, i64)>,
    last: BTreeMap<debug198x::SectionId, usize>,
}

impl DebugSpans {
    fn location(
        &mut self,
        parent: debug198x::SectionId,
        configured_count: usize,
        info: &ca65_layout::ResolvedSeg,
        physical: u32,
        logical: i64,
    ) -> (debug198x::SectionId, u64) {
        if logical == i64::from(info.base) + i64::from(physical) {
            return (parent, u64::from(physical));
        }
        if let Some(&index) = self.last.get(&parent) {
            let (section, _, start, base) = &self.spans[index];
            if let Some(delta) = physical.checked_sub(*start)
                && base.checked_add(i64::from(delta)) == Some(logical)
            {
                return (section.id, u64::from(delta));
            }
        }
        let id = debug198x::SectionId::try_from(configured_count + self.spans.len())
            .expect("debug section count fits section ID");
        let section = debug198x::Section {
            id,
            name: info.name.clone(),
            base: info
                .cpu_addressable
                .then(|| u64::try_from(logical).expect("validated PC")),
            space: None,
        };
        let file_offset = info
            .file_at
            .and_then(|at| (at as u64).checked_add(u64::from(physical)));
        self.last.insert(parent, self.spans.len());
        self.spans.push((section, file_offset, physical, logical));
        (id, 0)
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
    if matches!(mode, macros::Expand::No) {
        return Ok(format::parse(source));
    }
    // Assembly keeps definitions and calls unread for the selected sweep.
    let mut w = Walker::structural_source(set, source);
    ca65_flat::walk_source_expanded(&mut w, source, FileId(0))?;
    w.finish(source.lines().count() as u32)
}

/// Parse a multi-file NES ca65 program (language-surface U5, KTD1): the
/// **interleaved, environment-threaded walk** over the source map, resolving
/// `.include`/`.incbin` lazily through `loader` under ca65's probe-pinned
/// semantics ([`CA65_SEMANTICS`](ca65_flat::CA65_SEMANTICS) — ancestor-chain
/// resolution, the negative-size incbin sentinel; re-confirmed under the NES
/// link). Everything the parse accumulates crosses include boundaries in both
/// directions, exactly as ca65's textual splice does (probe-pinned):
/// `=` constants, the cheap-local anchor (the scoped environment's), the
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
    // Structure only (#481): includes are not resolved here — the projection
    // sweep opens each one at the moment it reads the directive, so a target
    // inside an untaken branch is never opened at all.
    let mut w = Walker::structural(set);
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
    strings: string_tokens::StringTokens,
    set: &'static isa::InstructionSet,
    extension: Option<&'static isa::InstructionSet>,
    cpu_stack: Vec<Option<&'static isa::InstructionSet>>,
    repeat_registers: Option<bool>,
    text_names: BTreeSet<String>,
    anons: AnonCtx,
    /// The scoped symbol environment (#484): the cheap-local anchor — the
    /// current global that `@cheap` labels key under, re-anchored by native
    /// definition events without restoration at scope exit — lives in the
    /// shared environment (`decisions/scoped-symbol-environment.md`); the
    /// `::`-path scope stack below is staged to follow.
    scope_env: crate::scopes::ScopeEnv,
    consts: BTreeMap<String, i64>,
    source_offsets: BTreeMap<String, u32>,
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
    /// Source-time symbol events. Unknown expression references allocate in
    /// the current scope and mask outer names for subsequent name queries.
    source_known: BTreeSet<String>,
    source_unresolved: RefCell<BTreeSet<String>>,
    source_referenced: RefCell<BTreeSet<String>>,
    /// Counts unnamed scopes, so each gets a path segment of its own.
    unnamed: usize,
    /// Set by `.end`, which stops the assembler reading: every line after it
    /// is ignored, including one that would not parse.
    ended: bool,
    /// The record definitions open here, outermost first. A `.struct` may hold
    /// another, and the inner one allocates its size in the outer.
    records: Vec<RecordBuild>,
    /// ca65 `.define` text symbols. Unlike `.macro`, these are deliberately
    /// consumed line-by-line so their visibility follows textual includes in
    /// both directions and remains positional around `.undefine`.
    text_defines: BTreeMap<String, String>,
    /// False only for formatter parsing, which must preserve source-only
    /// directives rather than consume or substitute them.
    expand_text_defines: bool,
    /// Structure-only mode (#481): the assembly parse groups blocks and
    /// resolves nothing — every line becomes an *unread* verbatim node, and
    /// the projection sweep reads it under the environment in force at that
    /// point. `decisions/parse-affecting-directives.md`.
    structural: bool,
    /// Complete macro ranges are opaque to the structural block cursor.
    macro_lines: RefCell<BTreeSet<usize>>,
    in_macro: Cell<bool>,
}

/// A `.struct`, `.union` or `.enum` being read.
struct RecordBuild {
    kind: RecordKind,
    /// The scope its names are defined in — the path opened by the header.
    path: String,
    /// A struct's next offset, or an enum's offset from its base. A union lays every field
    /// at zero, so this stays there and the size is the widest member.
    cursor: i64,
    /// Explicit enum base; the cursor is its relative offset, not its value.
    enum_base: Option<Expr>,
    /// Anonymous enums declare in the enclosing scope.
    owns_scope: bool,
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

/// Linked ca65 permits a source label before a macro definition. Keep the
/// shared flat macro grammar unchanged: the selected reader owns its binding.
fn macro_header(source: &str) -> Option<(String, Vec<String>)> {
    let head = split_label_head(source.trim())
        .or_else(|| split_generated_label_head(source.trim()))
        .map_or(source, |(_, rest)| rest);
    ca65_flat::Ca65Macros.header(head)
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
    /// A positive physical span causes a native size definition on closure.
    has_output: bool,
}

impl Walker {
    fn new(set: &'static isa::InstructionSet, expand_text_defines: bool) -> Self {
        Self {
            set,
            strings: string_tokens::StringTokens::default(),
            extension: None,
            cpu_stack: Vec::new(),
            repeat_registers: None,
            text_names: BTreeSet::new(),
            anons: AnonCtx::default(),
            scope_env: crate::scopes::ScopeEnv::new(),
            consts: BTreeMap::new(),
            source_offsets: BTreeMap::new(),
            pending_leading: Vec::new(),
            nodes: Vec::new(),
            scopes: Vec::new(),
            opened: BTreeSet::new(),
            source_known: BTreeSet::new(),
            source_unresolved: RefCell::new(BTreeSet::new()),
            source_referenced: RefCell::new(BTreeSet::new()),
            unnamed: 0,
            ended: false,
            records: Vec::new(),
            text_defines: BTreeMap::new(),
            expand_text_defines,
            structural: false,
            macro_lines: RefCell::new(BTreeSet::new()),
            in_macro: Cell::new(false),
        }
    }

    /// The structure-only walker (#481): blocks are grouped, every line is
    /// kept unread. The projection sweep owns the eager twin that reads them.
    fn structural(set: &'static isa::InstructionSet) -> Self {
        Self {
            structural: true,
            ..Self::new(set, true)
        }
    }

    /// Close the walk: flush trailing comments (a trailing block or a
    /// comment-only file), then fail any forward anonymous reference whose
    /// definition never arrived.
    /// CPU selection belongs to the live reader, including across includes
    /// and selected branches. Each instruction retains its selection (#722).
    fn structural_source(set: &'static isa::InstructionSet, source: &str) -> Self {
        let reader = Self::structural(set);
        reader.prepare_macro_lines(source);
        reader
    }

    fn prepare_macro_lines(&self, source: &str) {
        let lines: Vec<_> = source.lines().collect();
        let mut opaque = self.macro_lines.borrow_mut();
        opaque.clear();
        let syntax = ca65_flat::Ca65Macros;
        let mut index = 0;
        while index < lines.len() {
            if macro_header(lines[index]).is_some()
                && let Some(end) = (index + 1..lines.len()).find(|&i| syntax.is_end(lines[i]))
            {
                opaque.extend(index + 1..=end + 1);
                index = end + 1;
            } else {
                index += 1;
            }
        }
    }

    fn select_cpu(&mut self, rest: &str, line: usize) -> Result<bool, AsmError> {
        let (word, args) = split_first_word(rest);
        let word = word.to_ascii_lowercase();
        let selected = match word.as_str() {
            ".setcpu" => string_literal(args.trim()).map(|name| name.to_ascii_lowercase()),
            ".p02" => Some("6502".to_string()),
            ".p4510" => Some("4510".to_string()),
            ".pushcpu" | ".popcpu" => {
                if !args.trim().is_empty() {
                    return Err(AsmError::new(
                        line,
                        "unexpected arguments after processor stack directive",
                    ));
                }
                if word == ".pushcpu" {
                    self.cpu_stack.push(self.extension);
                } else {
                    self.extension = self
                        .cpu_stack
                        .pop()
                        .ok_or_else(|| AsmError::new(line, "CPU stack is empty"))?;
                }
                return Ok(true);
            }
            _ => None,
        };
        match selected.as_deref() {
            Some("6502") => self.extension = None,
            Some("4510") => self.extension = Some(&isa::csg65ce02::CSG4502_SET),
            _ => return Ok(false), // Existing validation diagnoses other profiles.
        }
        Ok(true)
    }

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
                    if open.has_output {
                        self.scope_env.replace_anchor(format!(
                            "scope-size{LABEL_SEP}{}",
                            qualify(&self.path(), &open.segment)
                        ));
                        if let Some(parent) = self.scopes.last_mut() {
                            parent.has_output = true;
                        }
                    }
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
            // Native ca65 treats an unnamed .proc as deprecated .scope,
            // including its closer: .endscope, rather than .endproc.
            ".proc" => !rest.is_empty(),
            ".scope" => false,
            _ => return Ok(None),
        };
        // Both directives can open an unnamed scope. Its unique synthetic
        // segment cannot be named from source, and each expansion gets a fresh
        // identity. ca65 deprecates unnamed `.proc` but accepts unnamed `.scope`.
        let name = if rest.is_empty() {
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
        if let Some(label) = &label {
            self.scope_env.replace_anchor(label.clone());
        }
        self.scopes.push(OpenScope {
            segment: name,
            is_proc,
            line,
            has_output: false,
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
        self.scopes.push(OpenScope {
            segment: name.to_string(),
            is_proc: false,
            line,
            has_output: false,
        });
        Ok(path)
    }

    /// The size a record declares, which `.sizeof` reads back. Stored as an
    /// ordinary constant under a key source cannot spell, so it resolves
    /// through the same scope lookup every other name uses.
    fn record_size_key(path: &str) -> String {
        format!("{path}{SIZE_SUFFIX}")
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
            if done.owns_scope {
                self.scopes.pop();
            }
            // Enum values have no native size property. Keep their last member
            // as the cheap anchor, without inventing a physical size anchor.
            if done.kind == RecordKind::Enum {
                return Ok(Some(Vec::new()));
            }
            let key = Self::record_size_key(&done.path);
            self.consts.insert(key.clone(), done.size());
            self.scope_env
                .replace_anchor(format!("scope-size{LABEL_SEP}{}", done.path));
            // A nested record allocates its size in the enclosing record.
            self.allocate(done.size());
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
            if self.expand_text_defines
                && self
                    .records
                    .last()
                    .is_some_and(|open| kind == RecordKind::Enum || open.kind == RecordKind::Enum)
            {
                return Err(AsmError::new(
                    line,
                    "an enum cannot nest a record or be nested in one",
                ));
            }
            let owns_scope = kind != RecordKind::Enum || !rest.trim().is_empty();
            let name = rest.trim();
            let name = if name.is_empty() {
                self.unnamed += 1;
                format!("{LABEL_SEP}rec#{}", self.unnamed)
            } else {
                if !is_ident(name) {
                    if kind == RecordKind::Enum && !self.expand_text_defines {
                        // A repeat or macro may produce the name through
                        // .ident. Formatting retains the unexpanded header.
                        self.unnamed += 1;
                        format!("{LABEL_SEP}rec#{}", self.unnamed)
                    } else {
                        return Err(AsmError::new(line, format!("invalid record name `{name}`")));
                    }
                } else {
                    name.to_string()
                }
            };
            let path = if owns_scope {
                if kind == RecordKind::Enum && self.expand_text_defines {
                    self.constant_key_at(&name, line, false)?;
                }
                self.push_scope(&name, line)?
            } else {
                self.path().join("::")
            };
            self.records.push(RecordBuild {
                kind,
                path,
                cursor: 0,
                enum_base: (kind == RecordKind::Enum).then_some(Expr::Num(0)),
                owns_scope,
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
            let expr = parse_value(&self.anons, self.scope_env.anchor_str(), text, line)?;
            SourceConstants(self).fold(&expr, line)
        };
        if self.expand_text_defines && allocator.eq_ignore_ascii_case(".res") && args.is_empty() {
            return Err(AsmError::new(line, "`.res` needs a field size"));
        }
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
        if self.expand_text_defines && count <= 0 {
            return Err(AsmError::new(line, "record field count must be positive"));
        }
        // Validate before defining symbols or advancing the record. A zero-size
        // tag is valid with a positive count, while a field may occupy at most
        // 24 bits' worth of bytes regardless of its allocator's unit size.
        let size = unit
            .checked_mul(count)
            .ok_or_else(|| AsmError::new(line, "record field allocation overflows"))?;
        if self.expand_text_defines && !(0..=0xFF_FFFF).contains(&size) {
            return Err(AsmError::new(
                line,
                "record field allocation exceeds 24 bits",
            ));
        }
        let open = self.records.last().expect("a record is open");
        let offset = match open.kind {
            RecordKind::Union => 0,
            _ => open.cursor,
        };
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
            self.scope_env.replace_anchor(key.clone());
            defined.push(Kind::Constant(size_key, size));
            defined.push(Kind::Constant(key, offset));
        }
        self.allocate(size);
        Ok(defined)
    }

    /// An explicit initializer resets the base; defaults clone that base plus
    /// an offset. Query results and lexical references are captured only once.
    fn enum_member(&mut self, text: &str, line: usize) -> Result<Vec<Kind>, AsmError> {
        // The authored member line is already retained by record_line.
        // Formatting does not execute repeat counters or macro expressions.
        if !self.expand_text_defines {
            return Ok(Vec::new());
        }
        let (name, initializer) = text.split_once('=').map_or((text, None), |(name, value)| {
            (name.trim(), Some(value.trim()))
        });
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid enum member `{name}`")));
        }
        let key = self.constant_key_at(name, line, false)?;
        if self.expand_text_defines {
            if self.source_known.contains(&key) || self.consts.contains_key(&key) {
                return Err(AsmError::new(
                    line,
                    format!("symbol `{name}` is already defined"),
                ));
            }
            self.source_unresolved.borrow_mut().insert(key.clone());
        }
        let explicit = initializer
            .map(|value| self.source_value(value, line))
            .transpose()?;
        let open = self.records.last_mut().expect("an enum is open");
        let value = if let Some(value) = explicit {
            open.enum_base = Some(value.clone());
            open.cursor = 0;
            value
        } else {
            Expr::Bin(
                crate::engine::BinOp::Add,
                Box::new(open.enum_base.as_ref().expect("enum base").clone()),
                Box::new(Expr::Num(open.cursor)),
            )
        };
        open.cursor = open
            .cursor
            .checked_add(1)
            .ok_or_else(|| AsmError::new(line, "too many enum members"))?;
        if self.expand_text_defines {
            self.source_known.insert(key.clone());
        }
        self.scope_env.replace_anchor(key.clone());
        Ok(vec![Kind::EnumValue(key, value)])
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
        Ok(Some(Kind::Res(bytes, None)))
    }

    /// Bind a known current-scope constant immediately. An unresolved name
    /// remains in its lexical scope until final outward binding, even when an
    /// outer constant currently has the same spelling.
    fn scoped_name(&self, name: &str) -> String {
        let path = self.path();
        let current = qualify(&path, name);
        if self.consts.contains_key(&current) || self.source_known.contains(&current) {
            current
        } else {
            scope_ref(&path, name)
        }
    }

    /// Size properties consult enclosing declarations, rather than allocating
    /// an ordinary value symbol in the current scope.
    fn scoped_property_name(&self, name: &str) -> String {
        let mut path = self.path();
        loop {
            let key = qualify(&path, name);
            if self.consts.contains_key(&key) {
                return key;
            }
            if path.pop().is_none() {
                break;
            }
        }
        scope_ref(&self.path(), name)
    }

    /// Look up a name without allocating a value reference. A current
    /// unresolved placeholder masks an already-defined outer symbol.
    fn query_key(&self, name: &str, line: usize) -> Result<String, AsmError> {
        if let Some(key) = name.strip_prefix(CHEAP_REF_MARK) {
            return Ok(key.to_string());
        }
        if let Some(local) = name.strip_prefix('@').filter(|name| is_cheap_ident(name)) {
            return checked_cheap_key(self.scope_env.anchor_str(), local, line);
        }
        if let Some(global) = name.strip_prefix("::") {
            return Ok(global.to_string());
        }
        if name.contains("::") {
            return self.resolve_qualified(name, line);
        }
        if name.contains(LABEL_SEP) {
            return Ok(name.to_string());
        }
        let mut path = self.path();
        loop {
            let key = qualify(&path, name);
            if self.consts.contains_key(&key)
                || self.source_known.contains(&key)
                || self.source_unresolved.borrow().contains(&key)
            {
                return Ok(key);
            }
            if path.pop().is_none() {
                return Ok(name.to_string());
            }
        }
    }

    fn name_defined(&self, name: &str, line: usize) -> Result<bool, AsmError> {
        let key = self.query_key(name, line)?;
        Ok(self.consts.contains_key(&key) || self.source_known.contains(&key))
    }

    fn name_referenced(&self, name: &str, line: usize) -> Result<bool, AsmError> {
        let key = self.query_key(name, line)?;
        Ok(self.source_referenced.borrow().contains(&key))
    }

    fn record_reference(&self, key: &str) {
        if !self.expand_text_defines {
            return;
        }
        let key = split_scope_ref(key).map_or_else(
            || key.to_string(),
            |(path, name)| {
                if path.is_empty() {
                    name.to_string()
                } else {
                    format!("{path}::{name}")
                }
            },
        );
        if !self.consts.contains_key(&key) && !self.source_known.contains(&key) {
            self.source_unresolved.borrow_mut().insert(key.clone());
        }
        self.source_referenced.borrow_mut().insert(key);
    }

    fn source_value(&self, text: &str, line: usize) -> Result<Expr, AsmError> {
        let value = parse_value(&self.anons, self.scope_env.anchor_str(), text, line)?;
        match self.scope_syms(&Kind::Bytes(vec![value]), line)? {
            Kind::Bytes(mut values) => Ok(values.pop().expect("one source expression")),
            _ => unreachable!("source expression remains bytes"),
        }
    }

    /// `sa::v` — the scope named must already be open or closed above this
    /// line, searched outward from here, which is ca65's `No such scope`.
    fn resolve_qualified(&self, text: &str, line: usize) -> Result<String, AsmError> {
        // Formatting preserves includes rather than opening them. A scope
        // may be declared by one, so existence is an assembly-time question.
        if !self.expand_text_defines {
            return Ok(text.to_string());
        }
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

    /// A definition binds in its named scope, which must already exist. Keep
    /// this separate from reference lookup: definitions never search outward
    /// for an unqualified name, and a qualified target's entire parent must
    /// exist (finding just its first component is insufficient).
    fn constant_key(&self, name: &str, line: usize) -> Result<String, AsmError> {
        self.constant_key_at(name, line, true)
    }

    /// Mnemonics are instruction heads in ordinary assignments, but native
    /// enum headers/members read them as identifiers. Register tokens stay reserved.
    fn constant_key_at(
        &self,
        name: &str,
        line: usize,
        statement_head: bool,
    ) -> Result<String, AsmError> {
        if let Some(local) = name.strip_prefix('@') {
            if !is_cheap_ident(local) {
                return Err(AsmError::new(
                    line,
                    format!("invalid cheap-local constant `{name}`"),
                ));
            }
            return if self.expand_text_defines {
                checked_cheap_key(self.scope_env.anchor_str(), local, line)
            } else {
                Ok(cheap_key(self.scope_env.anchor_str(), local))
            };
        }
        let plain = name.strip_prefix("::").unwrap_or(name);
        // The shared MOS helper permits dots for other dialects' local
        // names. ca65 tokenises a dot as a control-command prefix instead;
        // no component of an immutable definition may contain one.
        if !plain
            .split("::")
            .all(|part| is_ident(part) && !part.contains('.'))
        {
            return Err(AsmError::new(
                line,
                format!("invalid constant name `{name}`"),
            ));
        }
        let extended = self.extension.is_some();
        let register = plain.split("::").any(|part| {
            let upper = part.to_ascii_uppercase();
            matches!(upper.as_str(), "A" | "X" | "Y")
                || (extended && matches!(upper.as_str(), "Z" | "SP"))
        });
        let head = plain.split("::").next().unwrap_or("").to_ascii_uppercase();
        // ca65 checks an instruction at the statement head, before reading
        // a scoped definition. A leading :: bypasses that check. Register
        // tokens remain reserved in every component.
        let instruction = statement_head && !name.starts_with("::") && self.instruction_name(&head);
        // Formatting also reads inactive definitions. Reserved tokens are
        // a selected assembly check, so preserve those authored names there.
        if self.expand_text_defines && (register || instruction) {
            return Err(AsmError::new(
                line,
                format!("reserved constant name `{name}`"),
            ));
        }
        let qualified = if name.starts_with("::") {
            plain.to_string()
        } else if name.contains("::") {
            self.resolve_qualified(name, line)?
        } else {
            qualify(&self.path(), name)
        };
        if self.expand_text_defines
            && let Some((parent, _)) = qualified.rsplit_once("::")
            && !self.opened.contains(parent)
        {
            return Err(AsmError::new(line, format!("no such scope `{parent}`")));
        }
        Ok(qualified)
    }

    fn instruction_name(&self, head: &str) -> bool {
        let extended = self.extension.is_some();
        if extended && matches!(head, "BRU" | "LBRU") {
            // ISA aliases that native ca65 leaves available as names.
            false
        } else {
            self.set.instruction(head).is_some()
                || self.extension.is_some_and(|set| set.instruction(head).is_some())
                // Native source aliases need not be ISA mnemonics.
                || (extended && matches!(head, "INA" | "DEA" | "PHD"))
        }
    }

    /// Statement-head token policy after text processing. Explicit .ident
    /// creates an identifier with a register spelling; mnemonics stay reserved.
    fn reserved_head_label(&self, name: &str, generated: bool) -> bool {
        let upper = name.to_ascii_uppercase();
        self.instruction_name(&upper)
            || (!generated
                && (matches!(upper.as_str(), "A" | "X" | "Y" | "F" | "Z")
                    || (self.extension.is_some() && matches!(upper.as_str(), "S" | "SP"))))
    }

    /// Rewrite every name in a statement to the key it resolves under. A no-op
    /// outside a scope, except for the `::name` spelling, which means the top
    /// level from anywhere.
    fn scope_syms(&self, kind: &Kind, line: usize) -> Result<Kind, AsmError> {
        if let Kind::Visible {
            rule: VisRule::MustBeDefined,
            names,
            ..
        } = kind
        {
            for name in names {
                let key = self.constant_key_at(name, line, false)?;
                if self.expand_text_defines {
                    self.source_unresolved.borrow_mut().insert(key);
                }
            }
        }
        let failure: RefCell<Option<AsmError>> = RefCell::new(None);
        let mut out = map_kind_syms(kind, &|name: &str| {
            if self.expand_text_defines {
                let argument = name
                    .strip_prefix(DEFINED_MARK)
                    .or_else(|| name.strip_prefix(REF_MARK));
                if argument
                    .is_some_and(|key| key.contains(LABEL_SEP) && !key.starts_with(CHEAP_REF_MARK))
                {
                    *failure.borrow_mut() =
                        Some(AsmError::new(line, "name query needs an identifier"));
                    return None;
                }
                let query = if let Some(symbol) = name.strip_prefix(DEFINED_MARK) {
                    Some(self.name_defined(symbol, line))
                } else {
                    name.strip_prefix(REF_MARK)
                        .map(|symbol| self.name_referenced(symbol, line))
                };
                if let Some(query) = query {
                    return match query {
                        Ok(value) => Some(Expr::Num(i64::from(value))),
                        Err(error) => {
                            *failure.borrow_mut() = Some(error);
                            None
                        }
                    };
                }
            }
            if let Some(key) = name.strip_prefix(CHEAP_REF_MARK) {
                self.record_reference(key);
                return Some(Expr::Sym(key.to_string()));
            }
            if name.starts_with(LABEL_SEP) {
                return None;
            }
            let key = if let Some(global) = name.strip_prefix("::") {
                global.to_string()
            } else if name.contains("::") {
                match self.resolve_qualified(name, line) {
                    Ok(key) => key,
                    Err(error) => {
                        *failure.borrow_mut() = Some(error);
                        return None;
                    }
                }
            } else if self.scopes.is_empty() {
                name.to_string()
            } else if name.ends_with(SIZE_SUFFIX) {
                self.scoped_property_name(name)
            } else {
                self.scoped_name(name)
            };
            if !name.ends_with(SIZE_SUFFIX) {
                self.record_reference(&key);
            }
            (key != name).then_some(Expr::Sym(key))
        });
        if let Kind::Visible {
            rule,
            names,
            define,
            ..
        } = &mut out
        {
            for name in names {
                if *rule == VisRule::MustNotBeDefined {
                    continue;
                }
                let key = if define.is_none() {
                    self.scoped_name(name)
                } else {
                    self.constant_key_at(name, line, false)?
                };
                if *rule == VisRule::MustBeDefined {
                    self.record_reference(&key);
                }
                *name = key;
            }
        }
        match failure.into_inner() {
            Some(error) => Err(error),
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
                request: ca65_flat::include_value_request(args, line, ".include", &self.strings)?,
            })),
            ".incbin" => {
                let fold = |piece: &str| {
                    SourceConstants(self).fold(
                        &parse_value(&self.anons, self.scope_env.anchor_str(), piece, line)?,
                        line,
                    )
                };
                let (request, offset, size) =
                    ca65_flat::incbin_value_args(args, line, ".incbin", &fold, &self.strings)?;
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
    fn preprocess_line(
        &mut self,
        raw: &str,
        line: usize,
        _file: FileId,
    ) -> Result<Option<String>, AsmError> {
        if self.structural {
            self.in_macro.set(self.macro_lines.borrow().contains(&line));
            return Ok(Some(raw.to_string()));
        }
        if !self.expand_text_defines {
            return Ok(Some(raw.to_string()));
        }
        let code = strip_comment(raw);
        let trimmed = code.trim();
        let (word, rest) = split_first_word(trimmed);
        match word.to_ascii_lowercase().as_str() {
            ".define" => {
                let (name, value) = split_first_word(rest.trim());
                if !is_ident(name) {
                    return Err(AsmError::new(line, "`.define` needs an identifier"));
                }
                // ca65 refuses to redefine (probed: `A macro named 'val' is
                // already defined`) — there is no silent replacement.
                if self.text_defines.contains_key(name) {
                    return Err(AsmError::new(
                        line,
                        format!("a text symbol named `{name}` is already defined"),
                    ));
                }
                self.text_defines
                    .insert(name.to_string(), value.trim().to_string());
                return Ok(None);
            }
            ".undef" | ".undefine" => {
                let (name, trailing) = split_first_word(rest.trim());
                if !is_ident(name) || !trailing.trim().is_empty() {
                    return Err(AsmError::new(
                        line,
                        "`.undefine` needs exactly one identifier",
                    ));
                }
                // ca65 refuses to undefine what is not defined (probed:
                // `No such macro: nothing`).
                if self.text_defines.remove(name).is_none() {
                    return Err(AsmError::new(
                        line,
                        format!("no text symbol named `{name}` to undefine"),
                    ));
                }
                return Ok(None);
            }
            _ => {}
        }

        // `.ifdef`/`.ifndef` get no carve-out: ca65 expands an active text
        // symbol in the argument like any other use (probed: `.define hi 8`
        // then `.ifdef hi` is `Identifier expected` — the question lands on
        // the *value*), so the general substitution below is the reference
        // behaviour, and the conditional fold validates what arrives.
        substitute_text_defines(raw, &self.text_defines, line).map(Some)
    }

    /// ca65's block vocabulary, measured against ca65 V2.18 and
    /// case-insensitive: `.if` and its nine sibling heads, `.elseif`, `.else`,
    /// `.endif`, and `.repeat` / `.endrepeat`.
    ///
    /// Every head ca65 has is here. The last two to arrive, `.ifref` and
    /// `.ifnref`, ask whether a symbol has been *used* above the line, which
    /// the projection now records as it goes.
    fn block_keyword(&self, code: &str) -> Option<ca65_flat::BlockKw> {
        if self.structural && self.in_macro.get() {
            return None;
        }
        use ca65_flat::BlockKw;
        let code = code.trim();
        // A reserved .end token wins over a following colon/block keyword.
        if end_token_len(code).is_some() {
            return None;
        }
        let generated = split_generated_label_head(code);
        let code = split_label_head(code)
            .or(generated)
            .map_or(code, |(_, rest)| rest);
        let word = split_first_word(code).0.to_ascii_lowercase();
        let keyword = match word.as_str() {
            ".if" | ".ifdef" | ".ifndef" | ".ifblank" | ".ifnblank" | ".ifconst" | ".ifnconst"
            | ".ifref" | ".ifnref" => BlockKw::CondOpen,
            // Processor tests open ordinary conditional blocks. Their live
            // value is read by the projection sweep, not this structural parse.
            ".ifp02" | ".ifp4510" | ".ifp816" | ".ifpc02" | ".ifpsc02" => BlockKw::CondOpen,
            ".elseif" => BlockKw::ElseIf,
            ".else" => BlockKw::Else,
            ".endif" => BlockKw::CondClose,
            ".repeat" => BlockKw::RepeatOpen,
            ".endrepeat" => BlockKw::RepeatClose,
            _ => return None,
        };
        // Closing-delimiter labels need their own native binding events (#866).
        // Recognize generated opening heads here without consuming those labels.
        if generated.is_some() && !matches!(keyword, BlockKw::CondOpen | BlockKw::RepeatOpen) {
            return None;
        }
        Some(keyword)
    }

    fn block_open<'a>(
        &mut self,
        code: &'a str,
        line: usize,
    ) -> Result<(Option<crate::ast::Symbol>, &'a str), AsmError> {
        // Structure-only (#481): the label stays in the head text, unread —
        // the sweep splits and binds it when the block is reached, under the
        // scope and anonymous-label state in force at that point.
        if self.structural {
            return Ok((None, code.trim()));
        }
        let (symbol, rest) =
            split_label_symbol(&self.anons, line, &mut self.scope_env, code.trim())?;
        let symbol = symbol.map(|mut symbol| {
            if !self.scopes.is_empty()
                && matches!(symbol.scope, crate::ast::Scope::Global)
                && !symbol.qualified.starts_with(LABEL_SEP)
            {
                symbol.qualified = qualify(&self.path(), &symbol.qualified);
                self.scope_env.set_anchor(symbol.qualified.clone());
            }
            symbol
        });
        Ok((symbol, rest.trim()))
    }

    fn nodes_mut(&mut self) -> &mut Vec<crate::ast::Node> {
        &mut self.nodes
    }

    /// The multi-file walk expands too, or macros would work when a file is
    /// assembled alone and vanish the moment it is included from another.
    fn expand_source(&self, source: &str) -> Result<macros::Expansion, AsmError> {
        if self.structural {
            self.prepare_macro_lines(source);
            Ok(None)
        } else {
            ca65_flat::expand_ca65_macros(source, macros::Expand::Yes)
        }
    }

    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        self.read_line(raw, line, file, None)
    }

    fn push_node(&mut self, node: crate::ast::Node) {
        self.nodes.push(node);
    }
}

// The selected data parser borrows projection state only during this read.
type DataParser<'a> =
    dyn FnMut(&Walker, Option<&crate::ast::Symbol>, &str) -> Result<Kind, AsmError> + 'a;

impl Walker {
    fn read_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
        mut data_parser: Option<&mut DataParser<'_>>,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        use crate::ast::{Comment, Item, Node, Scope, Span, Symbol, Trivia};
        // Structure-only (#481): carry the line verbatim; the projection
        // sweep reads it under the environment in force when it is reached.
        // An untaken branch's lines are then never read at all — which is the
        // measured ca65 posture, by construction.
        if self.structural {
            self.nodes.push(Node {
                operand_span: None,
                label: None,
                item: Some(Item::Native(Box::new(Kind::Unread))),
                source: raw.trim_end().to_string(),
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia::default(),
            });
            return Ok(None);
        }
        let string_source;
        let raw = if self.expand_text_defines && data_parser.is_none() {
            let code = split_comment(raw).0.trim();
            let operation = split_label_head(code).map_or(code, |(_, rest)| rest);
            let word = split_first_word(operation).0.to_ascii_lowercase();
            let native_strings = lookup(DIRECTIVES, &word).is_some_and(|entry| {
                matches!(
                    entry.id,
                    "bytes" | "asciiz" | "include" | "incbin" | "out" | "message" | "assert"
                )
            });
            if native_strings {
                raw
            } else {
                string_source = self.strings.text_source(raw, line)?;
                &string_source
            }
        } else {
            raw
        };
        // Deferred anonymous-reference records need the current file; the
        // parse helpers below only know their line.
        self.anons.file.set(file);
        // `.end` stops the assembler reading. Nothing below it is parsed, so a
        // line that would be an error is not one — probed: ca65 takes
        // `.end zz`, trailing garbage and all, because it never looks.
        if self.ended {
            return Ok(None);
        }
        let (code, comment) = split_comment(raw);
        // A `{…}` token list is unevaluated source with no expression to
        // render it from, so the formatter hands the line back as written.
        // The text pass folded every one that reaches assembly.
        if data_parser.is_none() && ca65_flat::holds_a_token_list(code) {
            let leading = std::mem::take(&mut self.pending_leading);
            self.push_node(Node {
                operand_span: None,
                label: None,
                item: Some(crate::ast::Item::Verbatim),
                source: self.strings.display(raw.trim_end()),
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia {
                    leading,
                    trailing: None,
                },
            });
            return Ok(None);
        }
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

        if end_token_len(trimmed).is_some() {
            self.ended = true;
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
                // The directive already spells its implicit label. Only the
                // assembly sweep needs a separate address binding; giving it
                // to the formatter would emit a second definition.
                label: label
                    .filter(|_| self.expand_text_defines)
                    .map(|qualified| Symbol {
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
            let qualified = self.constant_key(name, line)?;
            if self.expand_text_defines {
                self.source_unresolved
                    .borrow_mut()
                    .insert(qualified.clone());
            }
            let expr = parse_value(
                &self.anons,
                self.scope_env.anchor_str(),
                &trimmed[eq + 1..],
                line,
            )?;
            let expr = match self.scope_syms(&Kind::Bytes(vec![expr]), line)? {
                Kind::Bytes(mut es) => es.pop().expect("one expression in, one out"),
                _ => unreachable!("Bytes maps to Bytes"),
            };
            // A constant inside a scope belongs to it: `v` in `.proc pa` is
            // `pa::v`, and `::v` outside is a different symbol.
            if let Ok(v) = fold_const(&expr, &self.consts, line) {
                self.consts.insert(qualified.clone(), v);
            }
            if self.expand_text_defines {
                self.source_known.insert(qualified.clone());
            }
            let scope = if name.starts_with('@') {
                Scope::Local {
                    in_global: self.scope_env.anchor_str().to_string(),
                }
            } else {
                // Native reads the RHS under the previous global, then the
                // new immutable symbol starts the next cheap namespace.
                self.scope_env.set_anchor(qualified.clone());
                Scope::Global
            };
            self.nodes.push(Node {
                operand_span: None,
                label: Some(Symbol {
                    qualified,
                    scope,
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
        let (symbol, rest) = split_label_symbol(&self.anons, line, &mut self.scope_env, trimmed)?;
        if self.expand_text_defines
            && let Some(symbol) = &symbol
            && !symbol.name.is_empty()
            && !symbol.name.starts_with('@')
        {
            let head = symbol.name.to_ascii_uppercase();
            if self.instruction_name(&head) {
                return Err(AsmError::new(
                    line,
                    format!("reserved label name `{}`", symbol.name),
                ));
            }
        }
        let symbol = symbol.map(|mut s| {
            // Only a plain label: a cheap local is already keyed on its global
            // (which is itself qualified, so two procs' `@l` stay apart), and an
            // anonymous one is positional rather than named.
            if !self.scopes.is_empty()
                && matches!(s.scope, crate::ast::Scope::Global)
                && !s.qualified.starts_with(LABEL_SEP)
            {
                s.qualified = qualify(&self.path(), &s.qualified);
                self.scope_env.set_anchor(s.qualified.clone());
            }
            s
        });
        if self.expand_text_defines
            && let Some(symbol) = &symbol
        {
            self.source_known.insert(symbol.qualified.clone());
        }

        // `.include`/`.incbin` are walk-handled, not parsed here: the target
        // must not be opened by the parse (KTD1 — `--fmt` succeeds with a
        // missing target), so hand them back for the driver to resolve (or
        // keep unresolved, in the single-source parse).
        if !self.expand_text_defines
            && let Some(item) = ca65_flat::formatter_file_item(rest, line)
        {
            let opaque = matches!(&item, Item::Verbatim);
            self.nodes.push(Node {
                operand_span: ca65_flat::directive_operand_span(raw, rest, line, file),
                label: if opaque { None } else { symbol },
                item: Some(item),
                source: if opaque { raw.trim_end() } else { rest.trim() }.to_string(),
                span,
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing: if opaque { None } else { trailing },
                },
            });
            return Ok(None);
        }
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
        // Processor selectors are source, even when they emit no bytes.
        // In particular, dropping a .p02 or .popcpu changes later instructions
        // when the formatted file is assembled again.
        if !self.expand_text_defines
            && matches!(
                split_first_word(rest).0.to_ascii_lowercase().as_str(),
                ".setcpu" | ".p02" | ".p4510" | ".pushcpu" | ".popcpu"
            )
        {
            self.nodes.push(Node {
                operand_span: None,
                label: symbol,
                item: Some(Item::Verbatim),
                source: rest.trim().to_string(),
                span,
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }
        let kind = if let Some(parser) = data_parser.as_mut() {
            parser(self, symbol.as_ref(), rest)?
        } else if self.expand_text_defines && self.select_cpu(rest, line)? {
            Kind::Empty
        } else {
            match self.tag_statement(rest, line)? {
                Some(kind) => kind,
                None => match parse_op(
                    self.set,
                    self.extension,
                    &self.anons,
                    self.scope_env.anchor_str(),
                    &SourceConstants(self),
                    rest,
                    line,
                ) {
                    Ok(kind) => kind,
                    // ca65 skips an untaken conditional branch lexically. Keep a
                    // parse failure as a fatal statement until projection chooses
                    // the live branch: reached source still reports the original
                    // diagnostic, while dead source is never required to parse.
                    Err(error) => {
                        Kind::Message(DiagSeverity::Error, self.strings.error(error).message)
                    }
                },
            }
        };
        let kind = if self.repeat_registers == Some(false)
            && matches!(
                &kind,
                Kind::Insn {
                    operand: mos6502::OperandSyntax::IndirectIndexedZ(_)
                        | mos6502::OperandSyntax::StackIndirectIndexedY(_),
                    ..
                }
            ) {
            // ca65 tokenises a repeat body before executing its selectors.
            Kind::Message(
                DiagSeverity::Error,
                "register was not available when the repeat body was tokenised".to_string(),
            )
        } else {
            kind
        };
        // Every name the statement mentions, as the key it resolves under.
        let kind = self.scope_syms(&kind, line)?;
        if self.expand_text_defines
            && let Kind::Visible {
                names,
                define: Some(_),
                ..
            } = &kind
        {
            self.source_known.extend(names.iter().cloned());
        }
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
                source: self.strings.display(rest.trim()),
                span,
                trivia,
            }),
        }
        Ok(None)
    }
}

/// Replace ca65 text-symbol tokens outside quoted strings and comments.
/// Expansion repeats because one `.define` may name another; the bounded loop
/// turns a recursive pair into a useful diagnostic instead of hanging.
fn substitute_text_defines(
    raw: &str,
    defines: &BTreeMap<String, String>,
    line: usize,
) -> Result<String, AsmError> {
    let mut current = raw.to_string();
    for _ in 0..32 {
        let mut out = String::with_capacity(current.len());
        let mut chars = current.chars().peekable();
        let mut quote = None;
        let mut changed = false;
        while let Some(ch) = chars.next() {
            if quote.is_none() && ch == ';' {
                out.push(ch);
                out.extend(chars);
                break;
            }
            if matches!(ch, '\'' | '"') {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                }
                out.push(ch);
                continue;
            }
            if quote.is_none() && (ch.is_ascii_alphabetic() || matches!(ch, '_' | '@')) {
                let mut name = String::from(ch);
                while chars
                    .peek()
                    .is_some_and(|next| next.is_ascii_alphanumeric() || matches!(next, '_' | '@'))
                {
                    name.push(chars.next().expect("peeked character exists"));
                }
                if let Some(value) = defines.get(&name) {
                    out.push_str(value);
                    changed = true;
                } else {
                    out.push_str(&name);
                }
            } else {
                out.push(ch);
            }
        }
        if !changed {
            return Ok(current);
        }
        current = out;
    }
    Err(AsmError::new(
        line,
        "ca65 text symbols are still expanding after 32 passes — are they recursive?",
    ))
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

/// End of a literal .end token, including leading whitespace. Native directive
/// names consume identifier characters; punctuation starts the unread tail.
fn end_token_len(text: &str) -> Option<usize> {
    let head = text.trim_start();
    let bytes = head.as_bytes();
    if !bytes
        .get(..4)
        .is_some_and(|word| word.eq_ignore_ascii_case(b".end"))
        || bytes
            .get(4)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        return None;
    }
    Some(text.len() - head.len() + 4)
}

/// Split the initial label token at a single colon. A scope's `::` pair
/// stays inside the token; quoted/operand colons never become label heads.
fn split_label_head(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b':' if bytes.get(at + 1) == Some(&b':') => at += 2,
            b':' => return Some((&text[..at + 1], text[at + 1..].trim_start())),
            c if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'@' | b'.') => at += 1,
            _ => return None,
        }
    }
    None
}

/// A generated label's authored head, without evaluating its .ident argument.
/// Both canonical formatting and selected end-tail admission need this syntax.
fn split_generated_label_head(text: &str) -> Option<(&str, &str)> {
    if !text.get(..6)?.eq_ignore_ascii_case(".ident") {
        return None;
    }
    let args = text[6..].trim_start_matches([' ', '\t']);
    if !args.starts_with('(') {
        return None;
    }
    let open = text.len() - args.len();
    let mut depth = 0usize;
    let mut quote = None;
    for (at, byte) in text.bytes().enumerate().skip(open) {
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let after = text[at + 1..].trim_start_matches([' ', '\t']);
                    if !after.starts_with(':') || after.starts_with("::") || after.starts_with(":=")
                    {
                        return None;
                    }
                    let end = text.len() - after.len() + 1;
                    return Some((&text[..end], text[end..].trim_start()));
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a leading `name:`, `@cheap:`, or bare `:` (anonymous) label into an AST
/// [`Symbol`](crate::ast::Symbol) carrying both the **source form** (`name` /
/// `@cheap` / empty for anonymous — what the formatter re-emits) and the
/// **resolved** name (what assembly uses: the synthetic anonymous key, the
/// `global@cheap` cheap key, or the plain name). Re-anchors the environment when a
/// non-cheap named label is defined (cheap locals scope to the preceding global).
fn split_label_symbol<'a>(
    anons: &AnonCtx,
    line: usize,
    env: &mut crate::scopes::ScopeEnv,
    trimmed: &'a str,
) -> Result<(Option<crate::ast::Symbol>, &'a str), AsmError> {
    use crate::ast::{Scope, Symbol};
    let (word, remainder) = split_label_head(trimmed).unwrap_or_else(|| split_first_word(trimmed));
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
        if !is_cheap_ident(cheap) {
            return Err(AsmError::new(
                line,
                format!("invalid cheap-local label `{name}`"),
            ));
        }
        return Ok((
            Some(Symbol {
                name: name.to_string(),
                scope: Scope::Local {
                    in_global: env.anchor_str().to_string(),
                },
                qualified: checked_cheap_key(env.anchor_str(), cheap, line)?,
            }),
            remainder,
        ));
    }
    if !is_ident(name) {
        return Err(AsmError::new(line, format!("invalid label `{name}`")));
    }
    env.set_anchor(name.to_string());
    Ok((
        Some(Symbol {
            name: name.to_string(),
            scope: Scope::Global,
            qualified: name.to_string(),
        }),
        remainder,
    ))
}

/// Borrow the live scope for source-time allocation folds without copying
/// the namespace. Numeric expressions do not fall outward implicitly here.
struct SourceConstants<'a>(&'a Walker);

impl SourceConstants<'_> {
    fn fold(&self, value: &Expr, line: usize) -> Result<i64, AsmError> {
        let reader = self.0;
        let scoped = reader.scope_syms(&Kind::Bytes(vec![value.clone()]), line)?;
        let Kind::Bytes(mut values) = scoped else {
            unreachable!("source allocation expression")
        };
        let scoped_value = values.pop().expect("one allocation expression");
        let result = scoped_value.eval_with(
            &|name| {
                let key = split_scope_ref(name).map(|(path, name)| {
                    if path.is_empty() {
                        name.to_string()
                    } else {
                        format!("{path}::{name}")
                    }
                });
                reader.consts.get(key.as_deref().unwrap_or(name)).copied()
            },
            None,
            line,
        );
        if !reader.expand_text_defines {
            result.or_else(|_| fold_const(value, &reader.consts, line))
        } else {
            result
        }
    }
}

fn parse_op(
    set: &'static isa::InstructionSet,
    extension: Option<&'static isa::InstructionSet>,
    anons: &AnonCtx,
    anchor: &str,
    consts: &SourceConstants<'_>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(Kind::Empty);
    }
    if let Some(directive) = rest.strip_prefix('.') {
        return parse_directive(anons, anchor, consts, directive, line);
    }
    let (mnemonic, operand_text) = split_first_word(rest);
    let mut mnemonic = mnemonic.to_ascii_uppercase();
    // ca65's 4510 spelling PHD aliases PHW; 65816's implied PHD is
    // handled by its own front-end and must keep its distinct opcode.
    if extension.is_some() && mnemonic == "PHD" {
        mnemonic = "PHW".to_string();
    }
    if let Some(form) = extension
        .and_then(|set| set.find_form(&mnemonic, "zeropage,relative"))
        .or_else(|| set.find_form(&mnemonic, "zeropage,relative"))
    {
        let parts = split_top_level(operand_text, ',');
        if parts.len() != 2 {
            return Err(AsmError::new(
                line,
                format!("`{mnemonic}` needs a zero-page byte and a branch target"),
            ));
        }
        return Ok(Kind::BitBranch {
            form,
            zero_page: parse_value(anons, anchor, parts[0], line)?,
            target: parse_value(anons, anchor, parts[1], line)?,
        });
    }
    let operand_text = operand_text.trim();
    let (address_size, operand_text) = match operand_text.as_bytes() {
        [b'a' | b'A', b':', ..] => (Some(AddressSize::Absolute), &operand_text[2..]),
        [b'z' | b'Z', b':', ..] => (Some(AddressSize::ZeroPage), &operand_text[2..]),
        [b'f' | b'F', b':', ..] => {
            return Err(AsmError::new(
                line,
                "far addressing is not available on this CPU",
            ));
        }
        _ => (None, operand_text),
    };
    let operand =
        mos6502::parse_operand(operand_text, line, &|s, l| parse_value(anons, anchor, s, l))?;
    if address_size.is_some()
        && !matches!(
            operand,
            mos6502::OperandSyntax::Direct(_) | mos6502::OperandSyntax::Indexed(_, _)
        )
    {
        return Err(AsmError::new(
            line,
            "address size requires a direct or indexed memory operand",
        ));
    }
    if set.instruction(&mnemonic).is_none()
        && extension.is_none_or(|set| set.instruction(&mnemonic).is_none())
    {
        return Err(AsmError::new(
            line,
            format!("unknown instruction `{mnemonic}`"),
        ));
    }
    Ok(Kind::Insn {
        operand,
        address_size,
        mnemonic,
        extension,
    })
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
            ".blank",
            ".concat",
            ".const",
            ".def",
            ".defined",
            ".definedmacro",
            ".hibyte",
            ".hiword",
            ".ident",
            ".ismnem",
            ".ismnemonic",
            ".left",
            ".match",
            ".mid",
            ".right",
            ".sprintf",
            ".lobyte",
            ".loword",
            ".max",
            ".min",
            ".paramcount",
            ".strat",
            ".string",
            ".strlen",
            ".tcount",
            ".xmatch",
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
    // `.org` names an address without moving the output; `.reloc` gives it
    // back to the segment.
    Directive {
        id: "placement",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["org", "reloc"],
            required: true,
        },
        category: Category::Operation,
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
    // `.end` stops the assembler reading the file.
    Directive {
        id: "end",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["end"],
            required: true,
        },
        category: Category::Operation,
    },
    // Live text symbols, consumed before ordinary line parsing so their state
    // follows ca65's textual includes.
    Directive {
        id: "text-defines",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["define", "undef", "undefine"],
            required: true,
        },
        category: Category::Operation,
    },
    // Remaining unsupported macro-management surface.
    Directive {
        id: "unsupported-macros",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["delmac", "delmacro", "macpack"],
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
    // The live reader handles 6502/4510 selectors and the CPU stack.
    // Other known processors keep their explicit unsupported-profile error.
    // `.smart` remains inert on these fixed-width processors.
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
    anchor: &str,
    consts: &SourceConstants<'_>,
    directive: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    parse_directive_source(anons, anchor, consts, directive, line, None)
}

trait DataSource {
    fn value(&self, text: &str, line: usize) -> Result<Expr, AsmError>;
    fn tokens(&self, text: &str, line: usize) -> Result<String, AsmError>;
}

fn parse_directive_source(
    anons: &AnonCtx,
    anchor: &str,
    consts: &SourceConstants<'_>,
    directive: &str,
    line: usize,
    source: Option<&dyn DataSource>,
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
        return Err(AsmError::unsupported(
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
            anchor,
            &consts.0.strings,
            rest,
            line,
            source,
        )?)),
        "words" => Ok(Kind::Words(parse_value_list(
            anons, anchor, rest, line, source,
        )?)),
        "dbyt" => Ok(Kind::DBytes(parse_value_list(
            anons, anchor, rest, line, source,
        )?)),
        "dword" => Ok(Kind::DWords(parse_value_list(
            anons, anchor, rest, line, source,
        )?)),
        // Each extractor is its list of values with a byte selector wrapped
        // round every one, so a forward label still resolves at layout time and
        // the size is the item count (times three for `.faraddr`). ca65 answers
        // a string with a syntax error, which `parse_value_list` does too.
        // `Expr::Bank` is the engine's 65816 `^` node — bits 16-23 — which is
        // byte 2 exactly.
        "lobytes" | "hibytes" | "bankbytes" | "faraddr" => {
            let values = parse_value_list(anons, anchor, rest, line, source)?;
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
            &consts.0.strings,
            rest,
            line,
            source,
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
                Some(name) if KNOWN.contains(&name) => Err(AsmError::unsupported(
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
        // `.org` names an address without moving the output, and `.reloc`
        // gives the address back to the segment. Probed against V2.18 + ld65:
        // `.byte $11 / .org $2000 / L: .byte $22 / .word L` is `11 22 00 20` —
        // the `$22` still lands next to the `$11`, and `L` reads `$2000`.
        "placement" => {
            let (word, _) = split_first_word(directive);
            if word.eq_ignore_ascii_case("reloc") {
                return Ok(Kind::Reloc);
            }
            Ok(Kind::Org(parse_value(anons, anchor, rest, line)?))
        }
        "res" => parse_res(anons, anchor, consts, rest, line),
        "align" => parse_align(anons, anchor, consts, rest, line),
        // `.out` is the strict one: ca65 answers a value list with
        // `Unexpected trailing garbage characters`. The other three read a
        // string and ignore whatever follows it on the line — probed, not
        // assumed, because the two shapes read alike in the manual.
        "out" => {
            let source = rest.trim();
            if !string_tokens::StringTokens::is_handle(source)
                && (!source.starts_with('"') || !source[1..].contains('"'))
            {
                return Err(AsmError::new(line, "`.out` takes one string"));
            }
            let (text, tail) = consts.0.strings.text_prefix(source, line)?;
            if !tail.trim().is_empty() {
                return Err(AsmError::new(line, "`.out` takes one string"));
            }
            Ok(Kind::Message(DiagSeverity::Note, text))
        }
        "message" => {
            let severity = if name.eq_ignore_ascii_case("warning") {
                DiagSeverity::Warning
            } else {
                DiagSeverity::Error
            };
            Ok(Kind::Message(
                severity,
                leading_string(rest, &consts.0.strings, line)?,
            ))
        }
        "assert" => parse_assert(anons, anchor, &consts.0.strings, rest, line),
        "export" | "import" | "global" => parse_visible(
            match entry.id {
                "export" => VisRule::MustBeDefined,
                "import" => VisRule::MustNotBeDefined,
                _ => VisRule::EitherWay,
            },
            name.to_ascii_lowercase().ends_with("zp"),
            anons,
            anchor,
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
/// or a `=` constant such as `NUM_ENEMIES`); omitted fill is supplied by layout.
fn parse_res(
    anons: &AnonCtx,
    anchor: &str,
    consts: &SourceConstants<'_>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let count_src = parts.next().unwrap_or("").trim();
    let count = consts
        .fold(&parse_value(anons, anchor, count_src, line)?, line)
        .map_err(|_| AsmError::new(line, "`.res` count must be a constant"))?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`.res` count must be non-negative"))?;
    let fill = match parts.next() {
        None => None,
        Some(v) => {
            let n = consts.fold(&parse_value(anons, anchor, v, line)?, line)?;
            Some(u8::try_from(n).map_err(|_| AsmError::new(line, "`.res` fill must be a byte"))?)
        }
    };
    Ok(Kind::Res(count, fill))
}

/// `.export name[, name...]`, and the `:= expr` form that defines as it
/// exports. ca65 takes the value form only on the export spellings and only for
/// a single name; `.global bar := 9` and `.import baz := 3` are both
/// `Unexpected trailing garbage characters`.
fn parse_visible(
    rule: VisRule,
    zero_page: bool,
    anons: &AnonCtx,
    anchor: &str,
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
        let name = part.split_once(":=").map_or(part, |(name, _)| name).trim();
        if name.contains("::") || name.starts_with('@') {
            return Err(AsmError::new(
                line,
                "a visibility directive needs an unqualified name",
            ));
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
            names.push(name.trim().to_string());
            define = Some(parse_value(anons, anchor, value.trim(), line)?);
        } else {
            names.push(part.to_string());
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
fn leading_string(
    rest: &str,
    strings: &string_tokens::StringTokens,
    line: usize,
) -> Result<String, AsmError> {
    strings.text_prefix(rest, line).map(|(text, _)| text)
}

/// Read the condition/action separators, leaving the message as one value.
/// Native character tokens contain one byte and an optional closing apostrophe;
/// punctuation inside either kind of quoted token is not a header delimiter.
fn assertion_fields(rest: &str) -> Vec<&str> {
    ca65_flat::split_fields(rest, 2)
}

/// `.assert cond, action[, "message"]`. The action decides whether a failure
/// stops the assembly: `error` and `lderror` do, `warning` and `ldwarning`
/// note it and carry on. With assembly and linking fused there is no moment
/// between the two `ld` forms and the others.
fn parse_assert(
    anons: &AnonCtx,
    anchor: &str,
    strings: &string_tokens::StringTokens,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let parts = assertion_fields(rest);
    let cond = parse_value(anons, anchor, parts.first().copied().unwrap_or(""), line)?;
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
    let message = if let Some(source) = parts.get(2) {
        let (text, tail) = strings.text_prefix(source, line)?;
        if !tail.trim().is_empty() {
            return Err(AsmError::new(
                line,
                "unexpected tokens after assertion message",
            ));
        }
        text
    } else {
        "Assertion failed".to_string()
    };
    Ok(Kind::Assert(cond, fatal, message))
}

/// `.align boundary [, fill]` — pad to the next multiple of `boundary` within
/// the active segment. The boundary need not be a power of two, and the pad
/// byte is supplied by layout when omitted.
fn parse_align(
    anons: &AnonCtx,
    anchor: &str,
    consts: &SourceConstants<'_>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let boundary_src = parts.next().unwrap_or("").trim();
    let boundary = consts
        .fold(&parse_value(anons, anchor, boundary_src, line)?, line)
        .map_err(|_| AsmError::new(line, "`.align` boundary must be a constant"))?;
    if boundary < 1 {
        return Err(AsmError::new(line, "`.align` boundary must be positive"));
    }
    let fill = match parts.next() {
        None => None,
        Some(v) => {
            let n = consts.fold(&parse_value(anons, anchor, v, line)?, line)?;
            Some(u8::try_from(n).map_err(|_| AsmError::new(line, "`.align` fill must be a byte"))?)
        }
    };
    Ok(Kind::Align(boundary, fill))
}

/// `.byte` list: `"..."` strings expand to raw ASCII bytes; values are bytes.
fn parse_data_list(
    anons: &AnonCtx,
    anchor: &str,
    strings: &string_tokens::StringTokens,
    rest: &str,
    line: usize,
    source: Option<&dyn DataSource>,
) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "`.byte` needs a value"));
    }
    let mut out = Vec::new();
    for piece in ca65_flat::split_fields(rest, usize::MAX) {
        // Only string-valued operands are expanded as text. Numeric calls
        // remain at their parser position, after any preceding name query.
        let expanded;
        let piece = if let Some(source) = source {
            expanded = source.tokens(piece, line)?;
            &expanded
        } else {
            piece
        };
        // A token selector can expose additional comma-separated operands.
        for piece in ca65_flat::split_fields(piece, usize::MAX) {
            if let Some(bytes) = strings.data(piece, line)? {
                out.extend(bytes.into_iter().map(|b| Expr::Num(i64::from(b))));
            } else {
                out.push(match source {
                    Some(source) => source.value(piece, line)?,
                    None => parse_value(anons, anchor, piece, line)?,
                });
            }
        }
    }
    Ok(out)
}

/// String operands followed by one terminating NUL for the whole directive.
fn parse_asciiz(
    strings: &string_tokens::StringTokens,
    rest: &str,
    line: usize,
    source: Option<&dyn DataSource>,
) -> Result<Vec<Expr>, AsmError> {
    Ok(strings
        .asciiz(
            ca65_flat::split_fields(rest, usize::MAX)
                .into_iter()
                .map(|piece| match source {
                    Some(source) => source.tokens(piece, line),
                    None => Ok(piece.to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .flat_map(|piece| ca65_flat::split_fields(piece, usize::MAX)),
            line,
        )?
        .into_iter()
        .map(|b| Expr::Num(i64::from(b)))
        .collect())
}

fn parse_value_list(
    anons: &AnonCtx,
    anchor: &str,
    rest: &str,
    line: usize,
    source: Option<&dyn DataSource>,
) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "directive needs a value"));
    }
    let mut out = Vec::new();
    for piece in ca65_flat::split_fields(rest, usize::MAX) {
        let expanded;
        let piece = if let Some(source) = source {
            expanded = source.tokens(piece, line)?;
            &expanded
        } else {
            piece
        };
        for piece in ca65_flat::split_fields(piece, usize::MAX) {
            out.push(match source {
                Some(source) => source.value(piece, line)?,
                None => parse_value(anons, anchor, piece, line)?,
            });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Value parsing over the shared expression core
// ---------------------------------------------------------------------------

/// Parse a ca65 value. A bare `:-`/`:+` run is an anonymous-label reference; a
/// bare `@cheap` operand is a cheap-local reference scoped to the current global;
/// otherwise it is an expression with `<`/`>` binding tight ([`BytePrec::Tight`]).
/// An expression with no label context — the text pass's reader, which walks
/// source order with no anonymous labels and no current global to resolve a
/// cheap local against. Both of those are label references, and a label's
/// address is not something that pass can reach anyway.
pub(crate) fn constant_value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    let value = parse_value(&AnonCtx::default(), "", raw, line)?;
    Ok(plain_references(&value))
}

fn plain_references(value: &Expr) -> Expr {
    map_expr_syms(value, &|name| {
        name.strip_prefix(CHEAP_REF_MARK)
            .map(|key| Expr::Sym(key.to_string()))
    })
}

fn parse_value(anons: &AnonCtx, anchor: &str, raw: &str, line: usize) -> Result<Expr, AsmError> {
    parse_value_source(anons, anchor, raw, line, None)
}

fn parse_value_source(
    anons: &AnonCtx,
    anchor: &str,
    raw: &str,
    line: usize,
    source: Option<&dyn mos6502::ExprSource>,
) -> Result<Expr, AsmError> {
    let t = raw.trim();
    if let Some((sign, level)) = anon_ref(t) {
        return Ok(Expr::Sym(anons.refer(sign, level, line)?));
    }
    let expr = mos6502::parse_expr_source(
        t,
        line,
        parse_number,
        mos6502::ExprOpts {
            logical: true,
            logical_not_tight: false,
            scoped_names: true,
            fixed_point: false,
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
        source,
    )?;
    // Bare names, arithmetic and name-query markers all share the same
    // cheap namespace. Binding only a bare operand loses that context.
    let failure = RefCell::new(None);
    let value = map_expr_syms(&expr, &|name| {
        let (prefix, symbol) = if let Some(symbol) = name.strip_prefix(DEFINED_MARK) {
            (DEFINED_MARK, symbol)
        } else if let Some(symbol) = name.strip_prefix(REF_MARK) {
            (REF_MARK, symbol)
        } else {
            ("", name)
        };
        let local = symbol
            .strip_prefix('@')
            .filter(|name| is_cheap_ident(name))?;
        match checked_cheap_key(anchor, local, line) {
            Ok(key) => Some(Expr::Sym(format!("{prefix}{CHEAP_REF_MARK}{key}"))),
            Err(error) => {
                *failure.borrow_mut() = Some(error);
                None
            }
        }
    });
    if let Some(error) = failure.into_inner() {
        Err(error)
    } else {
        Ok(value)
    }
}

// The colon is not a cheap identifier character, so this property key cannot
// collide with an authored @size constant or label.
const SIZE_SUFFIX: &str = "\u{1}:size";

const CHEAP_REF_MARK: &str = "\u{1}cheap-ref\u{1}";

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
    format!("{SCOPE_MARK}{}\u{2}{name}", path.join("::"))
}

/// Split a reference key back into the path it was written in and the name.
fn split_scope_ref(key: &str) -> Option<(&str, &str)> {
    // Both synthetic scope segments and cheap-local names can contain
    // LABEL_SEP. A distinct delimiter keeps either side unambiguous.
    key.strip_prefix(SCOPE_MARK)?.split_once('\u{2}')
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

/// ca65 cheap tokens allow digits after `@`, but never a dot keyword.
fn is_cheap_ident(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// A collision-proof symbol key for a cheap local, scoped to its global.
fn cheap_key(global: &str, name: &str) -> String {
    format!("{global}\u{1}{name}")
}

fn checked_cheap_key(global: &str, name: &str, line: usize) -> Result<String, AsmError> {
    if global.is_empty() {
        Err(AsmError::new(
            line,
            format!("no preceding global symbol for `@{name}`"),
        ))
    } else {
        Ok(cheap_key(global, name))
    }
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

    /// ca65's text layer, folded before the parse. Every value here was read
    /// off ca65 V2.18 first.
    #[test]
    fn the_string_functions_fold_the_way_ca65_folds_them() {
        // The ROM's payload starts after the 16-byte iNES header.
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 8].to_vec()
        };
        assert_eq!(&code(".byte .concat(\"ab\",\"cd\")")[..4], b"abcd");
        // One argument is a whole call; ca65 does not require two.
        assert_eq!(&code(".byte .concat(\"a\")")[..1], b"a");

        // `.string` stringifies the *token*, not the value: with `N = 7`,
        // `.string(N)` is `"N"` and not `"7"`. A literal keeps its digits.
        assert_eq!(&code("N = 7\n.byte .string(N)")[..1], b"N");
        assert_eq!(&code(".byte .string(42)")[..2], b"42");
        assert_eq!(&code("lbl:\n.byte .string(lbl)")[..3], b"lbl");

        // `.ident` builds a *name* from text, so it resolves like any other —
        // a forward reference included.
        assert_eq!(&code("foo = 5\n.byte .ident(\"foo\")")[..1], &[5]);
        assert_eq!(
            &code(".byte .ident(.concat(\"f\",\"oo\"))\nfoo = 9")[..1],
            &[9]
        );

        // The folds nest, innermost first, so a numeric function reads a
        // literal by the time it runs.
        assert_eq!(&code(".byte .strlen(.concat(\"ab\",\"cd\"))")[..1], &[4]);
    }

    /// `.paramcount` and `.definedmacro`, read off ca65 V2.18 first. Both are
    /// answered during macro expansion, which is the only place either has a
    /// meaning.
    #[test]
    fn the_macro_predicates_answer_the_way_ca65_answers() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 4].to_vec()
        };
        let m = ".macro m p1, p2\n.byte .paramcount\n.endmacro\n";

        // The **call site's** count, not the declared one: two parameters
        // called with one answers 1, and a blank argument still counts.
        assert_eq!(&code(&format!("{m}m 1, 2"))[..1], &[2]);
        assert_eq!(&code(&format!("{m}m 1"))[..1], &[1]);
        assert_eq!(&code(&format!("{m}m"))[..1], &[0]);
        assert_eq!(
            &code(".macro m p1, p2, p3\n.byte .paramcount\n.endmacro\nm 1, , 3")[..1],
            &[3]
        );
        // A nested invocation answers for itself, not for the macro around it.
        assert_eq!(
            &code(".macro i\n.byte .paramcount\n.endmacro\n.macro o p1\ni\n.endmacro\no 1")[..1],
            &[0]
        );
        // It reads as a number, so a conditional can branch on it.
        assert_eq!(
            &code(
                ".macro m p1, p2\n.if .paramcount > 1\n.byte 9\n.else\n.byte 8\n.endif\n\
                 .endmacro\nm 1\nm 1,2"
            )[..2],
            &[8, 9]
        );

        // `.definedmacro` is answered where it is *written*: a definition below
        // the line does not count, and the name is case-sensitive.
        assert_eq!(
            &code(
                ".macro d\n.endmacro\n.byte .definedmacro(d), .definedmacro(n), .definedmacro(D)"
            )[..3],
            &[1, 0, 0]
        );
        assert_eq!(
            &code(".byte .definedmacro(d)\n.macro d\n.endmacro")[..1],
            &[0]
        );
        // Inside a body it is answered at the line the macro was *invoked* on,
        // which is where ca65 answers it too.
        assert_eq!(
            &code(".macro d\n.endmacro\n.macro n\n.byte .definedmacro(d)\n.endmacro\nn")[..1],
            &[1]
        );
    }

    /// `.const` and `.ismnem`, read off ca65 V2.18 first.
    #[test]
    fn const_and_ismnem_answer_the_way_ca65_answers() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 4].to_vec()
        };

        // A literal and a constant above the line are constant; a label and the
        // location counter are not, and neither is a constant defined *below*
        // — which is exactly what a pass walking in source order can see.
        assert_eq!(
            &code("N = 5\nL: .byte .const(5), .const(N), .const(N*2)")[..3],
            &[1, 1, 1]
        );
        assert_eq!(&code("L: .byte .const(L), .const(*)")[..2], &[0, 0]);
        assert_eq!(&code(".byte .const(N)\nN = 5")[..1], &[0]);
        // A known scoped constant is read through the selected scope.
        assert_eq!(
            &code(".scope pa\nv=1\n.byte .const(v)\n.endscope")[..1],
            &[1]
        );

        // `.ismnem` follows the CPU. `bra` and `stz` are 65C02 additions that
        // plain 6502 ca65 does not know, and `nop` is in every one of them.
        assert_eq!(
            &code(".byte .ismnem(lda), .ismnemonic(lda), .ismnem(zzz)")[..3],
            &[1, 1, 0]
        );
        assert_eq!(
            &code(".byte .ismnem(bra), .ismnem(stz), .ismnem(nop)")[..3],
            &[0, 0, 1]
        );
        // The name is read case-insensitively.
        assert_eq!(&code(".byte .ismnem(LDA)")[..1], &[1]);
    }

    /// What `.const` and `.ismnem` refuse.
    #[test]
    fn const_and_ismnem_refuse_what_they_cannot_answer() {
        let refused = |src: &str| {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("should refuse `{src}`"))
        };
        // ca65 answers a name it has never seen with "Symbol is undefined"
        // rather than with 0, so a 0 here would be an invention.
        let e = refused(".byte .const(zzz)");
        assert!(e.to_string().contains("undefined symbol `zzz`"), "{e}");

        // Native ld65 rejects an imported name that the query actually reads.
        let e = refused(".import ext\n.byte .const(ext)");
        assert!(e.to_string().contains("undefined symbol `ext`"), "{e}");
        // Explicit qualification cannot fall back to the root's N.
        let e = refused("N = 1\n.byte .const(pa::N)");
        assert!(e.to_string().contains("no such scope `pa`"), "{e}");

        // ca65 takes a bare name for `.ismnem` and nothing else.
        refused(".byte .ismnem(\"lda\")");
        refused(".byte .ismnem(1+1)");
    }

    /// The token-list half, read off ca65 V2.18 first.
    ///
    /// A token list is unevaluated source, so these answer over what is
    /// *written*. The comparison rules are the part worth pinning: `.match`
    /// asks what each token **is** and `.xmatch` asks what it **says**.
    #[test]
    fn the_token_functions_fold_the_way_ca65_folds_them() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 6].to_vec()
        };

        // A comma is a token, and a string is one however much is inside it.
        assert_eq!(&code(".byte .tcount({1, 2, 3})")[..1], &[5]);
        assert_eq!(
            &code(".byte .tcount({}), .tcount({abc}), .tcount({\"a,b\"})")[..3],
            &[0, 1, 1]
        );
        assert_eq!(
            &code(".byte .tcount({#$12}), .tcount({pa::v}), .tcount({.byte 1})")[..3],
            &[2, 3, 2]
        );
        // Whitespace separates and is not a token, and `<<` is one where `+ +`
        // is two.
        assert_eq!(
            &code(".byte .tcount({++}), .tcount({<<}), .tcount({:+})")[..3],
            &[2, 1, 1]
        );
        assert_eq!(&code(".byte .blank({}), .blank({x})")[..2], &[1, 0]);

        // `.match` compares kinds. Two numbers match whatever they are worth,
        // and so do two strings — but `a` is the accumulator and `b` is an
        // identifier, so those do not.
        assert_eq!(
            &code(".byte .match({1},{2}), .match({\"a\"},{\"b\"}), .match({abc},{abd})")[..3],
            &[1, 1, 1]
        );
        assert_eq!(
            &code(".byte .match({a},{b}), .match({x},{y}), .match({a},{A})")[..3],
            &[0, 0, 1]
        );
        // `s` is a register on a 65816 and an ordinary identifier here, so
        // this same line answers 0 through the `ca65-816` dialect. ca65 takes
        // the register names from the CPU.
        assert_eq!(&code(".byte .match({s},{q})")[..1], &[1]);
        // Each dot-keyword and each punctuation mark is its own kind, and a
        // character literal is not a number.
        assert_eq!(
            &code(".byte .match({.byte},{.word}), .match({+},{-}), .match({'a'},{1})")[..3],
            &[0, 0, 0]
        );
        assert_eq!(
            &code(".byte .match({a},{a b}), .match({},{})")[..2],
            &[0, 1]
        );

        // `.xmatch` adds the value: numbers by what they are worth, so `1` and
        // `$1` agree, and identifiers case-sensitively, so `lda` and `LDA` do
        // not.
        assert_eq!(
            &code(".byte .xmatch({1},{2}), .xmatch({1},{$1})")[..2],
            &[0, 1]
        );
        assert_eq!(
            &code(".byte .xmatch({abc},{abd}), .xmatch({lda},{LDA})")[..2],
            &[0, 0]
        );
        assert_eq!(
            &code(".byte .xmatch({\"a\"},{\"b\"}), .xmatch({.byte},{.byte})")[..2],
            &[0, 1]
        );

        // The sub-list functions splice tokens back as source: `.left(3, …)`
        // takes `1`, `,` and `2`, which is a two-item byte list.
        assert_eq!(&code(".byte .left(1, {1, 2, 3})")[..1], &[1]);
        assert_eq!(&code(".byte .left(3, {1, 2, 3})")[..2], &[1, 2]);
        assert_eq!(
            &code(".byte .right(1, {1, 2, 3}), .mid(2, 1, {1, 2, 3})")[..2],
            &[3, 2]
        );
        // Past the end stops at the end rather than refusing.
        assert_eq!(
            &code(".byte .left(9, {1}), .right(9, {1}), .mid(0, 9, {1})")[..3],
            &[1, 1, 1]
        );

        // A string-valued function inside a list folds first and is then one
        // token, exactly as ca65 counts it — while a numeric one stays four.
        assert_eq!(
            &code(".byte .tcount({.concat(\"a\",\"b\")}), .tcount({.strlen(\"ab\")})")[..2],
            &[1, 4]
        );
    }

    /// `.sprintf` against ca65 V2.18, one row per measured case.
    ///
    /// The whole matrix is here rather than a sample because the rules are not
    /// C's and not guessable: `%x` is signed where `%X` is not, `%s` and `%c`
    /// align the opposite way to everything else, and `#` on `%x` shows a
    /// prefix even for zero. Every row was read off the tool before it was
    /// written down.
    #[test]
    fn sprintf_formats_the_way_ca65_formats() {
        let text = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            let body = &r[16..];
            let end = body.iter().position(|b| *b == 0).unwrap_or(0);
            String::from_utf8_lossy(&body[..end]).into_owned()
        };
        #[rustfmt::skip]
        let cases: &[(&str, &str, &str)] = &[
            ("%d", "5", "5"),
            ("%d", "-5", "-5"),
            ("%d", "0", "0"),
            ("%i", "5", "5"),
            ("%i", "-5", "-5"),
            ("%u", "5", "5"),
            ("%u", "-5", "18446744073709551611"),
            ("%x", "255", "ff"),
            ("%x", "-255", "-ff"),
            ("%x", "-1", "-1"),
            ("%X", "255", "FF"),
            ("%X", "-255", "FFFFFFFFFFFFFF01"),
            ("%X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%o", "9", "11"),
            ("%o", "-9", "1777777777777777777767"),
            ("%c", "65", "A"),
            ("%s", "\"ab\"", "ab"),
            ("%+d", "5", "+5"),
            ("%+d", "-5", "-5"),
            ("%+d", "0", "+0"),
            ("% d", "5", " 5"),
            ("% d", "-5", "-5"),
            ("% d", "0", " 0"),
            ("%#x", "255", "0xff"),
            ("%#x", "-255", "-0xff"),
            ("%#x", "-1", "-0x1"),
            ("%#X", "255", "0XFF"),
            ("%#X", "-255", "0XFFFFFFFFFFFFFF01"),
            ("%#X", "-1", "0XFFFFFFFFFFFFFFFF"),
            ("%#o", "9", "011"),
            ("%#o", "-9", "01777777777777777777767"),
            ("%#d", "5", "5"),
            ("%#d", "-5", "-5"),
            ("%#d", "0", "0"),
            ("%#s", "\"ab\"", "ab"),
            ("%#c", "65", "A"),
            ("%+u", "5", "+5"),
            ("%+u", "-5", "+18446744073709551611"),
            ("%+x", "255", "+ff"),
            ("%+x", "-255", "-ff"),
            ("%+x", "-1", "-1"),
            ("%+X", "255", "+FF"),
            ("%+X", "-255", "+FFFFFFFFFFFFFF01"),
            ("%+X", "-1", "+FFFFFFFFFFFFFFFF"),
            ("%+o", "9", "+11"),
            ("%+o", "-9", "+1777777777777777777767"),
            ("%+c", "65", "A"),
            ("%+s", "\"ab\"", "ab"),
            ("% u", "5", " 5"),
            ("% u", "-5", " 18446744073709551611"),
            ("% x", "255", " ff"),
            ("% x", "-255", "-ff"),
            ("% x", "-1", "-1"),
            ("% X", "255", " FF"),
            ("% X", "-255", " FFFFFFFFFFFFFF01"),
            ("% X", "-1", " FFFFFFFFFFFFFFFF"),
            ("% o", "9", " 11"),
            ("% o", "-9", " 1777777777777777777767"),
            ("% s", "\"ab\"", "ab"),
            ("% c", "65", "A"),
            ("%6d", "5", "     5"),
            ("%6d", "-5", "    -5"),
            ("%6d", "0", "     0"),
            ("%-6d", "5", "5     "),
            ("%-6d", "-5", "-5    "),
            ("%-6d", "0", "0     "),
            ("%06d", "5", "000005"),
            ("%06d", "-5", "-00005"),
            ("%06d", "0", "000000"),
            ("%-06d", "5", "5     "),
            ("%-06d", "-5", "-5    "),
            ("%-06d", "0", "0     "),
            ("%+06d", "5", "+00005"),
            ("%+06d", "-5", "-00005"),
            ("%+06d", "0", "+00000"),
            ("%6x", "255", "    ff"),
            ("%6x", "-255", "   -ff"),
            ("%6x", "-1", "    -1"),
            ("%-6x", "255", "ff    "),
            ("%-6x", "-255", "-ff   "),
            ("%-6x", "-1", "-1    "),
            ("%06x", "255", "0000ff"),
            ("%06x", "-255", "-000ff"),
            ("%06x", "-1", "-00001"),
            ("%#08x", "255", "0x0000ff"),
            ("%#08x", "-255", "-0x000ff"),
            ("%#08x", "-1", "-0x00001"),
            ("%+#08x", "255", "+0x000ff"),
            ("%+#08x", "-255", "-0x000ff"),
            ("%+#08x", "-1", "-0x00001"),
            ("%#08X", "255", "0X0000FF"),
            ("%#08X", "-255", "0XFFFFFFFFFFFFFF01"),
            ("%#08X", "-1", "0XFFFFFFFFFFFFFFFF"),
            ("%+#08X", "255", "+0X000FF"),
            ("%+#08X", "-255", "+0XFFFFFFFFFFFFFF01"),
            ("%+#08X", "-1", "+0XFFFFFFFFFFFFFFFF"),
            ("%6X", "255", "    FF"),
            ("%6X", "-255", "FFFFFFFFFFFFFF01"),
            ("%6X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%06X", "255", "0000FF"),
            ("%06X", "-255", "FFFFFFFFFFFFFF01"),
            ("%06X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%6o", "9", "    11"),
            ("%6o", "-9", "1777777777777777777767"),
            ("%06o", "9", "000011"),
            ("%06o", "-9", "1777777777777777777767"),
            ("%6u", "5", "     5"),
            ("%6u", "-5", "18446744073709551611"),
            ("%06u", "5", "000005"),
            ("%06u", "-5", "18446744073709551611"),
            ("%6s", "\"ab\"", "ab    "),
            ("%-6s", "\"ab\"", "    ab"),
            ("%06s", "\"ab\"", "ab    "),
            ("%.2s", "\"ab\"", "ab"),
            ("%.6s", "\"ab\"", "ab"),
            ("%6c", "65", "A     "),
            ("%-6c", "65", "     A"),
            ("%06c", "65", "A     "),
            ("%.1c", "65", "A"),
            ("%.3d", "5", "005"),
            ("%.3d", "-5", "-005"),
            ("%.3d", "0", "000"),
            ("%.4x", "255", "00ff"),
            ("%.4x", "-255", "-00ff"),
            ("%.4x", "-1", "-0001"),
            ("%.4X", "255", "00FF"),
            ("%.4X", "-255", "FFFFFFFFFFFFFF01"),
            ("%.4X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%.4o", "9", "0011"),
            ("%.4o", "-9", "1777777777777777777767"),
            ("%.4u", "5", "0005"),
            ("%.4u", "-5", "18446744073709551611"),
            ("%.0d", "5", "5"),
            ("%.0d", "-5", "-5"),
            ("%.0d", "0", ""),
        ];
        for (spec, value, want) in cases {
            let src = format!(".byte .sprintf(\"{spec}\", {value}), 0");
            assert_eq!(&text(&src), want, "`{spec}` of `{value}`");
        }
    }

    /// The pass folds a constants environment as it walks, so `%d` may be
    /// given a constant defined above the line — and the cases ca65 refuses
    /// itself are refused here for the same reason.
    #[test]
    fn sprintf_reads_the_constants_above_it() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 4].to_vec()
        };
        assert_eq!(&code("N = 5\n.byte .sprintf(\"n=%d\", N)")[..3], b"n=5");
        assert_eq!(&code("N = 5\n.byte .sprintf(\"%d\", N*2+1)")[..2], b"11");

        // A label's address and a constant defined *below* are both out of
        // reach of a pass that walks in source order — and ca65 answers both
        // with "Constant expression expected", so nothing is lost.
        for src in [
            "L: .byte .sprintf(\"%d\", L)",
            ".byte .sprintf(\"%d\", N)\nN = 5",
        ] {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("ca65 refuses `{src}`"));
        }

        // A constant defined inside a scope is not recorded: folding it flat
        // would let an inner `v` answer a `v` written outside, where ca65
        // resolves two different symbols.
        assemble(".segment \"CODE\"\n.scope pa\nv = 1\n.endscope\n.byte .sprintf(\"%d\", v)\n")
            .expect_err("an inner scope's constant is not in scope outside it");
    }

    /// What `.sprintf` refuses. ca65 refuses each of these itself.
    #[test]
    fn sprintf_refuses_what_ca65_refuses() {
        for src in [
            // Only eight conversions, and `%b`, `%f`, `%ld` and `%*d` are not
            // among them however ordinary they look in C.
            ".byte .sprintf(\"%q\", 1)",
            ".byte .sprintf(\"%b\", 1)",
            ".byte .sprintf(\"%f\", 1)",
            ".byte .sprintf(\"%ld\", 1)",
            ".byte .sprintf(\"%*d\", 5, 1)",
            // The two halves cannot be swapped.
            ".byte .sprintf(\"%d\", \"hi\")",
            ".byte .sprintf(\"%s\", 5)",
            // A character runs from 1 to 255.
            ".byte .sprintf(\"%c\", 0)",
            ".byte .sprintf(\"%c\", 256)",
            ".byte .sprintf(\"%c\", -1)",
            // The argument count is exact in both directions.
            ".byte .sprintf(\"%d %d\", 1)",
            ".byte .sprintf(\"%d\", 1, 2)",
        ] {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("ca65 refuses `{src}`"));
        }
    }

    /// What the three refuse. ca65 refuses each of these itself.
    #[test]
    fn the_string_functions_refuse_what_ca65_refuses() {
        for src in [
            // `.concat` joins strings, and a number is not one.
            ".byte .concat(\"a\", 5)",
            // `.string` takes one name or number — not a string, and not an
            // expression over them.
            ".byte .string(\"hi\")",
            "N = 7\n.byte .string(N*2)",
            ".byte .string(1, 2)",
            // `.ident` needs the text a name is built from.
            ".byte .ident(foo)",
        ] {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("ca65 refuses `{src}`"));
        }
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
        let err = assemble(".code\nV = 1\n lda #.bank(V)\n").expect_err("not implemented");
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

    #[test]
    fn a_label_may_share_a_conditional_or_repetition_head() {
        let image = rom(
            ".segment \"CODE\"\nLB: .if 1\n .byte $11\n.endif\nLR: .repeat 1\n .byte $22\n.endrepeat\n .word LB, LR\n",
        );
        assert_eq!(&image[16..22], &[0x11, 0x22, 0x00, 0x80, 0x01, 0x80]);
    }

    #[test]
    fn a_dead_conditional_branch_need_not_parse() {
        assert_eq!(bytes(".if 0\n .byte\n.endif\n .byte 1\n")[..1], [1]);
        crate::assemble_ca65(".if 1\n .byte\n.endif\n")
            .expect_err("the same malformed line is an error when reached");
    }

    #[test]
    fn a_blank_macro_argument_may_leave_invalid_source_in_the_dead_branch() {
        assert_eq!(
            bytes(
                ".macro fill v\n .ifblank v\n .byte $ff\n .else\n .byte v\n .endif\n.endmacro\n fill\n"
            )[..1],
            [0xFF]
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
        for d in [".condes foo, 1", ".charmap $41, $42", ".macpack cpu"] {
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

    #[test]
    fn text_defines_substitute_and_can_be_undefined() {
        let rom = rom(
            ".segment \"CODE\"\n.define VALUE $42\n.byte VALUE\n.undefine VALUE\n\
             .ifndef VALUE\n.byte $43\n.endif\n",
        );
        assert_eq!(&rom[16..18], &[0x42, 0x43]);
    }

    /// #481, probe-pinned against ca65 V2.18: an untaken branch's lines are
    /// never read, so a definition inside one is invisible afterwards — and
    /// so is anything else in there, a line that would not parse included.
    #[test]
    fn an_untaken_branch_defines_nothing_and_need_not_parse() {
        let rom = rom(
            ".segment \"CODE\"\n.if 0\n.define six 6\nthis is lexable garbage but not a statement\n\
             .end\n.endif\n.ifdef six\n.byte 6\n.else\n.byte 1\n.endif\n",
        );
        assert_eq!(rom[16], 1, "the untaken definition must be invisible");
    }

    /// #481: a taken branch's definition flows to the rest of the source —
    /// the following top-level line included (probed: `a9 06`).
    #[test]
    fn a_taken_branch_definition_flows_on() {
        let first = rom(".segment \"CODE\"\n.if 1\n.define six 6\n.endif\n.byte six\n");
        assert_eq!(first[16], 6);
        // ... across a later block too.
        let second =
            rom(".segment \"CODE\"\n.if 1\n.define five 5\n.endif\n.if 1\n.byte five\n.endif\n");
        assert_eq!(second[16], 5);
    }

    /// #481: `.undefine` has the same positional, selection-time semantics.
    #[test]
    fn an_undefine_inside_a_taken_branch_takes_effect() {
        let rom = rom(
            ".segment \"CODE\"\n.define six 6\n.if 1\n.undefine six\n.endif\n\
             .ifdef six\n.byte 6\n.else\n.byte 2\n.endif\n",
        );
        assert_eq!(rom[16], 2);
    }

    /// #481, the shape the old diagnostic refused and ca65 accepts (probed:
    /// `a9 07` three times): each iteration defines, uses, and undefines in
    /// execution order.
    #[test]
    fn a_repeat_body_applies_definitions_per_iteration() {
        let rom = rom(
            ".segment \"CODE\"\n.repeat 3\n.define val 7\n.byte val\n.undefine val\n.endrepeat\n",
        );
        assert_eq!(&rom[16..19], &[7, 7, 7]);
    }

    /// Probed: ca65 refuses a second `.define` of a live name — which is what
    /// makes the iteration above need its `.undefine`.
    #[test]
    fn redefining_a_live_text_symbol_is_refused() {
        let error = crate::assemble_ca65(
            ".segment \"CODE\"\n.repeat 2\n.define val 7\n.byte val\n.endrepeat\n",
        )
        .expect_err("iteration two redefines");
        assert!(error.to_string().contains("already defined"), "{error}");
        let error = crate::assemble_ca65(".define one 1\n.define one 2\n")
            .expect_err("ca65 refuses to redefine");
        assert!(error.to_string().contains("already defined"), "{error}");
    }

    /// Probed: `.undefine` of a name that is not defined is an error
    /// (`No such macro`), not a no-op.
    #[test]
    fn undefining_an_unknown_text_symbol_is_refused() {
        let error = crate::assemble_ca65(".undefine nothing\n")
            .expect_err("ca65 refuses to undefine the undefined");
        assert!(error.to_string().contains("to undefine"), "{error}");
    }

    /// Probed: `.ifdef` of a *live* text symbol substitutes the argument like
    /// any other use — `.define hi 8` then `.ifdef hi` is ca65's `Identifier
    /// expected`, because the question lands on the value.
    #[test]
    fn ifdef_of_a_live_text_symbol_lands_on_its_value() {
        let error = crate::assemble_ca65(".define hi 8\n.ifdef hi\n.byte 1\n.endif\n")
            .expect_err("the argument was substituted");
        assert!(error.to_string().contains("identifier"), "{error}");
    }

    /// Probed: `.end` inside a taken branch stops the reader before the
    /// closer, and ca65 reports the block unclosed even though the text
    /// closes it further down.
    #[test]
    fn end_inside_a_taken_branch_leaves_the_block_unclosed() {
        let error =
            crate::assemble_ca65(".segment \"CODE\"\n.if 1\n.byte 4\n.end\n.endif\n.byte 5\n")
                .expect_err("the reader stops before the closer");
        assert!(error.to_string().contains("never closed"), "{error}");
    }

    /// #481: an untaken branch touches no state — a label inside one neither
    /// binds nor resets the cheap-local scope, and a constant neither.
    #[test]
    fn an_untaken_branch_touches_no_binding_state() {
        // The cheap local before the block still resolves across it (probed).
        let rom = rom(".segment \"CODE\"\nfoo:\n@b: .byte 1\n.if 0\nother:\n.endif\n.word @b\n");
        assert_eq!(&rom[16..19], &[1, 0x00, 0x80]);
        // The constant never folds (probed: `Symbol 'k' is undefined`).
        let error = crate::assemble_ca65(".segment \"CODE\"\n.if 0\nk = 9\n.endif\n.byte k\n")
            .expect_err("an untaken constant must not leak");
        assert!(error.to_string().contains("k"), "{error}");
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
        for src in [".setcpu \"65C02\"", ".pc02", ".p816", ".psc02"] {
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

    /// `.org` moves the address a statement reports and leaves its bytes where
    /// they were going; `.reloc` gives the address back to the segment.
    #[test]
    fn org_moves_the_address_and_not_the_bytes() {
        let image = rom(".segment \"CODE\"\n .byte $11\n\
             .org $2000\nL: .byte $22\n .word L, *\n\
             .reloc\nM: .byte $33\n .word M\n");
        // The `$22` still lands next to the `$11` — only the address moved.
        assert_eq!(&image[16..18], &[0x11, 0x22]);
        // `L` reads from the new origin. The first word advances the counter,
        // so `*` is three bytes past that origin when its operand is read.
        assert_eq!(&image[18..22], &[0x00, 0x20, 0x03, 0x20]);
        // After `.reloc` the segment's own addressing is back: `M` is the
        // seventh byte of CODE, which is based at $8000.
        assert_eq!(image[22], 0x33);
        assert_eq!(&image[23..25], &[0x06, 0x80]);
    }

    /// `.end` stops the assembler reading, so what follows is not parsed at
    /// all — not even far enough to be an error.
    #[test]
    fn end_stops_the_read() {
        let image = rom(".segment \"CODE\"\n .byte $11\n.end\n .byte $22\n\
             .zzqq this is not a directive and never becomes one\n");
        assert_eq!(image[16], 0x11);
        assert_eq!(image[17], 0x00, "nothing below `.end` was assembled");
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
        for f in [
            ".lobyte", ".strlen", ".max", ".defined", ".sizeof", ".tcount",
        ] {
            assert_eq!(
                kind(f),
                Some(crate::directives::Category::ExpressionWord),
                "`{f}` is implemented, so it is declared as what it is"
            );
        }
        for f in [".time", ".version"] {
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
    #[test]
    fn single_source_numeric_admission_checks_skipped_captures() {
        for source in [
            ".if 0\n.byte <$100000000\n.endif\n",
            ".repeat 0\n.byte <$100000000\n.endrepeat\n",
            ".macro unused\n.byte <$100000000\n.endmacro\n",
        ] {
            let error = assemble(source).expect_err("native lexical overflow");
            let span = error.span.expect("source location");
            assert_eq!(span.file, crate::span::FileId(0));
            assert_eq!(span.line, 2);
        }
        assert!(
            assemble(
                ".if 0\n.include 3\n.setcpu \"not-a-cpu\"\n.byte MISSING,,\n.endif\n.byte 7\n"
            )
            .is_ok()
        );
        assert!(assemble(".end .sprintf(\"%d\",4294967300)\n.byte <$100000000\n").is_ok());
    }
}
