//! The 6502 dialect front-end.
//!
//! A deliberately small starting point, not yet specific to acme or ca65:
//! implied, accumulator, immediate, zero-page, absolute, the indexed and
//! indirect modes, and relative branches; plus `.org`, `.byte`/`.db`,
//! `.word`/`.dw`; plus `<`/`>` low/high-byte operators. Full source-
//! compatibility with a real 6502 dialect is the goal, tracked in
//! `decisions/syntax-stance.md` — not yet reached. Notable gaps marked `TODO`:
//! arithmetic in expressions, string/char escapes, scoped labels, macros.
//!
//! This module owns only the **dialect**: how 6502 source maps to addressing
//! modes. Encoding comes from [`isa::mos6502`]; the two-pass engine and byte
//! emission live in [`crate::engine`].

use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The 6502 dialect (ca65 / ACME shaped; an early subset for now).
pub(crate) struct Mos6502;

impl Dialect for Mos6502 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::mos6502::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let set = self.instruction_set();
        let mut out = Vec::new();
        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            // Strip line comments. TODO: a `;` inside a char literal would be
            // cut here; acceptable until char literals in operands are used.
            let code = raw.find(';').map_or(raw, |idx| &raw[..idx]);
            let trimmed = code.trim();
            if trimmed.is_empty() {
                continue;
            }

            let (label, rest) = split_label(trimmed, line)?;

            let op = if rest.is_empty() {
                None
            } else if let Some(directive) = rest.strip_prefix('.') {
                Some(parse_directive(directive, line)?)
            } else {
                let (mnemonic, remainder) = split_first_word(rest);
                let mnemonic = mnemonic.to_ascii_uppercase();
                let operand = parse_operand(remainder, line)?;
                let insn = set.instruction(&mnemonic).ok_or_else(|| {
                    AsmError::new(line, format!("unknown instruction `{mnemonic}`"))
                })?;
                let (mode, operand) = resolve_mode(insn, operand, line)?;
                Some(Operation::Instruction {
                    mnemonic,
                    mode,
                    operands: operand.into_iter().collect(),
                })
            };

            out.push(Statement { line, label, op });
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Operand syntax (parsed) and mode resolution (dialect -> spec)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Index {
    X,
    Y,
}

/// Operand syntax as parsed, before it is resolved to an addressing mode.
enum OperandSyntax {
    None,
    Accumulator,
    Immediate(Expr),
    Indirect(Expr),
    IndexedIndirect(Expr),
    IndirectIndexed(Expr),
    Indexed(Expr, Index),
    Direct(Expr),
}

/// Resolve parsed operand syntax to a spec addressing-mode label, consuming the
/// operand expression. Mode is chosen here (at parse time) so instruction size
/// never depends on a symbol value — the zero-page-vs-absolute choice keys off
/// whether the operand is a literal, never off a (possibly forward) label.
fn resolve_mode(
    insn: &isa::Instruction,
    operand: OperandSyntax,
    line: usize,
) -> Result<(&'static str, Option<Expr>), AsmError> {
    let resolved = match operand {
        OperandSyntax::None => {
            if insn.form("implied").is_some() {
                ("implied", None)
            } else if insn.form("accumulator").is_some() {
                ("accumulator", None)
            } else {
                return Err(AsmError::new(
                    line,
                    format!("`{}` requires an operand", insn.mnemonic),
                ));
            }
        }
        OperandSyntax::Accumulator => ("accumulator", None),
        OperandSyntax::Immediate(e) => ("immediate", Some(e)),
        OperandSyntax::Indirect(e) => ("indirect", Some(e)),
        OperandSyntax::IndexedIndirect(e) => ("(indirect,x)", Some(e)),
        OperandSyntax::IndirectIndexed(e) => ("(indirect),y", Some(e)),
        OperandSyntax::Indexed(e, Index::X) => {
            let mode = pick_zp_abs(insn, &e, "zeropage,x", "absolute,x");
            (mode, Some(e))
        }
        OperandSyntax::Indexed(e, Index::Y) => {
            let mode = pick_zp_abs(insn, &e, "zeropage,y", "absolute,y");
            (mode, Some(e))
        }
        OperandSyntax::Direct(e) => {
            // A bare operand on a branch instruction is a relative target.
            if insn.form("relative").is_some() {
                ("relative", Some(e))
            } else {
                let mode = pick_zp_abs(insn, &e, "zeropage", "absolute");
                (mode, Some(e))
            }
        }
    };
    Ok(resolved)
}

/// Prefer the zero-page form when the operand is a literal that fits in a byte
/// and the instruction has that form; otherwise use absolute. Keying off the
/// literal (not a resolved value) keeps instruction sizes stable across passes.
fn pick_zp_abs(
    insn: &isa::Instruction,
    e: &Expr,
    zp: &'static str,
    abs: &'static str,
) -> &'static str {
    let fits_zero_page = matches!(e, Expr::Num(n) if *n >= 0 && *n <= 0xFF);
    if fits_zero_page && insn.form(zp).is_some() {
        zp
    } else {
        abs
    }
}

// ---------------------------------------------------------------------------
// Tokenising
// ---------------------------------------------------------------------------

fn split_label(line: &str, number: usize) -> Result<(Option<String>, &str), AsmError> {
    if let Some(colon) = line.find(':') {
        let candidate = line[..colon].trim();
        if is_ident(candidate) {
            return Ok((Some(candidate.to_string()), line[colon + 1..].trim()));
        }
        return Err(AsmError::new(number, format!("invalid label `{candidate}`")));
    }
    Ok((None, line))
}

fn parse_directive(directive: &str, line: usize) -> Result<Operation, AsmError> {
    let (name, rest) = split_first_word(directive);
    match name.to_ascii_lowercase().as_str() {
        "org" => Ok(Operation::Org(parse_value(rest, line)?)),
        "byte" | "db" => Ok(Operation::Bytes(parse_list(rest, line)?)),
        "word" | "dw" => Ok(Operation::Words(parse_list(rest, line)?)),
        other => Err(AsmError::new(line, format!("unknown directive `.{other}`"))),
    }
}

fn parse_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    rest.split(',').map(|p| parse_value(p, line)).collect()
}

fn parse_operand(raw: &str, line: usize) -> Result<OperandSyntax, AsmError> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(OperandSyntax::None);
    }
    if t.eq_ignore_ascii_case("a") {
        return Ok(OperandSyntax::Accumulator);
    }
    if let Some(rest) = t.strip_prefix('#') {
        return Ok(OperandSyntax::Immediate(parse_value(rest, line)?));
    }
    if t.starts_with('(') {
        // TODO: this requires tight syntax, e.g. `($20,X)` not `( $20 , X )`.
        let upper = t.to_ascii_uppercase();
        if upper.ends_with(",X)") {
            return Ok(OperandSyntax::IndexedIndirect(parse_value(
                &t[1..t.len() - 3],
                line,
            )?));
        }
        if upper.ends_with("),Y") {
            return Ok(OperandSyntax::IndirectIndexed(parse_value(
                &t[1..t.len() - 3],
                line,
            )?));
        }
        if t.ends_with(')') {
            return Ok(OperandSyntax::Indirect(parse_value(&t[1..t.len() - 1], line)?));
        }
        return Err(AsmError::new(
            line,
            format!("malformed indirect operand `{raw}`"),
        ));
    }
    if let Some(comma) = t.rfind(',') {
        let index = match t[comma + 1..].trim() {
            i if i.eq_ignore_ascii_case("x") => Index::X,
            i if i.eq_ignore_ascii_case("y") => Index::Y,
            _ => {
                return Err(AsmError::new(line, format!("expected `,X` or `,Y` in `{raw}`")));
            }
        };
        return Ok(OperandSyntax::Indexed(parse_value(&t[..comma], line)?, index));
    }
    Ok(OperandSyntax::Direct(parse_value(t, line)?))
}

fn parse_value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix('<') {
        return Ok(Expr::Lo(Box::new(parse_value(rest, line)?)));
    }
    if let Some(rest) = t.strip_prefix('>') {
        return Ok(Expr::Hi(Box::new(parse_value(rest, line)?)));
    }
    // TODO: arithmetic (`label+1`, `start-end`) is not yet supported.
    let first = t
        .chars()
        .next()
        .ok_or_else(|| AsmError::new(line, "expected a value"))?;
    if first == '$' || first == '%' || first == '\'' || first.is_ascii_digit() {
        Ok(Expr::Num(parse_number(t, line)?))
    } else if is_ident(t) {
        Ok(Expr::Sym(t.to_string()))
    } else {
        Err(AsmError::new(line, format!("cannot parse value `{raw}`")))
    }
}

fn parse_number(tok: &str, line: usize) -> Result<i64, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("invalid number `{tok}`"));
    if let Some(hex) = t.strip_prefix('$') {
        i64::from_str_radix(hex, 16).map_err(|_| bad())
    } else if let Some(bin) = t.strip_prefix('%') {
        i64::from_str_radix(bin, 2).map_err(|_| bad())
    } else if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
        t.chars().nth(1).map(|c| c as i64).ok_or_else(bad)
    } else {
        t.parse::<i64>().map_err(|_| bad())
    }
}

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], s[idx..].trim()),
        None => (s, ""),
    }
}

fn is_ident(s: &str) -> bool {
    let s = s.trim();
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
