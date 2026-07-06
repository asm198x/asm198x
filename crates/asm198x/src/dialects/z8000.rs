//! The Zilog Z8000 dialect front-end (`asl` syntax), non-segmented (Z8002).
//!
//! Assembles against [`isa::z8000`] and produces a flat **big-endian** binary at
//! the `org`. Numbers are Intel `h`-suffix hex (shared with the 8080 dialect).
//! Registers are word `r0`–`r15`, byte `rh0`–`rh7` / `rl0`–`rl7`, long
//! `rr0`–`rr14`, quad `rq0`/`rq4`/`rq8`/`rq12`. Built as sweep-verified
//! increments (see `decisions/z8000-staged-build.md`); this covers the
//! **dyadic family** (increments 1–2: arithmetic / logic / compare / load /
//! exchange / load-address), **program control** (increment 3: `JP`/`CALL`/`JR`/
//! `RET`/`DJNZ`/`CALR` with condition codes), the **single-operand ALU**
//! (increment 4: `CLR`/`COM`/`NEG`/`TEST`/`TSET`, `INC`/`DEC`), the **stack**
//! ops (increment 5: `PUSH`/`POP`/`PUSHL`/`POPL`), the **shifts / rotates /
//! sign-extends** (increment 6: `SLA`/`SRA`/`SLL`/`SRL` + byte/long, `RL`/`RR`/
//! `RLC`/`RRC` + byte, `EXTSB`/`EXTS`/`EXTSL`), the **bit ops** (increment 7:
//! `BIT`/`SET`/`RES` + byte, static and dynamic), **multiply / divide**
//! (increment 8: `MULT`/`MULTL`/`DIV`/`DIVL`), the **block / string** repeat
//! group (increment 9: `LDx`/`CPx`/`CPSx`/`TRxB`/`TRTxB`), the privileged
//! **I/O** group (increment 10: `IN`/`OUT`/`SIN`/`SOUT` + the block-I/O repeat
//! ops, `asl` needing `supmode on`), the **CPU-control** group (increment 11:
//! `NOP`/`HALT`/`EI`/`DI`/`IRET`/`LDCTL`/`LDPS`/`MSET`/`SETFLG`/`SC`/…), and the
//! **cleanup** one-offs (`TCC`/`TCCB`, `LDK`, `RLDB`/`RRDB`, the PC-relative
//! `LDR`/`LDRB`/`LDRL`) — the complete non-segmented Z8002 instruction set.
//!
//! The [`seg`](Z8000::seg) flag selects the **segmented Z8001** target-extension
//! (increment 12): the same opcodes, but a direct / indexed address is a
//! two-word `<<seg>>offset` operand, an indirect pointer is a long pair
//! (`@RRn`), and `LDA` targets a long pair; I/O and the relative forms are
//! unchanged. The memory operand carries its own segment through [`Operand`], so
//! [`addr_ext`] emits one or two words uniformly.
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

/// The Zilog Z8000 dialect. `seg` selects the segmented Z8001 model (widened
/// memory operands) over the non-segmented Z8002 base.
pub(crate) struct Z8000 {
    pub(crate) seg: bool,
}

impl Dialect for Z8000 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z8000::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (0b field-packed migration):
        // parse into a `Program`, then lower to the engine's statement stream —
        // byte-identical to the old direct parse (AE1).
        crate::ast::lower(parse_program(source, self.seg)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source, self.seg)?))
    }

    /// asl `equ` (and `name = expr`) takes no colon on its label; a colon would
    /// fail to reassemble, since the label is disambiguated by the keyword / `=`.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse Zilog Z8000 source into the semantic [`Program`](crate::ast::Program).
/// Each line becomes a node carrying its (global) label, operation, verbatim
/// source, span, and comment trivia. The Z8000 has no local-label scoping, so
/// every label is a [`Scope::Global`](crate::ast::Scope) symbol and
/// [`lower`](crate::ast::lower) reproduces the old statements exactly, so bytes
/// are unchanged. Every instruction rides the `Encoded` seam (field-packed opcode
/// word + extension words) through
/// [`item_from_operation`](crate::ast::item_from_operation) unchanged. The `seg`
/// flag (segmented Z8001) is the dialect's, constant for the whole source.
pub(crate) fn parse_program(source: &str, seg: bool) -> Result<crate::ast::Program, AsmError> {
    use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
    let mut nodes = Vec::new();
    let mut consts: BTreeMap<String, i64> = BTreeMap::new();
    // Own-line comments seen since the last node, attached as leading trivia to
    // the next one. Comments never reach the encoder, so bytes are unchanged.
    let mut pending_leading: Vec<Comment> = Vec::new();

    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let (code, comment) = split_comment(raw);
        if code.trim().is_empty() {
            if let Some(text) = comment {
                pending_leading.push(Comment {
                    text: text.to_string(),
                    span: Span::at(line as u32, 1),
                });
            }
            continue;
        }
        let trailing = comment.map(|text| Comment {
            text: text.to_string(),
            span: Span::at(line as u32, (code.len() + 1) as u32),
        });

        // `NAME EQU expr` / `NAME = expr` — a constant binds its label on the
        // same line, so the label cannot split off (the formatter keeps it there).
        if let Some((name, expr, op_source)) = constant(code.trim(), line)? {
            if let Ok(v) = fold_const(&expr, &consts, line) {
                consts.insert(name.clone(), v);
            }
            nodes.push(Node {
                label: Some(Symbol {
                    qualified: name.clone(),
                    scope: Scope::Global,
                    name,
                }),
                item: Some(crate::ast::item_from_operation(Operation::Equ(expr))),
                source: op_source,
                span: Span::at(line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut pending_leading),
                    trailing,
                },
            });
            continue;
        }

        let (label, rest) = split_label(code);
        let op = if rest.is_empty() {
            None
        } else {
            parse_op(rest, line, seg)?
        };
        if label.is_none() && op.is_none() {
            continue;
        }
        nodes.push(Node {
            label: label.map(|name| Symbol {
                qualified: name.clone(),
                scope: Scope::Global,
                name,
            }),
            item: op.map(crate::ast::item_from_operation),
            source: rest.trim().to_string(),
            span: Span::at(line as u32, 1),
            trivia: Trivia {
                leading: std::mem::take(&mut pending_leading),
                trailing,
            },
        });
    }

    // Flush comments after the last node (a trailing block or comment-only file)
    // as a label-less, op-less node so the formatter keeps them.
    if !pending_leading.is_empty() {
        let line = source.lines().count() as u32;
        nodes.push(Node {
            label: None,
            item: None,
            source: String::new(),
            span: Span::at(line, 1),
            trivia: Trivia {
                leading: pending_leading,
                trailing: None,
            },
        });
    }
    Ok(Program { nodes })
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

fn parse_op(rest: &str, line: usize, seg: bool) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        "cpu" | "end" | "title" | "page" | "name" | "listing" | "supmode" => return Ok(None),
        "org" | "aorg" | "rorg" => Operation::Org(value(args, line)?),
        "byte" | "db" | "dc.b" => Operation::Bytes(byte_list(args, line)?),
        "word" | "dw" | "dc.w" => Operation::Words(value_list(args, line)?),
        other => {
            let mn = other.to_ascii_uppercase();
            if let Some(ctl) = isa::z8000::ctl_lookup(&mn) {
                encode_ctl(ctl, args, line, seg)?
            } else if let Some(m) = isa::z8000::mono_lookup(&mn) {
                encode_mono(m, args, line, seg)?
            } else if let Some(s) = isa::z8000::stack_lookup(&mn) {
                encode_stack(s, args, line, seg)?
            } else if let Some(sh) = isa::z8000::shift_lookup(&mn) {
                encode_shift(sh, args, line)?
            } else if let Some(e) = isa::z8000::extend_lookup(&mn) {
                encode_extend(e, args, line)?
            } else if let Some(b) = isa::z8000::bit_lookup(&mn) {
                encode_bit(b, args, line, seg)?
            } else if let Some(md) = isa::z8000::muldiv_lookup(&mn) {
                encode_muldiv(md, args, line, seg)?
            } else if let Some(blk) = isa::z8000::block_lookup(&mn) {
                encode_block(blk, args, line, seg)?
            } else if let Some(sio) = isa::z8000::simple_io_lookup(&mn) {
                encode_simple_io(sio, args, line)?
            } else if let Some(bio) = isa::z8000::block_io_lookup(&mn) {
                encode_block_io(bio, args, line, seg)?
            } else if let Some(c) = isa::z8000::control_lookup(&mn) {
                encode_control(c, args, line, seg)?
            } else if let Some(m) = isa::z8000::misc_lookup(&mn) {
                encode_misc(m, args, line)?
            } else {
                encode(&mn, args, line, seg)?
            }
        }
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

/// A parsed operand and the addressing mode it implies. The `Option<u8>` on the
/// memory forms is the segment of a Z8001 segmented address (`<<seg>>offset`),
/// or `None` in non-segmented (Z8002) mode.
enum Operand {
    /// A register (word / byte / long per the instruction size), by number.
    Reg(u16),
    /// Immediate `#n`.
    Imm(Expr),
    /// Indirect register `@Rn` (word) / `@RRn` (long pair, segmented).
    Ir(u16),
    /// Direct address `addr` / `<<seg>>addr`.
    Da(Expr, Option<u8>),
    /// Indexed `addr(Rn)` / `<<seg>>addr(Rn)`.
    Indexed(Expr, u16, Option<u8>),
}

/// The extension pieces for a direct / indexed address: one word (non-segmented)
/// or two (a Z8001 segmented address — `0x8000 | seg << 8`, then the 16-bit
/// offset).
fn addr_ext(offset: Expr, seg: Option<u8>) -> Vec<Piece> {
    match seg {
        Some(s) => {
            let hi = 0x8000u16 | (u16::from(s) << 8);
            let mut v = Vec::from(word_lit(hi));
            v.push(ext_word(offset));
            v
        }
        None => vec![ext_word(offset)],
    }
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

/// A PC-relative offset word (`LDR`): the engine lays down `target − (PC + 4)`,
/// range-checked as a signed 16-bit value.
fn rel_word(expr: Expr) -> Piece {
    Piece::Val {
        expr,
        bytes: 2,
        rel: true,
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

fn encode(mn: &str, args: &str, line: usize, seg: bool) -> Result<Operation, AsmError> {
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
    // In segmented mode `LDA` loads a 32-bit segmented address, so its register
    // destination is a long pair.
    let dest_size = if seg && insn.size == Size::Address {
        Size::Long
    } else {
        insn.size
    };
    let dst = operand(dst_s, dest_size, line, seg)?;
    let src = operand(src_s, insn.size, line, seg)?;

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
/// load, source for a store) that occupies the second byte's low nibble. A
/// direct/indexed address carries its own segment, so no `seg` flag is needed.
fn dyadic(insn: &Insn, variable: &Operand, reg: u16, line: usize) -> Result<Operation, AsmError> {
    let (mode, field, ext): (u8, u16, Vec<Piece>) = match variable {
        Operand::Reg(s) => (isa::z8000::R, *s, vec![]),
        Operand::Ir(p) => (isa::z8000::IR, *p, vec![]),
        Operand::Imm(e) => (isa::z8000::IM, 0, vec![imm_piece(e.clone(), insn.size)]),
        Operand::Da(e, sg) => (isa::z8000::DA, 0, addr_ext(e.clone(), *sg)),
        Operand::Indexed(e, i, sg) => (isa::z8000::X, *i, addr_ext(e.clone(), *sg)),
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

fn add(a: Expr, b: Expr) -> Expr {
    Expr::Bin(BinOp::Add, Box::new(a), Box::new(b))
}
fn sub(a: Expr, b: Expr) -> Expr {
    Expr::Bin(BinOp::Sub, Box::new(a), Box::new(b))
}

/// Encode a single-operand ALU instruction (`CLR`/`COM`/`NEG`/`TEST`/`TSET` and
/// `INC`/`DEC`). The operand register/pointer/index is the second byte's high
/// nibble; the low nibble is a fixed sub-opcode or `count − 1`.
fn encode_mono(
    m: &isa::z8000::Mono,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    use isa::z8000::{DA, IR, R, X};
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    // The low nibble: a count − 1 (INC/DEC, default 1) or the fixed sub-opcode.
    let (operand_str, low) = match (m.count, ops.as_slice()) {
        (false, [o]) => (*o, u16::from(m.subop)),
        (true, [o]) => (*o, 0), // count 1
        (true, [o, c]) => {
            let n = fold_const(
                &value(c.trim_start_matches('#'), line)?,
                &BTreeMap::new(),
                line,
            )?;
            if !(1..=16).contains(&n) {
                return Err(AsmError::new(
                    line,
                    format!("`{}` count must be 1..=16", m.mnemonic),
                ));
            }
            (*o, (n - 1) as u16)
        }
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` takes one operand", m.mnemonic),
            ));
        }
    };

    let (mode, field, ext): (u8, u16, Vec<Piece>) = match operand(operand_str, m.size, line, seg)? {
        Operand::Reg(r) => (R, r, vec![]),
        Operand::Ir(p) => (IR, p, vec![]),
        Operand::Da(e, sg) => (DA, 0, addr_ext(e, sg)),
        Operand::Indexed(e, i, sg) => (X, i, addr_ext(e, sg)),
        Operand::Imm(_) => {
            return Err(AsmError::new(
                line,
                format!("`{}` cannot take an immediate", m.mnemonic),
            ));
        }
    };
    // Every mode above is R/IR/DA/X — all legal for a single-operand op.
    let top = (mm(mode) << 6) | u16::from(m.base6);
    let mut pieces = Vec::from(word_lit((top << 8) | (field << 4) | low));
    pieces.extend(ext);
    Ok(Operation::Encoded(pieces))
}

/// Encode a stack instruction (`PUSH`/`POP`/`PUSHL`/`POPL`). Syntax is
/// `PUSH @Rsp, src` and `POP dst, @Rsp` — the stack pointer leads for a push and
/// trails for a pop. The pointer is the second byte's high nibble, the value
/// operand's field the low nibble.
fn encode_stack(
    s: &isa::z8000::Stack,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    use isa::z8000::{DA, IR, R, X};
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect();
    let [a, b] = match ops.as_slice() {
        [a, b] => [*a, *b],
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` takes two operands", s.mnemonic),
            ));
        }
    };
    // The stack pointer is `@Rsp` (`@RRsp` in segmented mode); it leads a push
    // and trails a pop.
    let (sp_tok, val_tok) = if s.push { (a, b) } else { (b, a) };
    let sp = ptr_reg(sp_tok, seg).ok_or_else(|| {
        AsmError::new(
            line,
            format!("`{}` needs an @Rn stack pointer (not R0)", s.mnemonic),
        )
    })?;

    let val = operand(val_tok, s.size, line, seg)?;

    // PUSH #imm is a special opcode (base6 0x0D, low nibble 9).
    if let Operand::Imm(e) = &val {
        if !s.has_imm {
            return Err(AsmError::new(
                line,
                format!("`{}` has no immediate form", s.mnemonic),
            ));
        }
        let top = u16::from(isa::z8000::PUSH_IMM_BASE6); // MM = 0
        let mut pieces = Vec::from(word_lit((top << 8) | (sp << 4) | 9));
        pieces.push(ext_word(e.clone()));
        return Ok(Operation::Encoded(pieces));
    }

    let (mode, field, ext): (u8, u16, Vec<Piece>) = match val {
        Operand::Reg(r) => (R, r, vec![]),
        Operand::Ir(p) => (IR, p, vec![]),
        Operand::Da(e, sg) => (DA, 0, addr_ext(e, sg)),
        Operand::Indexed(e, i, sg) => (X, i, addr_ext(e, sg)),
        Operand::Imm(_) => unreachable!("handled above"),
    };
    let top = (mm(mode) << 6) | u16::from(s.base6);
    let mut pieces = Vec::from(word_lit((top << 8) | (sp << 4) | field));
    pieces.extend(ext);
    Ok(Operation::Encoded(pieces))
}

/// Encode a shift or rotate (`SLA`/`SRA`/`SLL`/`SRL` + byte/long, `RL`/`RR`/
/// `RLC`/`RRC` + byte). Syntax is `mn reg,#count` (the count defaults to 1). The
/// register is the second byte's high nibble; the low nibble is a fixed
/// sub-opcode (shift, with a trailing signed count word) or `type·4 + (count−1)·2`
/// (rotate). A right shift (`SRx`) shares the left opcode with a negated count.
fn encode_shift(sh: &isa::z8000::Shift, args: &str, line: usize) -> Result<Operation, AsmError> {
    use isa::z8000::ShiftKind;
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let (reg_s, count) = match ops.as_slice() {
        [r] => (*r, 1i64),
        [r, c] => {
            let n = fold_const(
                &value(c.trim_start_matches('#'), line)?,
                &BTreeMap::new(),
                line,
            )?;
            (*r, n)
        }
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` takes a register and a count", sh.mnemonic),
            ));
        }
    };
    let reg = size_reg(reg_s, sh.size)
        .ok_or_else(|| AsmError::new(line, format!("`{}` needs a register", sh.mnemonic)))?;
    let top = (2u16 << 6) | u16::from(sh.base6); // MM = 10 (register group)

    match sh.kind {
        ShiftKind::Shift => {
            let max = isa::z8000::shift_max(sh.size);
            let lo = i64::from(sh.right); // right by 0 is invalid; left allows 0
            if !(lo..=max).contains(&count) {
                return Err(AsmError::new(
                    line,
                    format!("`{}` count must be {lo}..={max}", sh.mnemonic),
                ));
            }
            let signed = if sh.right { -count } else { count };
            // Word / long shifts carry a full 16-bit signed count; a byte shift's
            // count is a signed 8-bit value in the low byte (high byte zero).
            let count_word = if sh.size == Size::Byte {
                signed & 0xFF
            } else {
                signed
            };
            let mut pieces = Vec::from(word_lit((top << 8) | (reg << 4) | u16::from(sh.sel)));
            pieces.push(ext_word(Expr::Num(count_word)));
            Ok(Operation::Encoded(pieces))
        }
        ShiftKind::Rotate => {
            if count != 1 && count != 2 {
                return Err(AsmError::new(
                    line,
                    format!("`{}` count must be 1 or 2", sh.mnemonic),
                ));
            }
            let low = u16::from(sh.sel) * 4 + ((count as u16) - 1) * 2;
            Ok(Operation::Encoded(Vec::from(word_lit(
                (top << 8) | (reg << 4) | low,
            ))))
        }
    }
}

/// Encode a sign-extend (`EXTSB`/`EXTS`/`EXTSL`). One register operand — a word,
/// long pair, or quad per the mnemonic — in the second byte's high nibble, the
/// sub-opcode in the low nibble; `0xB1` is the top byte.
fn encode_extend(e: &isa::z8000::Extend, args: &str, line: usize) -> Result<Operation, AsmError> {
    let reg = size_reg(args.trim(), e.size)
        .ok_or_else(|| AsmError::new(line, format!("`{}` needs a register", e.mnemonic)))?;
    let word = (u16::from(isa::z8000::EXTEND_TOP) << 8) | (reg << 4) | u16::from(e.subop);
    Ok(Operation::Encoded(Vec::from(word_lit(word))))
}

/// Encode a bit instruction (`BIT`/`SET`/`RES` + byte). The bit source is either
/// a literal `#n` (static — the bit number is the second byte's low nibble, the
/// target reached through the usual R / IR / DA / X modes) or a word register
/// (dynamic — a two-word form with the target register in word 2).
fn encode_bit(
    b: &isa::z8000::Bit,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    use isa::z8000::{DA, IR, R, X};
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let [target_s, src_s] = match ops.as_slice() {
        [a, c] => [*a, *c],
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` takes two operands", b.mnemonic),
            ));
        }
    };

    // Dynamic form: the bit number lives in a word register (`bit r3,r1`).
    if !src_s.starts_with('#') {
        let Some(bitreg) = word_reg(src_s) else {
            return Err(AsmError::new(
                line,
                format!(
                    "`{}` bit source must be `#n` or a word register",
                    b.mnemonic
                ),
            ));
        };
        let Operand::Reg(target) = operand(target_s, b.size, line, seg)? else {
            return Err(AsmError::new(
                line,
                format!("`{}` dynamic form needs a register target", b.mnemonic),
            ));
        };
        let mut pieces = Vec::from(word_lit((u16::from(b.base6) << 8) | bitreg)); // MM = 00
        pieces.extend(word_lit(target << 8));
        return Ok(Operation::Encoded(pieces));
    }

    // Static form: a literal bit number in the low nibble.
    let bitnum = fold_const(&value(&src_s[1..], line)?, &BTreeMap::new(), line)?;
    let max = isa::z8000::bit_max(b.size);
    if !(0..=max).contains(&bitnum) {
        return Err(AsmError::new(
            line,
            format!("`{}` bit number must be 0..={max}", b.mnemonic),
        ));
    }
    let (mode, field, ext): (u8, u16, Vec<Piece>) = match operand(target_s, b.size, line, seg)? {
        Operand::Reg(r) => (R, r, vec![]),
        Operand::Ir(p) => (IR, p, vec![]),
        Operand::Da(e, sg) => (DA, 0, addr_ext(e, sg)),
        Operand::Indexed(e, i, sg) => (X, i, addr_ext(e, sg)),
        Operand::Imm(_) => {
            return Err(AsmError::new(
                line,
                format!("`{}` target cannot be an immediate", b.mnemonic),
            ));
        }
    };
    let top = (mm(mode) << 6) | u16::from(b.base6);
    let mut pieces = Vec::from(word_lit((top << 8) | (field << 4) | (bitnum as u16)));
    pieces.extend(ext);
    Ok(Operation::Encoded(pieces))
}

/// Encode a multiply / divide (`MULT`/`MULTL`/`DIV`/`DIVL`). Dyadic-shaped, but
/// the destination accumulator is double-width (long `rr` / quad `rq`) while the
/// source (and its immediate) is one size smaller (word / long).
fn encode_muldiv(
    md: &isa::z8000::MulDiv,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    use isa::z8000::{DA, IM, IR, R, X};
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let [dest_s, src_s] = match ops.as_slice() {
        [a, b] => [*a, *b],
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` takes two operands", md.mnemonic),
            ));
        }
    };
    let dest = size_reg(dest_s, md.dest).ok_or_else(|| {
        AsmError::new(
            line,
            format!("`{}` needs a valid accumulator register", md.mnemonic),
        )
    })?;
    let (mode, field, ext): (u8, u16, Vec<Piece>) = match operand(src_s, md.src, line, seg)? {
        Operand::Reg(s) => (R, s, vec![]),
        Operand::Ir(p) => (IR, p, vec![]),
        Operand::Imm(e) => (IM, 0, vec![imm_piece(e, md.src)]),
        Operand::Da(e, sg) => (DA, 0, addr_ext(e, sg)),
        Operand::Indexed(e, i, sg) => (X, i, addr_ext(e, sg)),
    };
    let top = (mm(mode) << 6) | u16::from(md.base6);
    let mut pieces = Vec::from(word_lit((top << 8) | (field << 4) | dest));
    pieces.extend(ext);
    Ok(Operation::Encoded(pieces))
}

/// Encode a miscellaneous instruction (`TCC`/`TCCB`, `LDK`, `RLDB`/`RRDB`,
/// `LDR`/`LDRB`/`LDRL`) — the last non-segmented instructions, each a one-off.
fn encode_misc(m: &isa::z8000::Misc, args: &str, line: usize) -> Result<Operation, AsmError> {
    use isa::z8000::MiscKind;
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let err = |t: &str| AsmError::new(line, format!("`{}` {t}", m.mnemonic));
    let top = u16::from(m.top);

    match m.kind {
        MiscKind::Tcc => {
            // `tcc [cc,] Rd` — an optional leading condition, register last.
            let (cc, reg_s) = match ops.as_slice() {
                [r] => (8u16, *r),
                [c, r] => (
                    u16::from(isa::z8000::cc_value(c).ok_or_else(|| err("unknown condition"))?),
                    *r,
                ),
                _ => return Err(err("takes [cc,] a register")),
            };
            let reg = size_reg(reg_s, m.size).ok_or_else(|| err("needs a register"))?;
            Ok(Operation::Encoded(Vec::from(word_lit(
                (top << 8) | (reg << 4) | cc,
            ))))
        }
        MiscKind::Ldk => {
            let [r, k] = ops.as_slice() else {
                return Err(err("takes a register and #n"));
            };
            let reg = word_reg(r).ok_or_else(|| err("needs a word register"))?;
            let n = fold_const(
                &value(k.trim_start_matches('#'), line)?,
                &BTreeMap::new(),
                line,
            )?;
            if !(0..=15).contains(&n) {
                return Err(err("constant must be 0..=15"));
            }
            Ok(Operation::Encoded(Vec::from(word_lit(
                (top << 8) | (reg << 4) | n as u16,
            ))))
        }
        MiscKind::Rotdig => {
            let [d, s] = ops.as_slice() else {
                return Err(err("takes two byte registers"));
            };
            let dst = byte_reg(d).ok_or_else(|| err("needs a byte register"))?;
            let src = byte_reg(s).ok_or_else(|| err("needs a byte register"))?;
            Ok(Operation::Encoded(Vec::from(word_lit(
                (top << 8) | (src << 4) | dst,
            ))))
        }
        MiscKind::Ldr => {
            let [a, b] = ops.as_slice() else {
                return Err(err("takes a register and an address"));
            };
            // A leading register is a load (reg <- addr); else a store.
            let (reg, addr_s, store) = match size_reg(a, m.size) {
                Some(r) => (r, *b, false),
                None => (
                    size_reg(b, m.size).ok_or_else(|| err("needs a register"))?,
                    *a,
                    true,
                ),
            };
            let word_top = if store { top | 2 } else { top };
            let mut pieces = Vec::from(word_lit((word_top << 8) | reg));
            pieces.push(rel_word(value(addr_s, line)?));
            Ok(Operation::Encoded(pieces))
        }
    }
}

/// Encode a CPU-control instruction (`NOP`/`HALT`/`EI`/`DI`/`IRET`/`LDCTL`/
/// `LDPS`/`MSET`/`MRES`/`MBIT`/`MREQ`/`SETFLG`/`RESFLG`/`COMFLG`/`SC`). Each
/// sub-group has its own small encoding; see [`isa::z8000::ControlKind`].
fn encode_control(
    c: &isa::z8000::Control,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    use isa::z8000::ControlKind;
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let err = |m: &str| AsmError::new(line, format!("`{}` {m}", c.mnemonic));
    let word = match c.kind {
        ControlKind::Fixed(w) => {
            if !ops.is_empty() {
                return Err(err("takes no operands"));
            }
            w
        }
        ControlKind::Mreq => {
            let [r] = ops.as_slice() else {
                return Err(err("takes one register"));
            };
            let reg = word_reg(r).ok_or_else(|| err("needs a register"))?;
            0x7B00 | (reg << 4) | 0x0D
        }
        ControlKind::Flag(subop) => {
            if ops.is_empty() {
                return Err(err("needs at least one flag"));
            }
            let mut mask = 0u16;
            for f in &ops {
                mask |= u16::from(isa::z8000::flag_bit(f).ok_or_else(|| err("bad flag"))?);
            }
            0x8D00 | (mask << 4) | u16::from(subop)
        }
        ControlKind::Intr(ei) => {
            let (mut vi, mut nvi) = (false, false);
            for op in &ops {
                match op.to_ascii_lowercase().as_str() {
                    "vi" => vi = true,
                    "nvi" => nvi = true,
                    _ => return Err(err("interrupt must be vi or nvi")),
                }
            }
            if !vi && !nvi {
                return Err(err("needs vi and/or nvi"));
            }
            let low = u16::from(ei) << 2 | u16::from(!vi) << 1 | u16::from(!nvi);
            0x7C00 | low
        }
        ControlKind::Ldctl(size) => {
            let [a, b] = ops.as_slice() else {
                return Err(err("takes a register and a control register"));
            };
            // The operand that parses as a register decides the direction: a
            // leading register is a load (reg <- ctrl); otherwise a store.
            let (reg, ctrl_s, store) = match size_reg(a, size) {
                Some(r) => (r, *b, false),
                None => (
                    size_reg(b, size).ok_or_else(|| err("needs a register"))?,
                    *a,
                    true,
                ),
            };
            let code = if matches!(size, Size::Byte) {
                if ctrl_s.eq_ignore_ascii_case("flags") {
                    1
                } else {
                    return Err(err("byte control register must be FLAGS"));
                }
            } else {
                isa::z8000::word_ctrl_code(ctrl_s, seg)
                    .ok_or_else(|| err("invalid control register"))?
            };
            let top: u16 = if matches!(size, Size::Byte) {
                0x8C
            } else {
                0x7D
            };
            (top << 8) | (reg << 4) | u16::from(code | if store { 8 } else { 0 })
        }
        ControlKind::Ldps => {
            let [s] = ops.as_slice() else {
                return Err(err("takes one source operand"));
            };
            return match operand(s, Size::Word, line, seg)? {
                Operand::Ir(p) if p != 0 => {
                    Ok(Operation::Encoded(Vec::from(word_lit(0x3900 | (p << 4)))))
                }
                Operand::Da(e, sg) => {
                    let mut pieces = Vec::from(word_lit(0x7900));
                    pieces.extend(addr_ext(e, sg));
                    Ok(Operation::Encoded(pieces))
                }
                Operand::Indexed(e, i, sg) => {
                    let mut pieces = Vec::from(word_lit(0x7900 | (i << 4)));
                    pieces.extend(addr_ext(e, sg));
                    Ok(Operation::Encoded(pieces))
                }
                _ => Err(err("needs an @Rn / address / indexed source")),
            };
        }
        ControlKind::Sc => {
            let [i] = ops.as_slice() else {
                return Err(err("takes one #n operand"));
            };
            let n = fold_const(
                &value(i.trim_start_matches('#'), line)?,
                &BTreeMap::new(),
                line,
            )?;
            if !(0..=255).contains(&n) {
                return Err(err("code must be 0..=255"));
            }
            0x7F00 | n as u16
        }
    };
    Ok(Operation::Encoded(Vec::from(word_lit(word))))
}

/// Encode a simple I/O instruction (`IN`/`OUT`/`SIN`/`SOUT` + byte). The port is
/// either a direct address (word 1 = `reg << 4 | sub`, then an address word) or
/// an `@Rn` register (its own top byte, word 1 = `port << 4 | reg`); `SIN`/`SOUT`
/// have only the direct form. The register leads for `IN`/`SIN`, trails for
/// `OUT`/`SOUT`.
fn encode_simple_io(
    sio: &isa::z8000::SimpleIo,
    args: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let [a, b] = match ops.as_slice() {
        [a, b] => [*a, *b],
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` takes two operands", sio.mnemonic),
            ));
        }
    };
    let (reg_s, port_s) = if sio.input { (a, b) } else { (b, a) };
    let reg = size_reg(reg_s, sio.size)
        .ok_or_else(|| AsmError::new(line, format!("`{}` needs a data register", sio.mnemonic)))?;

    if port_s.starts_with('@') {
        // I/O port registers are 16-bit even in segmented mode.
        let port = ptr_reg(port_s, false)
            .ok_or_else(|| AsmError::new(line, format!("`{}` bad @Rn port", sio.mnemonic)))?;
        let top = sio.indirect_top.ok_or_else(|| {
            AsmError::new(line, format!("`{}` has no @Rn port form", sio.mnemonic))
        })?;
        Ok(Operation::Encoded(Vec::from(word_lit(
            (u16::from(top) << 8) | (port << 4) | reg,
        ))))
    } else {
        let top = isa::z8000::io_direct_top(sio.size);
        let word1 = (u16::from(top) << 8) | (reg << 4) | u16::from(sio.direct_sub);
        let mut pieces = Vec::from(word_lit(word1));
        pieces.push(ext_word(value(port_s, line)?));
        Ok(Operation::Encoded(pieces))
    }
}

/// Encode a block-I/O instruction (`INI`/`OUTI`/…, special `SINI`/… + byte). A
/// two-word Load-shaped form `@Rd, @Rs, Rc`: source in word 1, dest and count in
/// word 2, the single/repeat marker in the control nibble.
fn encode_block_io(
    bio: &isa::z8000::BlockIo,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let [dst_s, src_s, count_s] = match ops.as_slice() {
        [a, b, c] => [*a, *b, *c],
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` takes three operands", bio.mnemonic),
            ));
        }
    };
    // In segmented mode the *memory* pointer is a long pair (`@RRn`) but the I/O
    // pointer stays a word register; `op_nib` bit 1 marks output (memory is the
    // source) vs input (memory is the destination).
    let (dst_seg, src_seg) = if bio.op_nib & 2 != 0 {
        (false, seg)
    } else {
        (seg, false)
    };
    let dst = ptr_reg(dst_s, dst_seg).ok_or_else(|| {
        AsmError::new(line, format!("`{}` needs an @Rn destination", bio.mnemonic))
    })?;
    let src = ptr_reg(src_s, src_seg)
        .ok_or_else(|| AsmError::new(line, format!("`{}` needs an @Rn source", bio.mnemonic)))?;
    let count = word_reg(count_s)
        .ok_or_else(|| AsmError::new(line, format!("`{}` needs a count register", bio.mnemonic)))?;
    let top = isa::z8000::io_direct_top(bio.size);
    let word1 = (u16::from(top) << 8) | (src << 4) | u16::from(bio.op_nib);
    let word2 = (count << 8) | (dst << 4) | u16::from(bio.ctrl);
    let mut pieces = Vec::from(word_lit(word1));
    pieces.extend(word_lit(word2));
    Ok(Operation::Encoded(pieces))
}

/// Parse an `@Rn` memory pointer — a nonzero word register (`R0` is not a legal
/// base), or, in segmented (`seg`) mode, a long register pair (`@RRn`).
fn ptr_reg(tok: &str, seg: bool) -> Option<u16> {
    let r = tok.trim().strip_prefix('@')?;
    if seg {
        long_reg(r)
    } else {
        word_reg(r).filter(|&v| v != 0)
    }
}

/// Encode a block / string instruction (`LDx`/`CPx`/`CPSx`/`TRxB`/`TRTxB`). A
/// two-word form: word 1 holds one pointer and the operation nibble, word 2 the
/// count register, the other pointer / data register, and the control nibble
/// (a single/repeat marker, or — for `CPx`/`CPSx` — a condition code).
fn encode_block(
    b: &isa::z8000::Block,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    use isa::z8000::BlockShape;
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let has_cc = b.has_cc();
    let (first, src_s, count_s, cc_opt) = match ops.as_slice() {
        [a, s, c] => (*a, *s, *c, None),
        [a, s, c, cc] if has_cc => (*a, *s, *c, Some(*cc)),
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{}` operand count", b.mnemonic),
            ));
        }
    };
    let src = ptr_reg(src_s, seg)
        .ok_or_else(|| AsmError::new(line, format!("`{}` needs an @Rn source", b.mnemonic)))?;
    let count = word_reg(count_s)
        .ok_or_else(|| AsmError::new(line, format!("`{}` needs a count register", b.mnemonic)))?;
    let cc = match cc_opt {
        Some(c) => u16::from(
            isa::z8000::cc_value(c)
                .ok_or_else(|| AsmError::new(line, format!("unknown condition `{c}`")))?,
        ),
        None => 8, // "always"
    };

    let (w1_field, w2_field) = match b.shape {
        BlockShape::Load | BlockShape::CompareString => {
            let dst = ptr_reg(first, seg).ok_or_else(|| {
                AsmError::new(line, format!("`{}` needs an @Rn destination", b.mnemonic))
            })?;
            (src, dst)
        }
        BlockShape::Compare => {
            let reg = size_reg(first, b.size).ok_or_else(|| {
                AsmError::new(line, format!("`{}` needs a data register", b.mnemonic))
            })?;
            (src, reg)
        }
        BlockShape::Translate => {
            let dst = ptr_reg(first, seg).ok_or_else(|| {
                AsmError::new(line, format!("`{}` needs an @Rn destination", b.mnemonic))
            })?;
            (dst, src)
        }
    };

    let ctrl = if has_cc { cc } else { u16::from(b.ctrl) };
    let top = (2u16 << 6) | u16::from(b.base6); // MM = 10
    let word1 = (top << 8) | (w1_field << 4) | u16::from(b.op_nib);
    let word2 = (count << 8) | (w2_field << 4) | ctrl;
    let mut pieces = Vec::from(word_lit(word1));
    pieces.extend(word_lit(word2));
    Ok(Operation::Encoded(pieces))
}

/// Encode a program-control instruction (`JP`/`CALL`/`JR`/`RET`/`DJNZ`/`CALR`).
/// Only the `JP`/`CALL` memory operands are affected by segmentation; the
/// relative forms are unchanged.
fn encode_ctl(
    ctl: &isa::z8000::Ctl,
    args: &str,
    line: usize,
    seg: bool,
) -> Result<Operation, AsmError> {
    use isa::z8000::CtlKind;
    let ops: Vec<&str> = split_top_level(args.trim(), ',')
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let mn = ctl.mnemonic;

    // Split off a leading condition code where the instruction allows one.
    let (cc, rest): (u16, &[&str]) = if ctl.cc && ops.len() > usize::from(ctl.kind != CtlKind::Ret)
    {
        let v = isa::z8000::cc_value(ops[0])
            .ok_or_else(|| AsmError::new(line, format!("unknown condition `{}`", ops[0])))?;
        (u16::from(v), &ops[1..])
    } else {
        (8, &ops[..]) // always
    };

    match ctl.kind {
        CtlKind::Jump => {
            let [t] = one_target(rest, mn, line)?;
            let dst = operand(t, Size::Word, line, seg)?;
            let (mode, field, ext): (u8, u16, Vec<Piece>) = match dst {
                Operand::Ir(p) => (isa::z8000::IR, p, vec![]),
                Operand::Da(e, sg) => (isa::z8000::DA, 0, addr_ext(e, sg)),
                Operand::Indexed(e, i, sg) => (isa::z8000::X, i, addr_ext(e, sg)),
                _ => {
                    return Err(AsmError::new(
                        line,
                        format!("`{mn}` needs a memory operand"),
                    ));
                }
            };
            if ctl.modes & mode == 0 {
                return Err(AsmError::new(line, format!("`{mn}` bad addressing mode")));
            }
            let top = (mm(mode) << 6) | ctl.base;
            let low = if ctl.cc { cc } else { 0 };
            let mut pieces = Vec::from(word_lit((top << 8) | (field << 4) | low));
            pieces.extend(ext);
            Ok(Operation::Encoded(pieces))
        }
        CtlKind::Jr => {
            let [t] = one_target(rest, mn, line)?;
            // target = PC + 2·disp -> disp = (target − (PC + 2)) / 2.
            Ok(Operation::Encoded(vec![Piece::Packed {
                expr: sub(value(t, line)?, add(Expr::Pc, Expr::Num(2))),
                bytes: 2,
                scale: 2,
                min: -128,
                max: 127,
                mask: 0xFF,
                or_bits: 0xE000 | (u32::from(cc) << 8),
                what: "JR distance",
            }]))
        }
        CtlKind::Ret => {
            if !rest.is_empty() {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` takes only a condition"),
                ));
            }
            Ok(Operation::Encoded(Vec::from(word_lit(ctl.base | cc))))
        }
        CtlKind::Calr => {
            let [t] = one_target(rest, mn, line)?;
            // target = PC − 2·disp -> disp = ((PC + 2) − target) / 2, 12-bit signed.
            Ok(Operation::Encoded(vec![Piece::Packed {
                expr: sub(add(Expr::Pc, Expr::Num(2)), value(t, line)?),
                bytes: 2,
                scale: 2,
                min: -2048,
                max: 2047,
                mask: 0xFFF,
                or_bits: u32::from(ctl.base) << 8,
                what: "CALR distance",
            }]))
        }
        CtlKind::Djnz => {
            let [r, t] = match rest {
                [a, b] => [*a, *b],
                _ => {
                    return Err(AsmError::new(
                        line,
                        format!("`{mn}` takes a register and a target"),
                    ));
                }
            };
            let reg = if ctl.byte { byte_reg(r) } else { word_reg(r) }
                .ok_or_else(|| AsmError::new(line, format!("`{mn}` needs a register")))?;
            let w = u32::from(!ctl.byte); // bit 7: 1 = word
            // Backward only: disp = ((PC + 2) − target) / 2, 0..=127.
            Ok(Operation::Encoded(vec![Piece::Packed {
                expr: sub(add(Expr::Pc, Expr::Num(2)), value(t, line)?),
                bytes: 2,
                scale: 2,
                min: 0,
                max: 127,
                mask: 0x7F,
                or_bits: (u32::from(ctl.base) << 8) | (u32::from(reg) << 8) | (w << 7),
                what: "DJNZ distance",
            }]))
        }
    }
}

/// Require exactly one target operand.
fn one_target<'a>(ops: &[&'a str], mn: &str, line: usize) -> Result<[&'a str; 1], AsmError> {
    match ops {
        [t] => Ok([*t]),
        _ => Err(AsmError::new(line, format!("`{mn}` takes one target"))),
    }
}

/// Parse a memory address, which in segmented (Z8001) mode is `<<seg>>offset`
/// (segment 0–127) and otherwise a plain 16-bit offset. Returns the offset and,
/// in segmented mode, the segment (defaulting to 0 when the `<<seg>>` is
/// omitted).
fn seg_addr(tok: &str, line: usize, seg: bool) -> Result<(Expr, Option<u8>), AsmError> {
    let t = tok.trim();
    if let Some(rest) = t.strip_prefix("<<") {
        let close = rest
            .find(">>")
            .ok_or_else(|| AsmError::new(line, "expected `>>` after segment"))?;
        let s = fold_const(&value(rest[..close].trim(), line)?, &BTreeMap::new(), line)?;
        if !(0..=127).contains(&s) {
            return Err(AsmError::new(line, format!("segment {s} out of 0..=127")));
        }
        let off = value(rest[close + 2..].trim(), line)?;
        Ok((off, seg.then_some(s as u8)))
    } else {
        Ok((value(t, line)?, seg.then_some(0)))
    }
}

/// Parse an operand; a bare register is named per the instruction `size`. In
/// segmented (`seg`) mode a memory address is `<<seg>>offset` and an indirect
/// pointer is a long register pair (`@RRn`).
fn operand(tok: &str, size: Size, line: usize, seg: bool) -> Result<Operand, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("bad operand `{tok}`"));

    if let Some(imm) = t.strip_prefix('#') {
        return Ok(Operand::Imm(value(imm, line)?));
    }
    if let Some(ptr) = t.strip_prefix('@') {
        let n = if seg { long_reg(ptr) } else { word_reg(ptr) }.ok_or_else(bad)?;
        return Ok(Operand::Ir(n));
    }
    if let Some(open) = t.find('(') {
        let close = t.rfind(')').ok_or_else(bad)?;
        let idx = word_reg(&t[open + 1..close]).ok_or_else(bad)?;
        let (off, sg) = seg_addr(&t[..open], line, seg)?;
        return Ok(Operand::Indexed(off, idx, sg));
    }
    if let Some(r) = size_reg(t, size) {
        return Ok(Operand::Reg(r));
    }
    // A bare expression is a direct address.
    let (off, sg) = seg_addr(t, line, seg)?;
    Ok(Operand::Da(off, sg))
}

/// Parse a register named for the instruction size. `Address` uses a word
/// register (the `LDA` destination).
fn size_reg(tok: &str, size: Size) -> Option<u16> {
    match size {
        Size::Byte => byte_reg(tok),
        Size::Long => long_reg(tok),
        Size::Quad => quad_reg(tok),
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

/// Quad register `rq0`/`rq4`/`rq8`/`rq12` (a multiple of four).
fn quad_reg(tok: &str) -> Option<u16> {
    let n = tok
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("rq")?
        .parse::<u16>()
        .ok()?;
    (n < 16 && n % 4 == 0).then_some(n)
}
