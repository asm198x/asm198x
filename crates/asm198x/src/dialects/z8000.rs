//! The Zilog Z8000 dialect front-end (`asl` syntax), non-segmented (Z8002).
//!
//! Assembles against [`isa::z8000`] and produces a flat **big-endian** binary at
//! the `org`. Numbers are Intel `h`-suffix hex (shared with the 8080 dialect).
//! Registers are word `r0`–`r15`, byte `rh0`–`rh7` / `rl0`–`rl7`. Built as
//! sweep-verified increments (see `decisions/z8000-staged-build.md`); this is
//! **increment 1**, the dyadic arithmetic / logic / load family.
//!
//! A dyadic instruction packs its operands as fields in the opcode word, emitted
//! through the engine's computed-operand seam ([`Operation::Encoded`]): a
//! literal first word (`MM ooooo b | ssss dddd`) followed, for the immediate /
//! direct / indexed modes, by one 16-bit extension word (a byte immediate
//! replicated into both halves). Validated byte-identical against `asl`
//! (`cpu Z8002`).

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

/// A parsed operand in one of the increment-1 addressing modes.
enum Operand {
    /// A register (word `r0`–`r15` or byte `rh`/`rl`), by encoded number.
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

/// A plain 16-bit big-endian extension word (an address or a word immediate).
fn ext_word(expr: Expr) -> Piece {
    Piece::Val {
        expr,
        bytes: 2,
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
    let dst = operand(dst_s, insn.byte, line)?;
    let src = operand(src_s, insn.byte, line)?;
    let b = u16::from(!insn.byte); // b bit: 1 = word, 0 = byte

    // LD/LDB with a memory destination is a store (register → memory).
    if matches!(mn, "LD" | "LDB") && !matches!(dst, Operand::Reg(_)) {
        let Operand::Reg(srcreg) = src else {
            return Err(AsmError::new(
                line,
                format!("`{mn}` store needs a register source"),
            ));
        };
        let store =
            isa::z8000::store_entry(mn).ok_or_else(|| AsmError::new(line, "missing store form"))?;
        return store_form(store.op, b, &dst, srcreg, line);
    }

    // Otherwise the destination must be a register; the source varies.
    let Operand::Reg(dstreg) = dst else {
        return Err(AsmError::new(
            line,
            format!("`{mn}` destination must be a register"),
        ));
    };
    let top = |mm: u16| ((mm << 6) | (u16::from(insn.op) << 1) | b) as u8;
    let mut pieces = Vec::new();
    match src {
        Operand::Reg(s) => pieces.extend(word_lit(u16::from(top(0b10)) << 8 | (s << 4) | dstreg)),
        Operand::Ir(ptr) => {
            if insn.reg_only {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` allows only a register source"),
                ));
            }
            pieces.extend(word_lit(u16::from(top(0b00)) << 8 | (ptr << 4) | dstreg));
        }
        Operand::Imm(e) => {
            if insn.reg_only {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` allows only a register source"),
                ));
            }
            pieces.extend(word_lit(u16::from(top(0b00)) << 8 | dstreg));
            pieces.push(if insn.byte { byte_imm(e) } else { ext_word(e) });
        }
        Operand::Da(e) => {
            if insn.reg_only {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` allows only a register source"),
                ));
            }
            pieces.extend(word_lit(u16::from(top(0b01)) << 8 | dstreg));
            pieces.push(ext_word(e));
        }
        Operand::Indexed(e, idx) => {
            if insn.reg_only {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` allows only a register source"),
                ));
            }
            pieces.extend(word_lit(u16::from(top(0b01)) << 8 | (idx << 4) | dstreg));
            pieces.push(ext_word(e));
        }
    }
    Ok(Operation::Encoded(pieces))
}

/// Encode an `LD`/`LDB` store (register → memory): the second byte is the
/// memory pointer/index in the high nibble, the source register in the low.
fn store_form(
    op: u8,
    b: u16,
    dst: &Operand,
    srcreg: u16,
    line: usize,
) -> Result<Operation, AsmError> {
    let top = |mm: u16| ((mm << 6) | (u16::from(op) << 1) | b) as u8;
    let mut pieces = Vec::new();
    match dst {
        Operand::Ir(ptr) => {
            pieces.extend(word_lit(u16::from(top(0b00)) << 8 | (ptr << 4) | srcreg))
        }
        Operand::Da(e) => {
            pieces.extend(word_lit(u16::from(top(0b01)) << 8 | srcreg));
            pieces.push(ext_word(e.clone()));
        }
        Operand::Indexed(e, idx) => {
            pieces.extend(word_lit(u16::from(top(0b01)) << 8 | (idx << 4) | srcreg));
            pieces.push(ext_word(e.clone()));
        }
        _ => return Err(AsmError::new(line, "store destination must be memory")),
    }
    Ok(Operation::Encoded(pieces))
}

/// Parse an operand. `byte` selects byte-register naming for a bare register.
fn operand(tok: &str, byte: bool, line: usize) -> Result<Operand, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("bad operand `{tok}`"));

    if let Some(imm) = t.strip_prefix('#') {
        return Ok(Operand::Imm(value(imm, line)?));
    }
    if let Some(ptr) = t.strip_prefix('@') {
        // Indirect register uses a word register.
        return Ok(Operand::Ir(word_reg(ptr).ok_or_else(bad)?));
    }
    if let Some(open) = t.find('(') {
        let close = t.rfind(')').ok_or_else(bad)?;
        let idx = word_reg(&t[open + 1..close]).ok_or_else(bad)?;
        return Ok(Operand::Indexed(value(&t[..open], line)?, idx));
    }
    if let Some(r) = reg(t, byte) {
        return Ok(Operand::Reg(r));
    }
    // A bare expression is a direct address.
    Ok(Operand::Da(value(t, line)?))
}

/// Parse a register operand, byte (`rh`/`rl`) or word (`r`) per `byte`.
fn reg(tok: &str, byte: bool) -> Option<u16> {
    if byte { byte_reg(tok) } else { word_reg(tok) }
}

/// Word register `r0`–`r15`.
fn word_reg(tok: &str) -> Option<u16> {
    let t = tok.trim();
    let n = t.strip_prefix(['r', 'R'])?;
    // Reject `rh`/`rl` here so a byte register isn't taken as a word register.
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
