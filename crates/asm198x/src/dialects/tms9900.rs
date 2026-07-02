//! The TI TMS9900 dialect front-end (`asl` syntax).
//!
//! Assembles against [`isa::tms9900`] and produces a flat **big-endian** binary
//! at the `org`. Numbers are Intel `h`-suffix hex (shared with the 8080 dialect)
//! and decimal; registers are `r0`–`r15`. The general-addressing operand shapes
//! — `Rn`, `*Rn`, `@addr` / `@addr(Rn)`, `*Rn+` — parse into a 2-bit `T` mode +
//! 4-bit register field; a symbolic/indexed operand appends one absolute address
//! word.
//!
//! Every instruction is one 16-bit opcode word plus 0–2 extension words, emitted
//! through the engine's computed-operand seam ([`Operation::Encoded`]). The
//! opcode word is usually two literal bytes (the register / mode / count fields
//! resolve at parse time); the jump and CRU-bit classes pack a range-checked
//! displacement into the word via [`Piece::Packed`] (the jumps word-scaled, so a
//! `scale` of 2 that also enforces `asl`'s even-distance rule). Extension words —
//! symbolic addresses and immediates — are plain 16-bit [`Piece::Val`]s; the
//! source operand's precedes the destination's.
//!
//! Validated byte-identical against `asl` (`cpu TMS9900`).

use std::collections::BTreeMap;

use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Piece, Statement};
use isa::tms9900::{Class, Insn};

/// The TI TMS9900 dialect.
pub(crate) struct Tms9900;

impl Dialect for Tms9900 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::tms9900::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let mut out = Vec::new();
        let mut consts: BTreeMap<String, i64> = BTreeMap::new();

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
                parse_op(rest, &consts, line)?
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

fn parse_op(
    rest: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        "cpu" | "end" | "title" | "page" | "name" | "listing" => return Ok(None),
        "org" | "aorg" | "rorg" => Operation::Org(value(args, line)?),
        "byte" | "db" | "dc.b" => Operation::Bytes(byte_list(args, line)?),
        "word" | "data" | "dw" | "dc.w" => Operation::Words(value_list(args, line)?),
        _ => encode(&word.to_ascii_uppercase(), args, consts, line)?,
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

/// Parse a TMS9900 expression: Intel `h`-suffix hex, decimal, `'c'` character.
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

/// A parsed general-addressing operand: its 2-bit `T` mode, 4-bit register, and,
/// for symbolic / indexed modes, the absolute address extension word.
struct General {
    t: u16,
    reg: u16,
    ext: Option<Expr>,
}

/// The two literal bytes of an opcode word, big-endian (high byte first).
fn word_lit(w: u16) -> [Piece; 2] {
    [Piece::Lit((w >> 8) as u8), Piece::Lit(w as u8)]
}

/// A plain 16-bit extension word (an absolute address or an immediate).
fn ext_piece(ext: Option<Expr>) -> Option<Piece> {
    ext.map(|expr| Piece::Val {
        expr,
        bytes: 2,
        rel: false,
        signed: false,
    })
}

fn encode(
    mn: &str,
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let insn = isa::tms9900::lookup(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let mut pieces = Vec::new();

    match insn.class {
        Class::DualGeneral => {
            let [s, d] = two(&ops, insn, line)?;
            let (src, dst) = (general(s, line)?, general(d, line)?);
            let w = insn.base | (dst.t << 10) | (dst.reg << 6) | (src.t << 4) | src.reg;
            pieces.extend(word_lit(w));
            pieces.extend(ext_piece(src.ext));
            pieces.extend(ext_piece(dst.ext));
        }
        Class::DualRegDst => {
            let [s, d] = two(&ops, insn, line)?;
            let src = general(s, line)?;
            let reg = register(d, line)?;
            pieces.extend(word_lit(insn.base | (reg << 6) | (src.t << 4) | src.reg));
            pieces.extend(ext_piece(src.ext));
        }
        Class::Xop => {
            let [s, d] = two(&ops, insn, line)?;
            let src = general(s, line)?;
            let num = field(d, consts, 0, 15, "XOP number", line)?;
            pieces.extend(word_lit(insn.base | (num << 6) | (src.t << 4) | src.reg));
            pieces.extend(ext_piece(src.ext));
        }
        Class::CruMulti => {
            let [s, c] = two(&ops, insn, line)?;
            let src = general(s, line)?;
            // Count is 1..=16; 16 encodes as the 4-bit field 0.
            let count = field(c, consts, 1, 16, "CRU count", line)? & 0xF;
            pieces.extend(word_lit(insn.base | (count << 6) | (src.t << 4) | src.reg));
            pieces.extend(ext_piece(src.ext));
        }
        Class::Shift => {
            let [r, c] = two(&ops, insn, line)?;
            let reg = register(r, line)?;
            // Count is 0..=15; 0 means "count in R0".
            let count = field(c, consts, 0, 15, "shift count", line)?;
            pieces.extend(word_lit(insn.base | (count << 4) | reg));
        }
        Class::SingleGeneral => {
            let s = one(&ops, insn, line)?;
            let src = general(s, line)?;
            pieces.extend(word_lit(insn.base | (src.t << 4) | src.reg));
            pieces.extend(ext_piece(src.ext));
        }
        Class::Control => {
            if !ops.is_empty() {
                return Err(AsmError::new(line, format!("`{mn}` takes no operand")));
            }
            pieces.extend(word_lit(insn.base));
        }
        Class::ImmReg => {
            let [r, imm] = two(&ops, insn, line)?;
            let reg = register(r, line)?;
            pieces.extend(word_lit(insn.base | reg));
            pieces.extend(ext_piece(Some(value(imm, line)?)));
        }
        Class::ImmOnly => {
            let imm = one(&ops, insn, line)?;
            pieces.extend(word_lit(insn.base));
            pieces.extend(ext_piece(Some(value(imm, line)?)));
        }
        Class::StoreReg => {
            let r = one(&ops, insn, line)?;
            pieces.extend(word_lit(insn.base | register(r, line)?));
        }
        Class::Jump => {
            let target = value(one(&ops, insn, line)?, line)?;
            // Byte distance from the following word, word-scaled to the field.
            pieces.push(Piece::Packed {
                expr: sub(target, add(Expr::Pc, Expr::Num(2))),
                bytes: 2,
                scale: 2,
                min: -128,
                max: 127,
                mask: 0xFF,
                or_bits: u32::from(insn.base),
                what: "jump distance",
            });
        }
        Class::Cru => {
            let disp = value(one(&ops, insn, line)?, line)?;
            pieces.push(Piece::Packed {
                expr: disp,
                bytes: 2,
                scale: 1,
                min: -128,
                max: 127,
                mask: 0xFF,
                or_bits: u32::from(insn.base),
                what: "CRU bit displacement",
            });
        }
    }
    Ok(Operation::Encoded(pieces))
}

fn add(a: Expr, b: Expr) -> Expr {
    Expr::Bin(BinOp::Add, Box::new(a), Box::new(b))
}
fn sub(a: Expr, b: Expr) -> Expr {
    Expr::Bin(BinOp::Sub, Box::new(a), Box::new(b))
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

/// Evaluate a parse-time constant field (a shift/CRU count, XOP or register
/// number) and range-check it. These fields must resolve to a constant, so a
/// forward reference is an error — as it is in `asl`.
fn field(
    tok: &str,
    consts: &BTreeMap<String, i64>,
    min: i64,
    max: i64,
    what: &str,
    line: usize,
) -> Result<u16, AsmError> {
    let v = fold_const(&value(tok, line)?, consts, line)?;
    if !(min..=max).contains(&v) {
        return Err(AsmError::new(
            line,
            format!("{what} out of range ({v}; must be {min}..={max})"),
        ));
    }
    Ok(v as u16)
}

/// Parse a bare workspace-register operand (`r0`–`r15`) to its number.
fn register(tok: &str, line: usize) -> Result<u16, AsmError> {
    parse_reg(tok).ok_or_else(|| AsmError::new(line, format!("expected a register, got `{tok}`")))
}

fn parse_reg(tok: &str) -> Option<u16> {
    tok.trim()
        .strip_prefix(['r', 'R'])
        .and_then(|n| n.parse::<u16>().ok())
        .filter(|&n| n < 16)
}

/// Parse a general-addressing operand into its `T` mode, register, and optional
/// absolute address extension word.
///
/// `Rn` (0), `*Rn` (1), `@addr` (2, reg 0), `@addr(Rn)` (2, reg n≠0), `*Rn+` (3).
fn general(tok: &str, line: usize) -> Result<General, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("bad operand `{tok}`"));

    // Symbolic / indexed: @addr or @addr(Rn).
    if let Some(rest) = t.strip_prefix('@') {
        if let Some(open) = rest.find('(') {
            let close = rest.rfind(')').ok_or_else(bad)?;
            let reg = parse_reg(&rest[open + 1..close]).ok_or_else(bad)?;
            if reg == 0 {
                // @addr(R0) is illegal — R0 cannot index.
                return Err(AsmError::new(line, "invalid register: R0 cannot index"));
            }
            let addr = value(&rest[..open], line)?;
            return Ok(General {
                t: 2,
                reg,
                ext: Some(addr),
            });
        }
        return Ok(General {
            t: 2,
            reg: 0,
            ext: Some(value(rest, line)?),
        });
    }

    // Indirect / autoincrement: *Rn or *Rn+.
    if let Some(rest) = t.strip_prefix('*') {
        let (body, auto) = match rest.strip_suffix('+') {
            Some(r) => (r, true),
            None => (rest, false),
        };
        let reg = parse_reg(body).ok_or_else(bad)?;
        return Ok(General {
            t: if auto { 3 } else { 1 },
            reg,
            ext: None,
        });
    }

    // Workspace register.
    let reg = parse_reg(t).ok_or_else(bad)?;
    Ok(General {
        t: 0,
        reg,
        ext: None,
    })
}
