//! The General Instrument CP1610 dialect front-end (`asl` syntax).
//!
//! Assembles against [`isa::cp1610`] and produces a flat **big-endian** binary at
//! the `org`, one 16-bit word per decle. Numbers are Intel `h`-suffix hex (shared
//! with the 8080 dialect) and decimal; registers are `r0`–`r7`. The jzIntv /
//! as1600 mnemonics `asl` accepts under `cpu CP-1600` are the homebrew standard.
//!
//! **Increments 1–2** cover the single-decle register / implied and shift / rotate
//! groups — one opcode word, register fields resolved at parse time.
//! **Increment 3** adds the two-decle relative branches: `Bcc`/`BEXT`/`NOPP`
//! emit a [`Piece::Branch`], whose opcode word takes a direction bit from the
//! sign of the displacement (forward `EA = PC + d`, backward `EA = PC − d − 1`).
//! **Increment 4** adds the memory / immediate addressing modes. **Increment 5**
//! adds `JUMP`/`JSR` (a three-decle absolute form whose address is split across
//! two words) and makes the engine **word-addressed** for this dialect
//! ([`addr_unit`](Dialect::addr_unit) = 2), so a label is a decle number and an
//! absolute-address operand matches `asl`. **Increment 6** adds the stateful
//! `SDBD` prefix: after it, the next immediate is emitted as two low-byte-first
//! decles (tracked by an `after_sdbd` flag through the parse loop). All ride the
//! engine's computed-operand seam ([`Operation::Encoded`]) — the CP1610 is
//! **complete**.
//!
//! Validated byte-identical against `asl` (`cpu CP-1600`).

use std::collections::BTreeMap;

use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Piece, Statement};
use isa::cp1610::{Class, Insn};

/// The GI CP1610 dialect.
pub(crate) struct Cp1610;

impl Dialect for Cp1610 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::cp1610::SET
    }

    /// The CP1610 is word-addressed: each decle is two bytes, and `asl` counts
    /// addresses in decles, so a label advances by one per two emitted bytes.
    fn addr_unit(&self) -> i64 {
        2
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let mut out = Vec::new();
        let mut consts: BTreeMap<String, i64> = BTreeMap::new();
        // Whether the previous instruction was `SDBD` — it makes the *next*
        // immediate a two-decle (low-byte-first) value. Set by an `SDBD`, cleared
        // by any other instruction or directive; a label-only line leaves it be.
        let mut after_sdbd = false;

        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            let code = strip_comment(raw);
            if code.trim().is_empty() {
                continue;
            }
            if let Some((name, expr)) = constant(code.trim(), line)? {
                if let Ok(v) = fold_const(&expr, &consts, line) {
                    consts.insert(name.clone(), v);
                }
                out.push(Statement {
                    line,
                    label: Some(name),
                    op: Some(Operation::Equ(expr)),
                });
                continue;
            }
            let (label, rest) = split_label(code);
            let op = if rest.is_empty() {
                None
            } else {
                let (word, _) = split_first_word(rest);
                let op = parse_op(rest, line, after_sdbd)?;
                after_sdbd = word.eq_ignore_ascii_case("sdbd");
                op
            };
            if label.is_some() || op.is_some() {
                out.push(Statement { line, label, op });
            }
        }
        Ok(out)
    }
}

fn strip_comment(line: &str) -> &str {
    let (mut in_char, mut in_str) = (false, false);
    for (i, b) in line.bytes().enumerate() {
        match b {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b';' if !in_char && !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

fn constant(code: &str, line: usize) -> Result<Option<(String, Expr)>, AsmError> {
    let (first, rest) = split_first_word(code);
    if !rest.is_empty() {
        let (kw, tail) = split_first_word(rest);
        if kw.eq_ignore_ascii_case("equ") && is_ident(first) {
            return Ok(Some((first.to_string(), value(tail, line)?)));
        }
    }
    if let Some(eq) = mos6502::assignment_split(code) {
        let name = code[..eq].trim();
        if is_ident(name) {
            return Ok(Some((
                name.to_string(),
                value(code[eq + 1..].trim(), line)?,
            )));
        }
    }
    Ok(None)
}

fn split_label(code: &str) -> (Option<String>, &str) {
    if code.starts_with([' ', '\t']) {
        return (None, code.trim());
    }
    let trimmed = code.trim();
    let (word, rest) = split_first_word(trimmed);
    match word.strip_suffix(':') {
        Some(name) if is_ident(name) => (Some(name.to_string()), rest),
        _ => (None, trimmed),
    }
}

fn parse_op(rest: &str, line: usize, after_sdbd: bool) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        "cpu" | "end" | "title" | "page" | "name" | "listing" | "relaxed" => return Ok(None),
        "org" => Operation::Org(value(args, line)?),
        "byte" | "db" | "dc.b" => Operation::Bytes(byte_list(args, line)?),
        "word" | "data" | "dw" | "dc.w" => Operation::Words(value_list(args, line)?),
        _ => encode(&word.to_ascii_uppercase(), args, line, after_sdbd)?,
    };
    Ok(Some(op))
}

fn byte_list(args: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let mut out = Vec::new();
    for item in split_data_items(args) {
        if let Some(s) = string_literal(item) {
            out.extend(s.bytes().map(|b| Expr::Num(i64::from(b))));
        } else {
            out.push(value(item, line)?);
        }
    }
    Ok(out)
}

fn value_list(args: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    split_top_level(args, ',')
        .iter()
        .map(|p| value(p, line))
        .collect()
}

/// Parse a CP1610 expression: Intel `h`-suffix hex, decimal, `'c'` character.
fn value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    mos6502::parse_expr(
        raw,
        line,
        parse_number_intel,
        ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: Caret::Xor,
            at_is_pc: false,
        },
    )
}

// ---------------------------------------------------------------------------
// Instruction encoding
// ---------------------------------------------------------------------------

/// The two literal bytes of a decle, big-endian (high byte first). The decle is
/// 10-bit, so the high byte carries only its top two bits.
fn word_lit(w: u16) -> [Piece; 2] {
    [Piece::Lit((w >> 8) as u8), Piece::Lit(w as u8)]
}

/// A CP1610 branch piece: a two-decle relative branch whose opcode word takes a
/// direction bit (`0x20`) from the sign of the displacement. `base` is the opcode
/// with that bit clear; the decle displacement comes straight from the
/// word-addressed location counter.
fn branch(target: Expr, base: u16) -> Piece {
    Piece::Branch {
        target,
        base,
        dir_bit: 0x20,
        what: "branch",
    }
}

fn encode(mn: &str, args: &str, line: usize, after_sdbd: bool) -> Result<Operation, AsmError> {
    // Branches are multi-word with a computed, sign-directed target, so they are
    // handled ahead of the single-decle table.
    if mn.eq_ignore_ascii_case("NOPP") {
        // "Branch never" — a two-word no-op: opcode 0x0208, zero magnitude.
        if !args.trim().is_empty() {
            return Err(AsmError::new(line, "`NOPP` takes no operand"));
        }
        return Ok(Operation::Encoded(vec![
            Piece::Lit(0x02),
            Piece::Lit(0x08),
            Piece::Lit(0x00),
            Piece::Lit(0x00),
        ]));
    }
    if mn.eq_ignore_ascii_case("BEXT") {
        // `BEXT target, ec` — external-condition branch; `ec` (0–15) sits in the
        // low nibble with bit 4 set (0x210 page).
        let ops = split_top_level(args.trim(), ',');
        let [t, e] = ops.as_slice() else {
            return Err(AsmError::new(line, "`BEXT` takes a target and a condition"));
        };
        let ec = const_field(e, 0, 15, "external condition", line)?;
        return Ok(Operation::Encoded(vec![branch(
            value(t, line)?,
            0x210 | ec,
        )]));
    }
    if let Some(cond) = isa::cp1610::branch_cond(mn) {
        let target = one_str(args, mn, line)?;
        return Ok(Operation::Encoded(vec![branch(
            value(target, line)?,
            0x200 | u16::from(cond),
        )]));
    }
    if let Some(op) = encode_jump(mn, args, line)? {
        return Ok(op);
    }
    if let Some(op) = encode_mem(mn, args, line, after_sdbd)? {
        return Ok(op);
    }

    let insn = isa::cp1610::lookup(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let word = match insn.class {
        Class::Implied => {
            if !ops.is_empty() {
                return Err(AsmError::new(line, format!("`{mn}` takes no operand")));
            }
            insn.base
        }
        Class::RegUnary => {
            let r = reg(one(&ops, insn, line)?, 7, line)?;
            insn.base | r
        }
        Class::GetStatus => {
            let r = reg(one(&ops, insn, line)?, 3, line)?;
            insn.base | r
        }
        Class::RegReg => {
            let [s, d] = two(&ops, insn, line)?;
            let (src, dst) = (reg(s, 7, line)?, reg(d, 7, line)?);
            insn.base | (src << 3) | dst
        }
        Class::Shift => {
            // `mn Rd` (shift once) or `mn Rd,2` (shift twice); R0–R3 only.
            let (r, count) = match ops.as_slice() {
                [r] => (reg(r, 3, line)?, 1),
                [r, c] => (reg(r, 3, line)?, shift_count(c, line)?),
                _ => {
                    return Err(AsmError::new(
                        line,
                        format!("`{mn}` takes a register and an optional count"),
                    ));
                }
            };
            insn.base | ((count - 1) << 2) | r
        }
    };
    Ok(Operation::Encoded(Vec::from(word_lit(word))))
}

/// Require exactly one operand.
fn one<'a>(ops: &[&'a str], insn: &Insn, line: usize) -> Result<&'a str, AsmError> {
    match ops {
        [a] => Ok(a),
        _ => Err(AsmError::new(
            line,
            format!("`{}` takes one operand", insn.mnemonic),
        )),
    }
}

/// Require exactly two operands.
fn two<'a>(ops: &[&'a str], insn: &Insn, line: usize) -> Result<[&'a str; 2], AsmError> {
    match ops {
        [a, b] => Ok([*a, *b]),
        _ => Err(AsmError::new(
            line,
            format!("`{}` takes two operands", insn.mnemonic),
        )),
    }
}

/// Parse a register operand `r0`–`rMAX` to its number, rejecting out-of-range
/// registers (e.g. `GSWD` allows only `R0`–`R3`).
fn reg(tok: &str, max: u16, line: usize) -> Result<u16, AsmError> {
    let n = tok
        .trim()
        .strip_prefix(['r', 'R'])
        .and_then(|n| n.parse::<u16>().ok())
        .filter(|&n| n <= max);
    n.ok_or_else(|| AsmError::new(line, format!("expected register r0..r{max}, got `{tok}`")))
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Bin(op, Box::new(a), Box::new(b))
}

/// Encode a `J`/`JE`/`JD` jump or `JSR`/`JSRE`/`JSRD` call, or `None` if `mn` is
/// neither. The three-decle form is `0x0004`, then a word carrying the return
/// register (`rr`: R4–R6 = 0–2, or 3 for a plain `J`) in bits `9:8`, the
/// interrupt action (`ii`: none / enable / disable = 0 / 1 / 2) in bits `1:0`,
/// and the address's high six bits (`addr >> 10`) in bits `7:2`; then a word with
/// the low ten bits (`addr & 0x3FF`). Both address words are built as expressions
/// so a forward label resolves in pass two.
fn encode_jump(mn: &str, args: &str, line: usize) -> Result<Option<Operation>, AsmError> {
    let (is_call, ii): (bool, u16) = match mn.to_ascii_uppercase().as_str() {
        "J" => (false, 0),
        "JE" => (false, 1),
        "JD" => (false, 2),
        "JSR" => (true, 0),
        "JSRE" => (true, 1),
        "JSRD" => (true, 2),
        _ => return Ok(None),
    };
    let (rr, addr) = if is_call {
        // `JSR Rr, addr` — the return register is R4–R6 (rr 0–2).
        let ops = split_top_level(args.trim(), ',');
        let [r, a] = ops.as_slice() else {
            return Err(AsmError::new(
                line,
                format!("`{mn}` takes a register and an address"),
            ));
        };
        let n = reg(r, 7, line)?;
        if !(4..=6).contains(&n) {
            return Err(AsmError::new(
                line,
                "`JSR` return register must be r4, r5 or r6",
            ));
        }
        (n - 4, value(a, line)?)
    } else {
        // `J addr` — no return register; rr is 3.
        (3, value(one_str(args, mn, line)?, line)?)
    };
    let regint = (rr << 8) | ii;
    // decle2 = regint | ((addr >> 10) & 0x3F) << 2
    let hi = bin(
        BinOp::Shl,
        bin(
            BinOp::And,
            bin(BinOp::Shr, addr.clone(), Expr::Num(10)),
            Expr::Num(0x3F),
        ),
        Expr::Num(2),
    );
    let decle2 = bin(BinOp::Or, Expr::Num(i64::from(regint)), hi);
    let decle3 = bin(BinOp::And, addr, Expr::Num(0x3FF));
    Ok(Some(Operation::Encoded(vec![
        Piece::Lit(0x00),
        Piece::Lit(0x04),
        ext_word(decle2),
        ext_word(decle3),
    ])))
}

/// A plain 16-bit extension word — a direct address or an immediate, in the
/// following decle.
fn ext_word(expr: Expr) -> Piece {
    Piece::Val {
        expr,
        bytes: 2,
        rel: false,
        signed: false,
    }
}

/// Parse an indirect pointer register `R1`–`R6` to its mode (1–6). The `@` sits
/// on the mnemonic (`MVI@`), not the operand, so the register is written bare.
/// `R0` is not a pointer (mode 0 is direct addressing) and `R7` is the immediate
/// mode, so both are rejected here — matching `asl`.
fn ptr_reg(tok: &str, line: usize) -> Result<u16, AsmError> {
    let n = tok
        .trim()
        .strip_prefix(['r', 'R'])
        .and_then(|n| n.parse::<u16>().ok())
        .filter(|&n| (1..=6).contains(&n));
    n.ok_or_else(|| AsmError::new(line, format!("expected pointer r1..r6, got `{tok}`")))
}

/// Encode a memory-referencing instruction (`MVI`/`MVO`/`ADD`/… and `PSHR`/
/// `PULR`), or `None` if `mn` is not one — leaving it to the single-decle table.
/// The mnemonic suffix picks the addressing mode: bare = direct (`mm=0`, a
/// following address word), `@` = indirect `@R1`–`@R6` (`mm=1..6`), `I` =
/// immediate (`mm=7`, a following value word). `MVO` is a store, so its register
/// operand comes first; the loads/ALU ops take the register last.
fn encode_mem(
    mn: &str,
    args: &str,
    line: usize,
    after_sdbd: bool,
) -> Result<Option<Operation>, AsmError> {
    let upper = mn.to_ascii_uppercase();
    // `PSHR`/`PULR` are the R6-stack aliases of `MVO@ Rs,R6` / `MVI@ R6,Rd`.
    if upper == "PSHR" || upper == "PULR" {
        let r = reg(one_str(args, mn, line)?, 7, line)?;
        let base = if upper == "PSHR" { 0x240 } else { 0x280 };
        return Ok(Some(Operation::Encoded(Vec::from(word_lit(
            base | (6 << 3) | r,
        )))));
    }

    // Classify the mnemonic into its family and addressing mode by suffix.
    let (fam_name, mode): (&str, MemMode) = if let Some(f) = upper.strip_suffix('@') {
        (f, MemMode::Indirect)
    } else if let Some(f) = upper.strip_suffix('I').filter(|f| is_mem_family(f)) {
        (f, MemMode::Immediate)
    } else {
        (upper.as_str(), MemMode::Direct)
    };
    let Some(fam) = isa::cp1610::mem_family_by_name(fam_name) else {
        return Ok(None); // not a memory instruction
    };

    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let [a, b] = ops.as_slice() else {
        return Err(AsmError::new(line, format!("`{mn}` takes two operands")));
    };

    let pieces = match mode {
        MemMode::Direct => {
            // Loads/ALU: `MN addr, Rd`. Store: `MN Rs, addr`.
            let (reg_tok, addr_tok) = if fam.store { (a, b) } else { (b, a) };
            let r = reg(reg_tok, 7, line)?;
            vec![
                Piece::Lit((fam.base >> 8) as u8),
                Piece::Lit((fam.base | r) as u8),
                ext_word(value(addr_tok, line)?),
            ]
        }
        MemMode::Indirect => {
            // Loads/ALU: `MN@ @Rp, Rd`. Store: `MN@ Rs, @Rp`.
            let (ptr_tok, reg_tok) = if fam.store { (b, a) } else { (a, b) };
            let m = ptr_reg(ptr_tok, line)?;
            let r = reg(reg_tok, 7, line)?;
            Vec::from(word_lit(fam.base | (m << 3) | r))
        }
        MemMode::Immediate => {
            if fam.store {
                return Err(AsmError::new(line, "`MVO` has no immediate form"));
            }
            // `MNI imm, Rd`.
            let r = reg(b, 7, line)?;
            let imm = value(a, line)?;
            let mut v = vec![
                Piece::Lit((fam.base >> 8) as u8),
                Piece::Lit((fam.base | (7 << 3) | r) as u8),
            ];
            if after_sdbd {
                // Under `SDBD` the immediate is two decles, low byte first.
                v.push(ext_word(bin(BinOp::And, imm.clone(), Expr::Num(0xFF))));
                v.push(ext_word(bin(
                    BinOp::And,
                    bin(BinOp::Shr, imm, Expr::Num(8)),
                    Expr::Num(0xFF),
                )));
            } else {
                v.push(ext_word(imm));
            }
            v
        }
    };
    Ok(Some(Operation::Encoded(pieces)))
}

/// Whether `name` (already upper-case) is a memory family mnemonic.
fn is_mem_family(name: &str) -> bool {
    isa::cp1610::mem_family_by_name(name).is_some()
}

/// The addressing mode a memory mnemonic's suffix selects.
enum MemMode {
    Direct,
    Indirect,
    Immediate,
}

/// Require exactly one operand from a raw argument string (a branch target).
fn one_str<'a>(args: &'a str, mn: &str, line: usize) -> Result<&'a str, AsmError> {
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    match ops.as_slice() {
        [a] => Ok(a),
        _ => Err(AsmError::new(line, format!("`{mn}` takes one operand"))),
    }
}

/// Evaluate a constant field (e.g. `BEXT`'s external condition) and range-check
/// it. It must resolve to a constant, so a forward reference is an error.
fn const_field(tok: &str, min: i64, max: i64, what: &str, line: usize) -> Result<u16, AsmError> {
    let v = fold_const(&value(tok, line)?, &BTreeMap::new(), line)?;
    if !(min..=max).contains(&v) {
        return Err(AsmError::new(
            line,
            format!("{what} out of range ({v}; must be {min}..={max})"),
        ));
    }
    Ok(v as u16)
}

/// Parse a shift count — either `1` or `2` (a shift shifts once or twice).
fn shift_count(tok: &str, line: usize) -> Result<u16, AsmError> {
    match tok.trim() {
        "1" => Ok(1),
        "2" => Ok(2),
        other => Err(AsmError::new(
            line,
            format!("shift count must be 1 or 2, got `{other}`"),
        )),
    }
}
