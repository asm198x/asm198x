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
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, Expr, Operation, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The RCA CDP1802 dialect.
pub(crate) struct Cdp1802;

impl Dialect for Cdp1802 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::cdp1802::SET
    }

    /// asl leaves reserved space and `org` gaps as holes; `p2bin`, the converter
    /// that turns asl's object into a binary, fills them with `$FF`. Matching the
    /// reference pipeline byte-for-byte means reserving `$FF`, not `$00`.
    fn gap_fill(&self) -> u8 {
        0xFF
    }

    /// asl writes nothing for reserved space, and `p2bin` materialises only the
    /// gaps inside the written range — so a trailing `ds` is absent from the
    /// image rather than filled.
    fn trims_trailing_gap(&self) -> bool {
        true
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

/// What this dialect accepts beyond its instruction set.
///
/// asl is the reference for the CDP1802, and these are the spellings it takes.
/// The ignored ones emit no bytes and change no encoding, so accepting and
/// discarding them lets source that carries them assemble unchanged.
pub const DIRECTIVES: &[Directive] = &[
    Directive {
        id: "org",
        pattern: Pattern::Exact(&["org"]),
        category: Category::Operation,
    },
    Directive {
        id: "bytes",
        pattern: Pattern::Exact(&["db", "dc", "byte"]),
        category: Category::Operation,
    },
    Directive {
        id: "words",
        pattern: Pattern::Exact(&["dw", "word"]),
        category: Category::Operation,
    },
    Directive {
        id: "reserve",
        pattern: Pattern::Exact(&["ds", "rmb"]),
        category: Category::Operation,
    },
    Directive {
        id: "ignored",
        pattern: Pattern::Exact(&["cpu", "end", "title", "page", "aseg", "listing"]),
        category: Category::Ignored,
    },
];

fn parse_op(
    set: &'static isa::InstructionSet,
    rest: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);

    // Dispatch through the declared surface, not through the spelling: a
    // directive this dialect does not declare cannot be accepted here, which
    // is what makes the declaration a description of the dialect rather than
    // a copy of one. See `crate::directives`.
    let op = match lookup(DIRECTIVES, word)
        .or_else(|| lookup(super::asl::SEMANTIC_DIRECTIVES, word))
    {
        Some(directive) => match directive.category {
            Category::Ignored => return Ok(None),
            Category::ExpressionWord => {
                return Err(AsmError::new(
                    line,
                    crate::directives::not_a_statement(word),
                ));
            }
            Category::KnownUnsupported => {
                return Err(AsmError::new(
                    line,
                    format!(
                        "`{word}` is a real directive here and asm198x does not implement it yet"
                    ),
                ));
            }
            // Declared for `cdp1802` only where asl itself refuses the word for the
            // binary we emit; the refusal is the match, not a gap.
            Category::RefusedByReference(rule) => {
                return Err(AsmError::new(
                    line,
                    crate::directives::refused_by_reference("asl", word, rule),
                ));
            }
            Category::Operation => match directive.id {
                "org" => Operation::Org(value(args, line)?),
                "bytes" => Operation::Bytes(byte_list(args, line)?),
                "words" => Operation::Words(value_list(args, line)?),
                "reserve" => parse_ds(args, consts, line)?,
                // Unreachable while the declaration and this match agree, and
                // `every_declared_directive_is_dispatched` is what keeps them
                // agreeing.
                other => {
                    return Err(AsmError::new(
                        line,
                        format!("`{other}` is declared but not dispatched"),
                    ));
                }
            },
        },
        None => {
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
    Ok(Operation::Reserve(count))
}

fn byte_list(args: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if args.trim().is_empty() {
        return Err(AsmError::new(line, "`db` needs a value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(args) {
        if let Some(text) = string_literal(piece) {
            out.extend(super::mos6502::asl_string_bytes(text).map(|b| Expr::Num(i64::from(b))));
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
            logical: false,
            logical_not_tight: false,
            scoped_names: false,
            fixed_point: false,
            compare: crate::dialects::mos6502::Compare::default(),
            function: None,
            bang_is_or: false,
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
mod directive_surface {
    //! R3/R4: dispatch flows through the declaration, and the two agree.

    use super::{DIRECTIVES, parse_op};
    use crate::assemble_1802 as asm;
    use crate::directives::Category;
    use std::collections::BTreeMap;

    /// Every spelling the surface declares is accepted by the parser.
    ///
    /// Accepted means "not rejected as an unknown mnemonic". A directive that
    /// needs an argument still fails without one; what this asserts is that the
    /// word itself is recognised, which is what the declaration claims.
    #[test]
    fn every_declared_spelling_is_recognised() {
        let consts = BTreeMap::new();
        for directive in DIRECTIVES {
            for spelling in &directive.spellings() {
                let line = format!("{spelling} 1");
                let result = parse_op(&isa::cdp1802::SET, &line, &consts, 1);
                if let Err(e) = &result {
                    assert!(
                        !e.to_string().contains("unknown"),
                        "`{spelling}` is declared but the parser does not know it: {e}"
                    );
                }
            }
        }
    }

    /// A word the surface does not declare is not treated as a directive.
    ///
    /// R3 is the point: the declaration is the only route to a directive, so a
    /// word outside it falls through to instruction resolution and is refused
    /// as a mnemonic. `include` would be the obvious probe and is the wrong
    /// one — the asl-family chips do implement it, which the plan's roster for
    /// that work does not yet say.
    #[test]
    fn an_undeclared_spelling_is_not_a_directive() {
        let err = asm(" frobnicate 1\n").expect_err("not a directive here");
        assert!(
            err.to_string().to_lowercase().contains("unknown"),
            "expected an unknown-mnemonic rejection, got: {err}"
        );
    }

    /// Every `Operation` entry has a dispatch arm.
    ///
    /// The arm bodies match on `directive.id`, and a declared id with no arm
    /// would reach the fallback that reports it as undispatched. This proves
    /// none does, so the declaration and the dispatch cannot drift apart.
    #[test]
    fn every_declared_directive_is_dispatched() {
        let consts = BTreeMap::new();
        for directive in DIRECTIVES {
            if directive.category != Category::Operation {
                continue;
            }
            let spelling = directive.spellings()[0].clone();
            let line = format!("{spelling} 1");
            if let Err(e) = parse_op(&isa::cdp1802::SET, &line, &consts, 1) {
                assert!(
                    !e.to_string().contains("declared but not dispatched"),
                    "`{}` has no dispatch arm",
                    directive.id
                );
            }
        }
    }

    /// The ignored spellings still assemble to nothing, as they did before the
    /// conversion.
    #[test]
    fn ignored_directives_emit_no_bytes() {
        for spelling in ["cpu", "end", "title", "page", "aseg", "listing"] {
            let src = format!(" {spelling} whatever\n");
            assert!(
                asm(&src).expect("assembles").bytes.is_empty(),
                "`{spelling}` should emit nothing"
            );
        }
    }
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
        // Reserved space follows asl + p2bin, which reserves rather than
        // materialises: a gap *inside* the written range is filled with $FF,
        // and a trailing reservation is absent from the image entirely.
        assert_eq!(
            bytes(" db 1\n ds 3\n db 9\n"),
            vec![0x01, 0xFF, 0xFF, 0xFF, 0x09]
        );
        assert_eq!(bytes(" db 9\n ds 3\n"), vec![0x09]);
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
