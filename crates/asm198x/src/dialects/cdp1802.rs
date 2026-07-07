//! The RCA CDP1802 (COSMAC) dialect front-end (asl syntax).
//!
//! Assembles against [`isa::cdp1802`] and produces a flat **big-endian** binary
//! at the `org`. Numbers are Intel `H`-suffix hex (shared with the 8080 dialect
//! via [`super::i8080::parse_number_intel`]). Operand resolution dispatches on
//! the mnemonic's form shape rather than probing:
//!
//! - a **register** op (`inc 3`) takes a bare register number 0..15 that is
//!   *embedded in the opcode* — the number becomes the spec's mode label, no
//!   operand byte is emitted;
//! - a **short** (page-relative) branch emits `Lo(target)` — the low byte of the
//!   target address, laid down as a plain one-byte operand (the page-relative
//!   trick needs no special engine path). The page match `asl` enforces is not
//!   yet validated here (a deferred nicety — it needs the resolved address);
//! - **immediate** and **long** ops take a value / a 16-bit address; **inherent**
//!   ops take nothing.
//!
//! Output is validated byte-identical against `asl` (`cpu 1802`).

use std::collections::BTreeMap;

use super::asl::{self, AslChip};
use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The RCA CDP1802 dialect.
pub(crate) struct Cdp1802;

impl Dialect for Cdp1802 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::cdp1802::SET
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

    /// asl `equ` takes no colon on its label (`name equ …`); a colon would fail
    /// to reassemble, since the label is disambiguated by the keyword.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse CDP1802 (COSMAC) source into the semantic [`Program`](crate::ast::Program)
/// via the shared asl-family walk ([`asl::parse_single`]): each line becomes
/// a node with its (global) label, operation, verbatim source, span, and
/// comment trivia — [`lower`](crate::ast::lower) reproduces the old
/// statements exactly. An `include`/`binclude` stays an unresolved item — the
/// target is never opened here (U4, KTD1).
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    asl::parse_single(Chip, source)
}

/// Parse a multi-file CDP1802 (COSMAC) program (language-surface U4): the shared
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

/// The CDP1802 (COSMAC)'s hooks into the shared asl-family walk (its own comment
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
        consts: &BTreeMap<String, i64>,
        line: usize,
    ) -> Result<Option<Operation>, AsmError> {
        parse_op(&isa::cdp1802::SET, rest, consts, line)
    }

    fn value(&self, raw: &str, line: usize) -> Result<Expr, AsmError> {
        value(raw, line)
    }

    fn operand_span(&self, raw: &str, rest: &str, line: usize) -> Option<crate::ast::Span> {
        crate::ast::operand_span(raw, rest, line as u32)
    }
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

/// Resolve an operand by the mnemonic's form shape.
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
    let t = operand.trim();

    if t.is_empty() {
        return if insn.form("inherent").is_some() {
            Ok(("inherent", vec![]))
        } else {
            Err(AsmError::new(line, format!("`{mn}` requires an operand")))
        };
    }
    // Short branch: emit the low byte of the (same-page) target.
    if insn.form("short").is_some() {
        return Ok(("short", vec![Expr::Lo(Box::new(value(t, line)?))]));
    }
    if insn.form("long").is_some() {
        return Ok(("long", vec![value(t, line)?]));
    }
    if insn.form("immediate").is_some() {
        return Ok(("immediate", vec![value(t, line)?]));
    }
    // Register op: the operand is a constant register number embedded in the
    // opcode; its decimal string is the spec's mode label.
    let n = fold_const(&value(t, line)?, consts, line)?;
    let label = n.to_string();
    let f = insn
        .form(&label)
        .ok_or_else(|| AsmError::new(line, format!("`{mn}` has no register {n} (valid 0..15)")))?;
    Ok((f.mode, vec![]))
}

#[cfg(test)]
mod tests {
    use crate::assemble_1802 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn register_ops() {
        assert_eq!(bytes(" inc 3\n"), vec![0x13]);
        assert_eq!(bytes(" inc 10\n"), vec![0x1A]);
        assert_eq!(bytes(" ldn 7\n"), vec![0x07]);
        assert_eq!(bytes(" glo 5\n"), vec![0x85]);
        assert_eq!(bytes(" sep 15\n"), vec![0xDF]);
        assert_eq!(bytes(" out 4\n"), vec![0x64]);
        assert_eq!(bytes(" inp 4\n"), vec![0x6C]);
    }

    #[test]
    fn immediate_and_inherent() {
        assert_eq!(bytes(" ldi 42h\n"), vec![0xF8, 0x42]);
        assert_eq!(bytes(" ani 0fh\n"), vec![0xFA, 0x0F]);
        assert_eq!(bytes(" adci 42h\n"), vec![0x7C, 0x42]);
        assert_eq!(bytes(" idl\n"), vec![0x00]);
        assert_eq!(bytes(" nop\n"), vec![0xC4]);
        assert_eq!(bytes(" sav\n"), vec![0x78]);
        assert_eq!(bytes(" shr\n"), vec![0xF6]);
    }

    #[test]
    fn short_branch_emits_low_byte() {
        // At org 1000h, `br` to 1050h emits the low byte 50h.
        assert_eq!(bytes(" org 1000h\n br 1050h\n"), vec![0x30, 0x50]);
        // A backward self-branch: br to a label on the same page.
        assert_eq!(bytes(" org 1000h\nl: br l\n"), vec![0x30, 0x00]);
        assert_eq!(bytes(" org 1000h\n bnz 10aah\n"), vec![0x3A, 0xAA]);
    }

    #[test]
    fn long_branch_is_big_endian() {
        assert_eq!(bytes(" lbr 1234h\n"), vec![0xC0, 0x12, 0x34]);
        assert_eq!(bytes(" lbnz 8000h\n"), vec![0xCA, 0x80, 0x00]);
    }

    #[test]
    fn directives() {
        assert_eq!(bytes(" db 1,2,\"AB\"\n"), vec![0x01, 0x02, 0x41, 0x42]);
        assert_eq!(bytes(" dw 1234h\n"), vec![0x12, 0x34]); // big-endian
        assert_eq!(bytes(" ds 3\n"), vec![0x00, 0x00, 0x00]);
    }

    /// U6 — the 1802 front-end routes through the AST, carrying comments as
    /// trivia without changing the emitted bytes (AE1).
    #[test]
    fn comments_are_carried_as_trivia() {
        let src = "; header\nstart:\n ldi 42h   ; load\n idl\n";
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
            bytes("start:\n ldi 42h\n idl\n"),
            "comments do not change bytes"
        );
    }
}
