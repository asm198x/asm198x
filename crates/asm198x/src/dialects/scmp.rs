//! The National SC/MP (INS8060) dialect front-end (asl syntax).
//!
//! Assembles against [`isa::scmp`] and produces a flat binary at the `org`.
//! Numbers are C-style (`0x..` hex, `0b..` binary, decimal), matching `asl`'s
//! SC/MP mode. Operand resolution dispatches on syntax:
//!
//! - no operand → an **inherent** form;
//! - `disp(ptr)` / `@disp(ptr)` → a **memory reference**: the pointer 0..3 and
//!   the optional `@` auto-index select the form (mode `"0"`..`"3"`/`"@1"`..
//!   `"@3"`), the signed displacement is emitted as the following byte. The
//!   literal `e` is the E-register index (displacement byte `0x80`);
//! - a bare operand → an **immediate** byte if the instruction has one
//!   (`LDI`/`ANI`/…/`DLY`), otherwise a **pointer number** 0..3 embedded in the
//!   opcode (the `XPAL`/`XPAH`/`XPPC` exchanges, no following byte).
//!
//! Every form is fixed-slot; no engine seam. Output is validated byte-identical
//! against `asl` (`cpu SC/MP`).

use std::collections::BTreeMap;

use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The National SC/MP (INS8060) dialect.
pub(crate) struct Scmp;

impl Dialect for Scmp {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::scmp::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (U6, fixed-slot): parse into a
        // `Program`, then lower to the engine's statement stream — byte-identical
        // to the old direct parse (AE1). Other CPUs stay on direct lowering
        // behind the dialect boundary (KTD6).
        crate::ast::lower(parse_program(source)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source)?))
    }

    /// asl `equ` takes no colon on its label (`name equ …`); a colon would fail
    /// to reassemble, since the label is disambiguated by the keyword.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse SC/MP source into the semantic [`Program`](crate::ast::Program). Each
/// line becomes a node with its (global) label, operation, verbatim source, span,
/// and comment trivia. SC/MP has no local-label scoping, so every label is a
/// [`Scope::Global`](crate::ast::Scope) symbol whose qualified name is the source
/// name — [`lower`](crate::ast::lower) reproduces the old statements exactly.
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
    let set = &isa::scmp::SET;
    let mut nodes = Vec::new();
    let mut consts: BTreeMap<String, i64> = BTreeMap::new();
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
            parse_op(set, rest, &consts, line)?
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

/// Split a line into its code and its `;` comment (delimiter kept, trailing
/// whitespace trimmed) for carrying comments as AST trivia; defined via
/// [`strip_comment`] so the comment is exactly what it removes.
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

/// `NAME EQU expr` / `NAME = expr`. Returns the name, the value expression, and
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
        _ => resolve(set, &word.to_ascii_uppercase(), args, consts, line)?,
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
        parse_number_c,
        ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: Caret::Xor,
            at_is_pc: false,
        },
    )
}

/// C-style number lexer: `0x`/`0X` hex, `0b`/`0B` binary, `'c'` char, decimal.
fn parse_number_c(tok: &str, line: usize) -> Result<i64, AsmError> {
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

/// Resolve an instruction by its operand syntax.
fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let insn = set
        .instruction(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    let instr = |mode: &'static str, operands: Vec<Expr>| Operation::Instruction {
        mnemonic: mn.to_string(),
        mode,
        operands,
    };
    let t = args.trim();

    if t.is_empty() {
        return match insn.form("") {
            Some(f) => Ok(instr(f.mode, vec![])),
            None => Err(AsmError::new(line, format!("`{mn}` requires an operand"))),
        };
    }

    // Memory reference: `disp(ptr)` or `@disp(ptr)`.
    if let Some(open) = t.find('(') {
        let close = t
            .rfind(')')
            .ok_or_else(|| AsmError::new(line, "missing `)` in operand"))?;
        let ptr = fold_const(&value(t[open + 1..close].trim(), line)?, consts, line)?;
        if !(0..=3).contains(&ptr) {
            return Err(AsmError::new(line, "pointer register must be 0..3"));
        }
        let mut disp = t[..open].trim();
        let at = disp.starts_with('@');
        if at {
            disp = disp[1..].trim();
        }
        let label = format!("{}{ptr}", if at { "@" } else { "" });
        let f = insn.form(&label).ok_or_else(|| {
            AsmError::new(line, format!("`{mn}` has no `{}` addressing mode", label))
        })?;
        // The literal `e` is the E-register index (displacement byte 0x80).
        let disp = if disp.eq_ignore_ascii_case("e") {
            Expr::Num(0x80)
        } else {
            value(disp, line)?
        };
        return Ok(instr(f.mode, vec![disp]));
    }

    // A bare operand: an immediate byte where the instruction has one, else a
    // pointer number embedded in the opcode.
    if let Some(f) = insn.form("imm") {
        return Ok(instr(f.mode, vec![value(t, line)?]));
    }
    let n = fold_const(&value(t, line)?, consts, line)?;
    if !(0..=3).contains(&n) {
        return Err(AsmError::new(line, "pointer register must be 0..3"));
    }
    match insn.form(&n.to_string()) {
        Some(f) => Ok(instr(f.mode, vec![])),
        None => Err(AsmError::new(line, format!("`{mn}` takes no operand"))),
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_scmp as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn inherent_and_pointer_exchange() {
        assert_eq!(bytes(" nop\n"), vec![0x08]);
        assert_eq!(bytes(" cae\n"), vec![0x78]);
        assert_eq!(bytes(" xpal 1\n"), vec![0x31]);
        assert_eq!(bytes(" xppc 3\n"), vec![0x3F]);
    }

    #[test]
    fn memory_reference() {
        assert_eq!(bytes(" ld 5(1)\n"), vec![0xC1, 0x05]);
        assert_eq!(bytes(" ld @5(2)\n"), vec![0xC6, 0x05]);
        assert_eq!(bytes(" ld -1(1)\n"), vec![0xC1, 0xFF]);
        assert_eq!(bytes(" st @1(1)\n"), vec![0xCD, 0x01]);
        assert_eq!(bytes(" ld e(1)\n"), vec![0xC1, 0x80]);
        assert_eq!(bytes(" cad 7(1)\n"), vec![0xF9, 0x07]);
        assert_eq!(bytes(" jmp 0(0)\n"), vec![0x90, 0x00]);
        assert_eq!(bytes(" ild 2(1)\n"), vec![0xA9, 0x02]);
    }

    #[test]
    fn immediates_c_hex() {
        assert_eq!(bytes(" ldi 0x42\n"), vec![0xC4, 0x42]);
        assert_eq!(bytes(" ani 0x0f\n"), vec![0xD4, 0x0F]);
        assert_eq!(bytes(" xri 0b10101010\n"), vec![0xE4, 0xAA]);
        assert_eq!(bytes(" dly 5\n"), vec![0x8F, 0x05]);
    }

    /// U6 — the SC/MP front-end routes through the AST, carrying comments as
    /// trivia without changing the emitted bytes (AE1).
    #[test]
    fn comments_are_carried_as_trivia() {
        let src = "; header\nstart:\n ldi 0x42   ; load\n nop\n";
        let prog = super::parse_program(src).expect("parses");
        assert!(
            prog.nodes[0]
                .trivia
                .leading
                .iter()
                .any(|c| c.text == "; header"),
            "own-line comment attaches as leading trivia"
        );
        assert!(
            prog.nodes.iter().any(|n| n
                .trivia
                .trailing
                .as_ref()
                .is_some_and(|c| c.text == "; load")),
            "same-line comment attaches as trailing trivia"
        );
        assert_eq!(
            bytes(src),
            bytes("start:\n ldi 0x42\n nop\n"),
            "comments do not change bytes"
        );
    }
}
