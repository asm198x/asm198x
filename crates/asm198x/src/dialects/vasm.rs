//! The vasm (Motorola syntax) 68000 dialect — Stage 1: the field-based encoder.
//!
//! The 68000 is word-oriented and big-endian: an instruction is a 16-bit opcode
//! word (size, register, and effective-address fields packed in) followed by
//! 0–4 extension words. This front-end parses Motorola syntax, resolves each
//! operand to an effective address, and fills the [`isa::m68k`] form's fields.
//!
//! Stage 1 emits a flat code image, validated against `vasmm68k_mot -Fbin
//! -no-opt`. The size-selection optimizer (PC-relative, short branches) and the
//! Amiga hunk-exe container come later — see `decisions/syntax-stance.md`.

use std::collections::BTreeMap;

use isa::m68k::{self, Size, SizeEnc, Slot, ea};

use super::mos6502::{self, is_ident, split_data_items, split_first_word, string_literal};
use crate::engine::{AsmError, BinOp, Expr};

/// Evaluate an expression against bound symbols, with `*` (the location counter)
/// resolving to `here`. Like the shared `fold_const` but PC-aware.
fn eval(e: &Expr, consts: &BTreeMap<String, i64>, here: i64, line: usize) -> Result<i64, AsmError> {
    let overflow = || AsmError::new(line, "arithmetic overflow in expression");
    Ok(match e {
        Expr::Num(n) => *n,
        Expr::Pc => here,
        Expr::Sym(s) => *consts
            .get(s)
            .ok_or_else(|| AsmError::new(line, format!("undefined symbol `{s}`")))?,
        Expr::Lo(b) => eval(b, consts, here, line)? & 0xFF,
        Expr::Hi(b) => (eval(b, consts, here, line)? >> 8) & 0xFF,
        Expr::Neg(b) => eval(b, consts, here, line)?
            .checked_neg()
            .ok_or_else(overflow)?,
        Expr::Bin(op, l, r) => {
            let a = eval(l, consts, here, line)?;
            let b = eval(r, consts, here, line)?;
            match op {
                BinOp::Add => a.checked_add(b).ok_or_else(overflow)?,
                BinOp::Sub => a.checked_sub(b).ok_or_else(overflow)?,
                BinOp::Mul => a.checked_mul(b).ok_or_else(overflow)?,
                BinOp::Div if b != 0 => a / b,
                BinOp::Div => return Err(AsmError::new(line, "division by zero")),
            }
        }
    })
}

/// Assemble Motorola-syntax 68000 source into a flat big-endian code image.
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble(source: &str) -> Result<Vec<u8>, AsmError> {
    let stmts = parse(source)?;

    // Pass 1: size every statement and bind labels and `equ` constants.
    let mut consts: BTreeMap<String, i64> = BTreeMap::new();
    let mut pc: i64 = 0;
    for s in &stmts {
        if let Stmt::Equ(name, e) = &s.kind {
            // PC-aware: `len equ *-buffer` resolves `*` to the current location.
            if let Ok(v) = eval(e, &consts, pc, s.line) {
                consts.insert(name.clone(), v);
            }
            continue;
        }
        if s.kind.aligns() && pc % 2 != 0 {
            pc += 1;
        }
        if let Some(label) = &s.label {
            consts.insert(label.clone(), pc);
        }
        pc += size_of(&s.kind, &consts, s.line)? as i64;
    }

    // Pass 2: emit.
    let mut out: Vec<u8> = Vec::new();
    for s in &stmts {
        if s.kind.aligns() && !out.len().is_multiple_of(2) {
            out.push(0);
        }
        match &s.kind {
            Stmt::Empty | Stmt::Equ(..) | Stmt::Even => {}
            Stmt::Dc(size, items) => {
                for e in items {
                    push_sized(&mut out, eval(e, &consts, 0, s.line)?, *size);
                }
            }
            Stmt::Ds(size, count) => {
                let n = count_of(count, &consts, s.line)?;
                out.resize(out.len() + n * size.bytes(), 0);
            }
            Stmt::Dcb(size, count, value) => {
                let n = count_of(count, &consts, s.line)?;
                let v = eval(value, &consts, 0, s.line)?;
                for _ in 0..n {
                    push_sized(&mut out, v, *size);
                }
            }
            Stmt::Insn {
                mnemonic,
                size,
                operands,
            } => {
                let here = out.len() as i64;
                let bytes = encode(mnemonic, *size, operands, &consts, here, s.line)?;
                out.extend_from_slice(&bytes);
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

fn encode(
    mnemonic: &str,
    size: Option<Size>,
    operands: &[Opnd],
    consts: &BTreeMap<String, i64>,
    here: i64,
    line: usize,
) -> Result<Vec<u8>, AsmError> {
    let insn = m68k::SET
        .instruction(mnemonic)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
    let sz = size.unwrap_or(Size::W);
    let form = match_form(insn, operands).ok_or_else(|| {
        AsmError::new(line, format!("`{mnemonic}` has no form for those operands"))
    })?;

    let mut word = form.base | size_bits(form.size, sz);
    let mut ext: Vec<u8> = Vec::new();
    let mut branch: Option<i64> = None;
    // MOVEM reverses its register mask when the effective address predecrements.
    let predec = operands
        .iter()
        .any(|o| matches!(o, Opnd::Mem { mode: 4, .. }));

    for (slot, op) in form.operands.iter().zip(operands) {
        match (slot, op) {
            (Slot::Dn { shift }, Opnd::DReg(n)) => word |= u16::from(*n) << shift,
            (Slot::An { shift }, Opnd::AReg(n)) => word |= u16::from(*n) << shift,
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
            (Slot::RegList, _) => {
                let mask = reglist_mask(op);
                let mask = if predec { mask.reverse_bits() } else { mask };
                ext.extend_from_slice(&mask.to_be_bytes());
            }
            (Slot::Ea { shift, dest, .. }, _) => {
                let (field6, words) = resolve_ea(op, sz, *dest, consts, here, line)?;
                word |= field6 << shift;
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
            return Ok(word.to_be_bytes().to_vec());
        }
        let d = i16::try_from(disp)
            .map_err(|_| AsmError::new(line, format!("branch out of range ({disp} bytes)")))?;
        let mut out = word.to_be_bytes().to_vec();
        out.extend_from_slice(&d.to_be_bytes());
        return Ok(out);
    }

    let mut out = word.to_be_bytes().to_vec();
    out.extend_from_slice(&ext);
    Ok(out)
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
        (Slot::Quick8 | Slot::Quick3 { .. } | Slot::ImmWord, Opnd::Imm(_)) => true,
        (Slot::BranchW | Slot::DispW, Opnd::Abs(_)) => true,
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
        Opnd::Abs(_) => ea::AL | ea::AW,
        Opnd::Imm(_) => ea::IMM,
        Opnd::RegList(_) => 0,
    }
}

/// Resolve an operand used as an effective address: its 6-bit field (in normal
/// or MOVE-destination layout) and its extension-word bytes. `Ok(None)` if the
/// operand can't be an EA at all.
fn resolve_ea(
    op: &Opnd,
    sz: Size,
    dest: bool,
    consts: &BTreeMap<String, i64>,
    here: i64,
    line: usize,
) -> Result<(u16, Vec<u8>), AsmError> {
    let field = |mode: u16, reg: u16| {
        if dest {
            (reg << 3) | mode
        } else {
            (mode << 3) | reg
        }
    };
    Ok(match op {
        Opnd::DReg(n) => (field(0, u16::from(*n)), vec![]),
        Opnd::AReg(n) => (field(1, u16::from(*n)), vec![]),
        Opnd::Mem {
            mode, reg, disp, ..
        } => {
            let mut ext = Vec::new();
            if let Some(e) = disp {
                let d = eval(e, consts, here, line)?;
                let d16 = i16::try_from(d)
                    .map_err(|_| AsmError::new(line, format!("displacement {d} out of range")))?;
                ext.extend_from_slice(&d16.to_be_bytes());
            }
            (field(u16::from(*mode), u16::from(*reg)), ext)
        }
        Opnd::Abs(e) => {
            // Stage 1 mirrors `vasm -no-opt`: a bare absolute is always (xxx).L.
            // Shrinking small values to (xxx).W is an optimization, added in
            // Stage 2; doing it here would also desync pass 1 from pass 2, since
            // forward labels are unresolved when pass 1 sizes them.
            let v = eval(e, consts, here, line)?;
            (field(7, 1), (v as u32).to_be_bytes().to_vec()) // (xxx).L
        }
        Opnd::Imm(e) => {
            let v = eval(e, consts, here, line)?;
            let words = match sz {
                Size::B => vec![0, (v as u8)],
                Size::W => (v as u16).to_be_bytes().to_vec(),
                Size::L => (v as u32).to_be_bytes().to_vec(),
            };
            (field(7, 4), words)
        }
        Opnd::RegList(_) => return Err(AsmError::new(line, "internal: register list used as EA")),
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

fn size_of(kind: &Stmt, consts: &BTreeMap<String, i64>, line: usize) -> Result<usize, AsmError> {
    Ok(match kind {
        Stmt::Empty | Stmt::Equ(..) | Stmt::Even => 0,
        Stmt::Dc(size, items) => items.len() * size.bytes(),
        Stmt::Ds(size, count) | Stmt::Dcb(size, count, _) => {
            count_of(count, consts, line)? * size.bytes()
        }
        Stmt::Insn {
            mnemonic,
            size,
            operands,
        } => {
            let insn = m68k::SET
                .instruction(mnemonic)
                .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
            let form = match_form(insn, operands).ok_or_else(|| {
                AsmError::new(line, format!("`{mnemonic}` has no form for those operands"))
            })?;
            let sz = size.unwrap_or(Size::W);
            // Opcode word, plus each slot's extension words (a Quick8 immediate
            // rides in the opcode, so it adds nothing).
            let mut bytes = 2;
            for (slot, op) in form.operands.iter().zip(operands) {
                bytes += match slot {
                    Slot::Dn { .. } | Slot::An { .. } | Slot::Quick8 | Slot::Quick3 { .. } => 0,
                    // A `.s`/`.b` branch packs its displacement in the opcode
                    // word; the word form adds a 16-bit extension word.
                    Slot::BranchW if matches!(size, Some(Size::B)) => 0,
                    Slot::BranchW | Slot::DispW | Slot::ImmWord | Slot::RegList => 2,
                    Slot::Ea { .. } => ea_ext_len(op, sz),
                };
            }
            bytes
        }
    })
}

/// Extension-word byte count an operand contributes as an effective address —
/// must match [`resolve_ea`] exactly so pass 1 and pass 2 agree on sizes.
fn ea_ext_len(op: &Opnd, sz: Size) -> usize {
    match op {
        Opnd::DReg(_) | Opnd::AReg(_) => 0,
        Opnd::Mem { disp, .. } => {
            if disp.is_some() {
                2
            } else {
                0
            }
        }
        // A bare absolute is always (xxx).L under `-no-opt` (see `resolve_ea`).
        Opnd::Abs(_) => 4,
        Opnd::Imm(_) => {
            if matches!(sz, Size::L) {
                4
            } else {
                2
            }
        }
        Opnd::RegList(_) => 0,
    }
}

// ---------------------------------------------------------------------------
// Operands / statements
// ---------------------------------------------------------------------------

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
    /// A bare absolute address (`.W`/`.L` chosen by value), or — when consumed
    /// by a `BranchW`/`DispW` slot — a branch target expression.
    Abs(Expr),
    /// `#expr`.
    Imm(Expr),
    /// A `MOVEM` register list as a normal-order mask (d0=bit0 … a7=bit15).
    RegList(u16),
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

enum Stmt {
    Empty,
    Equ(String, Expr),
    Even,
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
        Stmt::Empty | Stmt::Even => {}
    }
}

fn qualify_opnd(op: &mut Opnd, scope: &str) {
    match op {
        Opnd::Abs(e) | Opnd::Imm(e) => qualify_expr(e, scope),
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
        return Ok(Stmt::Empty); // Stage 1: a single flat image; sections are Stage 3 layout
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
    // disp(An) / disp(PC)
    if let (Some(open), Some(stripped)) = (t.find('('), t.strip_suffix(')')) {
        let disp = parse_value(&t[..open], line)?;
        let base = stripped[open + 1..].trim();
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
    mos6502::parse_expr(raw, line, mos6502::parse_number, mos6502::BytePrec::Tight)
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
