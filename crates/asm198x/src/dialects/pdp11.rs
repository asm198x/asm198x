//! The DEC PDP-11 dialect front-end (`asl` syntax).
//!
//! Assembles against [`isa::pdp11`] and produces a flat binary at the `org`.
//! **Little-endian**; numbers are decimal by default with `0x` hex / `0b` binary
//! (asl's C-style radix for the PDP-11). Registers are `r0`–`r7`, with `sp` =
//! `r6` and `pc` = `r7`.
//!
//! Every instruction is one 16-bit opcode word plus 0–2 extension words. The
//! opcode word packs each operand as a 6-bit `mode << 3 | reg` field; the dialect
//! parses the eight addressing modes into those fields and emits the word — and
//! any extension words — through the engine's computed-operand seam
//! ([`Operation::Encoded`]). The opcode word is usually a pair of literal bytes
//! (the fields resolve at parse time); the branch / `SOB` / `EMT` / `TRAP` /
//! `MARK` / `SPL` classes pack a range-checked operand into the word instead, so
//! their word rides a [`Piece::Packed`]. Extension words — index displacements,
//! immediates, absolute addresses — are plain 16-bit [`Piece::Val`]s; a
//! PC-relative operand (`addr` / `@addr`) is a `Val` with `rel` set, so the
//! engine lays down `target − PC_after` exactly as the hardware computes it. The
//! source operand's extension word precedes the destination's.
//!
//! Validated byte-identical against `asl` (`cpu MICROPDP-11/93`, its most
//! complete integer model).

use std::collections::BTreeMap;

use super::asl::{self, AslChip};
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, is_ident, split_data_items, split_first_word, split_top_level,
    string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Piece, Statement};
use crate::source::{SourceLoader, SourceMap};
use isa::pdp11::{Class, Insn};

/// The DEC PDP-11 dialect.
pub(crate) struct Pdp11;

impl Dialect for Pdp11 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::pdp11::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (0b field-packed migration):
        // parse into a `Program`, then lower to the engine's statement stream —
        // byte-identical to the old direct parse (AE1).
        crate::ast::lower(parse_program(source)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source)?))
    }

    /// The include-capable parse (language-surface U4): the shared asl-family
    /// walk, resolving `include`/`binclude` lazily through the loader — see
    /// [`parse_program_multi`].
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        crate::ast::lower(parse_program_multi(map, loader)?)
    }

    /// asl `equ` (and `name = expr`) takes no colon on its label; a colon would
    /// fail to reassemble, since the label is disambiguated by the keyword / `=`.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse PDP-11 source into the semantic [`Program`](crate::ast::Program)
/// via the shared asl-family walk ([`asl::parse_single`]): each line becomes
/// a node with its (global) label, operation, verbatim source, span, and
/// comment trivia — [`lower`](crate::ast::lower) reproduces the old
/// statements exactly. An `include`/`binclude` stays an unresolved item — the
/// target is never opened here (U4, KTD1).
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    asl::parse_single(Chip, source)
}

/// Parse a multi-file PDP-11 program (language-surface U4): the shared
/// asl-family interleaved walk with asl's probe-pinned semantics — see
/// [`asl::parse_multi_files`].
///
/// # Errors
/// Any per-line parse failure (stamped with its file), a missing target, an
/// include cycle, a bad `binclude` window, or the depth backstop — all at the
/// directive's span.
pub(crate) fn parse_program_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<crate::ast::Program, AsmError> {
    asl::parse_multi_files(Chip, map, loader, &asl::SEMANTICS)
}

/// The PDP-11's hooks into the shared asl-family walk (its own comment
/// scanner, constant recogniser, label split, number lexer, and operation
/// parse).
struct Chip;

impl AslChip for Chip {
    fn split_comment<'a>(&self, line: &'a str) -> (&'a str, Option<&'a str>) {
        split_comment(line)
    }

    fn constant(
        &self,
        code: &str,
        line: usize,
    ) -> Result<Option<(String, Expr, String)>, AsmError> {
        constant(code, line)
    }

    fn split_label<'a>(&self, code: &'a str) -> (Option<String>, &'a str) {
        split_label(code)
    }

    fn parse_op(
        &mut self,
        rest: &str,
        _consts: &BTreeMap<String, i64>,
        line: usize,
    ) -> Result<Option<Operation>, AsmError> {
        parse_op(rest, line)
    }

    fn value(&self, raw: &str, line: usize) -> Result<Expr, AsmError> {
        value(raw, line)
    }
}

/// Split a line into its code and its `;` comment (leading `;` and whitespace
/// trimmed) for carrying comments as AST trivia. Defined via [`strip_comment`] so
/// the comment is exactly what it removes — no behaviour change to assembly.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
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

/// `NAME EQU expr` or `NAME = expr`. Returns the name, the value expression, and
/// the operation's source text (`EQU expr` / `= expr`) so the formatter can
/// re-emit `NAME <source>` with the label kept on the same line.
fn constant(code: &str, line: usize) -> Result<Option<(String, Expr, String)>, AsmError> {
    let (first, rest) = split_first_word(code);
    if !rest.is_empty() {
        let (kw, tail) = split_first_word(rest);
        if kw.eq_ignore_ascii_case("equ") && is_ident(first) {
            return Ok(Some((
                first.to_string(),
                value(tail, line)?,
                rest.trim().to_string(),
            )));
        }
    }
    if let Some(eq) = mos6502::assignment_split(code) {
        let name = code[..eq].trim();
        if is_ident(name) {
            return Ok(Some((
                name.to_string(),
                value(code[eq + 1..].trim(), line)?,
                code[eq..].trim().to_string(),
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

/// Parse a PDP-11 expression: decimal by default, `0x` hex, `0b` binary, `'c'`
/// character. `@`/`#` are operand prefixes handled by [`parse_ea`], never
/// expression operators.
fn value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    mos6502::parse_expr(
        raw,
        line,
        parse_number,
        ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: Caret::Xor,
            at_is_pc: false,
        },
    )
}

fn parse_number(tok: &str, line: usize) -> Result<i64, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("invalid number `{tok}`"));
    if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
        return t.chars().nth(1).map(|c| c as i64).ok_or_else(bad);
    }
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).map_err(|_| bad());
    }
    if let Some(bin) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2).map_err(|_| bad());
    }
    t.parse::<i64>().map_err(|_| bad())
}

// ---------------------------------------------------------------------------
// Instruction encoding
// ---------------------------------------------------------------------------

/// One parsed effective address: its 6-bit `mode << 3 | reg` field and, for the
/// modes that need one, the extension word (`rel` = a PC-relative target).
struct Ea {
    field: u16,
    ext: Option<(Expr, bool)>,
}

/// The two literal bytes of an opcode word, little-endian.
fn word_lit(w: u16) -> [Piece; 2] {
    [Piece::Lit(w as u8), Piece::Lit((w >> 8) as u8)]
}

/// The extension-word piece for an EA, if it has one.
fn ext_piece(ext: Option<(Expr, bool)>) -> Option<Piece> {
    ext.map(|(expr, rel)| Piece::Val {
        expr,
        bytes: 2,
        rel,
        signed: rel,
    })
}

fn encode(mn: &str, args: &str, line: usize) -> Result<Operation, AsmError> {
    let insn = isa::pdp11::lookup(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    let ops = split_top_level(args.trim(), ',');
    let ops: Vec<&str> = ops
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let mut pieces = Vec::new();

    match insn.class {
        Class::Double => {
            let [s, d] = two(&ops, insn, line)?;
            let (src, dst) = (parse_ea(s, line)?, parse_ea(d, line)?);
            pieces.extend(word_lit(insn.base | (src.field << 6) | dst.field));
            pieces.extend(ext_piece(src.ext));
            pieces.extend(ext_piece(dst.ext));
        }
        Class::Single => {
            let d = one(&ops, insn, line)?;
            let dst = parse_ea(d, line)?;
            if matches!(mn, "JMP" | "WRTLCK") && dst.field >> 3 == 0 {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` cannot target a register"),
                ));
            }
            pieces.extend(word_lit(insn.base | dst.field));
            pieces.extend(ext_piece(dst.ext));
        }
        Class::Jsr | Class::Xor => {
            let [r, d] = two(&ops, insn, line)?;
            let reg = register(r, line)?;
            let dst = parse_ea(d, line)?;
            pieces.extend(word_lit(insn.base | (reg << 6) | dst.field));
            pieces.extend(ext_piece(dst.ext));
        }
        Class::RegSrc => {
            let [s, r] = two(&ops, insn, line)?;
            let src = parse_ea(s, line)?;
            let reg = register(r, line)?;
            pieces.extend(word_lit(insn.base | (reg << 6) | src.field));
            pieces.extend(ext_piece(src.ext));
        }
        Class::Rts => {
            let r = one(&ops, insn, line)?;
            pieces.extend(word_lit(insn.base | register(r, line)?));
        }
        Class::Branch => {
            let target = value(one(&ops, insn, line)?, line)?;
            // Byte distance from the following word; word-scaled to the field.
            pieces.push(Piece::Packed {
                expr: sub(target, add(Expr::Pc, Expr::Num(2))),
                bytes: 2,
                scale: 2,
                min: -128,
                max: 127,
                mask: 0xFF,
                or_bits: u32::from(insn.base),
                what: "branch distance",
            });
        }
        Class::Sob => {
            let [r, t] = two(&ops, insn, line)?;
            let reg = register(r, line)?;
            let target = value(t, line)?;
            // SOB only branches backward: PC_after − target, word-scaled.
            pieces.push(Piece::Packed {
                expr: sub(add(Expr::Pc, Expr::Num(2)), target),
                bytes: 2,
                scale: 2,
                min: 0,
                max: 63,
                mask: 0x3F,
                or_bits: u32::from(insn.base) | (u32::from(reg) << 6),
                what: "SOB distance",
            });
        }
        Class::Trap => match ops.as_slice() {
            [] => pieces.extend(word_lit(insn.base)),
            [n] => pieces.push(packed(
                value(n, line)?,
                0,
                255,
                0xFF,
                insn.base,
                "trap operand",
            )),
            _ => return Err(AsmError::new(line, format!("`{mn}` takes one operand"))),
        },
        Class::Mark => {
            let n = value(one(&ops, insn, line)?, line)?;
            pieces.push(packed(n, 0, 63, 0x3F, insn.base, "mark count"));
        }
        Class::Spl => {
            let n = value(one(&ops, insn, line)?, line)?;
            pieces.push(packed(n, 0, 7, 0x7, insn.base, "priority level"));
        }
        Class::NoArg => {
            if !ops.is_empty() {
                return Err(AsmError::new(line, format!("`{mn}` takes no operand")));
            }
            pieces.extend(word_lit(insn.base));
        }
    }
    Ok(Operation::Encoded(pieces))
}

/// A range-checked, unscaled operand packed into the low bits of the opcode word.
fn packed(expr: Expr, min: i64, max: i64, mask: u32, base: u16, what: &'static str) -> Piece {
    Piece::Packed {
        expr,
        bytes: 2,
        scale: 1,
        min,
        max,
        mask,
        or_bits: u32::from(base),
        what,
    }
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
        [a, b] => Ok([a, b]),
        _ => Err(AsmError::new(
            line,
            format!("`{}` takes two operands", insn.mnemonic),
        )),
    }
}

/// Parse a bare register operand (`r0`–`r7`, `sp`, `pc`) to its number 0–7.
fn register(tok: &str, line: usize) -> Result<u16, AsmError> {
    parse_reg(tok).ok_or_else(|| AsmError::new(line, format!("expected a register, got `{tok}`")))
}

fn parse_reg(tok: &str) -> Option<u16> {
    let t = tok.trim();
    match t.to_ascii_lowercase().as_str() {
        "sp" => Some(6),
        "pc" => Some(7),
        _ => t
            .strip_prefix(['r', 'R'])
            .and_then(|n| n.parse::<u16>().ok())
            .filter(|&n| n < 8),
    }
}

/// Parse an effective address into its 6-bit field and optional extension word.
///
/// Modes: `Rn` (0), `(Rn)` (1), `(Rn)+` (2), `@(Rn)+` (3), `-(Rn)` (4),
/// `@-(Rn)` (5), `X(Rn)` (6), `@X(Rn)` (7). With the PC: `#n` (2/7), `@#n` (3/7),
/// a bare `addr` is PC-relative (6/7), `@addr` relative-deferred (7/7).
fn parse_ea(tok: &str, line: usize) -> Result<Ea, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("bad operand `{tok}`"));

    // Immediate / absolute (PC autoincrement forms).
    if let Some(imm) = t.strip_prefix('#') {
        return Ok(Ea {
            field: 0o27,
            ext: Some((value(imm, line)?, false)),
        });
    }
    if let Some(abs) = t.strip_prefix("@#") {
        return Ok(Ea {
            field: 0o37,
            ext: Some((value(abs, line)?, false)),
        });
    }

    let (deferred, body) = match t.strip_prefix('@') {
        Some(rest) => (true, rest.trim()),
        None => (false, t),
    };

    // Autodecrement: -(Rn).
    if let Some(inner) = body.strip_prefix("-(").and_then(|s| s.strip_suffix(')')) {
        let reg = parse_reg(inner).ok_or_else(bad)?;
        let mode = if deferred { 5 } else { 4 };
        return Ok(Ea {
            field: (mode << 3) | reg,
            ext: None,
        });
    }

    // Parenthesised: (Rn) or (Rn)+.
    if body.starts_with('(') {
        let close = body.find(')').ok_or_else(bad)?;
        let reg = parse_reg(&body[1..close]).ok_or_else(bad)?;
        let mode = match body[close + 1..].trim() {
            "+" => {
                if deferred {
                    3
                } else {
                    2
                }
            }
            "" if !deferred => 1,
            _ => return Err(bad()),
        };
        return Ok(Ea {
            field: (mode << 3) | reg,
            ext: None,
        });
    }

    // Plain register.
    if let Some(reg) = parse_reg(body) {
        let mode = if deferred { 1 } else { 0 };
        return Ok(Ea {
            field: (mode << 3) | reg,
            ext: None,
        });
    }

    // Indexed: X(Rn).
    if let Some(open) = body.find('(') {
        let close = body.rfind(')').ok_or_else(bad)?;
        let reg = parse_reg(&body[open + 1..close]).ok_or_else(bad)?;
        let disp = value(&body[..open], line)?;
        let mode = if deferred { 7 } else { 6 };
        return Ok(Ea {
            field: (mode << 3) | reg,
            ext: Some((disp, false)),
        });
    }

    // Bare address → PC-relative (mode 6/7, reg 7).
    let mode = if deferred { 7 } else { 6 };
    Ok(Ea {
        field: (mode << 3) | 7,
        ext: Some((value(body, line)?, true)),
    })
}
