//! The Intel 8048 (MCS-48) dialect front-end (asl syntax).
//!
//! Assembles against [`isa::i8048`] and produces a flat binary at the `org`.
//! Numbers are Intel `H`-suffix hex (shared with the 8080 dialect via
//! [`super::i8080::parse_number_intel`]). Operand resolution builds the spec's
//! **mode label** from the operand text — fixed keywords (`a`, `psw`, `p1`,
//! `@r0`, `r3`, …) map to themselves, a `#`-prefixed operand becomes the `#N`
//! placeholder (emitting the immediate byte), and a bare jump target becomes
//! `rel`, emitting `Lo(target)` for the page-relative conditional jumps (the
//! CDP1802 short-branch trick). The joined label selects the form.
//!
//! `JMP`/`CALL` are the exception: their 11-bit absolute address packs its high
//! 3 bits into the opcode byte (`opcode = base | (addr>>8 & 7)<<5`), and the
//! address may be a forward label — so the opcode byte is a *function of the
//! operand*. They go through the computed-operand seam ([`Operation::Encoded`]),
//! the opcode byte laid down as an `Expr` resolved in pass 2. No engine change:
//! the seam already carries computed bytes (first used by the 6809).
//!
//! Output is validated byte-identical against `asl` (`cpu 8048`).

use std::collections::BTreeMap;

use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Piece, Statement};

/// The Intel 8048 (MCS-48) dialect.
///
/// `romless` selects the ROM-less parts (8035/8039/8040 and their CMOS kin),
/// which share the 8048's encoding but forbid the four BUS-port instructions
/// (`ORL`/`ANL BUS,#data`, `OUTL BUS,A`, `INS A,BUS`) — on a ROM-less part the
/// bus is committed to fetching external program memory. `asl` enforces the same
/// restriction for `cpu 8039`.
pub(crate) struct I8048 {
    pub(crate) romless: bool,
}

impl Dialect for I8048 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::i8048::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (0b, fixed-slot straggler
        // migration): parse into a `Program`, then lower to the engine's
        // statement stream — byte-identical to the old direct parse (AE1).
        crate::ast::lower(parse_program(source, self.romless)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source, self.romless)?))
    }

    /// Intel `equ` takes no colon on its label (`name equ …`); a colon would
    /// fail to reassemble, since the label is disambiguated by the keyword.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse MCS-48 source into the semantic [`Program`](crate::ast::Program). Each
/// line becomes a node carrying its (global) label, operation, verbatim source,
/// span, and comment trivia. The 8048 has no local-label scoping, so every label
/// is a [`Scope::Global`](crate::ast::Scope) symbol; [`lower`](crate::ast::lower)
/// reproduces the old statements exactly, so bytes are unchanged. `romless`
/// threads through to [`parse_op`], which rejects the BUS-port ops on the
/// ROM-less parts.
pub(crate) fn parse_program(source: &str, romless: bool) -> Result<crate::ast::Program, AsmError> {
    use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
    let set = &isa::i8048::SET;
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
            parse_op(set, rest, romless, &consts, line)?
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

/// Split a line into its code and its `;` comment (with the leading `;` and
/// whitespace trimmed) for carrying comments as AST trivia. Defined via
/// [`strip_comment`] so the comment is exactly what it removes — no behaviour
/// change to assembly.
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

fn parse_op(
    set: &'static isa::InstructionSet,
    rest: &str,
    romless: bool,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        "cpu" | "end" | "title" | "page" | "name" => return Ok(None),
        "org" => Operation::Org(value(args, line)?),
        "db" | "dc" | "byte" => Operation::Bytes(byte_list(args, line)?),
        "dw" | "word" => Operation::Words(value_list(args, line)?),
        "ds" | "rmb" => parse_ds(args, consts, line)?,
        _ => resolve(set, &word.to_ascii_uppercase(), args, romless, line)?,
    };
    Ok(Some(op))
}

fn parse_ds(
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let count = fold_const(&value(args.trim(), line)?, consts, line)?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`ds` count must be a non-negative constant"))?;
    Ok(Operation::Bytes(vec![Expr::Num(0); count]))
}

fn byte_list(args: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if args.trim().is_empty() {
        return Err(AsmError::new(line, "`db` needs a value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(args) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.bytes().map(|b| Expr::Num(i64::from(b))));
        } else {
            out.push(value(piece, line)?);
        }
    }
    Ok(out)
}

fn value_list(args: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if args.trim().is_empty() {
        return Err(AsmError::new(line, "`dw` needs a value"));
    }
    split_top_level(args, ',')
        .iter()
        .map(|p| value(p, line))
        .collect()
}

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

/// Resolve an instruction to an operation. `JMP`/`CALL` take the computed
/// seam; every other mnemonic builds a mode label from its operand text.
fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    args: &str,
    romless: bool,
    line: usize,
) -> Result<Operation, AsmError> {
    if mn == "JMP" || mn == "CALL" {
        return jump(mn, args, line);
    }
    let insn = set
        .instruction(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;

    let pieces = if args.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level(args, ',')
    };
    let mut tokens: Vec<String> = Vec::new();
    let mut operands: Vec<Expr> = Vec::new();
    for piece in &pieces {
        let t = piece.trim();
        if let Some(imm) = t.strip_prefix('#') {
            tokens.push("#N".to_string());
            operands.push(value(imm, line)?);
        } else if is_keyword(&t.to_ascii_lowercase()) {
            tokens.push(t.to_ascii_lowercase());
        } else {
            // A bare value is a page-relative jump target: emit its low byte.
            tokens.push("rel".to_string());
            operands.push(Expr::Lo(Box::new(value(t, line)?)));
        }
    }
    let label = tokens.join(",");
    if romless && is_bus_op(mn, &label) {
        return Err(AsmError::new(
            line,
            format!(
                "`{mn} {}` is not available on ROM-less MCS-48 parts (8035/8039/8040): \
                 the bus is reserved for external program memory",
                args.trim()
            ),
        ));
    }
    let f = insn
        .form(&label)
        .ok_or_else(|| AsmError::new(line, format!("`{mn}` has no form for `{}`", args.trim())))?;
    Ok(Operation::Instruction {
        mnemonic: mn.to_string(),
        mode: f.mode,
        operands,
    })
}

/// `JMP`/`CALL`: the opcode byte carries address bits 10-8, so it is computed
/// from the (possibly forward) target via the computed-operand seam.
fn jump(mn: &str, args: &str, line: usize) -> Result<Operation, AsmError> {
    let target = value(args.trim(), line)?;
    let base: i64 = if mn == "JMP" { 0x04 } else { 0x14 };
    // (target >> 8) & 7
    let page = Expr::Bin(
        BinOp::And,
        Box::new(Expr::Bin(
            BinOp::Shr,
            Box::new(target.clone()),
            Box::new(Expr::Num(8)),
        )),
        Box::new(Expr::Num(7)),
    );
    // base | (page << 5)
    let opcode = Expr::Bin(
        BinOp::Or,
        Box::new(Expr::Num(base)),
        Box::new(Expr::Bin(
            BinOp::Shl,
            Box::new(page),
            Box::new(Expr::Num(5)),
        )),
    );
    Ok(Operation::Encoded(vec![
        Piece::Val {
            expr: opcode,
            bytes: 1,
            rel: false,
            signed: false,
        },
        Piece::Val {
            expr: Expr::Lo(Box::new(target)),
            bytes: 1,
            rel: false,
            signed: false,
        },
    ]))
}

/// The four BUS-port instructions the ROM-less parts forbid (the bus is busy
/// fetching external program memory): `ORL`/`ANL BUS,#data`, `OUTL BUS,A`,
/// `INS A,BUS`. Keyed by `(mnemonic, mode label)`.
fn is_bus_op(mn: &str, label: &str) -> bool {
    matches!(
        (mn, label),
        ("ORL", "bus,#N") | ("ANL", "bus,#N") | ("OUTL", "bus,a") | ("INS", "a,bus")
    )
}

/// The 8048 fixed operand keywords: the accumulator, ports, registers, control
/// bits — anything that is part of the instruction identity rather than a value.
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "a" | "psw"
            | "t"
            | "bus"
            | "c"
            | "f0"
            | "f1"
            | "i"
            | "tcnti"
            | "rb0"
            | "rb1"
            | "mb0"
            | "mb1"
            | "cnt"
            | "tcnt"
            | "clk"
            | "@a"
            | "@r0"
            | "@r1"
            | "p1"
            | "p2"
            | "p4"
            | "p5"
            | "p6"
            | "p7"
            | "r0"
            | "r1"
            | "r2"
            | "r3"
            | "r4"
            | "r5"
            | "r6"
            | "r7"
    )
}

#[cfg(test)]
mod tests {
    use crate::{assemble_8039, assemble_8048 as asm};

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn accumulator_ops() {
        assert_eq!(bytes(" add a,r0\n"), vec![0x68]);
        assert_eq!(bytes(" add a,r7\n"), vec![0x6F]);
        assert_eq!(bytes(" add a,@r1\n"), vec![0x61]);
        assert_eq!(bytes(" add a,#12h\n"), vec![0x03, 0x12]);
        assert_eq!(bytes(" anl p2,#0fh\n"), vec![0x9A, 0x0F]);
        assert_eq!(bytes(" orl bus,#55h\n"), vec![0x88, 0x55]);
    }

    #[test]
    fn moves_and_registers() {
        assert_eq!(bytes(" mov a,#42h\n"), vec![0x23, 0x42]);
        assert_eq!(bytes(" mov r7,#01h\n"), vec![0xBF, 0x01]);
        assert_eq!(bytes(" mov a,psw\n"), vec![0xC7]);
        assert_eq!(bytes(" inc @r0\n"), vec![0x10]);
        assert_eq!(bytes(" dec r7\n"), vec![0xCF]);
        assert_eq!(bytes(" movx @r1,a\n"), vec![0x91]);
        assert_eq!(bytes(" sel mb1\n"), vec![0xF5]);
    }

    #[test]
    fn jmp_and_call_pack_the_page() {
        assert_eq!(bytes(" jmp 100h\n"), vec![0x24, 0x00]);
        assert_eq!(bytes(" call 200h\n"), vec![0x54, 0x00]);
        assert_eq!(bytes(" jmp 7ffh\n"), vec![0xE4, 0xFF]);
        // Forward label: the opcode page resolves in pass 2 (target 302h → page 3).
        assert_eq!(
            bytes(" org 300h\n jmp there\nthere: nop\n"),
            vec![0x64, 0x02, 0x00]
        );
    }

    #[test]
    fn conditional_jumps_emit_low_byte() {
        assert_eq!(bytes(" org 100h\n jz 150h\n"), vec![0xC6, 0x50]);
        assert_eq!(bytes(" org 100h\n jb7 1aah\n"), vec![0xF2, 0xAA]);
        assert_eq!(bytes(" org 100h\n djnz r3,150h\n"), vec![0xEB, 0x50]);
    }

    #[test]
    fn romless_shares_the_8048_encoding() {
        // Every non-BUS instruction encodes identically on the ROM-less parts.
        for src in [
            " add a,r7\n",
            " mov a,#42h\n",
            " orl p1,#5\n",
            " anl p2,#5\n",
            " outl p1,a\n",
            " movx @r0,a\n",
            " movp a,@a\n",
            " sel mb1\n",
            " org 100h\n jz 150h\n",
            " jmp 7ffh\n",
        ] {
            assert_eq!(
                assemble_8039(src).expect("8039 assemble").bytes,
                asm(src).expect("8048 assemble").bytes,
                "8039 vs 8048 differ for `{}`",
                src.trim()
            );
        }
    }

    #[test]
    fn romless_forbids_the_bus_port_ops() {
        // The bus is committed to fetching external program memory (asl agrees).
        for src in [
            " orl bus,#55h\n",
            " anl bus,#0fh\n",
            " outl bus,a\n",
            " ins a,bus\n",
        ] {
            assert!(
                assemble_8039(src).is_err(),
                "ROM-less part should reject `{}`",
                src.trim()
            );
            // ...but the full 8048 still accepts it.
            assert!(asm(src).is_ok(), "8048 should accept `{}`", src.trim());
        }
    }
}
