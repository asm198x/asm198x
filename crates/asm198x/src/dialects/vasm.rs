//! The vasm (Motorola syntax) 68000 dialect — the field-based encoder.
//!
//! The 68000 is word-oriented and big-endian: an instruction is a 16-bit opcode
//! word (size, register, and effective-address fields packed in) followed by
//! 0–4 extension words. This front-end parses Motorola syntax, resolves each
//! operand to an effective address, and fills the [`isa::m68k`] form's fields.
//!
//! Three output paths, each byte-identical to the matching `vasmm68k_mot`
//! invocation across the Amiga curriculum:
//! - [`assemble_with`]`(.., false)` — `-no-opt` flat binary.
//! - [`assemble`] — `-Fbin` (optimizer on): short-branch relaxation,
//!   PC-relative addressing for same-section labels, `addq`/`subq`, the
//!   `add #d,An`↔`lea`↔`addq` rewrites, zero-displacement dropping, and
//!   `cmp #0`→`tst`.
//! - [`assemble_exe`] — `-Fhunkexe -kick1hunks`: the multi-section hunk
//!   executable (header, code/data/bss hunks, reloc32 tables), matching the
//!   loadable image and omitting vasm's debug symbol table.
//!
//! See `decisions/syntax-stance.md`.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use isa::m68k::{self, EaModes, Size, SizeEnc, Slot, ea};

use super::ca65_flat::{self, DirectiveLine, FlatWalk, Resolution, WalkDirective, WalkSemantics};
use super::macros;
use super::mos6502::{self, is_ident, split_data_items, split_first_word, string_literal};
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, BinOp, Expr, Warning};
use crate::listing::{DebugCapture, DebugCaptureMulti};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

/// vasm's probe-pinned multi-file semantics (language-surface U6, KTD5;
/// `vasmm68k_mot` 2.0b): relative `include`/`incbin` requests anchor at the
/// **root input's directory** for every request, however deep the requester —
/// vasm searches its process cwd first and then the main source's directory,
/// never the *including* file's directory (a copy next to a nested include is
/// not found). Our input's directory stands in for the cwd, the documented
/// [`FsLoader`](crate::source::FsLoader) stance, then the `-I` dirs — the same
/// mapping rgbasm's probe pinned. The incbin window is [`vasm_incbin_window`];
/// there is no include extension defaulting.
pub(crate) const VASM_SEMANTICS: WalkSemantics = WalkSemantics {
    resolution: Resolution::Root,
    window: vasm_incbin_window,
    include_default_ext: None,
};

/// Apply vasm's `incbin "file"[,offset[,length]]` window — probe-pinned
/// (`vasmm68k_mot` 2.0b): a negative offset or an offset past EOF is an error
/// ("bad file-offset argument"); offset at EOF is legal and empty; a length
/// that is omitted, **zero, or negative** means the rest of the file (zero is
/// vasm's unspecified sentinel, unlike ca65's negative-only one); a length
/// past the remaining bytes **silently truncates** to what remains (vasm
/// exits 0 with no warning — mirrored, so the bytes stay identical).
fn vasm_incbin_window(
    data: &[u8],
    offset: Option<i64>,
    size: Option<i64>,
) -> Result<Vec<u8>, String> {
    let len = data.len() as i64;
    let off = offset.unwrap_or(0);
    if off < 0 {
        return Err(format!("offset {off} must not be negative"));
    }
    if off > len {
        return Err(format!(
            "offset {off} is past the end of the {len}-byte file"
        ));
    }
    let remaining = len - off;
    let take = match size {
        None => remaining,
        Some(s) if s <= 0 => remaining,
        Some(s) => s.min(remaining),
    };
    Ok(data[off as usize..(off + take) as usize].to_vec())
}

/// Evaluate an expression against bound symbols, with `*` (the location
/// counter) resolving to `here`. A thin PC-aware wrapper over the shared
/// [`Expr::eval_with`].
fn eval(e: &Expr, consts: &BTreeMap<String, i64>, here: i64, line: usize) -> Result<i64, AsmError> {
    e.eval_with(&|s| consts.get(s).copied(), Some(here), line)
}

/// The address-register spellings of `add`/`sub`/`cmp`. The spec encodes their
/// forms under the base mnemonic, so these names never reach it — which is why
/// they need naming here: without them, `adda d0,d1` is refused as an *unknown
/// instruction*, when the mnemonic is fine and the operands are not.
///
/// vasm draws the same line: `zzqq` is "unknown mnemonic", while `adda d0,d1`
/// is "instruction not supported on selected architecture". Telling a reader
/// their mnemonic does not exist, when the reference knows it, is the mistake
/// `Category::KnownUnsupported` exists to avoid one level up.
const ADDRESS_ALIASES: &[&str] = &["ADDA", "SUBA", "CMPA"];

/// The right refusal for a mnemonic the spec has no entry for: an alias that
/// survived lowering is a *known* mnemonic used with operands it has no form
/// for, and saying otherwise tells the reader their source is invalid when it
/// is the operands that are wrong.
fn unknown_or_unmatched(mnemonic: &str, line: usize) -> AsmError {
    if ADDRESS_ALIASES.contains(&mnemonic) {
        AsmError::new(line, format!("`{mnemonic}` has no form for those operands"))
    } else {
        AsmError::new(line, format!("unknown instruction `{mnemonic}`"))
    }
}

/// Apply vasm's instruction-rewriting optimizations, returning the effective
/// mnemonic and operands. Both rest only on the (constant) immediate, so they
/// stay stable across relaxation rounds.
///
/// - `add`/`sub` of a small immediate (1..=8) → the quick form `addq`/`subq`.
/// - `add.l`/`sub.l` of a 16-bit immediate into an address register →
///   `lea d16(An),An`, two bytes shorter than `adda.l #imm`.
/// - `lea d8(An),An` with a small offset → `addq`/`subq` (the reverse), which
///   is shorter still. The `Option<Size>` is a size override for that case
///   (`lea` is sizeless but the `addq` it becomes is a long).
fn lower<'a>(
    mnemonic: &'a str,
    size: Option<Size>,
    operands: &'a [Opnd],
    ctx: &Ctx,
    consts: &BTreeMap<String, i64>,
) -> (&'a str, Cow<'a, [Opnd]>, Option<Size>) {
    // adda/suba/cmpa are the address-register-destination spellings of add/sub/
    // cmp; the spec encodes those forms under the base mnemonic (form selection
    // picks the An-destination form). Alias them when the destination is an An,
    // as vasm requires — `adda d0,d1` stays unknown, matching vasm's rejection.
    let mnemonic = match (mnemonic, operands.last()) {
        ("ADDA", Some(Opnd::AReg(_))) => "ADD",
        ("SUBA", Some(Opnd::AReg(_))) => "SUB",
        ("CMPA", Some(Opnd::AReg(_))) => "CMP",
        _ => mnemonic,
    };
    // eor #imm,<ea> is always eori: unlike and/or (whose source can be an
    // immediate EA), eor's source is always a data register, so there is no
    // eor-immediate encoding. Holds for a Dn *or* memory destination (not An),
    // and regardless of optimization — it is a requirement, not an optimization.
    if let [Opnd::Imm(_), dest] = operands
        && mnemonic == "EOR"
        && !matches!(dest, Opnd::AReg(_) | Opnd::Imm(_))
    {
        return ("EORI", Cow::Borrowed(operands), None);
    }
    if !ctx.optimize {
        return (mnemonic, Cow::Borrowed(operands), None);
    }
    // lea d(An),An with a small offset → addq/subq #d,An (a long).
    if mnemonic == "LEA"
        && let [
            Opnd::Mem {
                mode: 5,
                reg,
                disp: Some(e),
                ..
            },
            Opnd::AReg(n),
        ] = operands
        && reg == n
        && let Ok(v) = eval(e, consts, 0, 0)
    {
        if (1..=8).contains(&v) {
            let ops = vec![Opnd::Imm(e.clone()), Opnd::AReg(*n)];
            return ("ADDQ", Cow::Owned(ops), Some(Size::L));
        }
        if (-8..=-1).contains(&v) {
            let ops = vec![Opnd::Imm(Expr::Neg(Box::new(e.clone()))), Opnd::AReg(*n)];
            return ("SUBQ", Cow::Owned(ops), Some(Size::L));
        }
    }
    if let [Opnd::Imm(e), dest] = operands
        && let Ok(v) = eval(e, consts, 0, 0)
    {
        // cmp #0,<ea> → tst <ea> (comparing against zero is a test; drops the
        // immediate word). Not for An, which tst can't address on the 68000.
        if mnemonic == "CMP" && v == 0 && !matches!(dest, Opnd::AReg(_) | Opnd::Imm(_)) {
            return ("TST", Cow::Owned(vec![dest.clone()]), None);
        }
        // add/sub of 1..=8 → the quick form.
        if (1..=8).contains(&v) && !matches!(dest, Opnd::Imm(_)) {
            match mnemonic {
                "ADD" => return ("ADDQ", Cow::Borrowed(operands), None),
                "SUB" => return ("SUBQ", Cow::Borrowed(operands), None),
                _ => {}
            }
        }
        // add/sub #d16,An → lea d16(An),An (subtraction negates the offset).
        // vasm prefers `lea` whenever the offset fits a word — always shorter
        // than `adda.l`, and the same size as (but preferred over) `adda.w`.
        if !matches!(size, Some(Size::B))
            && let Opnd::AReg(n) = dest
            && let Some(disp_expr) = match mnemonic {
                "ADD" => Some(e.clone()),
                "SUB" => Some(Expr::Neg(Box::new(e.clone()))),
                _ => None,
            }
            && i16::try_from(if mnemonic == "SUB" { -v } else { v }).is_ok()
        {
            let mem = Opnd::Mem {
                mode: 5,
                reg: *n,
                bit: ea::DI,
                disp: Some(disp_expr),
            };
            return ("LEA", Cow::Owned(vec![mem, Opnd::AReg(*n)]), None);
        }
    }
    // add/sub/cmp #imm,<memory> → the immediate-form instruction (addi/subi/
    // cmpi). vasm uses the shorter `<ea>,Dn` form with an immediate EA for a Dn
    // destination (handled by normal form selection), and adda/lea for An, so
    // this alias only fires for a genuine memory destination. (No `eval` needed —
    // it is a structural rewrite, so it also covers a forward immediate.)
    if let [Opnd::Imm(_), dest] = operands
        && !matches!(dest, Opnd::DReg(_) | Opnd::AReg(_) | Opnd::Imm(_))
    {
        match mnemonic {
            "ADD" => return ("ADDI", Cow::Borrowed(operands), None),
            "SUB" => return ("SUBI", Cow::Borrowed(operands), None),
            "CMP" => return ("CMPI", Cow::Borrowed(operands), None),
            // and/or of an immediate into memory: no Dn operand, so the
            // immediate form. (For a Dn destination the immediate is a valid
            // source EA — the plain and/or form — so this excludes it.)
            "AND" => return ("ANDI", Cow::Borrowed(operands), None),
            "OR" => return ("ORI", Cow::Borrowed(operands), None),
            _ => {}
        }
    }
    (mnemonic, Cow::Borrowed(operands), None)
}

/// Whether a `d16(An)` operand (mode 5) has a displacement that resolves to
/// zero — which the optimizer drops, shortening it to plain `(An)` (mode 2).
fn drops_zero_disp(
    mode: u8,
    disp: &Option<Expr>,
    ctx: &Ctx,
    consts: &BTreeMap<String, i64>,
) -> bool {
    ctx.optimize
        && mode == 5
        && disp
            .as_ref()
            .is_some_and(|e| eval(e, consts, 0, 0).is_ok_and(|v| v == 0))
}

/// Assemble Motorola-syntax 68000 source into a flat big-endian code image with
/// the optimizer on — matching `vasm -Fbin`'s default (Stage 2).
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure, or
/// if the source uses more than one non-empty section (which `-Fbin` rejects).
pub(crate) fn assemble(source: &str) -> Result<Vec<u8>, AsmError> {
    assemble_with(source, true)
}

/// As [`assemble`], but also returns any non-fatal [`Warning`]s (e.g. an
/// out-of-range immediate to CCR/SR). The bytes are identical to [`assemble`].
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure, or
/// if the source uses more than one non-empty section.
pub(crate) fn assemble_warned(source: &str) -> Result<(Vec<u8>, Vec<Warning>), AsmError> {
    let mut warnings = Vec::new();
    let (sections, _) = assemble_core(
        &parse_program(source, macros::Expand::Yes)?,
        true,
        &mut warnings,
    )?;
    let bytes = flatten_sections(&sections)?;
    Ok((bytes, warnings))
}

/// Assemble a **multi-file** 68000 program to a flat binary (language-surface
/// U6): the root is `map`'s `FileId(0)`, `include`/`incbin` resolve lazily
/// through `loader` under vasm's probe-pinned semantics ([`VASM_SEMANTICS`]),
/// and the capture's line records carry each statement's real file. Optimizer
/// on, matching `vasmm68k_mot -Fbin` exactly as [`assemble_warned`] does.
///
/// # Errors
/// Any per-line parse failure (stamped with its file), a missing target, an
/// include cycle, a bad `incbin` window, or any layout/encode failure.
pub(crate) fn assemble_warned_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<(Vec<u8>, Vec<Warning>, DebugCaptureMulti), AsmError> {
    let mut warnings = Vec::new();
    let (sections, mut capture) =
        assemble_core(&parse_program_multi(map, loader)?, true, &mut warnings)?;
    let bytes = flatten_sections(&sections)?;
    rebase_flat_capture(&sections, &mut capture);
    Ok((bytes, warnings, capture))
}

/// Assemble a **multi-file** 68000 program to an Amiga hunk executable
/// (language-surface U6): as [`assemble_warned_multi`], serialized like
/// [`assemble_exe`] (`-Fhunkexe -kick1hunks`, debug symbol table omitted).
///
/// # Errors
/// As [`assemble_warned_multi`].
pub(crate) fn assemble_exe_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<(Vec<u8>, Vec<Warning>, DebugCaptureMulti), AsmError> {
    let mut warnings = Vec::new();
    let (sections, capture) =
        assemble_core(&parse_program_multi(map, loader)?, true, &mut warnings)?;
    Ok((serialize_hunkexe(&sections), warnings, capture))
}

/// The flat binary *is* the emitted section's bytes, so its offsets are file
/// offsets: base that section at 0 so debug lookups resolve file-relative out
/// of the box (a consumer loading the blob elsewhere overrides via the
/// `BaseMap`, which wins over the recorded base).
fn rebase_flat_capture(sections: &[SecOut], capture: &mut DebugCaptureMulti) {
    if let Some(emitted) = sections.iter().position(|s| !s.bytes.is_empty())
        && let Some(section) = capture
            .sections
            .iter_mut()
            .find(|s| s.id == emitted as debug198x::SectionId)
    {
        section.base = Some(0);
    }
}

/// Assemble with the optimizer either on (Stage 2, matches `vasm -Fbin`) or off
/// (Stage 1, matches `vasm -no-opt`), to a flat binary.
///
/// # Errors
/// Returns an [`AsmError`] on any parse/range/symbol failure, or if more than one
/// section carries bytes (a flat binary can hold only one).
pub(crate) fn assemble_with(source: &str, optimize: bool) -> Result<Vec<u8>, AsmError> {
    let mut warnings = Vec::new();
    let (sections, _) = assemble_core(
        &parse_program(source, macros::Expand::Yes)?,
        optimize,
        &mut warnings,
    )?;
    flatten_sections(&sections)
}

/// Reduce assembled sections to a single flat binary's bytes: empty is empty,
/// one section is its bytes, and more than one is an error (`-Fbin` holds one).
/// Lay the sections into the flat image `-Fbin` writes.
///
/// A flat binary is not limited to one section: `vasmm68k_mot -Fbin` takes
/// several and writes one image over them, refusing only where two *overlap*
/// (`sections <one>:0-1 and <two>:0-1 must not overlap`). Placement is the
/// engine's, so a flat vasm image, a NES ROM and a Game Boy bank are laid by
/// one implementation.
///
/// Every section here is based at 0 today, so any two non-empty ones do
/// overlap and are refused — which is what this did before, for a reason it
/// stated wrongly. Sections stop coinciding once vasm's `org` is implemented,
/// and this needs no change when they do.
fn flatten_sections(sections: &[SecOut]) -> Result<Vec<u8>, AsmError> {
    let runs = sections
        .iter()
        .map(|s| crate::engine::Run {
            name: match s.kind {
                HunkKind::Code => "code",
                HunkKind::Data => "data",
                HunkKind::Bss => "bss",
            }
            .to_string(),
            base: 0,
            at: crate::engine::Place::ByAddress,
            bytes: s.bytes.clone(),
        })
        .collect();
    let (_, image) = crate::engine::lay_out(runs, 0, 1, None, |_| None)?;
    Ok(image)
}

/// Assemble to an Amiga hunk executable (`-Fhunkexe -kick1hunks`), optimizer on.
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble_exe(source: &str) -> Result<Vec<u8>, AsmError> {
    // The hunk-exe path discards warnings for now (the CLI surfaces them on the
    // flat path via `assemble_warned`); the bytes are unaffected either way.
    let mut warnings = Vec::new();
    let (sections, _) = assemble_core(
        &parse_program(source, macros::Expand::Yes)?,
        true,
        &mut warnings,
    )?;
    Ok(serialize_hunkexe(&sections))
}

/// As [`assemble_warned`], also returning the debug read-out (Debug198x U5).
/// Same `assemble_core` call, so the bytes are identical by construction (AE2).
///
/// # Errors
/// As [`assemble_warned`].
pub(crate) fn assemble_warned_with_debug(
    source: &str,
) -> Result<(Vec<u8>, Vec<Warning>, DebugCapture), AsmError> {
    let mut warnings = Vec::new();
    let (sections, mut capture) = assemble_core(
        &parse_program(source, macros::Expand::Yes)?,
        true,
        &mut warnings,
    )?;
    let bytes = flatten_sections(&sections)?;
    rebase_flat_capture(&sections, &mut capture);
    // The single-source API keeps its exact pre-multi-file record shape:
    // every line lives in the root input (U6 adopts `DebugCaptureMulti`
    // internally; this entry collapses it).
    Ok((bytes, warnings, capture.into_single()))
}

/// As [`assemble_exe`], also returning the debug read-out (Debug198x U5).
/// Same `assemble_core` call, so the bytes are identical by construction (AE2).
///
/// # Errors
/// As [`assemble_exe`].
pub(crate) fn assemble_exe_with_debug(source: &str) -> Result<(Vec<u8>, DebugCapture), AsmError> {
    let mut warnings = Vec::new();
    let (sections, capture) = assemble_core(
        &parse_program(source, macros::Expand::Yes)?,
        true,
        &mut warnings,
    )?;
    Ok((serialize_hunkexe(&sections), capture.into_single()))
}

/// A 32-bit relocation: a byte offset within a section, and the target section
/// whose load address gets added to the longword stored there.
type Reloc = (u32, usize);

/// One assembled section: its hunk kind and memory flag, its bytes, and the
/// 32-bit relocations within it.
struct SecOut {
    kind: HunkKind,
    flag: MemFlag,
    bytes: Vec<u8>,
    relocs: Vec<Reloc>,
}

/// Optimization context shared down the encode/size paths.
struct Ctx {
    reloc: BTreeSet<String>,
    /// Section index each relocatable label belongs to, for PC-relative gating
    /// (same-section only) and relocation bucketing.
    sec_of: BTreeMap<String, usize>,
    optimize: bool,
}

/// Assemble a parsed [`Program`](crate::ast::Program) into per-section byte
/// buffers with their relocations — the shared core behind both the flat and
/// the hunk-executable serializers, and behind both the single-source and
/// multi-file entries (one body, so their bytes can never drift). Also returns
/// the debug read-out (Debug198x U5, multi-file since language-surface U6):
/// section table, `(section, offset)` symbols, and per-statement line spans
/// carrying each statement's file, all **section-relative** with no fabricated
/// absolutes (KTD7 — the reader's `BaseMap` owns rebasing to loaded hunk
/// addresses). The capture is strictly passive: it observes emission and never
/// branches on it. Errors and warnings are stamped with the owning statement's
/// file, so a failure inside an included file names that file.
fn assemble_core(
    program: &crate::ast::Program,
    optimize: bool,
    warnings: &mut Vec<Warning>,
) -> Result<(Vec<SecOut>, DebugCaptureMulti), AsmError> {
    // The AST is the single front-end IR: the parse built the source-preserving
    // `Program` (which carries each line's native 68000 statement, qualified);
    // project it to the assembler's statement stream. Same bytes as the old
    // direct parse — see `decisions/ast-native-payload-for-multipass-cisc.md`.
    let stmts = lines_from_program(program)?;

    // Assign every statement to a section. A `section` directive opens one;
    // bytes emitted before any directive fall into an implicit code section.
    let mut sec_meta: Vec<(HunkKind, MemFlag)> = Vec::new();
    let mut sec_idx: Vec<usize> = Vec::with_capacity(stmts.len());
    let mut cur: Option<usize> = None;
    for s in &stmts {
        if let Stmt::Section(kind, flag) = &s.kind {
            sec_meta.push((*kind, *flag));
            cur = Some(sec_meta.len() - 1);
        } else if cur.is_none() && stmt_emits(&s.kind) {
            sec_meta.push((HunkKind::Code, MemFlag::Any));
            cur = Some(0);
        }
        sec_idx.push(cur.unwrap_or(0));
    }
    if sec_meta.is_empty() {
        sec_meta.push((HunkKind::Code, MemFlag::Any));
    }
    let nsec = sec_meta.len();

    check_redefinitions(&stmts)?;

    // Relocatable symbols and the section each lives in.
    let mut reloc: BTreeSet<String> = BTreeSet::new();
    let mut sec_of: BTreeMap<String, usize> = BTreeMap::new();
    for (i, s) in stmts.iter().enumerate() {
        if !matches!(s.kind, Stmt::Equ(..))
            && let Some(label) = &s.label
        {
            reloc.insert(label.clone());
            sec_of.insert(label.clone(), sec_idx[i]);
        }
    }
    let ctx = Ctx {
        reloc,
        sec_of,
        optimize,
    };

    // Branch relaxation: relaxable branches start short and grow to word form
    // when their (intra-section) displacement won't fit a byte. Grow-only, so it
    // converges. Grows are deferred to a clone so the running per-section pc
    // tracks `consts` within a round.
    let mut word_branch = vec![false; stmts.len()];
    loop {
        let (consts, _) = layout(&stmts, &sec_idx, nsec, &ctx, &word_branch)?;
        let mut next = word_branch.clone();
        let mut pc = vec![0i64; nsec];
        for (i, s) in stmts.iter().enumerate() {
            if matches!(s.kind, Stmt::Equ(..) | Stmt::Section(..)) {
                continue;
            }
            let sec = sec_idx[i];
            if let Some((pad, _)) = s
                .kind
                .pad_at(pc[sec], &consts, s.line)
                .map_err(|e| stamp(e, s))?
            {
                pc[sec] += pad;
            }
            if ctx.optimize
                && !word_branch[i]
                && let Some(target) = relaxable_branch_target(&s.kind)
            {
                let disp = eval(target, &consts, pc[sec], s.line).map_err(|e| stamp(e, s))?
                    - (pc[sec] + 2);
                if disp == 0 || i8::try_from(disp).is_err() {
                    next[i] = true;
                }
            }
            pc[sec] += stmt_size(&s.kind, &ctx, &consts, sec, word_branch[i], s.line)
                .map_err(|e| stamp(e, s))? as i64;
        }
        if next == word_branch {
            break;
        }
        word_branch = next;
    }

    // Emit each section's bytes and relocations.
    let (consts, _) = layout(&stmts, &sec_idx, nsec, &ctx, &word_branch)?;
    let mut out: Vec<SecOut> = sec_meta
        .iter()
        .map(|&(kind, flag)| SecOut {
            kind,
            flag,
            bytes: Vec::new(),
            relocs: Vec::new(),
        })
        .collect();
    let mut dbg_lines: Vec<(FileId, u32, debug198x::SectionId, u64, u64)> = Vec::new();
    for (i, s) in stmts.iter().enumerate() {
        // Pass-2 errors and warnings are stamped with the statement's file
        // (U6), so a failure inside an included file names that file.
        let stamp = |e: AsmError| stamp(e, s);
        let sec = sec_idx[i];
        let buf = &mut out[sec];
        if let Some((pad, fill)) = s
            .kind
            .pad_at(buf.bytes.len() as i64, &consts, s.line)
            .map_err(stamp)?
        {
            buf.bytes.extend(fill.bytes(pad));
        }
        // Span start is measured *after* the align pad: the hidden pad bytes
        // are fill, not this statement's emission (the padding rule — no span).
        let span_start = buf.bytes.len();
        match &s.kind {
            Stmt::Empty
            | Stmt::Equ(..)
            | Stmt::Even
            | Stmt::Section(..)
            | Stmt::Align(_)
            | Stmt::Cnop(..) => {}
            Stmt::Echo(text) => warnings.push(crate::engine::Warning {
                line: s.line,
                file: s.file,
                message: text.clone(),
                kind: crate::engine::WarningKind::Note,
            }),
            Stmt::Fail(message) => return Err(stamp(AsmError::new(s.line, message.clone()))),
            Stmt::Assert(cond, message) => {
                if eval(cond, &consts, buf.bytes.len() as i64, s.line).map_err(stamp)? == 0 {
                    return Err(stamp(AsmError::new(
                        s.line,
                        format!("assertion failed: {message}"),
                    )));
                }
            }
            Stmt::Raw(payload) => buf.bytes.extend_from_slice(payload),
            Stmt::Dc(size, items) => {
                for e in items {
                    // A longword `dc.l <label>` stores the label's
                    // section-relative offset and needs a RELOC32 so the loader
                    // fixes it up — the same as an absolute address in an
                    // instruction. Data tables of pointers into another hunk
                    // (e.g. flock's `vehtab: dc.l tractx …`) rely on this.
                    if size.bytes() == 4
                        && let Some(target) =
                            reloc_sym(e, &ctx.reloc).and_then(|s| ctx.sec_of.get(s).copied())
                    {
                        buf.relocs.push((buf.bytes.len() as u32, target));
                    }
                    push_sized(
                        &mut buf.bytes,
                        eval(e, &consts, 0, s.line).map_err(stamp)?,
                        *size,
                    );
                }
            }
            Stmt::Ds(size, count) => {
                let n = count_of(count, &consts, s.line).map_err(stamp)?;
                buf.bytes.resize(buf.bytes.len() + n * size.bytes(), 0);
            }
            Stmt::Dcb(size, count, value) => {
                let n = count_of(count, &consts, s.line).map_err(stamp)?;
                let v = eval(value, &consts, 0, s.line).map_err(stamp)?;
                for _ in 0..n {
                    push_sized(&mut buf.bytes, v, *size);
                }
            }
            Stmt::Insn {
                mnemonic,
                size,
                operands,
            } => {
                let here = buf.bytes.len() as i64;
                let size = branch_size(ctx.optimize, &s.kind, size, word_branch[i]);
                let before = warnings.len();
                let (bytes, relocs) = encode(
                    mnemonic, size, operands, &ctx, &consts, sec, here, s.line, warnings,
                )
                .map_err(stamp)?;
                for w in &mut warnings[before..] {
                    w.file = s.file;
                }
                buf.bytes.extend_from_slice(&bytes);
                buf.relocs.extend(relocs);
            }
        }
        let emitted = out[sec].bytes.len() - span_start;
        if emitted > 0 {
            dbg_lines.push((
                s.file,
                s.line as u32,
                sec as debug198x::SectionId,
                span_start as u64,
                emitted as u64,
            ));
        }
    }

    // The debug read-out's symbols: every label at its `(section, offset)`
    // placement (the final layout's value is the section-relative offset,
    // aligns included), and every `equ` as a constant.
    let mut dbg_symbols: Vec<debug198x::Symbol> = Vec::new();
    for (i, s) in stmts.iter().enumerate() {
        let Some(label) = &s.label else { continue };
        let Some(value) = consts.get(label) else {
            continue;
        };
        let kind = if matches!(s.kind, Stmt::Equ(..)) {
            debug198x::SymbolKind::Const {
                value: *value as u64,
            }
        } else {
            debug198x::SymbolKind::Label {
                section: sec_idx[i] as debug198x::SectionId,
                offset: *value as u64,
                space: None,
            }
        };
        dbg_symbols.push(debug198x::Symbol {
            name: label.clone(),
            kind,
        });
    }
    // The section table: hunk kind as the name, `base: None` throughout —
    // hunks are relocatable, so offsets stay section-relative and a consumer
    // (the Emu198x importer) supplies actual load addresses via a `BaseMap`.
    let dbg_sections: Vec<debug198x::Section> = sec_meta
        .iter()
        .enumerate()
        .map(|(id, (kind, _))| debug198x::Section {
            id: id as debug198x::SectionId,
            name: match kind {
                HunkKind::Code => "code".to_string(),
                HunkKind::Data => "data".to_string(),
                HunkKind::Bss => "bss".to_string(),
            },
            base: None,
            space: None,
        })
        .collect();

    Ok((
        out,
        DebugCaptureMulti {
            sections: dbg_sections,
            symbols: dbg_symbols,
            lines: dbg_lines,
        },
    ))
}

/// Serialize assembled sections into an AmigaDOS hunk executable, matching
/// `vasmm68k_mot -Fhunkexe -kick1hunks` for everything the loader consumes
/// (header, code/data/bss hunks, reloc32 tables). The optional HUNK_SYMBOL
/// table vasm also writes is debug-only and omitted — see the Stage 3 decision.
fn serialize_hunkexe(sections: &[SecOut]) -> Vec<u8> {
    fn push_u32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    // Each hunk's size in longwords: code/data padded to a longword, bss rounded.
    let size_longs = |s: &SecOut| -> u32 { s.bytes.len().div_ceil(4) as u32 };

    let mut out = Vec::new();
    // HUNK_HEADER: no resident libraries, then hunk count, first, last, sizes.
    push_u32(&mut out, 0x3f3);
    push_u32(&mut out, 0);
    push_u32(&mut out, sections.len() as u32);
    push_u32(&mut out, 0);
    push_u32(&mut out, sections.len() as u32 - 1);
    for s in sections {
        push_u32(&mut out, size_longs(s) | s.flag.bits());
    }

    for s in sections {
        match s.kind {
            HunkKind::Bss => {
                push_u32(&mut out, 0x3eb);
                push_u32(&mut out, size_longs(s));
            }
            HunkKind::Code | HunkKind::Data => {
                push_u32(
                    &mut out,
                    if matches!(s.kind, HunkKind::Code) {
                        0x3e9
                    } else {
                        0x3ea
                    },
                );
                push_u32(&mut out, size_longs(s));
                let mut data = s.bytes.clone();
                // Pad to a longword. A code hunk two bytes short takes a NOP
                // (0x4e71) — a whole instruction word, which is the only thing
                // a NOP can be; one or three bytes short there is no room for
                // one, and vasm pads with zeros instead. The decision is made
                // once from the length as it stands: padding a 17-byte code
                // hunk to 18 and *then* calling it two short produces
                // `00 4e 71` where vasm writes three zeros.
                let short = (4 - data.len() % 4) % 4;
                if matches!(s.kind, HunkKind::Code) && short == 2 {
                    data.extend_from_slice(&[0x4e, 0x71]);
                } else {
                    data.extend(std::iter::repeat_n(0u8, short));
                }
                out.extend_from_slice(&data);
            }
        }

        // HUNK_RELOC32: blocks of [count, target hunk, offsets…], target hunks
        // ascending, offsets in the order the assembler recorded them,
        // terminated by a zero count.
        //
        // Not sorted. vasm emits each block in discovery order, which is
        // usually ascending and is not guaranteed to be: an expression resolved
        // after the position it patches puts a lower offset later in the list.
        // Sorting matched vasm everywhere that happened not to occur and
        // diverged the moment it did — flock units 15-18 carry the same 142
        // entries as vasm in a different order. A loader applies them all
        // whatever the order, so this is invisible at runtime and fatal to
        // byte-identity, which is the claim being made.
        if !s.relocs.is_empty() {
            push_u32(&mut out, 0x3ec);
            for target in 0..sections.len() {
                let offs: Vec<u32> = s
                    .relocs
                    .iter()
                    .filter(|(_, t)| *t == target)
                    .map(|(o, _)| *o)
                    .collect();
                if offs.is_empty() {
                    continue;
                }
                push_u32(&mut out, offs.len() as u32);
                push_u32(&mut out, target as u32);
                for o in offs {
                    push_u32(&mut out, o);
                }
            }
            push_u32(&mut out, 0);
        }

        push_u32(&mut out, 0x3f2); // HUNK_END
    }
    out
}

/// The single relocatable symbol of a degree-1 address expression (`label`,
/// `label+n`, `label-n`) — the target whose hunk a relocation points into.
fn reloc_sym<'a>(e: &'a Expr, reloc: &BTreeSet<String>) -> Option<&'a str> {
    match e {
        Expr::Sym(s) if reloc.contains(s) => Some(s),
        Expr::Bin(BinOp::Add, l, r) => reloc_sym(l, reloc).or_else(|| reloc_sym(r, reloc)),
        Expr::Bin(BinOp::Sub, l, _) => reloc_sym(l, reloc),
        _ => None,
    }
}

/// Whether a statement contributes bytes to its section (so it forces an
/// implicit code section when none has been opened yet).
fn stmt_emits(kind: &Stmt) -> bool {
    matches!(
        kind,
        Stmt::Insn { .. } | Stmt::Dc(..) | Stmt::Ds(..) | Stmt::Dcb(..) | Stmt::Even | Stmt::Raw(_)
    )
}

/// Refuse a name defined twice (#126).
///
/// vasm has no rebindable form here — no `set` — so every label and every
/// `equ` is a definition, and a repeat is an error however the two are
/// spelled: label then label is *"label <x> redefined"*, and anything
/// involving an `equ` is *"repeatedly defined symbol <x>"*. Both messages are
/// carried through, because which one a reader gets tells them which
/// definition the reference thought was the odd one.
///
/// This is its own pass rather than a check inside [`layout`], which re-runs
/// per relaxation round and would report the same collision once a round.
///
/// The direction matters. Every other gap closed for the 1.0 bar was source
/// the reference takes and we refused; this is the reverse, and worse for it —
/// an accidental collision got a working binary from us and a build failure
/// from vasm.
fn check_redefinitions(stmts: &[Line]) -> Result<(), AsmError> {
    let mut seen: BTreeMap<&str, bool> = BTreeMap::new();
    for s in stmts {
        let (name, is_equ) = match &s.kind {
            Stmt::Equ(name, _) => (name.as_str(), true),
            _ => match &s.label {
                Some(label) => (label.as_str(), false),
                None => continue,
            },
        };
        match seen.insert(name, is_equ) {
            // Two plain labels: vasm calls this a redefined *label*.
            Some(false) if !is_equ => {
                return Err(stamp(
                    AsmError::new(s.line, format!("label `{name}` redefined")),
                    s,
                ));
            }
            // An `equ` on either side: a repeatedly defined *symbol*.
            Some(_) => {
                return Err(stamp(
                    AsmError::new(s.line, format!("repeatedly defined symbol `{name}`")),
                    s,
                ));
            }
            None => {}
        }
    }
    Ok(())
}

/// Walk every statement and bind labels and `equ` constants to their
/// offset-within-section, given the current branch-size decisions. Returns the
/// symbol→offset map and each section's total byte length. Re-run per relaxation
/// round.
fn layout(
    stmts: &[Line],
    sec_idx: &[usize],
    nsec: usize,
    ctx: &Ctx,
    word_branch: &[bool],
) -> Result<(BTreeMap<String, i64>, Vec<i64>), AsmError> {
    let mut consts: BTreeMap<String, i64> = BTreeMap::new();
    let mut pc = vec![0i64; nsec];
    for (i, s) in stmts.iter().enumerate() {
        if let Stmt::Equ(name, e) = &s.kind {
            // PC-aware: `len equ *-buffer` resolves `*` to the current location.
            if let Ok(v) = eval(e, &consts, pc[sec_idx[i]], s.line) {
                consts.insert(name.clone(), v);
            }
            continue;
        }
        let sec = sec_idx[i];
        if let Some((pad, _)) = s
            .kind
            .pad_at(pc[sec], &consts, s.line)
            .map_err(|e| stamp(e, s))?
        {
            pc[sec] += pad;
        }
        if let Some(label) = &s.label {
            consts.insert(label.clone(), pc[sec]);
        }
        pc[sec] += stmt_size(&s.kind, ctx, &consts, sec, word_branch[i], s.line)
            .map_err(|e| stamp(e, s))? as i64;
    }
    Ok((consts, pc))
}

/// The effective size of a statement's branch. With the optimizer on, a
/// relaxable branch (including a bare `bra`/`bsr`/`bcc`, which vasm shortens) is
/// short by default and word once grown. With it off, every branch keeps its
/// written size — a bare branch stays word, matching `-no-opt`.
fn branch_size(optimize: bool, kind: &Stmt, written: &Option<Size>, grown: bool) -> Option<Size> {
    if optimize && relaxable_branch_target(kind).is_some() {
        if grown { Some(Size::W) } else { Some(Size::B) }
    } else {
        *written
    }
}

/// The branch-target expression of a relaxable branch (BRA/BSR/Bcc, not forced
/// to `.w`), or `None` if the statement isn't one. DBcc has no short form, so it
/// never relaxes.
fn relaxable_branch_target(kind: &Stmt) -> Option<&Expr> {
    let Stmt::Insn {
        mnemonic,
        size,
        operands,
    } = kind
    else {
        return None;
    };
    if matches!(size, Some(Size::W | Size::L)) {
        return None; // explicitly forced to word/long
    }
    let insn = m68k::SET.instruction(mnemonic)?;
    let form = match_form(insn, operands)?;
    if !form.operands.iter().any(|s| matches!(s, Slot::BranchW)) {
        return None;
    }
    operands.iter().find_map(|o| match o {
        Opnd::Abs(e) => Some(e),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn encode(
    mnemonic: &str,
    size: Option<Size>,
    operands: &[Opnd],
    ctx: &Ctx,
    consts: &BTreeMap<String, i64>,
    cur_sec: usize,
    here: i64,
    line: usize,
    warnings: &mut Vec<Warning>,
) -> Result<(Vec<u8>, Vec<Reloc>), AsmError> {
    let (mnemonic, operands, size_override) = lower(mnemonic, size, operands, ctx, consts);
    let operands = operands.as_ref();
    let size = size_override.or(size);
    let insn = m68k::SET
        .instruction(mnemonic)
        .ok_or_else(|| unknown_or_unmatched(mnemonic, line))?;
    let sz = size.unwrap_or(Size::W);
    let form = match_form(insn, operands).ok_or_else(|| {
        AsmError::new(line, format!("`{mnemonic}` has no form for those operands"))
    })?;

    let mut word = form.base | size_bits(form.size, sz);
    let mut ext: Vec<u8> = Vec::new();
    let mut relocs: Vec<Reloc> = Vec::new();
    // Parallel to `relocs`: whether each came from the destination operand.
    // Used only to order them; see the reordering below.
    let mut reloc_is_dest: Vec<bool> = Vec::new();
    let mut branch: Option<i64> = None;
    // MOVEM reverses its register mask when the effective address predecrements.
    let predec = operands
        .iter()
        .any(|o| matches!(o, Opnd::Mem { mode: 4, .. }));

    for (slot, op) in form.operands.iter().zip(operands) {
        match (slot, op) {
            (Slot::Dn { shift }, Opnd::DReg(n)) => word |= u16::from(*n) << shift,
            (Slot::An { shift }, Opnd::AReg(n)) => word |= u16::from(*n) << shift,
            (Slot::AddrIndirect { shift, .. }, Opnd::Mem { reg, .. }) => {
                word |= u16::from(*reg) << shift;
            }
            (Slot::Quick8, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                // `moveq` holds a signed byte; vasm warns (not errors) beyond a
                // byte and keeps the low 8 bits.
                if !(-128..=255).contains(&v) {
                    warnings.push(Warning::new(line, "immediate operand out of range"));
                }
                word |= u16::from(v as u8);
            }
            // Fixed control-register tokens carry no opcode bits or extension.
            (Slot::Ccr, Opnd::Ccr) | (Slot::Sr, Opnd::Sr) | (Slot::Usp, Opnd::Usp) => {}
            (
                Slot::MovepDisp,
                Opnd::Mem {
                    reg, disp: Some(e), ..
                },
            ) => {
                word |= u16::from(*reg);
                let v = eval(e, consts, here, line)?;
                ext.extend_from_slice(&(v as i16).to_be_bytes());
            }
            (Slot::Vec4, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                // A trap vector is 4 bits (0..15); vasm warns (not errors) beyond
                // and lets the value spill into the low byte of the opcode word.
                if !(0..=15).contains(&v) {
                    warnings.push(Warning::new(line, "immediate operand out of range"));
                }
                word |= (v as u16) & 0xFF;
            }
            (Slot::Quick3 { shift }, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                // `addq`/`subq` count is 1..8 (8 encodes as 000); vasm warns (not
                // errors) outside that and keeps the low 3 bits.
                if !(1..=8).contains(&v) {
                    warnings.push(Warning::new(line, "immediate operand out of range"));
                }
                word |= u16::from((v & 7) as u8) << shift;
            }
            (Slot::ImmWord, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                // `#imm,CCR` (byte) / `#imm,SR` (word) carry the operand in a
                // word extension. An out-of-range immediate isn't fatal — vasm
                // emits the low half and warns (2037). Mirror that advisory; the
                // bytes stay byte-identical either way.
                let targets_ccr = form.operands.iter().any(|s| matches!(s, Slot::Ccr));
                let targets_sr = form.operands.iter().any(|s| matches!(s, Slot::Sr));
                if (targets_ccr && !(-128..=255).contains(&v))
                    || (targets_sr && !(-32768..=65535).contains(&v))
                {
                    warnings.push(Warning::new(line, "immediate operand out of range"));
                }
                ext.extend_from_slice(&(v as u16).to_be_bytes());
            }
            (Slot::ImmSized, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                // An out-of-range immediate isn't fatal in vasm — it warns and
                // keeps the low `size` bytes. A byte immediate normally rides
                // zero-extended in one word (`#-1` -> `00ff`); when it overflows
                // a byte, vasm keeps the raw low word instead (`#$1234` -> `1234`).
                let (lo, hi): (i64, i64) = match sz {
                    Size::B => (-128, 255),
                    Size::W => (-32768, 65535),
                    Size::L => (i64::from(i32::MIN), i64::from(u32::MAX)),
                };
                let in_range = (lo..=hi).contains(&v);
                if !in_range {
                    warnings.push(Warning::new(line, "immediate operand out of range"));
                }
                match sz {
                    Size::B if in_range => ext.extend_from_slice(&[0, v as u8]),
                    Size::B => ext.extend_from_slice(&(v as u16).to_be_bytes()),
                    Size::W => ext.extend_from_slice(&(v as u16).to_be_bytes()),
                    Size::L => ext.extend_from_slice(&(v as u32).to_be_bytes()),
                }
            }
            (Slot::RegList, _) => {
                let mask = reglist_mask(op);
                let mask = if predec { mask.reverse_bits() } else { mask };
                ext.extend_from_slice(&mask.to_be_bytes());
            }
            (Slot::Ea { shift, dest, modes }, _) => {
                // PC-relative displacement, when chosen, is measured from this
                // operand's own extension word (after the opcode and any prior
                // operand's extension words).
                let pc_ext = here + 2 + ext.len() as i64;
                let (field6, words, reloc) = resolve_ea(
                    op, sz, *dest, *modes, ctx, consts, cur_sec, pc_ext, here, line, warnings,
                )?;
                word |= field6 << shift;
                if let Some(target_sec) = reloc {
                    relocs.push((pc_ext as u32, target_sec));
                    reloc_is_dest.push(*dest);
                }
                ext.extend_from_slice(&words);
            }
            (Slot::BranchW | Slot::DispW, Opnd::Abs(e)) => {
                branch = Some(eval(e, consts, here, line)?)
            }
            // `match_form` guarantees shapes fit, so other pairings can't occur.
            _ => return Err(AsmError::new(line, "internal: operand/slot mismatch")),
        }
    }

    // vasm records an instruction's relocations destination-first, not in the
    // order its extension words are laid out. `move.w abs,abs` puts the source
    // address at the lower offset and the destination at the higher one, and
    // vasm still lists the destination first — so the RELOC32 table is not
    // ascending, and sorting it or emitting in slot order both diverge.
    //
    // Invisible at runtime, since a loader applies every entry whatever the
    // order, and fatal to byte-identity, which is the claim. Found by flock
    // units 15-18, the first curriculum source to use the two-absolute form.
    if reloc_is_dest.iter().any(|d| *d) {
        let mut ordered: Vec<Reloc> = Vec::with_capacity(relocs.len());
        for want_dest in [true, false] {
            for (r, is_dest) in relocs.iter().zip(&reloc_is_dest) {
                if *is_dest == want_dest {
                    ordered.push(*r);
                }
            }
        }
        relocs = ordered;
    }

    if let Some(target) = branch {
        // 68000 branch displacement is relative to PC after the opcode word.
        let disp = target - (here + 2);
        // `.s`/`.b` selects the short form: an 8-bit displacement packed into
        // the opcode word's low byte, no extension word. Anything else (`.w` or
        // a bare branch under `-no-opt`) is the 16-bit word form.
        if matches!(size, Some(Size::B)) {
            let d = i8::try_from(disp).map_err(|_| {
                AsmError::new(line, format!("short branch out of range ({disp} bytes)"))
            })?;
            if d == 0 {
                // A zero low byte is the word-form marker; vasm rejects `.s` here.
                return Err(AsmError::new(
                    line,
                    "short branch to the next instruction is not encodable",
                ));
            }
            word |= u16::from(d as u8);
            return Ok((word.to_be_bytes().to_vec(), relocs));
        }
        let d = i16::try_from(disp)
            .map_err(|_| AsmError::new(line, format!("branch out of range ({disp} bytes)")))?;
        let mut out = word.to_be_bytes().to_vec();
        out.extend_from_slice(&d.to_be_bytes());
        return Ok((out, relocs));
    }

    let mut out = word.to_be_bytes().to_vec();
    out.extend_from_slice(&ext);
    Ok((out, relocs))
}

/// The first form whose slots accept these operand shapes (shape + EA-mode
/// match only — no value evaluation), shared by sizing and encoding so the two
/// passes never disagree.
fn match_form<'a>(insn: &'a m68k::Insn, operands: &[Opnd]) -> Option<&'a m68k::Form> {
    insn.forms.iter().find(|f| {
        f.operands.len() == operands.len()
            && f.operands
                .iter()
                .zip(operands)
                .all(|(slot, op)| slot_accepts(slot, op))
    })
}

fn slot_accepts(slot: &Slot, op: &Opnd) -> bool {
    match (slot, op) {
        (Slot::Dn { .. }, Opnd::DReg(_)) => true,
        (Slot::An { .. }, Opnd::AReg(_)) => true,
        // A fixed indirect mode (`-(An)` or `(An)+`) named without displacement.
        (
            Slot::AddrIndirect { mode, .. },
            Opnd::Mem {
                mode: m,
                disp: None,
                ..
            },
        ) => *mode == *m,
        (
            Slot::Quick8 | Slot::Quick3 { .. } | Slot::ImmWord | Slot::ImmSized | Slot::Vec4,
            Opnd::Imm(_),
        ) => true,
        (Slot::BranchW | Slot::DispW, Opnd::Abs(_)) => true,
        (Slot::Ccr, Opnd::Ccr) | (Slot::Sr, Opnd::Sr) | (Slot::Usp, Opnd::Usp) => true,
        // MOVEP's `d16(Ay)`: displacement-indirect (mode 5) with the displacement
        // present (it is mandatory and never dropped to `(An)`).
        (
            Slot::MovepDisp,
            Opnd::Mem {
                mode: 5,
                disp: Some(_),
                ..
            },
        ) => true,
        // A register list, or a single register treated as a one-entry list.
        (Slot::RegList, Opnd::RegList(_) | Opnd::DReg(_) | Opnd::AReg(_)) => true,
        (Slot::Ea { modes, .. }, _) => modes.allows(ea_mode_bit(op)),
        _ => false,
    }
}

/// The `ea::` mask bit an operand presents (for the allowed-mode check).
fn ea_mode_bit(op: &Opnd) -> u16 {
    match op {
        Opnd::DReg(_) => ea::DN,
        Opnd::AReg(_) => ea::AN,
        Opnd::Mem { bit, .. } => *bit,
        Opnd::Idx { bit, .. } => *bit,
        Opnd::Abs(_) => ea::AL | ea::AW,
        Opnd::AbsW(_) => ea::AW,
        Opnd::AbsL(_) => ea::AL,
        Opnd::Imm(_) => ea::IMM,
        // Not effective addresses: never accepted by an EA slot.
        Opnd::RegList(_) | Opnd::Ccr | Opnd::Sr | Opnd::Usp => 0,
    }
}

/// Resolve an operand used as an effective address: its 6-bit field (in normal
/// or MOVE-destination layout), its extension-word bytes, and — if those bytes
/// are a 32-bit relocatable absolute address — the target section to relocate
/// into.
#[allow(clippy::too_many_arguments)]
fn resolve_ea(
    op: &Opnd,
    sz: Size,
    dest: bool,
    modes: EaModes,
    ctx: &Ctx,
    consts: &BTreeMap<String, i64>,
    cur_sec: usize,
    pc_ext: i64,
    here: i64,
    line: usize,
    warnings: &mut Vec<Warning>,
) -> Result<(u16, Vec<u8>, Option<usize>), AsmError> {
    let field = |mode: u16, reg: u16| {
        if dest {
            (reg << 3) | mode
        } else {
            (mode << 3) | reg
        }
    };
    Ok(match op {
        Opnd::DReg(n) => (field(0, u16::from(*n)), vec![], None),
        Opnd::AReg(n) => (field(1, u16::from(*n)), vec![], None),
        Opnd::Mem {
            mode, reg, disp, ..
        } => {
            // vasm drops a zero `d16(An)` displacement, shortening it to `(An)`.
            if drops_zero_disp(*mode, disp, ctx, consts) {
                return Ok((field(2, u16::from(*reg)), vec![], None));
            }
            let mut ext = Vec::new();
            if let Some(e) = disp {
                let raw = eval(e, consts, here, line)?;
                // `n(pc)`/`label(pc)` (mode 7) name the *target*, not the stored
                // displacement: the displacement is `target - pc` (vasm treats a
                // constant the same way as a label). An `d16(An)` displacement is
                // always literal.
                let d = if *mode == 7 { raw - pc_ext } else { raw };
                let d16 = i16::try_from(d)
                    .map_err(|_| AsmError::new(line, format!("displacement {d} out of range")))?;
                ext.extend_from_slice(&d16.to_be_bytes());
            }
            (field(u16::from(*mode), u16::from(*reg)), ext, None)
        }
        Opnd::Idx {
            reg,
            disp,
            index,
            long,
            bit,
        } => {
            let d = eval(disp, consts, here, line)?;
            let d8 = i8::try_from(d)
                .map_err(|_| AsmError::new(line, format!("index displacement {d} out of range")))?;
            // Brief extension word: D/A bit, index register, size, then the
            // 8-bit displacement. Scale (68020+) is always 0 on the 68000.
            let da = u16::from(*index >= 8) << 15;
            let ireg = u16::from(*index & 7) << 12;
            let sz_bit = u16::from(*long) << 11;
            let word = da | ireg | sz_bit | u16::from(d8 as u8);
            // Mode 6 (d8,An,Xn); PC-relative index is mode 7, register 3.
            let (mode, eareg) = if *bit == ea::PCX {
                (7, 3)
            } else {
                (6, u16::from(*reg))
            };
            (field(mode, eareg), word.to_be_bytes().to_vec(), None)
        }
        Opnd::Abs(e) => {
            let v = eval(e, consts, here, line)?;
            let target = reloc_sym(e, &ctx.reloc).and_then(|s| ctx.sec_of.get(s).copied());
            // A relocatable label in the same section, in a slot that accepts
            // PC-relative addressing, becomes `(d16,PC)` — shorter than (xxx).L
            // and position-independent (vasm's preference). Cross-section refs
            // can't be PC-relative (the hunks load independently), so they stay
            // (xxx).L with a relocation.
            if ctx.optimize && modes.allows(ea::PCD) && target == Some(cur_sec) {
                let disp = v - pc_ext;
                let d16 = i16::try_from(disp).map_err(|_| {
                    AsmError::new(
                        line,
                        format!("PC-relative displacement {disp} out of range"),
                    )
                })?;
                return Ok((field(7, 2), d16.to_be_bytes().to_vec(), None)); // (d16,PC)
            }
            // Otherwise (xxx).L; a relocatable target needs a relocation.
            (field(7, 1), (v as u32).to_be_bytes().to_vec(), target)
        }
        // Explicit `.w`/`.l` size forces — no auto-sizing or PC-relative rewrite.
        Opnd::AbsW(e) => {
            let v = eval(e, consts, here, line)?;
            (field(7, 0), (v as u16).to_be_bytes().to_vec(), None)
        }
        Opnd::AbsL(e) => {
            let v = eval(e, consts, here, line)?;
            let target = reloc_sym(e, &ctx.reloc).and_then(|s| ctx.sec_of.get(s).copied());
            (field(7, 1), (v as u32).to_be_bytes().to_vec(), target)
        }
        Opnd::Imm(e) => {
            let v = eval(e, consts, here, line)?;
            // Out-of-range isn't fatal in vasm — it warns and keeps the low
            // `size` bytes. A byte immediate rides zero-extended in one word
            // (`#-1` -> `00ff`); when it overflows a byte, vasm keeps the raw
            // low word instead (`#$1234` -> `1234`).
            let (lo, hi): (i64, i64) = match sz {
                Size::B => (-128, 255),
                Size::W => (-32768, 65535),
                Size::L => (i64::from(i32::MIN), i64::from(u32::MAX)),
            };
            let in_range = (lo..=hi).contains(&v);
            if !in_range {
                warnings.push(Warning::new(line, "immediate operand out of range"));
            }
            let words = match sz {
                Size::B if in_range => vec![0, v as u8],
                Size::B => (v as u16).to_be_bytes().to_vec(),
                Size::W => (v as u16).to_be_bytes().to_vec(),
                Size::L => (v as u32).to_be_bytes().to_vec(),
            };
            // A long immediate holding a relocatable address (`move.l #label,…`)
            // stores the in-hunk offset and gets relocated at load time.
            let reloc = if matches!(sz, Size::L) {
                reloc_sym(e, &ctx.reloc).and_then(|s| ctx.sec_of.get(s).copied())
            } else {
                None
            };
            (field(7, 4), words, reloc)
        }
        Opnd::RegList(_) | Opnd::Ccr | Opnd::Sr | Opnd::Usp => {
            return Err(AsmError::new(line, "internal: non-EA operand used as EA"));
        }
    })
}

fn size_bits(enc: SizeEnc, size: Size) -> u16 {
    match enc {
        SizeEnc::Fixed(_) => 0,
        SizeEnc::Std6 => {
            (match size {
                Size::B => 0,
                Size::W => 1,
                Size::L => 2,
            }) << 6
        }
        SizeEnc::Move => {
            (match size {
                Size::B => 1,
                Size::W => 3,
                Size::L => 2,
            }) << 12
        }
        SizeEnc::WL { shift } => u16::from(matches!(size, Size::L)) << shift,
    }
}

fn push_sized(out: &mut Vec<u8>, v: i64, size: DataSize) {
    match size {
        DataSize::B => out.push(v as u8),
        DataSize::W => out.extend_from_slice(&(v as u16).to_be_bytes()),
        DataSize::L => out.extend_from_slice(&(v as u32).to_be_bytes()),
    }
}

// ---------------------------------------------------------------------------
// Sizing (pass 1) — extension-word counts are fixed by operand shape and size
// ---------------------------------------------------------------------------

/// The byte length of one statement, given the optimizer context and whether a
/// relaxable branch here has grown to its word form.
fn stmt_size(
    kind: &Stmt,
    ctx: &Ctx,
    consts: &BTreeMap<String, i64>,
    cur_sec: usize,
    word_branch: bool,
    line: usize,
) -> Result<usize, AsmError> {
    Ok(match kind {
        Stmt::Empty
        | Stmt::Equ(..)
        | Stmt::Even
        | Stmt::Section(..)
        | Stmt::Align(_)
        | Stmt::Cnop(..)
        | Stmt::Fail(_)
        | Stmt::Assert(..)
        | Stmt::Echo(_) => 0,
        Stmt::Raw(payload) => payload.len(),
        Stmt::Dc(size, items) => items.len() * size.bytes(),
        Stmt::Ds(size, count) | Stmt::Dcb(size, count, _) => {
            count_of(count, consts, line)? * size.bytes()
        }
        Stmt::Insn {
            mnemonic,
            size,
            operands,
        } => {
            let (mnemonic, operands, size_override) = lower(mnemonic, *size, operands, ctx, consts);
            let operands = operands.as_ref();
            let written = size_override.or(*size);
            let insn = m68k::SET
                .instruction(mnemonic)
                .ok_or_else(|| unknown_or_unmatched(mnemonic, line))?;
            let form = match_form(insn, operands).ok_or_else(|| {
                AsmError::new(line, format!("`{mnemonic}` has no form for those operands"))
            })?;
            let eff = branch_size(ctx.optimize, kind, &written, word_branch);
            let sz = eff.unwrap_or(Size::W);
            // Opcode word, plus each slot's extension words (a Quick8 immediate
            // rides in the opcode, so it adds nothing).
            let mut bytes = 2;
            for (slot, op) in form.operands.iter().zip(operands) {
                bytes += match slot {
                    Slot::Dn { .. }
                    | Slot::An { .. }
                    | Slot::AddrIndirect { .. }
                    | Slot::Quick8
                    | Slot::Vec4
                    | Slot::Ccr
                    | Slot::Sr
                    | Slot::Usp
                    | Slot::Quick3 { .. } => 0,
                    // A `.s`/`.b` branch packs its displacement in the opcode
                    // word; the word form adds a 16-bit extension word.
                    Slot::BranchW if matches!(eff, Some(Size::B)) => 0,
                    // A long immediate needs two extension words; byte/word, one.
                    Slot::ImmSized if matches!(sz, Size::L) => 4,
                    Slot::BranchW
                    | Slot::DispW
                    | Slot::ImmWord
                    | Slot::RegList
                    | Slot::ImmSized
                    | Slot::MovepDisp => 2,
                    Slot::Ea { modes, .. } => ea_ext_len(op, sz, *modes, ctx, consts, cur_sec),
                };
            }
            bytes
        }
    })
}

/// Extension-word byte count an operand contributes as an effective address —
/// must match [`resolve_ea`] exactly so the layout and emit passes agree.
fn ea_ext_len(
    op: &Opnd,
    sz: Size,
    modes: EaModes,
    ctx: &Ctx,
    consts: &BTreeMap<String, i64>,
    cur_sec: usize,
) -> usize {
    match op {
        Opnd::DReg(_) | Opnd::AReg(_) => 0,
        Opnd::Mem { mode, disp, .. } => {
            // A dropped zero `d16(An)` displacement contributes no extension word.
            if disp.is_some() && !drops_zero_disp(*mode, disp, ctx, consts) {
                2
            } else {
                0
            }
        }
        // The brief index extension word.
        Opnd::Idx { .. } => 2,
        // A same-section relocatable label in a PC-capable slot becomes `(d16,PC)`
        // (2 bytes); otherwise (xxx).L (4 bytes). Must mirror `resolve_ea`.
        Opnd::Abs(e) => {
            let same_sec = reloc_sym(e, &ctx.reloc)
                .and_then(|s| ctx.sec_of.get(s))
                .is_some_and(|s| *s == cur_sec);
            if ctx.optimize && modes.allows(ea::PCD) && same_sec {
                2
            } else {
                4
            }
        }
        Opnd::AbsW(_) => 2,
        Opnd::AbsL(_) => 4,
        Opnd::Imm(_) => {
            if matches!(sz, Size::L) {
                4
            } else {
                2
            }
        }
        Opnd::RegList(_) | Opnd::Ccr | Opnd::Sr | Opnd::Usp => 0,
    }
}

// ---------------------------------------------------------------------------
// Operands / statements
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Opnd {
    DReg(u8),
    AReg(u8),
    /// `(An)`, `(An)+`, `-(An)`, `d16(An)`, `d16(PC)`. `bit` is its `ea::` mask.
    Mem {
        mode: u8,
        reg: u8,
        bit: u16,
        disp: Option<Expr>,
    },
    /// Indexed: `d8(An,Xn.size)` (mode 6) or `d8(PC,Xn.size)` (mode 7/reg 3).
    /// `index` is the index register 0–15 (Dn 0–7, An 8–15); `long` is its size.
    Idx {
        reg: u8,
        disp: Expr,
        index: u8,
        long: bool,
        bit: u16,
    },
    /// A bare absolute address (`.W`/`.L` chosen by value), or — when consumed
    /// by a `BranchW`/`DispW` slot — a branch target expression.
    Abs(Expr),
    /// An absolute address with an explicit `.w` size force — always `(xxx).W`.
    AbsW(Expr),
    /// An absolute address with an explicit `.l` size force — always `(xxx).L`,
    /// never PC-relative-optimised (unlike the auto [`Abs`](Self::Abs)).
    AbsL(Expr),
    /// `#expr`.
    Imm(Expr),
    /// A `MOVEM` register list as a normal-order mask (d0=bit0 … a7=bit15).
    RegList(u16),
    /// The condition-code register (`ccr`).
    Ccr,
    /// The status register (`sr`).
    Sr,
    /// The user stack pointer (`usp`).
    Usp,
}

#[derive(Clone, Copy)]
enum DataSize {
    B,
    W,
    L,
}

impl DataSize {
    fn bytes(self) -> usize {
        match self {
            DataSize::B => 1,
            DataSize::W => 2,
            DataSize::L => 4,
        }
    }
}

/// A hunk's content kind, from a `section` directive's attribute.
#[derive(Clone, Copy, PartialEq)]
enum HunkKind {
    Code,
    Data,
    Bss,
}

/// A hunk's memory-placement preference, from the `_c`/`_f` attribute suffix.
#[derive(Clone, Copy)]
enum MemFlag {
    Any,
    Chip,
    Fast,
}

impl MemFlag {
    /// The two-bit memory flag OR-ed into a hunk's size longword in the header.
    fn bits(self) -> u32 {
        match self {
            MemFlag::Any => 0,
            MemFlag::Chip => 0x4000_0000,
            MemFlag::Fast => 0x8000_0000,
        }
    }
}

// `Clone` so the assembler can project a statement out of the AST node that owns
// it (the multi-pass driver runs on an owned `Vec<Line>`; see `lines_from_program`).
#[derive(Clone)]
enum Stmt {
    Empty,
    Equ(String, Expr),
    Even,
    /// `section name,attr` — opens a new hunk of the given kind and memory flag.
    Section(HunkKind, MemFlag),
    /// `echo "text"[,value]` — print, emit nothing, carry on. Values render in
    /// decimal here; each reference has its own radix.
    Echo(String),
    /// `assert <expr>[,message]` — stop the assembly when the expression is
    /// zero. Held as a statement so the condition folds against the finished
    /// symbol table: an assertion over a label defined later is the point of
    /// having one.
    Assert(Expr, String),
    /// `fail "message"` — stop the assembly where the source says to. Held as
    /// a statement rather than raised at parse time because vasm has
    /// conditional assembly, and a `fail` in an untaken branch must stay
    /// silent.
    Fail(String),
    /// `cnop offset,alignment` — align **up** to `alignment`, then add
    /// `offset`. Padding is a whole instruction word where one fits: an odd
    /// pad takes a leading `$00` and the rest is `NOP` (`$4e71`), the same
    /// "a NOP is a word or it is nothing" rule the hunk serialiser follows.
    Cnop(Expr, Expr),
    /// `align n` — pad to the next multiple of `2^n`, zero-filled. vasm's
    /// operand is an **exponent**, not the boundary: `align 2` is a four-byte
    /// boundary. Kept as an expression so it can name an `equ` constant, the
    /// way `ds`/`dcb` counts do; it folds where the constants are in hand.
    Align(Expr),
    Dc(DataSize, Vec<Expr>),
    /// `ds.x count` — reserve `count` zeroed items. The count is an expression so
    /// it can reference `equ` constants resolved in pass 1.
    Ds(DataSize, Expr),
    /// `dcb.x count,value` — `count` copies of `value` (defaults to 0). Both are
    /// expressions, resolved in pass 1.
    Dcb(DataSize, Expr, Expr),
    /// A resolved `incbin` payload (language-surface U6): raw asset bytes at
    /// the directive's location in the current section. Never parsed into a
    /// native node — the multi-file walk resolves the directive into a shared
    /// [`Item::Binary`](crate::ast::Item) and the projection carries it here.
    Raw(Vec<u8>),
    Insn {
        mnemonic: String,
        size: Option<Size>,
        operands: Vec<Opnd>,
    },
}

impl Stmt {
    /// The boundary this statement must begin on, if it insists on one.
    /// Instructions and `even` begin on an even address; `dc`/`ds` — and an
    /// `incbin` payload, probe-pinned — do not pad on their own. `align n`
    /// carries whatever boundary it named.
    fn pad_at(
        &self,
        at: i64,
        consts: &BTreeMap<String, i64>,
        line: usize,
    ) -> Result<Option<(i64, PadFill)>, AsmError> {
        Ok(match self {
            Stmt::Insn { .. } | Stmt::Even => Some((align_pad(at, 2), PadFill::Zero)),
            Stmt::Align(e) => Some((
                align_pad(at, align_boundary(e, consts, line)?),
                PadFill::Zero,
            )),
            Stmt::Cnop(offset, alignment) => {
                let alignment = eval(alignment, consts, 0, line)?;
                let offset = eval(offset, consts, 0, line)?;
                if alignment < 1 {
                    return Err(AsmError::new(line, "`cnop` alignment must be positive"));
                }
                Some((align_pad(at, alignment) + offset, PadFill::Nop))
            }
            _ => None,
        })
    }
}

/// What a statement's alignment padding is made of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PadFill {
    /// Zero bytes — `even`, `align`, and the implicit word alignment an
    /// instruction takes.
    Zero,
    /// `NOP` (`$4e71`) instruction words, with a single leading `$00` when the
    /// pad is odd and a whole word will not fit.
    Nop,
}

impl PadFill {
    /// The `pad` filler bytes this style produces.
    fn bytes(self, pad: i64) -> Vec<u8> {
        let pad = pad.max(0) as usize;
        match self {
            PadFill::Zero => vec![0u8; pad],
            PadFill::Nop => {
                let mut out = vec![0u8; pad % 2];
                while out.len() < pad {
                    out.extend_from_slice(&[0x4e, 0x71]);
                }
                out
            }
        }
    }
}

/// Fold `align`'s exponent and turn it into the boundary it names.
fn align_boundary(e: &Expr, consts: &BTreeMap<String, i64>, line: usize) -> Result<i64, AsmError> {
    let exp = eval(e, consts, 0, line)?;
    if !(0..=30).contains(&exp) {
        return Err(AsmError::new(
            line,
            format!("`align` exponent {exp} is out of range"),
        ));
    }
    Ok(1 << exp)
}

/// How far short of the next multiple of `boundary` an offset stands.
fn align_pad(at: i64, boundary: i64) -> i64 {
    if boundary <= 1 {
        return 0;
    }
    (boundary - at.rem_euclid(boundary)) % boundary
}

// The 68000 statement is the family-owned native payload carried in the AST
// (`decisions/ast-native-payload-for-multipass-cisc.md`): parse builds and
// qualifies it into the tree, and the multi-pass assembler reads it back.
impl crate::ast::NativeItem for Stmt {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// `equ`/`=` binds its label on the same line, so the formatter keeps the
    /// label there (no colon — the keyword disambiguates it).
    fn inline_label(&self) -> bool {
        matches!(self, Stmt::Equ(..))
    }
}

struct Line {
    line: usize,
    /// The macro expansions this statement's text came through, innermost
    /// first — empty for a line the author wrote.
    ///
    /// vasm's errors come from the multi-pass layout and encode, long after the
    /// expansion's origins are out of scope, so the frames have to travel with
    /// the statement. Every other dialect attaches them from the node as it
    /// lowers; this one has no lowering step to attach them in.
    frames: Vec<crate::span::ExpansionFrame>,
    /// The file `line` counts within (language-surface U6): the root for a
    /// single-file assemble, an include's `FileId` otherwise. Layout/encode
    /// errors, warnings, and debug line records are stamped with it.
    file: FileId,
    label: Option<String>,
    kind: Stmt,
}

/// Parse vasm (Motorola-syntax) 68000 source into the source-preserving semantic
/// [`Program`](crate::ast::Program) — the single front-end IR the multi-pass
/// assembler and the `--fmt` formatter both consume. Each line becomes a node
/// carrying its label (with `.local` scope resolved as vasm's), the verbatim
/// operation source, and comment trivia (`;` inline and `*`-column-0 whole-line
/// comments). Because the formatter re-emits each operation's source verbatim,
/// an `equ` node only needs its native marker so emit keeps the binding on its
/// label's line.
///
/// Single-source: an `include`/`incbin` stays an **unresolved** item (KTD1) —
/// `--fmt` renders the directive verbatim without opening the target, and the
/// assembly projection rejects it with a pointer to the multi-file entry.
pub(crate) fn parse_program(
    source: &str,
    mode: macros::Expand,
) -> Result<crate::ast::Program, AsmError> {
    // Macros expand before parsing (#93), but only for assembly: the formatter
    // asks with `Expand::No`, because laying source out must not replace a
    // definition with its expansions.
    let expanded = expand_vasm(source, mode)?;
    let text = macros::expanded_text(&expanded, source);
    let origins = macros::line_origins(&expanded);
    let mut w = Walker::default();
    // The shared cursor: it groups `if`/`rept` blocks and keeps every include
    // unresolved (KTD1), which is what `--fmt` needs.
    ca65_flat::walk_source_expanded(&mut w, text, FileId(0))
        .map_err(|e| macros::remap_lines(e, origins))?;
    let mut program = w.finish(text.lines().count() as u32);
    macros::place_nodes(&mut program.nodes, origins);
    Ok(program)
}

/// Parse a multi-file 68000 program (language-surface U6, KTD1): the
/// **interleaved walk** over the source map, resolving `include`/`incbin`
/// lazily through `loader` under vasm's probe-pinned semantics
/// ([`VASM_SEMANTICS`] — root-anchored resolution, the zero/negative-length
/// "rest of file" incbin sentinel with silent truncation). Everything the
/// parse accumulates crosses include boundaries in both directions, exactly
/// as vasm's textual splice does (probe-pinned): the `.local` scope's current
/// global (a global defined inside an include rescopes the includer's later
/// locals), the parse-time constant folds feeding `incbin` offsets, and — via
/// the projection reading the spliced node order — the active `section` (a
/// switch inside an include persists into the includer). `equ` constants
/// defined in an include feed the includer's later instruction selection
/// (`addq`/`lea`/`moveq`) through the ordinary layout passes, which see the
/// spliced statement stream whole.
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing target, an include cycle, a bad `incbin` window, or the depth
/// backstop — all at the directive's span.
pub(crate) fn parse_program_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<crate::ast::Program, AsmError> {
    let mut w = Walker::default();
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
        &VASM_SEMANTICS,
    )?;
    Ok(w.finish(root_lines))
}

/// The per-line parse walk shared by [`parse_program`] (single source) and
/// [`parse_program_multi`] (the include-capable walk). The environment — the
/// enclosing global label a leading-`.` local qualifies against, the
/// parse-time constant folds `incbin` offset/length arguments consult, and
/// pending comment trivia — lives here, so in the multi-file walk it threads
/// *through* include boundaries in both directions (KTD1, probe-pinned).
#[derive(Default)]
struct Walker {
    /// The enclosing global label — a leading-`.` local qualifies against it,
    /// the same scoping vasm's `qualify_local_labels` applies (R4).
    scope: String,
    /// `equ`/`=` constants folded in source order — only for `incbin`
    /// offset/length folding (vasm: "expression must be constant" on a
    /// forward reference, probe-pinned). Assembly itself re-resolves every
    /// constant in its own layout passes.
    consts: BTreeMap<String, i64>,
    /// Own-line comments seen since the last node, attached as leading trivia
    /// to the next one. Comments never reach the encoder, so bytes are
    /// unchanged.
    pending_leading: Vec<crate::ast::Comment>,
    /// Whether the walk is inside a macro definition it is copying rather than
    /// reading. Only the formatter's parse ever sets it: the assembling path
    /// expands definitions away before the walk sees a line.
    in_macro: bool,
    /// The macros defined so far, so an **invocation** is recognised too. A
    /// call is not an instruction, and nothing else tells `two` from a typo.
    macro_names: BTreeSet<String>,
    nodes: Vec<crate::ast::Node>,
}

impl Walker {
    /// Flush comments after the last node (a trailing block or comment-only
    /// file) as a label-less, op-less node so the formatter keeps them.
    fn finish(mut self, last_line: u32) -> crate::ast::Program {
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
        Program { nodes: self.nodes }
    }

    /// Recognise a walk-handled `include`/`incbin` operation, parsing the
    /// `incbin` offset/length against the live constant folds (a forward
    /// reference is vasm's "expression must be constant" posture,
    /// probe-pinned).
    fn walk_directive(&self, rest: &str, line: usize) -> Result<Option<WalkDirective>, AsmError> {
        let (word, args) = split_first_word(rest);
        match word.to_ascii_lowercase().as_str() {
            "include" => Ok(Some(WalkDirective::Include {
                request: file_request(args, line, "include")?.0,
            })),
            "incbin" => {
                let (request, offset, size) = incbin_args(args, line, &self.consts)?;
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
    /// vasm's block vocabulary, measured against vasm 1.9 (m68k mot) and
    /// case-insensitive.
    ///
    /// The numeric forms compare against **zero**, `ifd`/`ifnd` test whether a
    /// symbol is defined — **not** `ifdef`, which vasm answers with `unknown
    /// mnemonic` — and both `endif` and `endc` close.
    fn block_keyword(&self, code: &str) -> Option<ca65_flat::BlockKw> {
        use ca65_flat::BlockKw;
        let word = code.split_whitespace().next()?.to_ascii_lowercase();
        Some(match word.as_str() {
            "if" | "ifne" | "ifeq" | "ifgt" | "ifge" | "iflt" | "ifle" | "ifd" | "ifnd" => {
                BlockKw::CondOpen
            }
            "else" => BlockKw::Else,
            "endif" | "endc" => BlockKw::CondClose,
            "rept" => BlockKw::RepeatOpen,
            "endr" => BlockKw::RepeatClose,
            _ => return None,
        })
    }

    fn nodes_mut(&mut self) -> &mut Vec<crate::ast::Node> {
        &mut self.nodes
    }

    /// The multi-file walk expands too, or macros would work when a file is
    /// assembled alone and vanish the moment it is included from another.
    fn expand_source(&self, source: &str) -> Result<macros::Expansion, AsmError> {
        expand_vasm(source, macros::Expand::Yes)
    }

    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        use crate::ast::{Comment, Node, Scope, Span, Symbol, Trivia};
        let (code, comment) = split_comment(raw);
        if code.trim().is_empty() {
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

        // A macro definition is copied, not read — and copied **verbatim**,
        // because vasm spells one `name macro` with the name in the label
        // column. Laying it out would move the name, and real vasm answers
        // `unknown mnemonic <two>` to an indented one (probed 2026-08-22). Hence
        // `Item::Verbatim` rather than the source-only node the ca65 walk uses:
        // there the keyword carries a `.` and the column is free.
        //
        // A body is a template rather than code — `\1` and `\@` are not
        // expressions — so no line between the header and the close is parsed.
        {
            use crate::dialects::macros::MacroSyntax as _;
            let text = code.trim();
            let opened = VasmMacros.header(text);
            if self.in_macro
                || opened.is_some()
                || self
                    .macro_names
                    .contains(text.split_whitespace().next().unwrap_or(""))
            {
                if let Some((name, _)) = opened {
                    self.macro_names.insert(name);
                    self.in_macro = true;
                } else if VasmMacros.is_end(text) {
                    self.in_macro = false;
                }
                self.nodes.push(Node {
                    operand_span: None,
                    label: None,
                    item: Some(crate::ast::Item::Verbatim),
                    source: code.trim_end().to_string(),
                    span: Span::in_file(file, line as u32, 1),
                    trivia: Trivia {
                        leading: std::mem::take(&mut self.pending_leading),
                        trailing,
                    },
                });
                return Ok(None);
            }
        }

        let (label, rest) = split_label(code, line)?;
        // A non-local label (including an `equ` name) opens a new scope; a
        // leading-`.` name qualifies against the current one — the same forward
        // pass vasm's old `qualify_local_labels` ran, done inline so the tree
        // carries fully-resolved symbols. Include boundaries do not reset it
        // (textual-splice semantics, probe-pinned).
        let symbol = label.as_ref().map(|name| {
            if name.starts_with('.') {
                Symbol {
                    qualified: format!("{}{name}", self.scope),
                    scope: Scope::Local {
                        in_global: self.scope.clone(),
                    },
                    name: name.clone(),
                }
            } else {
                self.scope = name.clone();
                Symbol {
                    qualified: name.clone(),
                    scope: Scope::Global,
                    name: name.clone(),
                }
            }
        });
        let span = Span::in_file(file, line as u32, 1);

        // `include`/`incbin` are walk-handled, not parsed here: the target
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

        // Parse the 68000 statement and qualify its local references against the
        // now-current scope, then store it in the node as the family-owned native
        // payload (the assembler reads it back). An `equ`/`=` reports
        // `inline_label`, so emit keeps its label on the operation's line.
        let mut stmt = parse_op(&label, rest, line)?;
        qualify_stmt(&mut stmt, &self.scope);
        // Fold an `equ` constant for later `incbin` argument folding (the
        // qualified name, so a local `equ` resolves like any other reference).
        if let Stmt::Equ(name, e) = &stmt
            && let Ok(v) = e.eval_with(&|s| self.consts.get(s).copied(), None, line)
        {
            self.consts.insert(name.clone(), v);
        }
        let trivia = Trivia {
            leading: std::mem::take(&mut self.pending_leading),
            trailing,
        };
        match stmt {
            // A label-only line: keep the label, no operation (emit renders the
            // label alone; the projection reads it back as `Stmt::Empty`).
            Stmt::Empty if symbol.is_some() => self.nodes.push(Node {
                operand_span: None,
                label: symbol,
                item: None,
                source: String::new(),
                span,
                trivia,
            }),
            // A bare line with neither label nor operation — nothing to keep.
            // (Unreachable in practice: an empty operation implies empty code,
            // already skipped above.)
            Stmt::Empty => {
                self.pending_leading = trivia.leading;
            }
            stmt => self.nodes.push(Node {
                operand_span: None,
                label: symbol,
                item: Some(crate::ast::Item::Native(Box::new(stmt))),
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

/// The file name of a vasm `include`/`incbin` directive, and whatever follows
/// it. Probe-pinned spellings: `"file"`, `'file'`, or a bare token (stopping
/// at whitespace or a comma, so `incbin data.bin,2` parses). Anything after a
/// quoted name that is not an argument tail is silently ignored, as vasm
/// ignores it (source-compatible: real Amiga source with trailing text still
/// assembles).
fn file_request<'a>(
    args: &'a str,
    line: usize,
    directive: &str,
) -> Result<(String, &'a str), AsmError> {
    let t = args.trim();
    let (name, rest) = if let Some(quote) = t.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let inner = &t[1..];
        let end = inner
            .find(quote)
            .ok_or_else(|| AsmError::new(line, format!("unterminated `{directive}` file name")))?;
        (&inner[..end], &inner[end + 1..])
    } else {
        let end = t
            .find(|c: char| c.is_whitespace() || c == ',')
            .unwrap_or(t.len());
        (&t[..end], &t[end..])
    };
    if name.is_empty() {
        return Err(AsmError::new(
            line,
            format!("`{directive}` needs a file name"),
        ));
    }
    Ok((name.to_string(), rest))
}

/// Parse an `incbin`'s arguments: the file name, then an optional
/// `,offset[,length]` tail of parse-time constant expressions (probe-pinned:
/// vasm folds them when the directive is read — an `equ` defined before works,
/// a forward reference or `*` is "expression must be constant").
fn incbin_args(
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<(String, Option<i64>, Option<i64>), AsmError> {
    let (name, rest) = file_request(args, line, "incbin")?;
    let rest = rest.trim();
    let Some(tail) = rest.strip_prefix(',') else {
        // No `,offset` tail: trailing junk after the name is ignored
        // (probe-pinned — vasm assembles `incbin "f" junk` silently).
        return Ok((name, None, None));
    };
    let pieces = split_operands(tail);
    if pieces.len() > 2 {
        return Err(AsmError::new(
            line,
            "`incbin` takes at most a file name, an offset, and a length",
        ));
    }
    let fold = |what: &str, piece: &str| -> Result<i64, AsmError> {
        parse_value(piece, line)?
            .eval_with(&|s| consts.get(s).copied(), None, line)
            .map_err(|e| {
                AsmError::new(
                    line,
                    format!(
                        "`incbin` {what} must be a constant expression: {}",
                        e.message
                    ),
                )
            })
    };
    let offset = fold("offset", pieces[0])?;
    let size = pieces.get(1).map(|p| fold("length", p)).transpose()?;
    Ok((name, Some(offset), size))
}

/// Project the semantic [`Program`](crate::ast::Program) into the assembler's
/// statement stream — the multi-pass driver runs on an owned `Vec<Line>`. Each
/// node's qualified label and native [`Stmt`] payload (built and qualified in
/// [`parse_program`]) are read straight back out of the tree; nothing is
/// re-parsed. A resolved `incbin` payload (the multi-file walk's lowering)
/// becomes [`Stmt::Raw`]; a label-only node becomes an empty statement
/// carrying its label; the comment-only flush node (no label, no item)
/// carries no statement.
///
/// # Errors
/// An **unresolved** `include`/`incbin` cannot assemble: it needs a loader,
/// which only the multi-file entry has (U6, KTD1). The single-source API
/// keeps meaning "one file, no includes" — with a pointer, not the old
/// unknown-directive rejection.
fn lines_from_program(program: &crate::ast::Program) -> Result<Vec<Line>, AsmError> {
    let mut out = Vec::new();
    let mut env = BTreeMap::new();
    // `REPTN` reads -1 outside any repetition, which is vasm's own answer and
    // not an absence — `dc.b REPTN` at the top level emits $FF.
    let mut reptn: Vec<i64> = Vec::new();
    project_lines(&program.nodes, &mut out, &mut env, &mut reptn)?;
    Ok(out)
}

/// vasm's `REPTN`: the innermost repetition's 0-based counter.
const REPTN: &str = "REPTN";

/// Project a run of nodes, folding conditionals and repetitions **once, in
/// source order, before layout** — `decisions/conditionals-in-multipass-dialects.md`.
fn project_lines(
    nodes: &[crate::ast::Node],
    out: &mut Vec<Line>,
    env: &mut BTreeMap<String, i64>,
    reptn: &mut Vec<i64>,
) -> Result<(), AsmError> {
    use crate::ast::Item;
    for node in nodes {
        match &node.item {
            Some(Item::Conditional {
                head,
                then_body,
                else_body,
                ..
            }) => {
                if fold_vasm_condition(head, env, reptn, node.span.line as usize)? {
                    project_lines(then_body, out, env, reptn)?;
                } else if let Some(body) = else_body {
                    project_lines(body, out, env, reptn)?;
                }
                continue;
            }
            Some(Item::Repeat { head, body, .. }) => {
                let line = node.span.line as usize;
                let (_, args) = split_first_word(head.trim());
                let count = eval(
                    &parse_value(args.trim(), line)?,
                    &reptn_env(env, reptn),
                    0,
                    line,
                )?;
                // vasm runs a negative count zero times rather than refusing it,
                // where ca65 answers `Range error`. Measured, not assumed.
                for i in 0..count.max(0) {
                    reptn.push(i);
                    let done = project_lines(body, out, env, reptn);
                    reptn.pop();
                    done?;
                }
                continue;
            }
            _ => {}
        }
        project_one_line(node, out, env, reptn)?;
    }
    Ok(())
}

/// The environment a fold sees: the `equ` constants plus `REPTN`, which is the
/// innermost repetition's counter, or -1 outside every one.
fn reptn_env(env: &BTreeMap<String, i64>, reptn: &[i64]) -> BTreeMap<String, i64> {
    let mut e = env.clone();
    e.insert(REPTN.to_string(), reptn.last().copied().unwrap_or(-1));
    e
}

/// Fold one conditional head.
fn fold_vasm_condition(
    head: &str,
    env: &BTreeMap<String, i64>,
    reptn: &[i64],
    line: usize,
) -> Result<bool, AsmError> {
    let (word, args) = split_first_word(head.trim());
    let word = word.to_ascii_lowercase();
    let args = args.trim();
    let env = reptn_env(env, reptn);
    if word == "ifd" || word == "ifnd" {
        let name = args
            .split_whitespace()
            .next()
            .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
        let defined = env.contains_key(name);
        return Ok(if word == "ifd" { defined } else { !defined });
    }
    if args.is_empty() {
        return Err(AsmError::new(line, format!("`{word}` needs a condition")));
    }
    let expr = parse_value(args, line)?;
    // `*` in a condition folds against the **unrelaxed** address in vasm, which
    // this projection does not compute — it runs before layout, deliberately, so
    // that a condition and the relaxation fixpoint cannot feed each other. A
    // condition that reads `*` is refused rather than folded against the wrong
    // address; see `decisions/conditionals-in-multipass-dialects.md`.
    if mentions_pc(&expr) {
        return Err(AsmError::new(
            line,
            "a condition on the location counter `*` is not supported here — vasm folds \
             one against the *unrelaxed* address, which this pass does not compute",
        ));
    }
    let value = eval(&expr, &env, 0, line).map_err(|_| {
        AsmError::new(
            line,
            format!(
                "`{args}` must be a constant here — vasm folds a condition against the \
                 values above it, and refuses a forward reference"
            ),
        )
    })?;
    Ok(match word.as_str() {
        "if" | "ifne" => value != 0,
        "ifeq" => value == 0,
        "ifgt" => value > 0,
        "ifge" => value >= 0,
        "iflt" => value < 0,
        "ifle" => value <= 0,
        _ => {
            return Err(AsmError::new(
                line,
                format!("internal error: `{head}` is not a conditional head"),
            ));
        }
    })
}

/// Whether an expression reads the location counter.
fn mentions_pc(e: &Expr) -> bool {
    match e {
        Expr::Pc => true,
        Expr::Lo(i) | Expr::Hi(i) | Expr::Bank(i) | Expr::Neg(i) => mentions_pc(i),
        Expr::Bin(_, l, r) => mentions_pc(l) || mentions_pc(r),
        Expr::Num(_) | Expr::Sym(_) => false,
    }
}

/// Project one ordinary node.
fn project_one_line(
    node: &crate::ast::Node,
    out: &mut Vec<Line>,
    env: &mut BTreeMap<String, i64>,
    reptn: &[i64],
) -> Result<(), AsmError> {
    use crate::ast::Item;
    {
        let label = node.label.as_ref().map(|s| s.qualified.clone());
        let kind = match &node.item {
            Some(Item::Native(n)) => n
                .as_any()
                .downcast_ref::<Stmt>()
                .expect("vasm stores a Stmt in every native node")
                .clone(),
            Some(Item::Binary(payload)) => Stmt::Raw(payload.clone()),
            Some(Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `include \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `incbin \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            None if label.is_some() => Stmt::Empty,
            // A comment-only flush node, or any other shared item (vasm
            // produces only native/binary items) — nothing to assemble.
            _ => return Ok(()),
        };
        let mut kind = kind;
        bake_reptn(&mut kind, reptn.last().copied().unwrap_or(-1));
        // Bind an `equ` as it is projected, so a later condition folds against
        // what a **taken** branch bound and nothing an untaken one held.
        if let Stmt::Equ(name, e) = &kind
            && let Ok(v) = eval(e, env, 0, node.span.line as usize)
        {
            env.insert(name.clone(), v);
        }
        out.push(Line {
            line: node.span.line as usize,
            file: node.span.file,
            frames: node.span.expansion_frames.clone(),
            label,
            kind,
        });
    }
    Ok(())
}

/// Stamp a statement's file **and** its expansion frames onto an error.
///
/// `stamp_file` alone leaves a diagnostic from inside a macro saying only what
/// was wrong, never that the text was generated — which for generated code is
/// most of the answer, because the failing line is nowhere in the file.
fn stamp(e: AsmError, s: &Line) -> AsmError {
    let mut e = ca65_flat::stamp_file(e, s.file);
    if !s.frames.is_empty()
        && let Some(span) = e.span.as_mut()
        && span.expansion_frames.is_empty()
    {
        span.expansion_frames.clone_from(&s.frames);
    }
    e
}

/// Split a line into its code and its comment for carrying comments as AST
/// trivia — a `*`-column-0 line is a whole-line comment, otherwise the text from
/// the first `;` (a naive scan, no string awareness — exactly what vasm's parser
/// treats as a comment), so the code half is precisely what assembly sees.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    if line.starts_with('*') {
        return ("", Some(line.trim_end()));
    }
    match line.find(';') {
        Some(i) => (&line[..i], Some(line[i..].trim_end())),
        None => (line, None),
    }
}

/// Resolve vasm local labels (names starting with `.`) to their enclosing global
/// label, so the same `.loop` can recur under different routines: each local
/// definition and reference is rewritten to `<global>.<local>`, a key no ordinary
/// identifier collides with. Definition and reference share the global scope
/// current at their line, so they always agree. [`parse_program`] applies this to
/// each statement inline (the labels themselves are resolved as symbols are
/// built), so the tree carries fully-qualified statements.
/// Substitute `REPTN` for this pass's value in every expression a statement
/// holds — the same walk `qualify_stmt` makes, for the same reason acme's and
/// ca65's loop variables are baked: the value differs on every iteration, and a
/// symbol resolves once, later, against one table.
///
/// Applied to **every** projected statement, not only those inside a `rept`,
/// because vasm answers `REPTN` outside one with **-1** rather than leaving it
/// undefined — `dc.b REPTN` at the top level emits `$FF`.
fn bake_reptn(kind: &mut Stmt, value: i64) {
    match kind {
        Stmt::Equ(_, e) => bake_reptn_expr(e, value),
        Stmt::Dc(_, items) => items.iter_mut().for_each(|e| bake_reptn_expr(e, value)),
        Stmt::Ds(_, count) => bake_reptn_expr(count, value),
        Stmt::Dcb(_, count, v) => {
            bake_reptn_expr(count, value);
            bake_reptn_expr(v, value);
        }
        Stmt::Insn { operands, .. } => operands.iter_mut().for_each(|o| bake_reptn_opnd(o, value)),
        Stmt::Empty
        | Stmt::Even
        | Stmt::Section(..)
        | Stmt::Align(_)
        | Stmt::Cnop(..)
        | Stmt::Fail(_)
        | Stmt::Assert(..)
        | Stmt::Echo(_)
        | Stmt::Raw(_) => {}
    }
}

fn bake_reptn_opnd(op: &mut Opnd, value: i64) {
    match op {
        Opnd::Abs(e) | Opnd::AbsW(e) | Opnd::AbsL(e) | Opnd::Imm(e) | Opnd::Idx { disp: e, .. } => {
            bake_reptn_expr(e, value)
        }
        Opnd::Mem { disp: Some(e), .. } => bake_reptn_expr(e, value),
        _ => {}
    }
}

fn bake_reptn_expr(e: &mut Expr, value: i64) {
    match e {
        Expr::Sym(name) if name == REPTN => *e = Expr::Num(value),
        Expr::Lo(i) | Expr::Hi(i) | Expr::Bank(i) | Expr::Neg(i) => bake_reptn_expr(i, value),
        Expr::Bin(_, l, r) => {
            bake_reptn_expr(l, value);
            bake_reptn_expr(r, value);
        }
        _ => {}
    }
}

fn qualify_stmt(kind: &mut Stmt, scope: &str) {
    match kind {
        Stmt::Equ(name, e) => {
            if name.starts_with('.') {
                *name = format!("{scope}{name}");
            }
            qualify_expr(e, scope);
        }
        Stmt::Dc(_, items) => items.iter_mut().for_each(|e| qualify_expr(e, scope)),
        Stmt::Ds(_, count) => qualify_expr(count, scope),
        Stmt::Dcb(_, count, value) => {
            qualify_expr(count, scope);
            qualify_expr(value, scope);
        }
        Stmt::Insn { operands, .. } => operands.iter_mut().for_each(|o| qualify_opnd(o, scope)),
        Stmt::Empty
        | Stmt::Even
        | Stmt::Section(..)
        | Stmt::Align(_)
        | Stmt::Cnop(..)
        | Stmt::Fail(_)
        | Stmt::Assert(..)
        | Stmt::Echo(_)
        | Stmt::Raw(_) => {}
    }
}

fn qualify_opnd(op: &mut Opnd, scope: &str) {
    match op {
        Opnd::Abs(e) | Opnd::AbsW(e) | Opnd::AbsL(e) | Opnd::Imm(e) | Opnd::Idx { disp: e, .. } => {
            qualify_expr(e, scope)
        }
        Opnd::Mem { disp: Some(e), .. } => qualify_expr(e, scope),
        _ => {}
    }
}

fn qualify_expr(e: &mut Expr, scope: &str) {
    match e {
        Expr::Sym(s) if s.starts_with('.') => *s = format!("{scope}{s}"),
        Expr::Lo(b) | Expr::Hi(b) | Expr::Neg(b) => qualify_expr(b, scope),
        Expr::Bin(_, l, r) => {
            qualify_expr(l, scope);
            qualify_expr(r, scope);
        }
        _ => {}
    }
}

fn split_label(code: &str, line: usize) -> Result<(Option<String>, &str), AsmError> {
    if code.starts_with([' ', '\t']) {
        return Ok((None, code.trim()));
    }
    let trimmed = code.trim();
    let (word, rest) = split_first_word(trimmed);
    let name = word.strip_suffix(':').unwrap_or(word);
    if !is_ident(name) {
        return Err(AsmError::new(line, format!("invalid label `{name}`")));
    }
    Ok((Some(name.to_string()), rest))
}

/// What this dialect accepts beyond the 68000 instruction set.
///
/// `dc`, `dcb` and `ds` are declared as families rather than an enumerated
/// cross-product (R6): one entry each, carrying the size vocabulary. The old
/// dispatch used `strip_prefix`, which made `dcb` before `dc` load-bearing —
/// matching on the stem retires that, so these can be declared in any order.
pub const DIRECTIVES: &[Directive] = &[
    // Walk-handled: the shared cursor reads these into `Item::Conditional` /
    // `Item::Repeat` before `parse_op` sees a line.
    //
    // `ifd`/`ifnd` and **not** `ifdef` — vasm answers that one with `unknown
    // mnemonic`, so declaring it would accept a spelling the reference refuses.
    // Both `endif` and `endc` close.
    Directive {
        id: "conditional",
        pattern: Pattern::Exact(&[
            "if", "ifne", "ifeq", "ifgt", "ifge", "iflt", "ifle", "ifd", "ifnd", "else", "endif",
            "endc",
        ]),
        category: Category::Operation,
    },
    // `REPTN` is not declared: it is a *symbol* the repetition binds, not a
    // directive the parser looks up.
    Directive {
        id: "repeat",
        pattern: Pattern::Exact(&["rept", "endr"]),
        category: Category::Operation,
    },
    Directive {
        id: "equ",
        pattern: Pattern::Exact(&["equ", "="]),
        category: Category::Operation,
    },
    Directive {
        id: "even",
        pattern: Pattern::Exact(&["even"]),
        category: Category::Operation,
    },
    Directive {
        id: "section",
        pattern: Pattern::Exact(&["section"]),
        category: Category::Operation,
    },
    // The shorthands: each opens a section whose attribute *is* the word, so
    // `code_c` means `section CODE,code_c`. vasm names the new section after
    // the word (or after an argument, if one is given); the name reaches no
    // part of an executable hunk file, so only the kind and flag are kept.
    Directive {
        id: "align",
        pattern: Pattern::Exact(&["align"]),
        category: Category::Operation,
    },
    Directive {
        id: "cnop",
        pattern: Pattern::Exact(&["cnop"]),
        category: Category::Operation,
    },
    Directive {
        id: "fail",
        pattern: Pattern::Exact(&["fail"]),
        category: Category::Operation,
    },
    Directive {
        id: "assert",
        pattern: Pattern::Exact(&["assert"]),
        category: Category::Operation,
    },
    Directive {
        id: "echo",
        pattern: Pattern::Exact(&["echo"]),
        category: Category::Operation,
    },
    Directive {
        id: "section_shorthand",
        pattern: Pattern::Exact(&[
            "code", "code_c", "code_f", "data", "data_c", "data_f", "bss", "bss_c", "bss_f",
        ]),
        category: Category::Operation,
    },
    Directive {
        id: "dc",
        pattern: Pattern::Sized {
            stem: "dc",
            separator: '.',
            sizes: &["b", "w", "l"],
            bare: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "dcb",
        pattern: Pattern::Sized {
            stem: "dcb",
            separator: '.',
            sizes: &["b", "w", "l"],
            bare: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "ds",
        pattern: Pattern::Sized {
            stem: "ds",
            separator: '.',
            sizes: &["b", "w", "l"],
            bare: true,
        },
        category: Category::Operation,
    },
    // Walk-handled, like acme's `!src`/`!bin`: `parse_op` never sees these,
    // the multi-file walk does. Declared because the surface describes the
    // dialect rather than whichever parser inside it reads the line.
    Directive {
        id: "include",
        pattern: Pattern::Exact(&["include"]),
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Exact(&["incbin"]),
        category: Category::Operation,
    },
    // Expander-handled, before `parse_op` sees a line. The opener only: `endm`
    // is part of the block, not vocabulary of its own.
    Directive {
        id: "macro",
        pattern: Pattern::Exact(&["macro"]),
        category: Category::Operation,
    },
    // What vasm has here and we do not.
    //
    // 114 spellings, each confirmed in statement position against vasm 2.0b —
    // an argument-expected error, a block-context error, or accepted silently
    // as state. Its 68020+, FPU and Apollo mnemonics are deliberately absent:
    // vasm refuses those itself under `-m68000`, and
    // `68000-isa-completeness.md` scopes ours to the 68000.
    Directive {
        id: "unsupported-vasm",
        pattern: Pattern::Exact(&[
            "ac68080",
            "auto",
            "basereg",
            "blk",
            "cargs",
            "clrfo",
            "clrso",
            "comm",
            "comment",
            "cpu32",
            "cseg",
            "db",
            "debug",
            "dl",
            "dr",
            "dseg",
            "dsource",
            "dw",
            "dx",
            "einline",
            "elif",
            "elseif",
            "end",
            "endb",
            "endm",
            "entry",
            "erem",
            "export",
            "extrn",
            "far",
            "fo",
            "fpu",
            "global",
            "idnt",
            "if1",
            "if2",
            "ifb",
            "ifc",
            "ifmacrod",
            "ifmacrond",
            "ifmi",
            "ifnb",
            "ifnc",
            "ifp1",
            "ifpl",
            "image",
            "import",
            "incdir",
            "initnear",
            "inline",
            "jumperr",
            "jumpptr",
            "line_a",
            "line_f",
            "linea",
            "linef",
            "list",
            "llen",
            "load",
            "local",
            "machine",
            "mask2",
            "mexit",
            "module",
            "msource",
            "near",
            "nolist",
            "nopage",
            "nref",
            "odd",
            "offset",
            "opt",
            "org",
            "output",
            "page",
            "plen",
            "popsection",
            "printt",
            "printv",
            "public",
            "pushsection",
            "rem",
            "rorg",
            "rs",
            "rseven",
            "rsreset",
            "rsset",
            "sdreg",
            "setfo",
            "setso",
            "showoffset",
            "so",
            "spc",
            "symdebug",
            "text",
            "ttl",
            "vdebug",
            "weak",
            "xdef",
            "xref",
        ]),
        category: Category::KnownUnsupported,
    },
];

fn parse_op(label: &Option<String>, rest: &str, line: usize) -> Result<Stmt, AsmError> {
    if rest.is_empty() {
        return Ok(Stmt::Empty);
    }
    let (word, args) = split_first_word(rest);
    let lower = word.to_ascii_lowercase();

    // Dispatch through the declared surface. The size suffix is left to the
    // arm bodies: matching recognises the stem so `dc.x` still reports a bad
    // data size instead of falling through as an unknown mnemonic.
    if let Some(directive) = lookup(DIRECTIVES, &lower) {
        let suffix = lower
            .split_once('.')
            .map_or("", |(_, tail)| &lower[lower.len() - tail.len()..]);
        if let crate::directives::Category::RefusedByReference(rule) = directive.category {
            return Err(AsmError::new(
                line,
                crate::directives::refused_by_reference("vasm", &lower, rule),
            ));
        }
        if directive.category == crate::directives::Category::KnownUnsupported {
            return Err(AsmError::new(
                line,
                format!(
                    "`{lower}` is a real directive here and asm198x does not implement \
                     it yet — the source is valid and the gap is ours"
                ),
            ));
        }
        return match directive.id {
            "equ" => {
                let name = label
                    .clone()
                    .ok_or_else(|| AsmError::new(line, "`equ` needs a label"))?;
                Ok(Stmt::Equ(name, parse_value(args, line)?))
            }
            "even" => Ok(Stmt::Even),
            "section" => Ok(parse_section(args, line)),
            "section_shorthand" => Ok(section_from_attr(&lower)),
            "align" => parse_align(args, line),
            "cnop" => parse_cnop(args, line),
            "fail" => Ok(Stmt::Fail(args.trim().trim_matches('"').to_string())),
            "echo" => Ok(Stmt::Echo(render_message(args, line, 10)?)),
            "assert" => {
                let parts = split_operands(args);
                let cond = parse_value(parts.first().copied().unwrap_or("").trim(), line)?;
                let message = parts
                    .get(1)
                    .map(|m| m.trim().trim_matches('"').to_string())
                    .unwrap_or_default();
                Ok(Stmt::Assert(cond, message))
            }
            // dcb.x count,value — reserve `count` items of `value`. Stage 1:
            // treat as a constant-sized run (value defaults to 0 if omitted).
            "dcb" => parse_dcb(suffix, args, line),
            "dc" => Ok(Stmt::Dc(
                data_size(suffix, line)?,
                parse_data_list(args, line)?,
            )),
            "ds" => Ok(Stmt::Ds(data_size(suffix, line)?, parse_value(args, line)?)),
            other => Err(AsmError::new(
                line,
                format!("`{other}` is declared but not dispatched"),
            )),
        };
    }

    let (mnemonic, size) = split_size(word, line)?;
    let operands = parse_operands(args, line)?;
    Ok(Stmt::Insn {
        mnemonic,
        size,
        operands,
    })
}

/// Parse `section name,attr`. The attribute names the content kind (`code`,
/// `data`, `bss`) and, via a `_c`/`_f` suffix, the memory placement (chip/fast).
/// vasm also accepts the standalone words `chip`/`fast`.
fn parse_section(args: &str, _line: usize) -> Stmt {
    let attr = split_operands(args)
        .get(1)
        .copied()
        .unwrap_or("")
        .to_ascii_lowercase();
    section_from_attr(&attr)
}

/// `align n` — pad to a `2^n` boundary. The operand is an exponent, so
/// `align 2` means four bytes; folding it here keeps the statement's size a
/// function of the program counter alone.
fn parse_align(args: &str, line: usize) -> Result<Stmt, AsmError> {
    Ok(Stmt::Align(parse_value(args.trim(), line)?))
}

/// Render a `"text", value, ...` list into one message, values in base
/// `radix`. Each reference prints numbers its own way — vasm decimal, rgbasm
/// `$5`, sjasmplus `0x0005` — and nothing compares console output, so the
/// radix is the dialect's and the rest is its own business.
fn render_message(args: &str, line: usize, radix: u32) -> Result<String, AsmError> {
    let mut out = String::new();
    for part in split_operands(args) {
        let part = part.trim();
        if let Some(text) = part.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
            out.push_str(text);
        } else if let Ok(Expr::Num(v)) = parse_value(part, line) {
            match radix {
                16 => out.push_str(&format!("${v:X}")),
                _ => out.push_str(&v.to_string()),
            }
        } else {
            out.push_str(part);
        }
    }
    Ok(out)
}

/// `cnop offset,alignment` — both operands are required, and both may name an
/// `equ` constant, so they fold where the constants are known.
fn parse_cnop(args: &str, line: usize) -> Result<Stmt, AsmError> {
    let parts = split_operands(args);
    if parts.len() != 2 {
        return Err(AsmError::new(line, "`cnop` needs `offset,alignment`"));
    }
    Ok(Stmt::Cnop(
        parse_value(parts[0].trim(), line)?,
        parse_value(parts[1].trim(), line)?,
    ))
}

/// The content kind and memory placement a section attribute names. Shared by
/// `section name,attr` and by the shorthands, where the directive word is the
/// attribute: `bss_c` is `section BSS,bss_c`.
fn section_from_attr(attr: &str) -> Stmt {
    let kind = if attr.contains("bss") {
        HunkKind::Bss
    } else if attr.contains("data") {
        HunkKind::Data
    } else {
        HunkKind::Code
    };
    let flag = if attr.contains("_c") || attr.contains("chip") {
        MemFlag::Chip
    } else if attr.contains("_f") || attr.contains("fast") {
        MemFlag::Fast
    } else {
        MemFlag::Any
    };
    Stmt::Section(kind, flag)
}

fn parse_dcb(sz: &str, args: &str, line: usize) -> Result<Stmt, AsmError> {
    let parts = split_operands(args);
    let count = parse_value(parts.first().copied().unwrap_or(""), line)?;
    let value = match parts.get(1) {
        Some(v) => parse_value(v, line)?,
        None => Expr::Num(0),
    };
    Ok(Stmt::Dcb(data_size(sz, line)?, count, value))
}

/// Evaluate a `ds`/`dcb` repeat count against the pass-1 symbol table.
fn count_of(e: &Expr, consts: &BTreeMap<String, i64>, line: usize) -> Result<usize, AsmError> {
    match eval(e, consts, 0, line)? {
        v if v >= 0 => Ok(v as usize),
        v => Err(AsmError::new(line, format!("negative repeat count {v}"))),
    }
}

/// The item width a `dc`/`dcb`/`ds` suffix names.
///
/// **A bare directive is a word**, not a byte: `dc 1,2` assembles to
/// `0001 0002`, `dcb 3,$aa` to three `$00aa` words, and `ds 2` to four zero
/// bytes. Motorola syntax defaults to `.w` and vasm follows it; reading the
/// empty suffix as a byte silently emitted a third of the data.
fn data_size(suffix: &str, line: usize) -> Result<DataSize, AsmError> {
    match suffix.trim_start_matches('.') {
        "" => Ok(DataSize::W),
        "b" => Ok(DataSize::B),
        "w" => Ok(DataSize::W),
        "l" => Ok(DataSize::L),
        other => Err(AsmError::new(line, format!("bad data size `.{other}`"))),
    }
}

fn split_size(word: &str, line: usize) -> Result<(String, Option<Size>), AsmError> {
    if let Some((mnem, sz)) = word.split_once('.') {
        let size = match sz.to_ascii_lowercase().as_str() {
            // `.s` (short branch) reuses `B`; the branch encoder reads `Some(B)`
            // as the 8-bit form.
            "b" | "s" => Size::B,
            "w" => Size::W,
            "l" => Size::L,
            other => return Err(AsmError::new(line, format!("bad size suffix `.{other}`"))),
        };
        Ok((mnem.to_ascii_uppercase(), Some(size)))
    } else {
        Ok((word.to_ascii_uppercase(), None))
    }
}

fn parse_operands(args: &str, line: usize) -> Result<Vec<Opnd>, AsmError> {
    let args = args.trim();
    if args.is_empty() {
        return Ok(Vec::new());
    }
    split_operands(args)
        .iter()
        .map(|p| parse_operand(p, line))
        .collect()
}

fn parse_operand(text: &str, line: usize) -> Result<Opnd, AsmError> {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix('#') {
        return Ok(Opnd::Imm(parse_value(rest, line)?));
    }
    if let Some(reg) = parse_reg(t) {
        return Ok(reg);
    }
    // A MOVEM register list: `d0-d7/a0-a6` (multi-register; single registers are
    // already handled above). Detected by `/` or a register-to-register range.
    if (t.contains('/') || t.contains('-'))
        && !t.starts_with('-')
        && let Some(mask) = parse_reglist(t)
    {
        return Ok(Opnd::RegList(mask));
    }
    parse_ea(t, line)
}

/// The 16-bit mask an operand contributes to a `MOVEM` register list
/// (d0=bit0 … d7=bit7, a0=bit8 … a7=bit15).
fn reglist_mask(op: &Opnd) -> u16 {
    match op {
        Opnd::DReg(n) => 1 << n,
        Opnd::AReg(n) => 1 << (8 + n),
        Opnd::RegList(m) => *m,
        _ => 0,
    }
}

/// Parse a register list (`d0-d3/a0-a1`) into a normal-order mask, or `None` if
/// any part is not a register or register range.
fn parse_reglist(t: &str) -> Option<u16> {
    let mut mask = 0u16;
    for part in t.split('/') {
        let part = part.trim();
        if let Some((a, b)) = part.split_once('-') {
            let (lo, hi) = (reg_index(a)?, reg_index(b)?);
            if lo > hi {
                return None;
            }
            for i in lo..=hi {
                mask |= 1 << i;
            }
        } else {
            mask |= 1 << reg_index(part)?;
        }
    }
    Some(mask)
}

/// A register's mask-bit index: d0–d7 → 0–7, a0–a7 → 8–15.
fn reg_index(t: &str) -> Option<u16> {
    match parse_reg(t.trim()) {
        Some(Opnd::DReg(n)) => Some(u16::from(n)),
        Some(Opnd::AReg(n)) => Some(8 + u16::from(n)),
        _ => None,
    }
}

fn parse_reg(t: &str) -> Option<Opnd> {
    let t = t.to_ascii_lowercase();
    if t == "sp" {
        return Some(Opnd::AReg(7));
    }
    match t.as_str() {
        "ccr" => return Some(Opnd::Ccr),
        "sr" => return Some(Opnd::Sr),
        "usp" => return Some(Opnd::Usp),
        _ => {}
    }
    if t.len() == 2 {
        let n = t.as_bytes()[1].checked_sub(b'0')?;
        if n <= 7 {
            return match t.as_bytes()[0] {
                b'd' => Some(Opnd::DReg(n)),
                b'a' => Some(Opnd::AReg(n)),
                _ => None,
            };
        }
    }
    None
}

fn parse_ea(t: &str, line: usize) -> Result<Opnd, AsmError> {
    if let Some(inner) = t.strip_prefix("-(").and_then(|s| s.strip_suffix(')')) {
        return Ok(Opnd::Mem {
            mode: 4,
            reg: areg(inner, line)?,
            bit: ea::PD,
            disp: None,
        });
    }
    if let Some(inner) = t.strip_prefix('(').and_then(|s| s.strip_suffix(")+")) {
        return Ok(Opnd::Mem {
            mode: 3,
            reg: areg(inner, line)?,
            bit: ea::PI,
            disp: None,
        });
    }
    if let Some(inner) = t.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        // New-style parenthesised EA — `(d,An)`, `(d,An,Xn.size)`, `(An,Xn.size)`
        // — where the whole effective address sits inside the parentheses.
        // Rewrite it to the equivalent old-style `d(An…)` and re-dispatch, so
        // the displacement/index/PC handling below is reused, not duplicated.
        if let Some((first, rest)) = inner.split_once(',') {
            let (first, rest) = (first.trim(), rest.trim());
            let rewritten = if matches!(parse_reg(first), Some(Opnd::AReg(_))) {
                // Base register first: an implicit zero displacement.
                format!("0({inner})")
            } else {
                // Displacement first.
                format!("{first}({rest})")
            };
            return parse_ea(&rewritten, line);
        }
        return Ok(Opnd::Mem {
            mode: 2,
            reg: areg(inner, line)?,
            bit: ea::AI,
            disp: None,
        });
    }
    // disp(An) / disp(PC) / disp(An,Xn.size) / disp(PC,Xn.size)
    if let (Some(open), Some(stripped)) = (t.find('('), t.strip_suffix(')')) {
        let disp = parse_value(&t[..open], line)?;
        let base = stripped[open + 1..].trim();
        if let Some((reg_part, index_part)) = base.split_once(',') {
            // Indexed addressing: an index register, optionally `.w`/`.l`.
            let (idx_name, long) = match index_part.trim().rsplit_once('.') {
                Some((r, "l" | "L")) => (r, true),
                Some((r, "w" | "W")) => (r, false),
                Some((_, other)) => {
                    return Err(AsmError::new(line, format!("bad index size `.{other}`")));
                }
                None => (index_part.trim(), false),
            };
            let index = reg_index(idx_name)
                .ok_or_else(|| AsmError::new(line, format!("bad index register `{idx_name}`")))?
                as u8;
            let reg_part = reg_part.trim();
            if reg_part.eq_ignore_ascii_case("pc") {
                return Ok(Opnd::Idx {
                    reg: 3,
                    disp,
                    index,
                    long,
                    bit: ea::PCX,
                });
            }
            return Ok(Opnd::Idx {
                reg: areg(reg_part, line)?,
                disp,
                index,
                long,
                bit: ea::IX,
            });
        }
        if base.eq_ignore_ascii_case("pc") {
            return Ok(Opnd::Mem {
                mode: 7,
                reg: 2,
                bit: ea::PCD,
                disp: Some(disp),
            });
        }
        return Ok(Opnd::Mem {
            mode: 5,
            reg: areg(base, line)?,
            bit: ea::DI,
            disp: Some(disp),
        });
    }
    // An absolute address with an explicit size force: `expr.w` / `expr.l`.
    if let Some(base) = t.strip_suffix(".w").or_else(|| t.strip_suffix(".W")) {
        return Ok(Opnd::AbsW(parse_value(base, line)?));
    }
    if let Some(base) = t.strip_suffix(".l").or_else(|| t.strip_suffix(".L")) {
        return Ok(Opnd::AbsL(parse_value(base, line)?));
    }
    Ok(Opnd::Abs(parse_value(t, line)?))
}

fn areg(t: &str, line: usize) -> Result<u8, AsmError> {
    match parse_reg(t.trim()) {
        Some(Opnd::AReg(n)) => Ok(n),
        _ => Err(AsmError::new(
            line,
            format!("expected an address register, got `{t}`"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Expression + list helpers (reused from the shared core)
// ---------------------------------------------------------------------------

fn parse_value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    // vasm has no `<`/`>` byte prefixes; it does have `& | ^ << >>`.
    mos6502::parse_expr(
        raw,
        line,
        mos6502::parse_number,
        mos6502::ExprOpts {
            compare: mos6502::Compare {
                eq: true,
                eq_eq: false,
                ne_angle: true,
                ne_bang: false,
                relational: true,
                ordered_eq: true,
                minus_one: true,
            },
            function: None,
            bang_is_or: true,
            prec: mos6502::BytePrec::Tight,
            byte_prefix: false,
            caret: mos6502::Caret::Xor,
            at_is_pc: false,
        },
    )
}

fn parse_data_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "`dc` needs a value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.bytes().map(|b| Expr::Num(i64::from(b))));
        } else {
            out.push(parse_value(piece, line)?);
        }
    }
    Ok(out)
}

/// Split operand text on top-level commas (commas inside parentheses are kept).
fn split_operands(args: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(args[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(args[start..].trim());
    out
}

// ---------------------------------------------------------------------------
// Macros (#93)
//
// The mechanics live in [`crate::dialects::macros`]; this is vasm's grammar,
// measured against vasm 1.9. It shares lwasm's positional parameters and
// differs on both of the other axes:
//
//   * a definition may be written either way round, `name macro` or
//     ` macro name`, where lwasm rejects the second (`Missing macro name`).
//   * a per-expansion name is spelled `spin\@`, and `\@` is a *substitution*
//     rather than a scoping rule — it may appear mid-name and in as many names
//     as the body likes. lwasm's `?`/`@` suffixes are not vasm's.
//
// Arity is unchecked in both directions, like lwasm and pasmo: extras are
// dropped, a missing argument substitutes empty and the emptied operand
// complains (`number or identifier expected`).
// ---------------------------------------------------------------------------

/// vasm's macro grammar.
struct VasmMacros;

impl macros::MacroSyntax for VasmMacros {
    /// `name macro` or ` macro name` — vasm takes both.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        let text = macros::without_comment(line);
        let trimmed = text.trim();
        let (first, rest) = trimmed.split_once(char::is_whitespace)?;
        if first.eq_ignore_ascii_case("macro") {
            let name = rest.trim();
            return (!name.is_empty()).then(|| (name.to_string(), Vec::new()));
        }
        // Name first: only in label position, or it is a mnemonic.
        if text.starts_with(char::is_whitespace) {
            return None;
        }
        rest.trim()
            .eq_ignore_ascii_case("macro")
            .then(|| (first.trim_end_matches(':').to_string(), Vec::new()))
    }

    fn is_end(&self, line: &str) -> bool {
        macros::without_comment(line)
            .trim()
            .eq_ignore_ascii_case("endm")
    }

    fn end_keyword(&self) -> &'static str {
        "endm"
    }

    /// None. vasm scopes no name by its spelling — a label in a body is
    /// global, and a second expansion gets `label <spin> redefined`. Per
    /// expansion uniqueness comes from [`expansion_token`](Self::expansion_token)
    /// instead, which the author must ask for.
    fn locals(&self, _body: &[String]) -> Vec<String> {
        Vec::new()
    }

    /// `\1`, `\2`, … for as many arguments as the call site passed.
    fn argument_names(&self, _declared: &[String], count: usize) -> Vec<String> {
        (1..=count).map(|n| format!("\\{n}")).collect()
    }

    /// A backslash opens a positional parameter, so it must be inside the token
    /// for substitution to see it.
    fn is_symbol_char(&self, c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'\\'
    }

    /// `\@` numbers each expansion, so `spin\@` is a fresh label every time.
    fn expansion_token(&self) -> Option<&'static str> {
        Some("\\@")
    }

    /// vasm checks nothing: extras are dropped, and a missing argument
    /// substitutes empty so the complaint arrives from the operand it emptied.
    fn fit_arguments(
        &self,
        _name: &str,
        _params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        Ok(args)
    }
}

/// Expand vasm's macros, unless this parse is the formatter's.
fn expand_vasm(source: &str, mode: macros::Expand) -> Result<macros::Expansion, AsmError> {
    macros::expansion(mode, source, |s| {
        macros::expand(&VasmMacros, s).map(|e| Some((e.text, e.origins)))
    })
}

#[cfg(test)]
mod directive_surface {
    //! R3/R4/R6: dispatch flows through the declaration, the families are
    //! declared as families, and the two cannot drift apart.

    use super::DIRECTIVES;
    use crate::assemble_vasm as asm;
    use crate::directives::{Category, lookup};

    fn bytes(body: &str) -> Vec<u8> {
        asm(&format!("        section code,code\n        {body}\n"))
            .expect("assembles")
            .bytes
    }

    /// `fail "msg"` stops where the source says to, and stays silent inside an
    /// untaken conditional — the reason it is a statement and not a parse error.
    #[test]
    fn fail_stops_the_assembly_unless_the_branch_is_untaken() {
        let err = asm("        section code,code\n        fail \"stop\"\n").expect_err("aborts");
        assert!(err.to_string().contains("stop"), "got `{err}`");
        assert_eq!(
            // `ifeq <expr>` is "expr == 0", so this takes the first branch and
            // the `fail` in the second is never reached.
            bytes("ifeq 0\n        dc.b 1\n        else\n        fail \"never\"\n        endc"),
            vec![1]
        );
    }

    /// `echo` prints and emits nothing, and reaches the caller as a **note**
    /// rather than a warning: the source asked to say it, the assembler is not
    /// complaining about anything.
    #[test]
    fn echo_is_a_note_and_emits_nothing() {
        // The warned entry point: `assemble_vasm` returns bytes only.
        let out = crate::assemble_vasm_warned(
            "        section code,code\n        echo \"n=\",5\n        dc.b 1,2\n",
        )
        .expect("assembles");
        assert_eq!(out.bytes, vec![1, 2]);
        let note = out.warnings.first().expect("one note");
        assert_eq!(note.message, "n=5");
        assert_eq!(note.kind, crate::engine::WarningKind::Note);
        assert!(note.to_string().contains("note:"), "got `{note}`");
    }

    /// `assert` fires only when its expression is zero, and takes an optional
    /// message.
    #[test]
    fn assert_fires_only_when_false() {
        assert_eq!(bytes("assert 1\n        dc.b 9"), vec![9]);
        let err = asm("        section code,code\n        assert 0,\"boom\"\n").expect_err("false");
        assert!(err.to_string().contains("boom"), "got `{err}`");
    }

    /// vasm's `align` names an **exponent**: `align 2` is a four-byte
    /// boundary, not a two-byte one. Zero-filled, and the operand may be an
    /// `equ` constant like a `ds` count.
    #[test]
    fn align_takes_an_exponent_not_a_boundary() {
        assert_eq!(
            bytes("dc.b 1\n        align 2\n        dc.b 2"),
            vec![1, 0, 0, 0, 2]
        );
        assert_eq!(
            bytes("dc.b 1\n        align 1\n        dc.b 2"),
            vec![1, 0, 2]
        );
        assert_eq!(bytes("dc.b 1\n        align 0\n        dc.b 2"), vec![1, 2]);
        assert_eq!(
            asm("N equ 2\n        section code,code\n        dc.b 1\n        align N\n        dc.b 2\n")
                .expect("equ operand")
                .bytes,
            vec![1, 0, 0, 0, 2]
        );
        // Already on the boundary: nothing to pad.
        assert_eq!(
            bytes("dc.b 1,2,3,4\n        align 2\n        dc.b 9"),
            vec![1, 2, 3, 4, 9]
        );
    }

    /// The shorthands say the same thing as the long form: `code_c` is
    /// `section CODE,code_c`, down to the chip-memory bit in the hunk header.
    #[test]
    fn each_section_shorthand_matches_its_long_form() {
        for (short, long) in [
            ("code", "section CODE,code"),
            ("code_c", "section CODE,code_c"),
            ("code_f", "section CODE,code_f"),
            ("data", "section DATA,data"),
            ("data_c", "section DATA,data_c"),
            ("data_f", "section DATA,data_f"),
            ("bss", "section BSS,bss"),
            ("bss_c", "section BSS,bss_c"),
            ("bss_f", "section BSS,bss_f"),
        ] {
            let body = |opener: &str| format!("\t{opener}\n\tmoveq #1,d0\n");
            assert_eq!(
                super::assemble_exe(&body(short)).expect("short assembles"),
                super::assemble_exe(&body(long)).expect("long assembles"),
                "`{short}` should be `{long}`"
            );
        }
    }

    /// R6: one entry per family, not an enumerated cross-product.
    #[test]
    fn the_data_families_are_declared_as_families() {
        for (id, expected) in [
            ("dc", vec!["dc", "dc.b", "dc.w", "dc.l"]),
            ("dcb", vec!["dcb", "dcb.b", "dcb.w", "dcb.l"]),
            ("ds", vec!["ds", "ds.b", "ds.w", "ds.l"]),
        ] {
            let entry = DIRECTIVES
                .iter()
                .find(|d| d.id == id)
                .unwrap_or_else(|| panic!("`{id}` is declared"));
            assert_eq!(entry.spellings(), expected, "{id}");
        }
    }

    /// The ordering constraint the old `strip_prefix` dispatch carried is gone:
    /// `dc` is declared before `dcb`, and `dcb.w` still reaches `dcb`.
    #[test]
    fn a_longer_stem_is_not_swallowed_by_a_shorter_one() {
        assert_eq!(lookup(DIRECTIVES, "dcb.w").map(|d| d.id), Some("dcb"));
        assert_eq!(lookup(DIRECTIVES, "dcb").map(|d| d.id), Some("dcb"));
        assert_eq!(bytes("dcb.w 2,3"), vec![0x00, 0x03, 0x00, 0x03]);
        assert_eq!(bytes("dc.w 3"), vec![0x00, 0x03]);
    }

    /// A bare stem is a **word**, which is Motorola's default and vasm's.
    ///
    /// This test used to assert bytes and called that "the documented
    /// default"; it was pinning our own bug, and no probe contradicted it
    /// because none exercised a bare stem. `vasmm68k_mot` assembles `dc 1,2`
    /// to `0001 0002` and `dcb 3,$aa` to three `$00aa` words.
    #[test]
    fn a_bare_stem_is_a_word() {
        assert_eq!(bytes("dc 1"), vec![0x00, 0x01]);
        assert_eq!(bytes("dcb 2,3"), vec![0x00, 0x03, 0x00, 0x03]);
        assert_eq!(bytes("ds 1"), vec![0x00, 0x00]);
    }

    /// An unknown size reaches its stem, so the arm reports the real problem
    /// rather than the word being refused as an unknown mnemonic.
    #[test]
    fn an_unknown_size_keeps_its_own_diagnostic() {
        for body in ["dc.x 1", "dcb.x 1,2", "ds.q 1"] {
            let err =
                asm(&format!("        section code,code\n        {body}\n")).expect_err("bad size");
            assert!(
                err.to_string().contains("bad data size"),
                "`{body}` should report a bad data size, got: {err}"
            );
        }
    }

    /// R3: every declared spelling is recognised as a directive.
    #[test]
    fn every_declared_spelling_is_recognised() {
        for directive in DIRECTIVES {
            if directive.category != Category::Operation {
                continue;
            }
            for spelling in &directive.spellings() {
                assert_eq!(
                    lookup(DIRECTIVES, spelling).map(|d| d.id),
                    Some(directive.id),
                    "`{spelling}` should reach `{}`",
                    directive.id
                );
            }
        }
    }

    /// A word outside the declaration is not a directive: it falls through to
    /// instruction resolution.
    #[test]
    fn an_undeclared_spelling_is_not_a_directive() {
        assert!(lookup(DIRECTIVES, "frobnicate").is_none());
        let err =
            asm("        section code,code\n        frobnicate 1\n").expect_err("not a thing");
        assert!(
            !err.to_string().contains("declared but not dispatched"),
            "should be refused as a mnemonic, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Conditionals and repetition. Measured against vasm 1.9 (m68k mot).
    // -----------------------------------------------------------------------

    /// The numeric forms compare against **zero**; `ifd`/`ifnd` test a symbol.
    #[test]
    fn each_comparison_tests_against_zero() {
        // nop = 4E71, rts = 4E75.
        for (src, want) in [
            ("\tifne 1\n\tnop\n\tendif\n", vec![0x4E, 0x71]),
            ("\tifne 0\n\tnop\n\tendif\n\trts\n", vec![0x4E, 0x75]),
            ("\tifeq 0\n\tnop\n\tendif\n", vec![0x4E, 0x71]),
            ("\tifgt 1\n\tnop\n\tendif\n", vec![0x4E, 0x71]),
            ("\tifge 0\n\tnop\n\tendif\n", vec![0x4E, 0x71]),
            ("\tiflt 1\n\tnop\n\tendif\n\trts\n", vec![0x4E, 0x75]),
            ("\tifle 0\n\tnop\n\tendif\n", vec![0x4E, 0x71]),
            ("\tif 1\n\tnop\n\tendif\n", vec![0x4E, 0x71]),
        ] {
            assert_eq!(crate::assemble_vasm(src).expect(src).bytes, want, "{src}");
        }
    }

    /// `ifd`/`ifnd` — and **not** `ifdef`, which vasm answers with `unknown
    /// mnemonic`. Declaring that spelling would accept source vasm refuses.
    #[test]
    fn the_defined_tests_are_ifd_and_ifnd() {
        assert_eq!(
            crate::assemble_vasm("sym\tequ 1\n\tifd sym\n\tnop\n\tendif\n")
                .expect("assembles")
                .bytes,
            vec![0x4E, 0x71]
        );
        assert_eq!(
            crate::assemble_vasm("\tifnd nosuch\n\tnop\n\tendif\n")
                .expect("assembles")
                .bytes,
            vec![0x4E, 0x71]
        );
        crate::assemble_vasm("sym\tequ 1\n\tifdef sym\n\tnop\n\tendif\n")
            .expect_err("vasm: unknown mnemonic <ifdef>");
    }

    /// **`REPTN` is implicit** — vasm names no loop parameter — 0-based, and it
    /// tracks the *innermost* repetition. Outside every one it reads **-1**,
    /// which is why it is baked into each projected statement rather than only
    /// those inside a `rept`.
    #[test]
    fn reptn_is_an_implicit_zero_based_counter() {
        assert_eq!(
            crate::assemble_vasm("\trept 3\n\tdc.b REPTN\n\tendr\n")
                .expect("assembles")
                .bytes,
            vec![0, 1, 2]
        );
        assert_eq!(
            crate::assemble_vasm("\trept 2\n\trept 2\n\tdc.b REPTN\n\tendr\n\tendr\n")
                .expect("assembles")
                .bytes,
            vec![0, 1, 0, 1],
            "the innermost loop's counter"
        );
        assert_eq!(
            crate::assemble_vasm("\tdc.b REPTN\n")
                .expect("assembles")
                .bytes,
            vec![0xFF],
            "-1 outside every repetition"
        );
    }

    /// A negative count runs the body no times — vasm's answer, where ca65
    /// gives `Range error` for the same shape.
    #[test]
    fn a_negative_repeat_count_is_empty_rather_than_an_error() {
        assert_eq!(
            crate::assemble_vasm("\trept -1\n\tdc.b 1\n\tendr\n\tdc.b 2\n")
                .expect("assembles")
                .bytes,
            vec![2]
        );
        assert_eq!(
            crate::assemble_vasm("\trept 0\n\tdc.b 1\n\tendr\n\tdc.b 2\n")
                .expect("assembles")
                .bytes,
            vec![2]
        );
    }

    /// A condition on the location counter is refused rather than folded
    /// against the wrong address. vasm folds one against the **unrelaxed**
    /// address — measured — and this pass runs before layout deliberately, so
    /// that a condition and the relaxation fixpoint cannot feed each other.
    /// See `decisions/conditionals-in-multipass-dialects.md`.
    #[test]
    fn a_condition_on_the_location_counter_is_refused_not_guessed() {
        let err =
            crate::assemble_vasm("\tifne *\n\tnop\n\tendif\n").expect_err("not supported here");
        assert!(err.message.contains("unrelaxed"), "{err:?}");
    }

    /// Formatting a block changes the layout and not the program — including
    /// the closer, which keeps the author's spelling.
    #[test]
    fn a_formatted_block_assembles_to_the_same_bytes() {
        for src in [
            "n\tequ 1\n\tifne n\n\tnop\n\telse\n\trts\n\tendif\n",
            "\trept 3\n\tdc.b REPTN\n\tendr\n",
        ] {
            let before = crate::assemble_vasm(src).expect(src).bytes;
            let formatted = crate::format_vasm(src).expect(src);
            let after = crate::assemble_vasm(&formatted)
                .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
                .bytes;
            assert_eq!(
                before, after,
                "formatting changed the program:\n{formatted}"
            );
            let again = crate::format_vasm(&formatted).expect("formats");
            assert_eq!(formatted, again, "{formatted}");
        }
        // The repetition closer is the author's word, not a chosen one.
        let out = crate::format_vasm("\trept 2\n\tnop\n\tendr\n").expect("formats");
        assert!(out.contains("endr"), "{out}");
    }

    /// A name defined twice is refused, however the two are spelled (#126).
    /// vasm has no rebindable form here, so every label and every `equ` is a
    /// definition — and the message says which kind of collision it was, as
    /// the reference's does.
    #[test]
    fn a_name_defined_twice_is_refused() {
        let err = |src: &str| {
            crate::assemble_vasm(src)
                .expect_err("duplicate")
                .to_string()
        };
        assert!(err("spin nop\nspin nop\n").contains("label `spin` redefined"));
        assert!(err("lbl:\nlbl:\n nop\n").contains("label `lbl` redefined"));
        assert!(err("a equ 1\na equ 2\n dc.b a\n").contains("repeatedly defined symbol `a`"));
        assert!(err("lbl nop\nlbl equ 4\n").contains("repeatedly defined symbol `lbl`"));
    }

    /// The two things that must still assemble: distinct names, and the
    /// local labels that repeat under different globals by design.
    #[test]
    fn distinct_names_and_locals_still_assemble() {
        assert_eq!(
            crate::assemble_vasm(" nop\nlbl nop\n")
                .expect("assemble")
                .bytes,
            vec![0x4E, 0x71, 0x4E, 0x71]
        );
        assert_eq!(
            crate::assemble_vasm("a nop\n.l nop\nb nop\n.l nop\n")
                .expect("assemble")
                .bytes,
            vec![0x4E, 0x71, 0x4E, 0x71, 0x4E, 0x71, 0x4E, 0x71],
            "`.l` under `a` and under `b` are different names"
        );
    }

    /// A mnemonic the reference knows is never refused as *unknown*. `adda`
    /// with a data-register destination has no form — vasm says "instruction
    /// not supported", not "unknown mnemonic", and the difference is whether
    /// the reader goes looking for a typo.
    #[test]
    fn a_known_mnemonic_is_refused_for_its_operands() {
        let err = |src: &str| crate::assemble_vasm(src).expect_err("refused").to_string();
        assert!(err("\tadda\n").contains("`ADDA` has no form for those operands"));
        assert!(err("\tadda d0,d1\n").contains("`ADDA` has no form for those operands"));
        assert!(err("\tzzqq\n").contains("unknown instruction `ZZQQ`"));
        assert_eq!(
            crate::assemble_vasm("\tadda.l #4,a0\n")
                .expect("assemble")
                .bytes,
            vec![0x58, 0x88],
            "the form that does exist is unaffected"
        );
    }
}
