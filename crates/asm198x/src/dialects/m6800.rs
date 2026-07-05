//! The Motorola 6800 dialect front-end (asl / classic Motorola syntax).
//!
//! Assembles against [`isa::m6800`] and produces a flat **big-endian** binary at
//! the `org`. Motorola syntax reuses the shared `$`-prefix number lexer and the
//! `mos6502` expression grammar; what is 6800-specific lives here: the addressing
//! modes (`#` immediate, `$nn,X` indexed, direct-vs-extended by operand size or a
//! `>`/`<` force), the branch/inherent shapes, and the `fcb`/`fdb`/`rmb`
//! directives.
//!
//! Direct-vs-extended mirrors the 6502's zero-page-vs-absolute choice: a
//! constant that fits a byte and has a direct form uses it, otherwise extended;
//! a `>`/`<` prefix forces extended/direct explicitly (as `asl` does).
//!
//! Output is validated byte-identical against `asl` (`cpu 6800`).

use std::collections::BTreeMap;

use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, split_top_level, string_literal, top_level_rfind,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The Motorola 6800 dialect.
pub(crate) struct M6800;

impl Dialect for M6800 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::m6800::SET
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

    /// Motorola `equ` takes no colon on its label (`name equ …`); a colon would
    /// fail to reassemble, since the label is disambiguated by the keyword (as
    /// on the 8080).
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse Motorola-6800 source into the semantic [`Program`](crate::ast::Program).
/// Each line becomes a node with its (global) label, operation, verbatim source,
/// span, and comment trivia. The 6800 has no local-label scoping, so every label
/// is a [`Scope::Global`](crate::ast::Scope) symbol whose qualified name is the
/// source name — [`lower`](crate::ast::lower) then reproduces the old statements
/// exactly. `equ`/`=` constants fold at parse time, as before.
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
    let set = &isa::m6800::SET;
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
        "fcb" | "db" => Operation::Bytes(byte_list(args, line)?),
        "fdb" | "dw" => Operation::Words(value_list(args, line)?),
        "rmb" | "ds" => parse_rmb(args, consts, line)?,
        _ => {
            let mn = word.to_ascii_uppercase();
            let (mode, operands) = resolve(set, &mn, args, consts, line)?;
            Operation::Instruction {
                mnemonic: mn,
                mode,
                operands,
            }
        }
    };
    Ok(Some(op))
}

/// `rmb count` — reserve `count` zero bytes.
fn parse_rmb(
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let count = fold_const(&value(args.trim(), line)?, consts, line)?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`rmb` count must be a non-negative constant"))?;
    Ok(Operation::Bytes(vec![Expr::Num(0); count]))
}

fn byte_list(args: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if args.trim().is_empty() {
        return Err(AsmError::new(line, "`fcb` needs a value"));
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
        return Err(AsmError::new(line, "`fdb` needs a value"));
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
        parse_number,
        ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: Caret::Xor,
            at_is_pc: false,
        },
    )
}

// ---------------------------------------------------------------------------
// Operand resolution (Motorola syntax -> spec mode label)
// ---------------------------------------------------------------------------

/// A `>`/`<` address-size force.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Force {
    None,
    Direct,
    Extended,
}

fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    operand: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let insn = set
        .instruction(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    let has = |mode: &str| insn.form(mode).is_some();
    let t = operand.trim();

    if t.is_empty() {
        return Ok(("inherent", vec![]));
    }
    if let Some(imm) = t.strip_prefix('#') {
        return Ok(("immediate", vec![value(imm, line)?]));
    }
    // Indexed: `expr,X`.
    if let Some(c) = top_level_rfind(t, ',')
        && t[c + 1..].trim().eq_ignore_ascii_case("x")
    {
        return Ok(("indexed", vec![value(t[..c].trim(), line)?]));
    }
    // Branch target (relative) — the only single-address form these mnemonics have.
    if has("relative") {
        return Ok(("relative", vec![value(t, line)?]));
    }
    // Direct vs extended, honouring a `>`/`<` force.
    let (force, body) = strip_force(t);
    let expr = value(body, line)?;
    let mode = pick(&has, force, &expr, consts);
    Ok((mode, vec![expr]))
}

/// Strip a leading `>` (force extended) or `<` (force direct).
fn strip_force(t: &str) -> (Force, &str) {
    if let Some(r) = t.strip_prefix('>') {
        (Force::Extended, r.trim())
    } else if let Some(r) = t.strip_prefix('<') {
        (Force::Direct, r.trim())
    } else {
        (Force::None, t)
    }
}

/// Pick direct or extended: an explicit force wins; otherwise a byte-sized
/// constant with a direct form uses direct, and a forward/large value uses
/// extended (matching `asl`).
fn pick(
    has: &dyn Fn(&str) -> bool,
    force: Force,
    expr: &Expr,
    consts: &BTreeMap<String, i64>,
) -> &'static str {
    let fits_direct = match force {
        Force::Direct => true,
        Force::Extended => false,
        Force::None => fold_const(expr, consts, 0).is_ok_and(|v| (0..=0xFF).contains(&v)),
    };
    if fits_direct && has("direct") {
        "direct"
    } else {
        "extended"
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_m6800 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn addressing_modes() {
        assert_eq!(bytes(" ldaa #$42\n"), vec![0x86, 0x42]);
        assert_eq!(bytes(" ldaa $42\n"), vec![0x96, 0x42]); // direct
        assert_eq!(bytes(" ldaa $1234\n"), vec![0xB6, 0x12, 0x34]); // extended, big-endian
        assert_eq!(bytes(" ldaa $05,x\n"), vec![0xA6, 0x05]); // indexed
        assert_eq!(bytes(" ldab #$11\n"), vec![0xC6, 0x11]);
        assert_eq!(bytes(" staa $1234\n"), vec![0xB7, 0x12, 0x34]);
    }

    #[test]
    fn sixteen_bit_and_force() {
        assert_eq!(bytes(" ldx #$1234\n"), vec![0xCE, 0x12, 0x34]); // 16-bit immediate BE
        assert_eq!(bytes(" ldx $1234\n"), vec![0xFE, 0x12, 0x34]);
        assert_eq!(bytes(" cpx #$0010\n"), vec![0x8C, 0x00, 0x10]);
        // `>` forces extended even for a low address.
        assert_eq!(bytes(" ldaa >$50\n"), vec![0xB6, 0x00, 0x50]);
        // JMP has no direct form, so a low address is extended automatically.
        assert_eq!(bytes(" jmp $50\n"), vec![0x7E, 0x00, 0x50]);
    }

    #[test]
    fn inherent_and_branches() {
        assert_eq!(bytes(" nop\n"), vec![0x01]);
        assert_eq!(bytes(" aba\n"), vec![0x1B]);
        assert_eq!(bytes(" clra\n"), vec![0x4F]);
        assert_eq!(bytes(" negb\n"), vec![0x50]);
        // A backward branch to a label at origin 0.
        assert_eq!(bytes("l:\n bra l\n"), vec![0x20, 0xFE]);
        assert_eq!(bytes("l:\n bne l\n"), vec![0x26, 0xFE]);
        assert_eq!(bytes(" jsr $05,x\n"), vec![0xAD, 0x05]);
    }

    #[test]
    fn directives() {
        assert_eq!(bytes(" fcb $01,$02,\"AB\"\n"), vec![0x01, 0x02, 0x41, 0x42]);
        assert_eq!(bytes(" fdb $1234\n"), vec![0x12, 0x34]); // big-endian
        assert_eq!(bytes(" rmb 3\n"), vec![0x00, 0x00, 0x00]);
    }

    /// U6 — the 6800 front-end routes through the AST, carrying comments as
    /// trivia without changing the emitted bytes (AE1).
    #[test]
    fn comments_are_carried_as_trivia() {
        let src = "; header\nstart:\n ldaa #$42   ; load\n rts\n";
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
            bytes("start:\n ldaa #$42\n rts\n"),
            "comments do not change bytes"
        );
    }
}
