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

use super::mos6502::{self, is_ident, split_data_items, split_first_word, string_literal};
use crate::engine::{AsmError, BinOp, Expr};

/// Evaluate an expression against bound symbols, with `*` (the location
/// counter) resolving to `here`. A thin PC-aware wrapper over the shared
/// [`Expr::eval_with`].
fn eval(e: &Expr, consts: &BTreeMap<String, i64>, here: i64, line: usize) -> Result<i64, AsmError> {
    e.eval_with(&|s| consts.get(s).copied(), Some(here), line)
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

/// Assemble with the optimizer either on (Stage 2, matches `vasm -Fbin`) or off
/// (Stage 1, matches `vasm -no-opt`), to a flat binary.
///
/// # Errors
/// Returns an [`AsmError`] on any parse/range/symbol failure, or if more than one
/// section carries bytes (a flat binary can hold only one).
pub(crate) fn assemble_with(source: &str, optimize: bool) -> Result<Vec<u8>, AsmError> {
    let sections = assemble_core(source, optimize)?;
    let nonempty: Vec<&SecOut> = sections.iter().filter(|s| !s.bytes.is_empty()).collect();
    match nonempty.as_slice() {
        [] => Ok(Vec::new()),
        [s] => Ok(s.bytes.clone()),
        _ => Err(AsmError::new(
            0,
            "a flat binary holds one section; this source has several (use the executable output)",
        )),
    }
}

/// Assemble to an Amiga hunk executable (`-Fhunkexe -kick1hunks`), optimizer on.
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble_exe(source: &str) -> Result<Vec<u8>, AsmError> {
    let sections = assemble_core(source, true)?;
    Ok(serialize_hunkexe(&sections))
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

/// Assemble into per-section byte buffers with their relocations — the shared
/// core behind both the flat and the hunk-executable serializers.
fn assemble_core(source: &str, optimize: bool) -> Result<Vec<SecOut>, AsmError> {
    let stmts = parse(source)?;

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
            if s.kind.aligns() && pc[sec] % 2 != 0 {
                pc[sec] += 1;
            }
            if ctx.optimize
                && !word_branch[i]
                && let Some(target) = relaxable_branch_target(&s.kind)
            {
                let disp = eval(target, &consts, pc[sec], s.line)? - (pc[sec] + 2);
                if disp == 0 || i8::try_from(disp).is_err() {
                    next[i] = true;
                }
            }
            pc[sec] += stmt_size(&s.kind, &ctx, &consts, sec, word_branch[i], s.line)? as i64;
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
    for (i, s) in stmts.iter().enumerate() {
        let sec = sec_idx[i];
        let buf = &mut out[sec];
        if s.kind.aligns() && !buf.bytes.len().is_multiple_of(2) {
            buf.bytes.push(0);
        }
        match &s.kind {
            Stmt::Empty | Stmt::Equ(..) | Stmt::Even | Stmt::Section(..) => {}
            Stmt::Dc(size, items) => {
                for e in items {
                    push_sized(&mut buf.bytes, eval(e, &consts, 0, s.line)?, *size);
                }
            }
            Stmt::Ds(size, count) => {
                let n = count_of(count, &consts, s.line)?;
                buf.bytes.resize(buf.bytes.len() + n * size.bytes(), 0);
            }
            Stmt::Dcb(size, count, value) => {
                let n = count_of(count, &consts, s.line)?;
                let v = eval(value, &consts, 0, s.line)?;
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
                let (bytes, relocs) =
                    encode(mnemonic, size, operands, &ctx, &consts, sec, here, s.line)?;
                buf.bytes.extend_from_slice(&bytes);
                buf.relocs.extend(relocs);
            }
        }
    }
    Ok(out)
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
                // Code hunks pad to a longword with NOP (0x4e71); data with zero.
                while !data.len().is_multiple_of(4) {
                    if matches!(s.kind, HunkKind::Code) && data.len() % 4 == 2 {
                        data.extend_from_slice(&[0x4e, 0x71]);
                    } else {
                        data.push(0);
                    }
                }
                out.extend_from_slice(&data);
            }
        }

        // HUNK_RELOC32: blocks of [count, target hunk, offsets…], target hunks
        // ascending, offsets ascending, terminated by a zero count.
        if !s.relocs.is_empty() {
            push_u32(&mut out, 0x3ec);
            for target in 0..sections.len() {
                let mut offs: Vec<u32> = s
                    .relocs
                    .iter()
                    .filter(|(_, t)| *t == target)
                    .map(|(o, _)| *o)
                    .collect();
                if offs.is_empty() {
                    continue;
                }
                offs.sort_unstable();
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
        Stmt::Insn { .. } | Stmt::Dc(..) | Stmt::Ds(..) | Stmt::Dcb(..) | Stmt::Even
    )
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
        if s.kind.aligns() && pc[sec] % 2 != 0 {
            pc[sec] += 1;
        }
        if let Some(label) = &s.label {
            consts.insert(label.clone(), pc[sec]);
        }
        pc[sec] += stmt_size(&s.kind, ctx, &consts, sec, word_branch[i], s.line)? as i64;
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
) -> Result<(Vec<u8>, Vec<Reloc>), AsmError> {
    let (mnemonic, operands, size_override) = lower(mnemonic, size, operands, ctx, consts);
    let operands = operands.as_ref();
    let size = size_override.or(size);
    let insn = m68k::SET
        .instruction(mnemonic)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
    let sz = size.unwrap_or(Size::W);
    let form = match_form(insn, operands).ok_or_else(|| {
        AsmError::new(line, format!("`{mnemonic}` has no form for those operands"))
    })?;

    let mut word = form.base | size_bits(form.size, sz);
    let mut ext: Vec<u8> = Vec::new();
    let mut relocs: Vec<Reloc> = Vec::new();
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
                if !(-128..=255).contains(&v) {
                    return Err(AsmError::new(
                        line,
                        format!("quick immediate {v} out of range"),
                    ));
                }
                word |= u16::from(v as u8);
            }
            // Fixed control-register tokens carry no opcode bits or extension.
            (Slot::Ccr, Opnd::Ccr) | (Slot::Sr, Opnd::Sr) | (Slot::Usp, Opnd::Usp) => {}
            (Slot::Vec4, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                if !(0..=15).contains(&v) {
                    return Err(AsmError::new(line, format!("trap vector {v} must be 0..=15")));
                }
                word |= (v as u16) & 0xF;
            }
            (Slot::Quick3 { shift }, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                if !(1..=8).contains(&v) {
                    return Err(AsmError::new(
                        line,
                        format!("quick immediate {v} must be 1..=8"),
                    ));
                }
                word |= u16::from((v & 7) as u8) << shift; // 8 encodes as 000
            }
            (Slot::ImmWord, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                ext.extend_from_slice(&(v as u16).to_be_bytes());
            }
            (Slot::ImmSized, Opnd::Imm(e)) => {
                let v = eval(e, consts, here, line)?;
                match sz {
                    // A byte immediate rides in the low byte of one word.
                    Size::B => ext.extend_from_slice(&[0, v as u8]),
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
                    op, sz, *dest, *modes, ctx, consts, cur_sec, pc_ext, here, line,
                )?;
                word |= field6 << shift;
                if let Some(target_sec) = reloc {
                    relocs.push((pc_ext as u32, target_sec));
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
        (Slot::AddrIndirect { mode, .. }, Opnd::Mem { mode: m, disp: None, .. }) => *mode == *m,
        (
            Slot::Quick8 | Slot::Quick3 { .. } | Slot::ImmWord | Slot::ImmSized | Slot::Vec4,
            Opnd::Imm(_),
        ) => true,
        (Slot::BranchW | Slot::DispW, Opnd::Abs(_)) => true,
        (Slot::Ccr, Opnd::Ccr) | (Slot::Sr, Opnd::Sr) | (Slot::Usp, Opnd::Usp) => true,
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
        Opnd::Imm(e) => {
            let v = eval(e, consts, here, line)?;
            let words = match sz {
                Size::B => vec![0, (v as u8)],
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
        Stmt::Empty | Stmt::Equ(..) | Stmt::Even | Stmt::Section(..) => 0,
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
                .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
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
                    | Slot::ImmSized => 2,
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

enum Stmt {
    Empty,
    Equ(String, Expr),
    Even,
    /// `section name,attr` — opens a new hunk of the given kind and memory flag.
    Section(HunkKind, MemFlag),
    Dc(DataSize, Vec<Expr>),
    /// `ds.x count` — reserve `count` zeroed items. The count is an expression so
    /// it can reference `equ` constants resolved in pass 1.
    Ds(DataSize, Expr),
    /// `dcb.x count,value` — `count` copies of `value` (defaults to 0). Both are
    /// expressions, resolved in pass 1.
    Dcb(DataSize, Expr, Expr),
    Insn {
        mnemonic: String,
        size: Option<Size>,
        operands: Vec<Opnd>,
    },
}

impl Stmt {
    /// Whether this statement begins on an even address (instructions and `even`
    /// align; `dc`/`ds` do not pad on their own).
    fn aligns(&self) -> bool {
        matches!(self, Stmt::Insn { .. } | Stmt::Even)
    }
}

struct Line {
    line: usize,
    label: Option<String>,
    kind: Stmt,
}

fn parse(source: &str) -> Result<Vec<Line>, AsmError> {
    let mut out = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let code = strip_comment(raw);
        if code.trim().is_empty() {
            continue;
        }
        let (label, rest) = split_label(code, line)?;
        let kind = parse_op(&label, rest, line)?;
        if label.is_none() && matches!(kind, Stmt::Empty) {
            continue;
        }
        out.push(Line { line, label, kind });
    }
    qualify_local_labels(&mut out);
    Ok(out)
}

/// Resolve vasm local labels (names starting with `.`) to their enclosing
/// global label, so the same `.loop` can recur under different routines. Each
/// local definition and reference is rewritten to `<global>.<local>`, a key no
/// ordinary identifier collides with. Definition and reference share the global
/// scope current at their line, so they always agree.
fn qualify_local_labels(lines: &mut [Line]) {
    let mut scope = String::new();
    for l in lines.iter_mut() {
        // A non-local label (or equ name) opens a new scope for the labels below.
        if let Some(name) = &l.label
            && !name.starts_with('.')
        {
            scope = name.clone();
        }
        if let Some(name) = &mut l.label
            && name.starts_with('.')
        {
            *name = format!("{scope}{name}");
        }
        qualify_stmt(&mut l.kind, &scope);
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
        Stmt::Empty | Stmt::Even | Stmt::Section(..) => {}
    }
}

fn qualify_opnd(op: &mut Opnd, scope: &str) {
    match op {
        Opnd::Abs(e) | Opnd::Imm(e) | Opnd::Idx { disp: e, .. } => qualify_expr(e, scope),
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

/// Strip a `;` comment, or a whole-line `*`-comment (column 0).
fn strip_comment(line: &str) -> &str {
    if line.starts_with('*') {
        return "";
    }
    line.find(';').map_or(line, |i| &line[..i])
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

fn parse_op(label: &Option<String>, rest: &str, line: usize) -> Result<Stmt, AsmError> {
    if rest.is_empty() {
        return Ok(Stmt::Empty);
    }
    let (word, args) = split_first_word(rest);
    let lower = word.to_ascii_lowercase();

    if lower == "equ" || lower == "=" {
        let name = label
            .clone()
            .ok_or_else(|| AsmError::new(line, "`equ` needs a label"))?;
        return Ok(Stmt::Equ(name, parse_value(args, line)?));
    }
    if lower == "even" {
        return Ok(Stmt::Even);
    }
    if lower == "section" {
        return Ok(parse_section(args, line));
    }
    if let Some(sz) = lower.strip_prefix("dcb") {
        // dcb.x count,value — reserve `count` items of `value`. Stage 1: treat
        // as a constant-sized run (value defaults to 0 if omitted).
        return parse_dcb(sz, args, line);
    }
    if let Some(sz) = lower.strip_prefix("dc") {
        return Ok(Stmt::Dc(data_size(sz, line)?, parse_data_list(args, line)?));
    }
    if let Some(sz) = lower.strip_prefix("ds") {
        return Ok(Stmt::Ds(data_size(sz, line)?, parse_value(args, line)?));
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

fn data_size(suffix: &str, line: usize) -> Result<DataSize, AsmError> {
    match suffix.trim_start_matches('.') {
        "b" | "" => Ok(DataSize::B),
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
    mos6502::parse_expr_opts(
        raw,
        line,
        mos6502::parse_number,
        mos6502::BytePrec::Tight,
        true,
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
