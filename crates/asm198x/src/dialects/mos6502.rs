//! Shared 6502 core: operand-to-mode resolution and the expression parser used
//! by every 6502 dialect (ACME, ca65, …).
//!
//! The 6502 addressing-mode syntax is the same across assemblers — `lda #$00`,
//! `sta $0400,x`, `($20),y` are written identically — so operand classification,
//! the zero-page-vs-absolute choice, and the arithmetic expression grammar live
//! here. Each dialect keeps only what genuinely differs: its directives, label
//! and segment rules, comment and number formats, and where the `<`/`>`
//! byte-extraction operators sit in precedence ([`BytePrec`]). This mirrors the
//! `Z80Syntax` split for the Z80 dialects.

use std::collections::BTreeMap;

use crate::engine::{AsmError, BinOp, Expr};

// ---------------------------------------------------------------------------
// Operand syntax (parsed) and mode resolution (dialect -> spec)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) enum Index {
    X,
    Y,
}

/// Operand syntax as parsed, before it is resolved to an addressing mode.
pub(crate) enum OperandSyntax {
    None,
    Accumulator,
    Immediate(Expr),
    Indirect(Expr),
    IndexedIndirect(Expr),
    IndirectIndexed(Expr),
    Indexed(Expr, Index),
    Direct(Expr),
}

/// Resolve parsed operand syntax to a spec mode label, choosing zero-page vs
/// absolute from a parse-time-constant operand (never a forward symbol) so the
/// form size is stable between passes.
pub(crate) fn resolve_mode(
    insn: &isa::Instruction,
    operand: OperandSyntax,
    env: &BTreeMap<String, i64>,
    force_abs: bool,
    line: usize,
) -> Result<(&'static str, Option<Expr>), AsmError> {
    let resolved = match operand {
        OperandSyntax::None => {
            if insn.form("implied").is_some() {
                ("implied", None)
            } else if insn.form("accumulator").is_some() {
                ("accumulator", None)
            } else {
                return Err(AsmError::new(line, format!("`{}` requires an operand", insn.mnemonic)));
            }
        }
        OperandSyntax::Accumulator => ("accumulator", None),
        OperandSyntax::Immediate(e) => ("immediate", Some(e)),
        OperandSyntax::Indirect(e) => ("indirect", Some(e)),
        OperandSyntax::IndexedIndirect(e) => ("(indirect,x)", Some(e)),
        OperandSyntax::IndirectIndexed(e) => ("(indirect),y", Some(e)),
        OperandSyntax::Indexed(e, Index::X) => (pick_zp_abs(insn, &e, env, force_abs, "zeropage,x", "absolute,x"), Some(e)),
        OperandSyntax::Indexed(e, Index::Y) => (pick_zp_abs(insn, &e, env, force_abs, "zeropage,y", "absolute,y"), Some(e)),
        OperandSyntax::Direct(e) => {
            if insn.form("relative").is_some() {
                ("relative", Some(e))
            } else {
                (pick_zp_abs(insn, &e, env, force_abs, "zeropage", "absolute"), Some(e))
            }
        }
    };
    Ok(resolved)
}

/// Choose zero-page when the operand folds to a constant that fits in a byte (a
/// literal, or a symbol already bound to a low value) and the instruction has
/// that form; otherwise absolute. A forward or address symbol stays absolute,
/// keeping form sizes stable across passes. `force_abs` skips the zero-page
/// pick — ACME treats a `≥3`-digit hex literal (`$0010`) as 16-bit even though
/// its value is low.
fn pick_zp_abs(
    insn: &isa::Instruction,
    e: &Expr,
    env: &BTreeMap<String, i64>,
    force_abs: bool,
    zp: &'static str,
    abs: &'static str,
) -> &'static str {
    let fits_zero_page = !force_abs && fold_const(e, env, 0).is_ok_and(|v| (0..=0xFF).contains(&v));
    if fits_zero_page && insn.form(zp).is_some() {
        zp
    } else {
        abs
    }
}

/// Fold an expression to a constant, resolving symbols against the parse-time
/// `env`. Errors on the location counter or an unknown symbol.
pub(crate) fn fold_const(e: &Expr, env: &BTreeMap<String, i64>, line: usize) -> Result<i64, AsmError> {
    let overflow = || AsmError::new(line, "arithmetic overflow in expression");
    Ok(match e {
        Expr::Num(n) => *n,
        Expr::Sym(s) => *env
            .get(s)
            .ok_or_else(|| AsmError::new(line, format!("`{s}` is not a parse-time constant")))?,
        Expr::Pc => return Err(AsmError::new(line, "`*` cannot be used here")),
        Expr::Lo(b) => fold_const(b, env, line)? & 0xFF,
        Expr::Hi(b) => (fold_const(b, env, line)? >> 8) & 0xFF,
        Expr::Neg(b) => fold_const(b, env, line)?.checked_neg().ok_or_else(overflow)?,
        Expr::Bin(op, l, r) => {
            let a = fold_const(l, env, line)?;
            let b = fold_const(r, env, line)?;
            match op {
                BinOp::Add => a.checked_add(b).ok_or_else(overflow)?,
                BinOp::Sub => a.checked_sub(b).ok_or_else(overflow)?,
                BinOp::Mul => a.checked_mul(b).ok_or_else(overflow)?,
                BinOp::Div if b != 0 => a / b,
                BinOp::Div => return Err(AsmError::new(line, "division by zero in expression")),
            }
        }
    })
}

/// Parse operand structure (immediate, indirect, indexed, direct), delegating
/// each sub-expression to the dialect's `value` parser. The 6502 operand shapes
/// are the same across dialects; only the expression contents differ.
pub(crate) fn parse_operand(
    raw: &str,
    line: usize,
    value: &dyn Fn(&str, usize) -> Result<Expr, AsmError>,
) -> Result<OperandSyntax, AsmError> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(OperandSyntax::None);
    }
    if t.eq_ignore_ascii_case("a") {
        return Ok(OperandSyntax::Accumulator);
    }
    if let Some(rest) = t.strip_prefix('#') {
        return Ok(OperandSyntax::Immediate(value(rest, line)?));
    }
    if t.starts_with('(') {
        // The three indirect forms, tolerant of spaces around `,` and `)`:
        //   `(expr)`      indirect            `(expr,x)`  indexed-indirect
        //   `(expr),y`    indirect-indexed
        let malformed = || AsmError::new(line, format!("malformed indirect operand `{raw}`"));
        if let Some(inner) = t.strip_suffix(')') {
            let inner = &inner[1..];
            if let Some(c) = top_level_rfind(inner, ',')
                && inner[c + 1..].trim().eq_ignore_ascii_case("x")
            {
                return Ok(OperandSyntax::IndexedIndirect(value(&inner[..c], line)?));
            }
            return Ok(OperandSyntax::Indirect(value(inner, line)?));
        }
        let close = t.rfind(')').ok_or_else(malformed)?;
        let after = t[close + 1..].trim();
        let idx = after.strip_prefix(',').map(str::trim);
        if idx.is_some_and(|i| i.eq_ignore_ascii_case("y")) {
            return Ok(OperandSyntax::IndirectIndexed(value(&t[1..close], line)?));
        }
        return Err(malformed());
    }
    if let Some(comma) = top_level_rfind(t, ',') {
        let index = match t[comma + 1..].trim() {
            i if i.eq_ignore_ascii_case("x") => Index::X,
            i if i.eq_ignore_ascii_case("y") => Index::Y,
            _ => return Err(AsmError::new(line, format!("expected `,X` or `,Y` in `{raw}`"))),
        };
        return Ok(OperandSyntax::Indexed(value(&t[..comma], line)?, index));
    }
    Ok(OperandSyntax::Direct(value(t, line)?))
}

// ---------------------------------------------------------------------------
// Expression parser: `+ - * /` with C precedence, parentheses, `*` as the
// program counter, and `<`/`>` low/high-byte prefixes whose precedence the
// dialect selects via `BytePrec`.
// ---------------------------------------------------------------------------

/// Where the `<`/`>` byte-extraction operators sit in precedence.
///
/// - `Loose`: they apply to the whole expression to their right, so
///   `<a+1` is `<(a+1)` (ACME).
/// - `Tight`: they are unary operators binding to the next term, so
///   `<a+1` is `(<a)+1` (ca65).
///
/// Both were verified against the respective assembler binaries.
#[derive(Clone, Copy)]
pub(crate) enum BytePrec {
    Loose,
    Tight,
}

/// Parse a value expression. `parse_number` lexes the dialect's numeric literal
/// forms; `prec` places the `<`/`>` operators.
pub(crate) fn parse_expr(
    raw: &str,
    line: usize,
    parse_number: fn(&str, usize) -> Result<i64, AsmError>,
    prec: BytePrec,
) -> Result<Expr, AsmError> {
    let tokens = tokenize(raw, line, parse_number)?;
    if tokens.is_empty() {
        return Err(AsmError::new(line, "expected a value"));
    }
    let mut parser = ExprParser { tokens, pos: 0, line, prec };
    let expr = parser.expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(AsmError::new(line, format!("unexpected trailing tokens in `{}`", raw.trim())));
    }
    Ok(expr)
}

#[derive(Clone)]
enum Tok {
    Num(i64),
    Sym(String),
    Star,
    Plus,
    Minus,
    Slash,
    Lo,
    Hi,
    LParen,
    RParen,
}

fn tokenize(
    raw: &str,
    line: usize,
    parse_number: fn(&str, usize) -> Result<i64, AsmError>,
) -> Result<Vec<Tok>, AsmError> {
    let chars: Vec<char> = raw.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ws if ws.is_whitespace() => i += 1,
            '+' => { tokens.push(Tok::Plus); i += 1; }
            '-' => { tokens.push(Tok::Minus); i += 1; }
            '*' => { tokens.push(Tok::Star); i += 1; }
            '/' => { tokens.push(Tok::Slash); i += 1; }
            '<' => { tokens.push(Tok::Lo); i += 1; }
            '>' => { tokens.push(Tok::Hi); i += 1; }
            '(' => { tokens.push(Tok::LParen); i += 1; }
            ')' => { tokens.push(Tok::RParen); i += 1; }
            '\'' => {
                if i + 2 < chars.len() && chars[i + 2] == '\'' {
                    let s: String = chars[i..=i + 2].iter().collect();
                    tokens.push(Tok::Num(parse_number(&s, line)?));
                    i += 3;
                } else {
                    return Err(AsmError::new(line, "malformed character literal"));
                }
            }
            '$' | '%' => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                tokens.push(Tok::Num(parse_number(&chars[start..i].iter().collect::<String>(), line)?));
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                tokens.push(Tok::Num(parse_number(&chars[start..i].iter().collect::<String>(), line)?));
            }
            l if l.is_ascii_alphabetic() || l == '_' || l == '.' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                tokens.push(Tok::Sym(chars[start..i].iter().collect()));
            }
            other => return Err(AsmError::new(line, format!("unexpected character `{other}` in expression"))),
        }
    }
    Ok(tokens)
}

struct ExprParser {
    tokens: Vec<Tok>,
    pos: usize,
    line: usize,
    prec: BytePrec,
}

impl ExprParser {
    fn expr(&mut self) -> Result<Expr, AsmError> {
        // Loose `<`/`>` wrap the whole expression to their right.
        if matches!(self.prec, BytePrec::Loose) {
            match self.tokens.get(self.pos) {
                Some(Tok::Lo) => {
                    self.pos += 1;
                    return Ok(Expr::Lo(Box::new(self.expr()?)));
                }
                Some(Tok::Hi) => {
                    self.pos += 1;
                    return Ok(Expr::Hi(Box::new(self.expr()?)));
                }
                _ => {}
            }
        }
        self.add_sub()
    }

    fn add_sub(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.mul_div()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.mul_div()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn mul_div(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let right = self.unary()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, AsmError> {
        if matches!(self.tokens.get(self.pos), Some(Tok::Minus)) {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        // Tight `<`/`>` are unary operators binding to the next term.
        if matches!(self.prec, BytePrec::Tight) {
            match self.tokens.get(self.pos) {
                Some(Tok::Lo) => {
                    self.pos += 1;
                    return Ok(Expr::Lo(Box::new(self.unary()?)));
                }
                Some(Tok::Hi) => {
                    self.pos += 1;
                    return Ok(Expr::Hi(Box::new(self.unary()?)));
                }
                _ => {}
            }
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, AsmError> {
        let tok = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or_else(|| AsmError::new(self.line, "expected a value"))?;
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Sym(s) => Ok(Expr::Sym(s)),
            Tok::Star => Ok(Expr::Pc),
            Tok::LParen => {
                let inner = self.expr()?;
                if matches!(self.tokens.get(self.pos), Some(Tok::RParen)) {
                    self.pos += 1;
                    Ok(inner)
                } else {
                    Err(AsmError::new(self.line, "expected `)`"))
                }
            }
            _ => Err(AsmError::new(self.line, "expected a value")),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared lexical helpers
// ---------------------------------------------------------------------------

pub(crate) fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], s[idx..].trim()),
        None => (s, ""),
    }
}

/// Split on `sep` at the top level (outside parentheses), trimming each piece.
pub(crate) fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(s[start..i].trim());
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

/// The byte index of the last top-level (non-parenthesised) `sep`, if any.
pub(crate) fn top_level_rfind(s: &str, sep: char) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut found = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => found = Some(i),
            _ => {}
        }
    }
    found
}

/// The 6502 numeric literal forms shared by acme and ca65: `$hex`, `%binary`,
/// `'c'` char, decimal.
pub(crate) fn parse_number(tok: &str, line: usize) -> Result<i64, AsmError> {
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

/// The byte index of a lone `=` used as a symbol assignment, or `None`. Skips
/// the comparison operators `==`/`!=`/`<=`/`>=`. (A leading `*=` is handled by
/// each dialect before this is reached.)
pub(crate) fn assignment_split(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' {
            let prev = i.checked_sub(1).map(|p| bytes[p]);
            let next = bytes.get(i + 1).copied();
            if !matches!(prev, Some(b'!' | b'<' | b'>' | b'=')) && next != Some(b'=') {
                return Some(i);
            }
        }
    }
    None
}

/// Split a data list on commas that are not inside a `"..."` string.
pub(crate) fn split_data_items(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_string = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            ',' if !in_string => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

/// The contents of a `"..."` string literal, or `None` if `piece` is not one.
pub(crate) fn string_literal(piece: &str) -> Option<&str> {
    let p = piece.trim();
    (p.len() >= 2 && p.starts_with('"') && p.ends_with('"')).then(|| &p[1..p.len() - 1])
}

/// An identifier: letters, digits, `_`, and `.` (so local-style labels like
/// `.loop` read as ordinary names), not starting with a digit.
pub(crate) fn is_ident(s: &str) -> bool {
    let s = s.trim();
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '.' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(tok: &str, line: usize) -> Result<i64, AsmError> {
        tok.strip_prefix('$')
            .map_or_else(|| tok.parse::<i64>(), |h| i64::from_str_radix(h, 16))
            .map_err(|_| AsmError::new(line, "bad number"))
    }

    fn eval(raw: &str, prec: BytePrec) -> i64 {
        let env = BTreeMap::new();
        fold_const(&parse_expr(raw, 1, num, prec).expect("parse"), &env, 1).expect("fold")
    }

    #[test]
    fn byte_operator_precedence_differs_by_dialect() {
        // Loose (ACME): `>` applies to the whole expression -> high($1235) = $12.
        assert_eq!(eval(">$1234+1", BytePrec::Loose), 0x12);
        // Tight (ca65): `>` binds to the term -> high($1234) + 1 = $13.
        assert_eq!(eval(">$1234+1", BytePrec::Tight), 0x13);
        // Arithmetic precedence is the same regardless.
        assert_eq!(eval("1+2*3", BytePrec::Loose), 7);
        assert_eq!(eval("1+2*3", BytePrec::Tight), 7);
    }
}
