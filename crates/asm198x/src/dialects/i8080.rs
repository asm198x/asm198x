//! The Intel 8080 dialect front-end (asl / classic Intel syntax).
//!
//! Assembles against [`isa::i8080`] and produces a flat binary at the `org`.
//! Intel syntax differs from the other dialects in two ways handled here: the
//! **mnemonics** are Intel's (`MOV`/`MVI`/`LXI`/…, resolved by the spec), and
//! **numbers are radix-suffixed** (`42H`, `101B`, `377Q`, `65D`) rather than
//! `$`/`%`-prefixed — so this dialect supplies its own number lexer to the
//! shared expression parser. A hex literal must start with a digit (`0FFH`, not
//! `FFH`), matching `asl`; a letter-leading token is a symbol.
//!
//! Operand resolution is the candidate-label probe the Z80/rgbasm front-ends
//! use: each operand contributes register/pair tokens and/or `N`/`NN` value
//! placeholders, and the product is looked up against the spec. A bare register
//! word also offers an address interpretation, so a like-named label resolves.
//! `rst`'s vector number is embedded in the opcode.
//!
//! Output is validated byte-identical against `asl` (`cpu 8080`).

use std::collections::BTreeMap;

use super::asl::{self, AslChip};
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The Intel 8080 dialect.
pub(crate) struct I8080;

impl Dialect for I8080 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::i8080::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (U6, fixed-slot migration):
        // parse into a `Program`, then lower to the engine's statement stream —
        // byte-identical to the old direct parse (AE1). Other CPUs stay on
        // direct lowering behind the dialect boundary (KTD6).
        crate::ast::lower(parse_program(source)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source)?))
    }

    /// The include-capable parse (language-surface U4): the shared asl-family
    /// walk, resolving `INCLUDE`/`BINCLUDE` lazily through the loader — see
    /// [`parse_program_multi`].
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        crate::ast::lower(parse_program_multi(map, loader)?)
    }

    /// Intel `equ` takes no colon on its label (`name equ …`); a colon would
    /// fail to reassemble, since the label is disambiguated by the keyword.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse Intel-8080 source into the semantic [`Program`](crate::ast::Program)
/// via the shared asl-family walk ([`asl::parse_single`]): each line becomes a
/// node carrying its (global) label, operation, verbatim source, span, and
/// comment trivia. The 8080 has no local-label scoping, so every label is a
/// global symbol whose qualified name is the source name —
/// [`lower`](crate::ast::lower) then reproduces the old statements exactly.
/// `equ`/`=` constants fold at parse time, as before, so a `ds`/`rst` operand
/// that references one still resolves. An `INCLUDE`/`BINCLUDE` stays an
/// unresolved item — the target is never opened here (U4, KTD1).
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    asl::parse_single(Chip, source)
}

/// Parse a multi-file Intel-8080 program (language-surface U4): the shared
/// asl-family interleaved walk with asl's probe-pinned semantics — see
/// [`asl::parse_multi_files`].
///
/// # Errors
/// Any per-line parse failure (stamped with its file), a missing target, an
/// include cycle, a bad `BINCLUDE` window, or the depth backstop — all at the
/// directive's span.
pub(crate) fn parse_program_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<crate::ast::Program, AsmError> {
    asl::parse_multi_files(Chip, map, loader, &asl::SEMANTICS)
}

/// The 8080's hooks into the shared asl-family walk: its own comment scanner,
/// constant recogniser, label split, Intel number lexer, and operation parse.
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
        parse_op(&isa::i8080::SET, rest, consts, line)
    }

    fn value(&self, raw: &str, line: usize) -> Result<Expr, AsmError> {
        value(raw, line)
    }

    fn operand_span(&self, raw: &str, rest: &str, line: usize) -> Option<crate::ast::Span> {
        crate::ast::operand_span(raw, rest, line as u32)
    }
}

/// Split a line into its code and its `;` comment (with the delimiter, trailing
/// whitespace trimmed) for carrying comments as AST trivia. Ignores a `;` inside
/// a `'…'` char or `"…"` string; defined via [`strip_comment`] so the comment is
/// exactly what it removes — no behaviour change to assembly.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
}

/// Strip a `;` comment, ignoring `;` inside a `'…'` char or `"…"` string.
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
/// re-emit `NAME: <source>` with the label kept on the same line.
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

/// Split a leading `label:` from the line. A colon-terminated column-0 word is a
/// label; otherwise the line is an operation.
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

/// Parse the operation part: a directive or an instruction.
fn parse_op(
    set: &'static isa::InstructionSet,
    rest: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        // Assembler-control directives the flat model ignores.
        "cpu" | "end" | "title" | "page" | "name" | "aseg" | "cseg" => return Ok(None),
        "org" => Operation::Org(value(args, line)?),
        "db" | "defb" | "dc.b" => Operation::Bytes(byte_list(args, line)?),
        "dw" | "defw" | "dc.w" => Operation::Words(value_list(args, line)?),
        "ds" | "defs" => parse_ds(args, consts, line)?,
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

/// `ds count` — reserve `count` zero bytes.
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

/// The Intel radix-suffix number forms: `nnH` hex, `nnB` binary, `nnQ`/`nnO`
/// octal, `nnD`/bare decimal, and a `'c'` character literal. A hex literal
/// starts with a digit (the shared tokenizer guarantees it — a letter-leading
/// token lexes as a symbol).
pub(crate) fn parse_number_intel(tok: &str, line: usize) -> Result<i64, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("invalid number `{tok}`"));
    if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
        return t.chars().nth(1).map(|c| c as i64).ok_or_else(bad);
    }
    let (body, radix) = match t.as_bytes().last().map(u8::to_ascii_lowercase) {
        Some(b'h') => (&t[..t.len() - 1], 16),
        Some(b'b') => (&t[..t.len() - 1], 2),
        Some(b'o' | b'q') => (&t[..t.len() - 1], 8),
        Some(b'd') => (&t[..t.len() - 1], 10),
        _ => (t, 10),
    };
    i64::from_str_radix(body, radix).map_err(|_| bad())
}

// ---------------------------------------------------------------------------
// Operand resolution (Intel syntax -> spec mode label)
// ---------------------------------------------------------------------------

enum Cls {
    /// A bare word that names a register/pair but could also be a label.
    RegOrLabel(String, Expr),
    /// A value: an immediate, address, or `rst` vector number.
    Value(Expr),
}

type Alternative = (String, Vec<Expr>);

fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let pieces = if args.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level(args, ',')
    };
    let mut per_operand: Vec<Vec<Alternative>> = Vec::new();
    for piece in &pieces {
        per_operand.push(alternatives(mn, piece, consts, line)?);
    }

    for combo in product(&per_operand) {
        let label = combo
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if let Some(f) = set.instruction(mn).and_then(|i| i.form(&label)) {
            let emitted = combo.into_iter().flat_map(|(_, v)| v).collect();
            return Ok((f.mode, emitted));
        }
    }
    Err(AsmError::new(
        line,
        format!("`{mn}` has no form for operands `{}`", args.trim()),
    ))
}

fn alternatives(
    mn: &str,
    piece: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Vec<Alternative>, AsmError> {
    Ok(match classify(piece, line)? {
        Cls::RegOrLabel(t, e) => vec![
            (t, vec![]),
            ("N".to_string(), vec![e.clone()]),
            ("NN".to_string(), vec![e]),
        ],
        Cls::Value(expr) => {
            // `rst n` embeds the vector number in the opcode (no operand byte).
            if mn == "RST" {
                let n = fold_const(&expr, consts, line).map_err(|_| {
                    AsmError::new(line, "`rst` needs a constant vector number 0..7")
                })?;
                vec![(format!("{n}"), vec![])]
            } else {
                vec![
                    ("N".to_string(), vec![expr.clone()]),
                    ("NN".to_string(), vec![expr]),
                ]
            }
        }
    })
}

fn classify(piece: &str, line: usize) -> Result<Cls, AsmError> {
    let t = piece.trim();
    let lower = t.to_ascii_lowercase();
    if is_reg_or_pair(&lower) {
        Ok(Cls::RegOrLabel(lower, Expr::Sym(t.to_string())))
    } else {
        Ok(Cls::Value(value(t, line)?))
    }
}

/// The 8080 registers (`a`–`l`, `m`) and register pairs (`b`/`d`/`h`/`sp`/`psw`).
fn is_reg_or_pair(s: &str) -> bool {
    matches!(
        s,
        "a" | "b" | "c" | "d" | "e" | "h" | "l" | "m" | "sp" | "psw"
    )
}

/// Cartesian product of each operand's alternatives.
fn product(lists: &[Vec<Alternative>]) -> Vec<Vec<Alternative>> {
    let mut result: Vec<Vec<Alternative>> = vec![Vec::new()];
    for list in lists {
        let mut next = Vec::new();
        for combo in &result {
            for item in list {
                let mut extended = combo.clone();
                extended.push(item.clone());
                next.push(extended);
            }
        }
        result = next;
    }
    result
}

#[cfg(test)]
mod tests {
    use crate::assemble_i8080 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    /// U6 — the 8080 front-end routes through the AST, carrying comments as
    /// trivia (leading own-line + trailing same-line) without changing the
    /// emitted bytes (AE1).
    #[test]
    fn comments_are_carried_as_trivia() {
        let src = "; header\nstart:\n mvi a,5   ; load five\n ret\n";
        let prog = super::parse_program(src).expect("parses");

        // The header comment is leading trivia on the first node (`start:`).
        assert!(
            prog.nodes[0]
                .trivia
                .leading
                .iter()
                .any(|c| c.text == "; header"),
            "own-line comment attaches as leading trivia"
        );
        // The `mvi a,5` line carries its same-line comment as trailing trivia.
        assert!(
            prog.nodes.iter().any(|n| n
                .trivia
                .trailing
                .as_ref()
                .is_some_and(|c| c.text == "; load five")),
            "same-line comment attaches as trailing trivia"
        );
        // Comments never reach the encoder — bytes are unchanged.
        assert_eq!(
            bytes(src),
            bytes("start:\n mvi a,5\n ret\n"),
            "comments do not change bytes"
        );
    }

    #[test]
    fn moves_and_immediates() {
        assert_eq!(bytes(" mov a,b\n"), vec![0x78]);
        assert_eq!(bytes(" mov m,a\n"), vec![0x77]);
        assert_eq!(bytes(" mvi a,42h\n"), vec![0x3E, 0x42]);
        assert_eq!(bytes(" mvi m,0ffh\n"), vec![0x36, 0xFF]);
        assert_eq!(bytes(" lxi h,1234h\n"), vec![0x21, 0x34, 0x12]);
        assert_eq!(bytes(" lxi sp,0fffeh\n"), vec![0x31, 0xFE, 0xFF]);
    }

    #[test]
    fn number_radixes() {
        assert_eq!(bytes(" mvi a,101b\n"), vec![0x3E, 0x05]); // binary
        assert_eq!(bytes(" mvi a,377q\n"), vec![0x3E, 0xFF]); // octal
        assert_eq!(bytes(" mvi a,65d\n"), vec![0x3E, 0x41]); // explicit decimal
        assert_eq!(bytes(" mvi a,65\n"), vec![0x3E, 0x41]); // bare decimal
        assert_eq!(bytes(" mvi a,'A'\n"), vec![0x3E, 0x41]); // char
    }

    #[test]
    fn arithmetic_and_pairs() {
        assert_eq!(bytes(" add b\n"), vec![0x80]);
        assert_eq!(bytes(" add m\n"), vec![0x86]);
        assert_eq!(bytes(" cmp a\n"), vec![0xBF]);
        assert_eq!(bytes(" cpi 42h\n"), vec![0xFE, 0x42]);
        assert_eq!(bytes(" dad sp\n"), vec![0x39]);
        assert_eq!(bytes(" inx h\n"), vec![0x23]);
        assert_eq!(bytes(" push psw\n"), vec![0xF5]);
        assert_eq!(bytes(" pop b\n"), vec![0xC1]);
        assert_eq!(bytes(" ldax b\n"), vec![0x0A]);
        assert_eq!(bytes(" stax d\n"), vec![0x12]);
    }

    #[test]
    fn direct_jumps_calls_rst() {
        assert_eq!(bytes(" lda 1234h\n"), vec![0x3A, 0x34, 0x12]);
        assert_eq!(bytes(" shld 1234h\n"), vec![0x22, 0x34, 0x12]);
        assert_eq!(bytes(" jmp 1234h\n"), vec![0xC3, 0x34, 0x12]);
        assert_eq!(bytes(" jnz 1234h\n"), vec![0xC2, 0x34, 0x12]);
        assert_eq!(bytes(" call 1234h\n"), vec![0xCD, 0x34, 0x12]);
        assert_eq!(bytes(" out 0feh\n"), vec![0xD3, 0xFE]);
        assert_eq!(bytes(" rst 7\n"), vec![0xFF]);
        assert_eq!(bytes(" rst 0\n"), vec![0xC7]);
    }

    #[test]
    fn jump_to_label_not_register() {
        // `h` is register-pair H, but as a jump target it is a label.
        assert_eq!(bytes("h:\n jmp h\n"), vec![0xC3, 0x00, 0x00]);
        assert_eq!(bytes(" xchg\n pchl\n"), vec![0xEB, 0xE9]);
    }
}
