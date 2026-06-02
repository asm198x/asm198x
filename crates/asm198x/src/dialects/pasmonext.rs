//! The PasmoNext Z80 dialect front-end.
//!
//! PasmoNext (the ZX Spectrum Next fork of pasmo, invoked as `pasmonext`) is
//! the Code198x curriculum's assembler, so it is Asm198x's primary Z80 dialect
//! (see `decisions/syntax-stance.md`). It is a syntactic superset of vanilla
//! pasmo; for standard Z80 the two are byte-identical, and this front-end
//! covers that shared core. PasmoNext's Z80N extended opcodes are a deferred
//! ISA-spec extension — the current curriculum uses only standard Z80.
//!
//! This front-end targets [`isa::z80`] and covers the base page and ED group,
//! plus the directives real curriculum source uses: `org`, `equ`, `defb`/`db`,
//! `defw`/`dw`, `end`. Numbers are `$hex`, `%binary`, decimal, and `'c'` char
//! literals. Labels sit in column 0 (with or without a trailing `:`);
//! instructions are indented. Local-style labels beginning with `.` are
//! accepted as ordinary identifiers.
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

use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The PasmoNext Z80 dialect.
pub(crate) struct PasmoNext;

impl Dialect for PasmoNext {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z80::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let set = self.instruction_set();
        let mut out = Vec::new();
        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            let code = strip_comment(raw);
            if code.trim().is_empty() {
                continue;
            }
            let (label, rest) = split_label(set, code, line)?;
            let op = parse_op(set, rest, line)?;
            if label.is_none() && op.is_none() {
                continue;
            }
            out.push(Statement { line, label, op });
        }
        Ok(out)
    }
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
    if is_known_op(set, word) {
        return Ok((None, trimmed));
    }
    if !is_ident(word) {
        return Err(AsmError::new(line, format!("invalid label `{word}`")));
    }
    Ok((Some(word.to_string()), remainder))
}

/// Whether `word` names a Z80 instruction or a pasmo directive.
fn is_known_op(set: &'static isa::InstructionSet, word: &str) -> bool {
    set.instruction(&word.to_ascii_uppercase()).is_some()
        || matches!(
            word.to_ascii_lowercase().as_str(),
            "org" | "equ" | "defb" | "db" | "defm" | "defw" | "dw" | "end"
        )
}

fn parse_op(
    set: &'static isa::InstructionSet,
    rest: &str,
    line: usize,
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
            let insn = set.instruction(&mnemonic).ok_or_else(|| {
                AsmError::new(line, format!("unknown instruction `{mnemonic}`"))
            })?;
            let (mode, operand) = resolve(&mnemonic, insn, args, line)?;
            Ok(Some(Operation::Instruction {
                mnemonic,
                mode,
                operand,
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Operand resolution (dialect syntax -> spec mode label)
// ---------------------------------------------------------------------------

/// One classified operand: a fixed signature token, or a value that becomes
/// bytes on the wire (and so offers width-candidate tokens).
enum Operand {
    Fixed(String),
    Value { expr: Expr, paren: bool },
}

fn resolve(
    mnemonic: &str,
    insn: &isa::Instruction,
    args: &str,
    line: usize,
) -> Result<(&'static str, Option<Expr>), AsmError> {
    let pieces = split_operands(args);
    let mut candidates: Vec<Vec<String>> = Vec::new();
    let mut exprs: Vec<Expr> = Vec::new();
    for (idx, piece) in pieces.iter().enumerate() {
        match classify(piece, line)? {
            Operand::Fixed(token) => candidates.push(vec![token]),
            Operand::Value { expr, paren } => {
                candidates.push(value_tokens(mnemonic, paren, idx, &expr, line)?);
                exprs.push(expr);
            }
        }
    }

    for combo in product(&candidates) {
        let label = combo.join(",");
        if let Some(f) = insn.form(&label) {
            let operand = match exprs.as_slice() {
                [] => None,
                [single] => Some(single.clone()),
                _ => {
                    return Err(AsmError::new(
                        line,
                        format!("`{mnemonic}` with multiple value operands is not yet supported"),
                    ));
                }
            };
            return Ok((f.mode, operand));
        }
    }
    Err(AsmError::new(
        line,
        format!("`{mnemonic}` has no form for operands `{}`", args.trim()),
    ))
}

fn classify(piece: &str, line: usize) -> Result<Operand, AsmError> {
    let t = piece.trim();
    if let Some(inner) = strip_parens(t) {
        let up = inner.trim().to_ascii_uppercase();
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

/// Candidate signature tokens for a value operand. Width is left ambiguous
/// (both candidates offered) except where the mnemonic fixes it.
fn value_tokens(
    mnemonic: &str,
    paren: bool,
    _index: usize,
    expr: &Expr,
    line: usize,
) -> Result<Vec<String>, AsmError> {
    if paren {
        return Ok(vec!["(n)".to_string(), "(nn)".to_string()]);
    }
    match mnemonic {
        // Relative branches take a PC-relative displacement.
        "JR" | "DJNZ" => Ok(vec!["e".to_string()]),
        // RST's target is one of eight fixed addresses, encoded in the opcode
        // and named in the mode label as two hex digits.
        "RST" => {
            let v = literal(expr, line)?;
            Ok(vec![format!("{v:02X}")])
        }
        _ => Ok(vec!["n".to_string(), "nn".to_string()]),
    }
}

/// Evaluate a parse-time literal (for operands encoded in the opcode, like
/// `RST`). Symbols are not yet known here, so only numbers are accepted.
fn literal(expr: &Expr, line: usize) -> Result<i64, AsmError> {
    match expr {
        Expr::Num(n) => Ok(*n),
        _ => Err(AsmError::new(line, "operand must be a literal value")),
    }
}

/// Cartesian product of per-operand candidate tokens.
fn product(lists: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut result = vec![Vec::new()];
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

fn parse_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    rest.split(',').map(|p| parse_value(p, line)).collect()
}

fn parse_value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    let t = raw.trim();
    // TODO: arithmetic (`label+1`) and `$` as the program counter.
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
    fn ed_block_move_assembles() {
        // LDIR is an ED-prefix op: ED B0.
        assert_eq!(asm("        ldir\n").expect("ldir").bytes, vec![0xED, 0xB0]);
    }

    #[test]
    fn unimplemented_prefix_op_errors_clearly() {
        // RLC is a CB-prefix op, not yet authored: a clean error, not a
        // miscompile. (Indented, as the curriculum writes instructions.)
        let err = asm("        rlc b\n").expect_err("rlc not yet supported");
        assert!(err.message.contains("RLC"), "unexpected: {err}");
    }
}
