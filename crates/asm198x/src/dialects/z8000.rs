//! The Zilog Z8000 dialect front-end (`asl` syntax), non-segmented (Z8002).
//!
//! Assembles against [`isa::z8000`] and produces a flat **big-endian** binary at
//! the `org`. Numbers are Intel `h`-suffix hex (shared with the 8080 dialect).
//! Registers are word `r0`–`r15`, byte `rh0`–`rh7` / `rl0`–`rl7`, long
//! `rr0`–`rr14`. Built as sweep-verified increments (see
//! `decisions/z8000-staged-build.md`); this covers the **dyadic family**
//! (increments 1–2): arithmetic / logic / compare / load / exchange /
//! load-address.
//!
//! A dyadic instruction packs its operands as fields in the opcode word, emitted
//! through the engine's computed-operand seam ([`Operation::Encoded`]): a
//! literal first word (`MM base6 | ssss dddd`) followed, for the immediate /
//! direct / indexed modes, by an extension word (a byte immediate replicated
//! into both halves, or a 32-bit long immediate). The instruction's
//! [`Size`](isa::z8000::Size) fixes register naming and immediate width; its
//! modes bitmask gates which addressing modes are legal. Validated byte-identical
//! against `asl` (`cpu Z8002`).

use std::collections::BTreeMap;

use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Piece, Statement};

/// The Zilog Z8000 dialect (non-segmented Z8002).
pub(crate) struct Z8000;

impl Dialect for Z8000 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z8000::SET
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
        "cpu" | "end" | "title" | "page" | "name" | "listing" => return Ok(None),
        "org" | "aorg" | "rorg" => Operation::Org(value(args, line)?),
        "byte" | "db" | "dc.b" => Operation::Bytes(byte_list(args, line)?),
        "word" | "dw" | "dc.w" => Operation::Words(value_list(args, line)?),
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

/// Parse a Z8000 expression: Intel `h`-suffix hex, decimal, `'c'` character.
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

use isa::z8000::{Insn, Size};

/// A parsed operand and the addressing mode it implies.
enum Operand {
    /// A register (word / byte / long per the instruction size), by number.
    Reg(u16),
    /// Immediate `#n`.
    Imm(Expr),
    /// Indirect register `@Rn`.
    Ir(u16),
    /// Direct address `addr`.
    Da(Expr),
    /// Indexed `addr(Rn)`.
    Indexed(Expr, u16),
}

/// The two literal bytes of an opcode word, big-endian (high byte first).
fn word_lit(w: u16) -> [Piece; 2] {
    [Piece::Lit((w >> 8) as u8), Piece::Lit(w as u8)]
}

/// A big-endian extension word (an address or a word immediate).
fn ext_word(expr: Expr) -> Piece {
    Piece::Val {
        expr,
        bytes: 2,
        rel: false,
        signed: false,
    }
}

/// A 32-bit big-endian long immediate (two words).
fn ext_long(expr: Expr) -> Piece {
    Piece::Val {
        expr,
        bytes: 4,
        rel: false,
        signed: false,
    }
}

/// A byte immediate replicated into both halves of its extension word, as `asl`
/// lays it down: `(v & 0xFF) | ((v & 0xFF) << 8)`.
fn byte_imm(expr: Expr) -> Piece {
    let lo = Expr::Bin(BinOp::And, Box::new(expr), Box::new(Expr::Num(0xFF)));
    let dup = Expr::Bin(
        BinOp::Or,
        Box::new(lo.clone()),
        Box::new(Expr::Bin(BinOp::Shl, Box::new(lo), Box::new(Expr::Num(8)))),
    );
    ext_word(dup)
}

/// The addressing-mode group (`MM`) for a mode bit.
fn mm(mode: u8) -> u16 {
    use isa::z8000::{IM, IR, R};
    if mode & (IM | IR) != 0 {
        0
    } else if mode == R {
        2
    } else {
        1
    }
}

/// The immediate extension piece for a source of the given size.
fn imm_piece(e: Expr, size: Size) -> Piece {
    match size {
        Size::Byte => byte_imm(e),
        Size::Long => ext_long(e),
        _ => ext_word(e),
    }
}

fn encode(mn: &str, args: &str, line: usize) -> Result<Operation, AsmError> {
    let insn = isa::z8000::lookup(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    let ops = split_top_level(args.trim(), ',');
    let ops: Vec<&str> = ops
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let [dst_s, src_s] = match ops.as_slice() {
        [a, b] => [*a, *b],
        _ => return Err(AsmError::new(line, format!("`{mn}` takes two operands"))),
    };
    let dst = operand(dst_s, insn.size, line)?;
    let src = operand(src_s, insn.size, line)?;

    // A store-capable load with a memory destination is a store.
    if let (Some(store), false) = (isa::z8000::store_entry(mn), matches!(dst, Operand::Reg(_))) {
        let Operand::Reg(srcreg) = src else {
            return Err(AsmError::new(
                line,
                format!("`{mn}` store needs a register source"),
            ));
        };
        return dyadic(store, &dst, srcreg, line);
    }

    // Otherwise the destination is a register; the source is the varying operand.
    let Operand::Reg(dstreg) = dst else {
        return Err(AsmError::new(
            line,
            format!("`{mn}` destination must be a register"),
        ));
    };
    dyadic(insn, &src, dstreg, line)
}

/// Encode one dyadic form: `variable` is the memory/immediate/register operand
/// whose mode is being encoded; `reg` is the fixed register (destination for a
/// load, source for a store) that occupies the second byte's low nibble.
fn dyadic(insn: &Insn, variable: &Operand, reg: u16, line: usize) -> Result<Operation, AsmError> {
    let (mode, field, ext): (u8, u16, Option<Piece>) = match variable {
        Operand::Reg(s) => (isa::z8000::R, *s, None),
        Operand::Ir(p) => (isa::z8000::IR, *p, None),
        Operand::Imm(e) => (isa::z8000::IM, 0, Some(imm_piece(e.clone(), insn.size))),
        Operand::Da(e) => (isa::z8000::DA, 0, Some(ext_word(e.clone()))),
        Operand::Indexed(e, i) => (isa::z8000::X, *i, Some(ext_word(e.clone()))),
    };
    if insn.modes & mode == 0 {
        return Err(AsmError::new(
            line,
            format!("`{}` does not allow that addressing mode", insn.mnemonic),
        ));
    }
    let top = (mm(mode) << 6) | u16::from(insn.base6);
    let mut pieces = Vec::from(word_lit((top << 8) | (field << 4) | reg));
    pieces.extend(ext);
    Ok(Operation::Encoded(pieces))
}

/// Parse an operand; a bare register is named per the instruction `size`.
fn operand(tok: &str, size: Size, line: usize) -> Result<Operand, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("bad operand `{tok}`"));

    if let Some(imm) = t.strip_prefix('#') {
        return Ok(Operand::Imm(value(imm, line)?));
    }
    if let Some(ptr) = t.strip_prefix('@') {
        return Ok(Operand::Ir(word_reg(ptr).ok_or_else(bad)?));
    }
    if let Some(open) = t.find('(') {
        let close = t.rfind(')').ok_or_else(bad)?;
        let idx = word_reg(&t[open + 1..close]).ok_or_else(bad)?;
        return Ok(Operand::Indexed(value(&t[..open], line)?, idx));
    }
    if let Some(r) = size_reg(t, size) {
        return Ok(Operand::Reg(r));
    }
    // A bare expression is a direct address.
    Ok(Operand::Da(value(t, line)?))
}

/// Parse a register named for the instruction size. `Address` uses a word
/// register (the `LDA` destination).
fn size_reg(tok: &str, size: Size) -> Option<u16> {
    match size {
        Size::Byte => byte_reg(tok),
        Size::Long => long_reg(tok),
        Size::Word | Size::Address => word_reg(tok),
    }
}

/// Word register `r0`–`r15`.
fn word_reg(tok: &str) -> Option<u16> {
    let n = tok.trim().strip_prefix(['r', 'R'])?;
    // Reject `rh`/`rl`/`rr`/`rq` so a byte/long register isn't taken as a word.
    if n.starts_with(['h', 'H', 'l', 'L', 'r', 'R', 'q', 'Q']) {
        return None;
    }
    n.parse::<u16>().ok().filter(|&v| v < 16)
}

/// Byte register `rh0`–`rh7` (0–7) or `rl0`–`rl7` (8–15).
fn byte_reg(tok: &str) -> Option<u16> {
    let t = tok.trim().to_ascii_lowercase();
    let (base, rest) = if let Some(r) = t.strip_prefix("rh") {
        (0u16, r)
    } else {
        (8u16, t.strip_prefix("rl")?)
    };
    rest.parse::<u16>()
        .ok()
        .filter(|&v| v < 8)
        .map(|n| base + n)
}

/// Long register pair `rr0`–`rr14` (even).
fn long_reg(tok: &str) -> Option<u16> {
    let n = tok
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("rr")?
        .parse::<u16>()
        .ok()?;
    (n < 16 && n % 2 == 0).then_some(n)
}
