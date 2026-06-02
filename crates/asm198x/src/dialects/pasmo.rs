//! The pasmo-family Z80 dialect front-ends.
//!
//! Two dialects share this one parser: [`Pasmo`] (vanilla pasmo, by Julián
//! Albo) and [`PasmoNext`] (the ZX Spectrum Next fork, invoked as `pasmonext`,
//! which the Code198x curriculum uses). PasmoNext is a syntactic *superset* of
//! pasmo, differing only in the Spectrum Next's Z80N extended opcodes. For
//! standard Z80 the two are byte-identical, so they run the same code here; the
//! only future divergence is that PasmoNext will also accept Z80N mnemonics (a
//! deferred ISA-spec extension), which base Pasmo never will. See
//! `decisions/syntax-stance.md`.
//!
//! This front-end targets [`isa::z80`] and covers the standard Z80 instruction
//! set, plus the directives real curriculum source uses: `org`, `equ`,
//! `defb`/`db`, `defw`/`dw`, `end`. Numbers are `$hex`, `%binary`, decimal, and
//! `'c'` char literals; `defb` also takes `"strings"`. Operand values are
//! arithmetic expressions (`+ - * /`, parens, C precedence). Labels sit in
//! column 0 (with or without a trailing `:`); instructions are indented.
//! Local-style labels beginning with `.` are accepted as ordinary identifiers.
//!
//! ## Resolving operands to spec mode labels
//!
//! The Z80 packs registers and conditions into the opcode, so a form's mode is
//! its operand signature (see [`isa::z80`]). This front-end classifies each
//! parsed operand as either a *fixed* token (a register, condition, or
//! register-indirect like `(HL)`) or a *value* (an immediate or a `(nn)`
//! address, carrying an [`Expr`]). It then builds candidate signature strings
//! and probes the instruction for a matching form — so `ld a,c` resolves to the
//! register form `A,C` while `jr c,loop` resolves to the condition form `C,e`,
//! with no need to pre-judge whether `C` is a register or a flag.
//!
//! Width is settled by which form exists, not by the operand's value: a bare
//! immediate offers `n` then `nn`, a parenthesised value offers `(n)` then
//! `(nn)`, and only one of each pair is ever a real form for a given mnemonic.
//!
//! TODO: arithmetic in expressions, `$` as the program counter, string `defb`,
//! `defs`; and the IM/BIT/SET/RES operand forms that arrive with the CB/ED
//! prefix groups.

use std::collections::BTreeMap;

use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Statement};

/// The pasmo-family Z80 dialect. Syntax is identical regardless of target; the
/// `z80n` flag selects the **target CPU's instruction set** — `false` for a
/// plain Z80 (vanilla pasmo), `true` for the ZX Spectrum Next's Z80N
/// (pasmonext). Z80N availability is a target property, not a syntax one, so it
/// lives here as a flag rather than as a separate dialect (see
/// `decisions/syntax-stance.md`).
pub(crate) struct Pasmo {
    pub(crate) z80n: bool,
}

impl Dialect for Pasmo {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z80::SET
    }
    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        self.z80n.then_some(&isa::z80::NEXT)
    }
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        parse_z80(self.instruction_set(), self.extension_set(), source)
    }
}

/// Parse pasmo-family Z80 source into the engine's statement stream. `ext` is
/// the optional Z80N set (present for PasmoNext, absent for base pasmo).
fn parse_z80(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    source: &str,
) -> Result<Vec<Statement>, AsmError> {
    let mut out = Vec::new();
    // Constants defined with `equ`, recorded as parsed. Opcode-embedded
    // operands (BIT n, IM n, RST n) must be known at parse time to pick the
    // form, so they resolve against this — not the engine's pass-2 symbols.
    let mut consts: BTreeMap<String, i64> = BTreeMap::new();
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let code = strip_comment(raw);
        if code.trim().is_empty() {
            continue;
        }
        let (label, rest) = split_label(set, ext, code, line)?;
        let op = parse_op(set, ext, rest, line, &consts)?;
        if let (Some(name), Some(Operation::Equ(e))) = (&label, &op)
            && let Some(v) = eval_const(e, &consts)
        {
            consts.insert(name.clone(), v);
        }
        if label.is_none() && op.is_none() {
            continue;
        }
        out.push(Statement { line, label, op });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Line structure
// ---------------------------------------------------------------------------

/// Strip a `;` line comment. TODO: a `;` inside a string/char literal is cut
/// here too — acceptable until `defb "..."`/char operands are exercised.
fn strip_comment(line: &str) -> &str {
    line.find(';').map_or(line, |idx| &line[..idx])
}

/// Split a (comment-stripped) line into its optional label and the remainder.
///
/// pasmo layout: a `name:` token is always a label; otherwise a label sits in
/// column 0 and instructions are indented. To stay robust when an instruction
/// is written at column 0, a column-0 first word that names a known mnemonic or
/// directive is treated as the operation, not a label.
fn split_label<'a>(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    code: &'a str,
    line: usize,
) -> Result<(Option<String>, &'a str), AsmError> {
    let trimmed = code.trim();
    // A `name:` token (no whitespace before the colon) is always a label.
    if let Some(colon) = trimmed.find(':') {
        let before = &trimmed[..colon];
        if !before.contains(char::is_whitespace) {
            if !is_ident(before.trim()) {
                return Err(AsmError::new(line, format!("invalid label `{}`", before.trim())));
            }
            return Ok((Some(before.trim().to_string()), trimmed[colon + 1..].trim()));
        }
    }
    // Indented lines carry no label.
    if code.starts_with([' ', '\t']) {
        return Ok((None, trimmed));
    }
    // Column 0, no colon: a known op/directive is the operation; anything else
    // is a label.
    let (word, remainder) = split_first_word(trimmed);
    if is_known_op(set, ext, word) {
        return Ok((None, trimmed));
    }
    if !is_ident(word) {
        return Err(AsmError::new(line, format!("invalid label `{word}`")));
    }
    Ok((Some(word.to_string()), remainder))
}

/// Whether `word` names an instruction (in either set) or a pasmo directive.
fn is_known_op(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    word: &str,
) -> bool {
    has_mnemonic(set, ext, &word.to_ascii_uppercase())
        || matches!(
            word.to_ascii_lowercase().as_str(),
            "org" | "equ" | "defb" | "db" | "defm" | "defw" | "dw" | "end"
        )
}

fn parse_op(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    rest: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<Option<Operation>, AsmError> {
    if rest.is_empty() {
        return Ok(None);
    }
    let (word, args) = split_first_word(rest);
    match word.to_ascii_lowercase().as_str() {
        "org" => Ok(Some(Operation::Org(parse_value(args, line)?))),
        "equ" => Ok(Some(Operation::Equ(parse_value(args, line)?))),
        "defb" | "db" | "defm" => Ok(Some(Operation::Bytes(parse_list(args, line)?))),
        "defw" | "dw" => Ok(Some(Operation::Words(parse_list(args, line)?))),
        "end" => Ok(None), // entry-point marker; a flat binary ignores it
        _ => {
            let mnemonic = word.to_ascii_uppercase();
            if !has_mnemonic(set, ext, &mnemonic) {
                return Err(AsmError::new(line, format!("unknown instruction `{mnemonic}`")));
            }
            let (mode, operands) = resolve(set, ext, &mnemonic, args, line, consts)?;
            Ok(Some(Operation::Instruction {
                mnemonic,
                mode,
                operands,
            }))
        }
    }
}

/// Find a form across the primary set and an optional extension (Z80N).
fn find_form(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
    mode: &str,
) -> Option<&'static isa::Form> {
    set.find_form(mnemonic, mode)
        .or_else(|| ext.and_then(|e| e.find_form(mnemonic, mode)))
}

/// Whether a mnemonic exists in the primary set or the extension.
fn has_mnemonic(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
) -> bool {
    set.has_mnemonic(mnemonic) || ext.is_some_and(|e| e.has_mnemonic(mnemonic))
}

// ---------------------------------------------------------------------------
// Operand resolution (dialect syntax -> spec mode label)
// ---------------------------------------------------------------------------

/// One classified operand.
enum Operand {
    /// A register, condition, or register-indirect — a fixed signature token.
    Fixed(String),
    /// A value: an immediate or a `(nn)` address. `paren` marks the memory form.
    Value { expr: Expr, paren: bool },
    /// An indexed operand `(IX+d)` / `(IY+d)`. `disp` is `None` for a bare
    /// `(IX)` — which may be either register-indirect (`JP (IX)`) or `(IX+0)`,
    /// resolved by which form exists.
    Indexed { reg: &'static str, disp: Option<Expr> },
}

/// One way an operand can be written into a mode label: the token it
/// contributes, and the value(s) it emits as bytes (empty if the operand is
/// consumed into the opcode, e.g. a BIT bit-number).
type Alternative = (String, Vec<Expr>);

/// Resolve a mnemonic's operands to a spec mode label and the values to emit
/// (one per the form's operand byte-slot, in order). Each operand offers one or
/// more [`Alternative`]s; the first combination that names a real form wins, and
/// that combination's emitted values are returned.
fn resolve(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let pieces = split_operands(args);
    let mut per_operand: Vec<Vec<Alternative>> = Vec::new();
    for (idx, piece) in pieces.iter().enumerate() {
        per_operand.push(alternatives(mnemonic, idx, piece, consts, line)?);
    }

    for combo in product(&per_operand) {
        let label = combo
            .iter()
            .map(|(token, _)| token.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if let Some(f) = find_form(set, ext, mnemonic, &label) {
            let emitted = combo.into_iter().flat_map(|(_, values)| values).collect();
            return Ok((f.mode, emitted));
        }
    }
    Err(AsmError::new(
        line,
        format!("`{mnemonic}` has no form for operands `{}`", args.trim()),
    ))
}

/// The mode-label/emitted-values alternatives for one operand.
fn alternatives(
    mnemonic: &str,
    idx: usize,
    piece: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Vec<Alternative>, AsmError> {
    Ok(match classify(piece, line)? {
        Operand::Fixed(token) => vec![(token, vec![])],
        Operand::Indexed { reg, disp } => match disp {
            Some(d) => vec![(format!("({reg}+d)"), vec![d])],
            // Bare `(IX)`: register-indirect, or `(IX+0)` if only that exists.
            None => vec![
                (format!("({reg})"), vec![]),
                (format!("({reg}+d)"), vec![Expr::Num(0)]),
            ],
        },
        Operand::Value { expr, paren } => {
            if let Some(token) = embedded_token(mnemonic, paren, idx, &expr, consts, line)? {
                vec![(token, vec![])] // consumed into the opcode
            } else {
                emitted_tokens(mnemonic, paren)
                    .into_iter()
                    .map(|token| (token, vec![expr.clone()]))
                    .collect()
            }
        }
    })
}

fn classify(piece: &str, line: usize) -> Result<Operand, AsmError> {
    let t = piece.trim();
    if let Some(inner) = strip_parens(t) {
        let inner = inner.trim();
        if let Some((reg, rest)) = index_register(inner) {
            let disp = if rest.is_empty() {
                None
            } else if let Some(after_plus) = rest.strip_prefix('+') {
                Some(parse_value(after_plus, line)?)
            } else {
                // Starts with '-': parse the whole thing (unary minus).
                Some(parse_value(rest, line)?)
            };
            return Ok(Operand::Indexed { reg, disp });
        }
        let up = inner.to_ascii_uppercase();
        if is_indirect_reg(&up) {
            return Ok(Operand::Fixed(format!("({up})")));
        }
        return Ok(Operand::Value {
            expr: parse_value(inner, line)?,
            paren: true,
        });
    }
    let up = t.to_ascii_uppercase();
    if is_reg_or_cond(&up) {
        return Ok(Operand::Fixed(up));
    }
    Ok(Operand::Value {
        expr: parse_value(t, line)?,
        paren: false,
    })
}

/// If `inner` names an index register with an optional displacement —
/// `IX`, `IY`, `IX+d`, `IY-d` — return the canonical register and the rest
/// (the displacement text, possibly empty). Guards against symbols that merely
/// start with "ix"/"iy" by requiring the next char to be `+`, `-`, or nothing.
fn index_register(inner: &str) -> Option<(&'static str, &str)> {
    for reg in ["IX", "IY"] {
        if inner.len() >= 2 && inner[..2].eq_ignore_ascii_case(reg) {
            let rest = inner[2..].trim_start();
            if rest.is_empty() || rest.starts_with('+') || rest.starts_with('-') {
                return Some((reg, rest));
            }
        }
    }
    None
}

/// For an operand whose value is encoded *in the opcode* (RST target, IM mode,
/// BIT/RES/SET bit number), return its mode-label token. Returns `None` for
/// operands that become bytes on the wire.
fn embedded_token(
    mnemonic: &str,
    paren: bool,
    index: usize,
    expr: &Expr,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<String>, AsmError> {
    if paren {
        return Ok(None);
    }
    let token = match mnemonic {
        "RST" => format!("{:02X}", literal(expr, consts, line)?),
        "IM" => format!("{}", literal(expr, consts, line)?),
        "BIT" | "RES" | "SET" if index == 0 => format!("{}", literal(expr, consts, line)?),
        _ => return Ok(None),
    };
    Ok(Some(token))
}

/// Candidate signature tokens for a value operand that becomes bytes on the
/// wire. Width is left ambiguous (both offered) except for relative branches.
fn emitted_tokens(mnemonic: &str, paren: bool) -> Vec<String> {
    if paren {
        return vec!["(n)".to_string(), "(nn)".to_string()];
    }
    match mnemonic {
        "JR" | "DJNZ" => vec!["e".to_string()],
        _ => vec!["n".to_string(), "nn".to_string()],
    }
}

/// Resolve an operand that is encoded *in the opcode* (RST/IM/BIT/RES/SET), so
/// its value must be known at parse time to pick the form. It may be a number,
/// an arithmetic expression, or a constant defined earlier with `equ` — but not
/// a label (an address isn't known until the engine's later passes).
fn literal(expr: &Expr, consts: &BTreeMap<String, i64>, line: usize) -> Result<i64, AsmError> {
    eval_const(expr, consts).ok_or_else(|| {
        AsmError::new(
            line,
            "operand must be a constant here (a number, an expression of \
             constants, or a value defined with `equ` above)",
        )
    })
}

/// Fold an expression to a constant, resolving symbols only against `equ`
/// constants. Returns `None` if it references an unknown symbol or overflows.
fn eval_const(expr: &Expr, consts: &BTreeMap<String, i64>) -> Option<i64> {
    match expr {
        Expr::Num(n) => Some(*n),
        Expr::Sym(s) => consts.get(s).copied(),
        Expr::Lo(e) => Some(eval_const(e, consts)? & 0xFF),
        Expr::Hi(e) => Some((eval_const(e, consts)? >> 8) & 0xFF),
        Expr::Neg(e) => eval_const(e, consts)?.checked_neg(),
        Expr::Bin(op, l, r) => {
            let a = eval_const(l, consts)?;
            let b = eval_const(r, consts)?;
            match op {
                BinOp::Add => a.checked_add(b),
                BinOp::Sub => a.checked_sub(b),
                BinOp::Mul => a.checked_mul(b),
                BinOp::Div => {
                    if b == 0 {
                        None
                    } else {
                        a.checked_div(b)
                    }
                }
            }
        }
    }
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

// ---------------------------------------------------------------------------
// Register / condition vocabulary
// ---------------------------------------------------------------------------

/// Registers that have a `(reg)` indirect form on the base page.
fn is_indirect_reg(up: &str) -> bool {
    matches!(up, "HL" | "BC" | "DE" | "SP" | "C")
}

/// Tokens that name a register or condition code (used verbatim in a mode
/// label). `C` is both a register and the carry condition; the spec's form
/// lookup disambiguates by mnemonic, so it needs no special handling here.
fn is_reg_or_cond(up: &str) -> bool {
    matches!(
        up,
        "A" | "B" | "C" | "D" | "E" | "H" | "L" | "I" | "R" | "AF" | "AF'"
            | "BC" | "DE" | "HL" | "SP" | "IX" | "IY"
            | "NZ" | "Z" | "NC" | "PO" | "PE" | "P" | "M"
    )
}

// ---------------------------------------------------------------------------
// Tokenising helpers
// ---------------------------------------------------------------------------

/// Split operand text on top-level commas (commas inside parentheses are kept).
fn split_operands(args: &str) -> Vec<&str> {
    let args = args.trim();
    if args.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(args[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(args[start..].trim());
    out
}

/// If `t` is wrapped in a single outer pair of parentheses, return the inside.
fn strip_parens(t: &str) -> Option<&str> {
    let t = t.trim();
    let inner = t.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner)
}

/// Parse a `defb`/`defw` value list. Items are comma-separated; a `"..."`
/// string expands to one byte per character (its char code), so `defb
/// "AB", 0` becomes three values. TODO: escape sequences in strings.
fn parse_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.chars().map(|c| Expr::Num(c as i64)));
        } else {
            out.push(parse_value(piece, line)?);
        }
    }
    Ok(out)
}

/// Split a data list on commas that are not inside a `"..."` string.
fn split_data_items(s: &str) -> Vec<&str> {
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

/// If `piece` is a `"..."` string literal, return its contents.
fn string_literal(piece: &str) -> Option<&str> {
    let p = piece.trim();
    if p.len() >= 2 && p.starts_with('"') && p.ends_with('"') {
        Some(&p[1..p.len() - 1])
    } else {
        None
    }
}

/// Parse an operand value: an arithmetic expression over numbers, symbols, and
/// `+`/`-`/`*`/`/` with C-style precedence (`*`/`/` bind tighter than `+`/`-`)
/// and parentheses, matching pasmo. TODO: `$` as the program counter; bitwise
/// and shift operators (not yet used by the curriculum).
fn parse_value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    let tokens = tokenize(raw, line)?;
    if tokens.is_empty() {
        return Err(AsmError::new(line, "expected a value"));
    }
    let mut parser = ExprParser { tokens, pos: 0, line };
    let expr = parser.expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(AsmError::new(
            line,
            format!("unexpected trailing tokens in `{}`", raw.trim()),
        ));
    }
    Ok(expr)
}

#[derive(Clone)]
enum Tok {
    Num(i64),
    Sym(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(raw: &str, line: usize) -> Result<Vec<Tok>, AsmError> {
    let chars: Vec<char> = raw.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ws if ws.is_whitespace() => i += 1,
            '+' => {
                tokens.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Tok::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Tok::Slash);
                i += 1;
            }
            '(' => {
                tokens.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Tok::RParen);
                i += 1;
            }
            '\'' => {
                // Char literal 'c'.
                if i + 2 < chars.len() && chars[i + 2] == '\'' {
                    let s: String = chars[i..=i + 2].iter().collect();
                    tokens.push(Tok::Num(parse_number(&s, line)?));
                    i += 3;
                } else {
                    return Err(AsmError::new(line, "malformed character literal"));
                }
            }
            // A `$hex`/`%binary`/decimal number: prefix (if any) then alnum.
            '$' | '%' => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Tok::Num(parse_number(&s, line)?));
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Tok::Num(parse_number(&s, line)?));
            }
            // An identifier: letters, digits, `_`, `.` (not starting with a digit).
            l if l.is_ascii_alphabetic() || l == '_' || l == '.' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                tokens.push(Tok::Sym(chars[start..i].iter().collect()));
            }
            other => {
                return Err(AsmError::new(
                    line,
                    format!("unexpected character `{other}` in expression"),
                ));
            }
        }
    }
    Ok(tokens)
}

/// A precedence-climbing expression parser: `add_sub` over `mul_div` over
/// `unary` over `atom`, so `*`/`/` bind tighter than `+`/`-`.
struct ExprParser {
    tokens: Vec<Tok>,
    pos: usize,
    line: usize,
}

impl ExprParser {
    fn expr(&mut self) -> Result<Expr, AsmError> {
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

/// A pasmo identifier: letters, digits, `_`, and `.` (the last so local-style
/// labels like `.loop` read as ordinary names), not starting with a digit.
fn is_ident(s: &str) -> bool {
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
    use crate::assemble_pasmonext as asm;

    #[test]
    fn loads_registers_and_immediates() {
        assert_eq!(asm("ld a, 0").expect("ld a,0").bytes, vec![0x3E, 0x00]);
        assert_eq!(asm("ld a, c").expect("ld a,c").bytes, vec![0x79]);
        // 16-bit immediate, little-endian.
        assert_eq!(asm("ld hl, $5800").expect("ld hl").bytes, vec![0x21, 0x00, 0x58]);
        assert_eq!(asm("ld bc, 767").expect("ld bc").bytes, vec![0x01, 0xFF, 0x02]);
        assert_eq!(asm("ld (hl), $0F").expect("ld (hl),n").bytes, vec![0x36, 0x0F]);
    }

    #[test]
    fn port_io_uses_eight_bit_operand() {
        // `out ($FE),a`: the parenthesised value resolves to the 8-bit port form.
        assert_eq!(asm("out ($FE), a").expect("out").bytes, vec![0xD3, 0xFE]);
        assert_eq!(asm("in a, ($FE)").expect("in").bytes, vec![0xDB, 0xFE]);
    }

    #[test]
    fn sixteen_bit_add_and_indirect() {
        assert_eq!(asm("add hl, de").expect("add").bytes, vec![0x19]);
        assert_eq!(asm("ld a, (bc)").expect("ld a,(bc)").bytes, vec![0x0A]);
        assert_eq!(asm("ld ($5800), hl").expect("ld (nn),hl").bytes, vec![0x22, 0x00, 0x58]);
    }

    #[test]
    fn equ_defines_a_constant() {
        let a = asm("COBBLE equ %00000001\n        ld (hl), COBBLE\n").expect("equ");
        assert_eq!(a.bytes, vec![0x36, 0x01]);
        assert_eq!(a.symbols.get("COBBLE"), Some(&0x0001));
    }

    #[test]
    fn relative_jumps_resolve_against_labels() {
        // JR back over the NOP: target $8000, PC after JR $8003, so e = -3.
        let a = asm("        org $8000\n.loop:  nop\n        jr .loop\n").expect("jr");
        assert_eq!(a.bytes, vec![0x00, 0x18, 0xFD]);
        assert_eq!(a.symbols.get(".loop"), Some(&0x8000));
    }

    #[test]
    fn condition_codes_disambiguate_from_registers() {
        // `c` is the carry condition for JR, the register for LD.
        assert!(asm("jr c, $0000").is_ok());
        assert_eq!(asm("ld b, c").expect("ld b,c").bytes, vec![0x41]);
        assert_eq!(asm("ret c").expect("ret c").bytes, vec![0xD8]);
        assert_eq!(asm("ret nc").expect("ret nc").bytes, vec![0xD0]);
    }

    #[test]
    fn arithmetic_respects_c_precedence() {
        // $5800 + 23*32 = $5800 + 736 = $5AE0 (the figure unit-01 hand-computes).
        assert_eq!(
            asm("ld hl, $5800 + 23*32").expect("precedence").bytes,
            vec![0x21, 0xE0, 0x5A]
        );
        // Parentheses override precedence.
        assert_eq!(
            asm("ld hl, (1+2)*3").expect("parens").bytes,
            vec![0x21, 0x09, 0x00]
        );
        // Division, and a symbol term.
        let a = asm("ROW equ 64\n        ld a, ROW / 8\n").expect("div");
        assert_eq!(a.bytes, vec![0x3E, 0x08]);
    }

    #[test]
    fn im_selects_mode_by_literal() {
        assert_eq!(asm("        im 1\n").expect("im 1").bytes, vec![0xED, 0x56]);
        assert_eq!(asm("        im 0\n").expect("im 0").bytes, vec![0xED, 0x46]);
        assert_eq!(asm("        im 2\n").expect("im 2").bytes, vec![0xED, 0x5E]);
    }

    #[test]
    fn ed_block_move_assembles() {
        // LDIR is an ED-prefix op: ED B0.
        assert_eq!(asm("        ldir\n").expect("ldir").bytes, vec![0xED, 0xB0]);
    }

    #[test]
    fn base_pasmo_and_pasmonext_agree_on_standard_z80() {
        // The two dialects are byte-identical for standard Z80.
        let src = "        org $8000\n.loop:  ld a, 0\n        ldir\n        bit 7,(hl)\n        jr .loop\n";
        let base = crate::assemble_pasmo(src).expect("pasmo");
        let next = crate::assemble_pasmonext(src).expect("pasmonext");
        assert_eq!(base.bytes, next.bytes);
    }

    #[test]
    fn defb_string_expands_to_char_bytes() {
        assert_eq!(
            asm("        defb \"AB\", 0\n").expect("defb").bytes,
            vec![0x41, 0x42, 0x00]
        );
    }

    #[test]
    fn cb_bit_ops_assemble() {
        assert_eq!(asm("        bit 7,(hl)\n").expect("bit").bytes, vec![0xCB, 0x7E]);
        assert_eq!(asm("        set 0,a\n").expect("set").bytes, vec![0xCB, 0xC7]);
        assert_eq!(asm("        rlc b\n").expect("rlc").bytes, vec![0xCB, 0x00]);
    }

    #[test]
    fn ix_iy_ops_assemble() {
        assert_eq!(asm("        push ix\n").expect("push ix").bytes, vec![0xDD, 0xE5]);
        assert_eq!(asm("        ld a,(ix+5)\n").expect("ld a,(ix+d)").bytes, vec![0xDD, 0x7E, 0x05]);
        // LD (IX+d),n is the two-operand form: DD 36 <d> <n>.
        assert_eq!(asm("        ld (ix+5),$0a\n").expect("ld (ix+d),n").bytes, vec![0xDD, 0x36, 0x05, 0x0A]);
        // DD CB puts the displacement before the trailing opcode byte.
        assert_eq!(asm("        bit 7,(iy-1)\n").expect("bit (iy+d)").bytes, vec![0xFD, 0xCB, 0xFF, 0x7E]);
    }

    #[test]
    fn z80n_opcodes_follow_the_target_not_the_dialect() {
        // Z80N is available when targeting the Next (pasmonext), and rejected
        // on a plain Z80 (pasmo) — same syntax, different target.
        assert_eq!(
            crate::assemble_pasmonext("        swapnib\n").expect("z80n on").bytes,
            vec![0xED, 0x23]
        );
        let err = crate::assemble_pasmo("        swapnib\n").expect_err("z80n off");
        assert!(err.message.contains("SWAPNIB"), "unexpected: {err}");
    }
}
