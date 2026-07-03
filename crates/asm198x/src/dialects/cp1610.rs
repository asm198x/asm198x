//! The General Instrument CP1610 dialect front-end (`asl` syntax).
//!
//! Assembles against [`isa::cp1610`] and produces a flat **big-endian** binary at
//! the `org`, one 16-bit word per decle. Numbers are Intel `h`-suffix hex (shared
//! with the 8080 dialect) and decimal; registers are `r0`–`r7`. The jzIntv /
//! as1600 mnemonics `asl` accepts under `cpu CP-1600` are the homebrew standard.
//!
//! **Increment 1** covers the single-decle register / implied groups, so every
//! instruction is exactly one opcode word (two literal bytes; the register
//! fields resolve at parse time) emitted through the engine's computed-operand
//! seam ([`Operation::Encoded`]). Later increments add the memory / immediate /
//! shift / branch groups with their extension words.
//!
//! Validated byte-identical against `asl` (`cpu CP-1600`).

use std::collections::BTreeMap;

use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};
use isa::cp1610::{Class, Insn};

/// The GI CP1610 dialect.
pub(crate) struct Cp1610;

impl Dialect for Cp1610 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::cp1610::SET
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
                parse_op(rest, line)?
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

fn parse_op(rest: &str, line: usize) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        "cpu" | "end" | "title" | "page" | "name" | "listing" | "relaxed" => return Ok(None),
        "org" => Operation::Org(value(args, line)?),
        "byte" | "db" | "dc.b" => Operation::Bytes(byte_list(args, line)?),
        "word" | "data" | "dw" | "dc.w" => Operation::Words(value_list(args, line)?),
        _ => encode(&word.to_ascii_uppercase(), args, line)?,
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

fn encode(mn: &str, args: &str, line: usize) -> Result<Operation, AsmError> {
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
