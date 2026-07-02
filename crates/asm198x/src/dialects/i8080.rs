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

use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The Intel 8080 dialect.
pub(crate) struct I8080;

impl Dialect for I8080 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::i8080::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let set = self.instruction_set();
        let mut out = Vec::new();
        let mut consts: BTreeMap<String, i64> = BTreeMap::new();

        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            let code = strip_comment(raw);
            if code.trim().is_empty() {
                continue;
            }
            // `NAME EQU expr` / `NAME = expr` — a constant.
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
                parse_op(set, rest, &consts, line)?
            };
            if label.is_some() || op.is_some() {
                out.push(Statement { line, label, op });
            }
        }
        Ok(out)
    }
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

/// `NAME EQU expr` or `NAME = expr`. Returns the name and value expression.
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
fn parse_number_intel(tok: &str, line: usize) -> Result<i64, AsmError> {
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
