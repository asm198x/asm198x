//! Asm198x — a family of modern assemblers for retro CPUs.
//!
//! This first slice assembles **6502** source to a flat binary. It is a
//! two-pass assembler: pass one assigns addresses to labels, pass two emits
//! bytes with labels resolved. Instruction *encoding* comes entirely from the
//! shared [`isa`] spec — this crate owns only the **dialect** (how 6502 source
//! syntax maps to addressing modes) and the engine around it.
//!
//! The dialect here is a deliberately small starting point: implied,
//! accumulator, immediate, zero-page, absolute, the indexed and indirect
//! modes, and relative branches; plus `.org`, `.byte`/`.db`, `.word`/`.dw`;
//! plus `<`/`>` low/high-byte operators. Full source-compatibility with an
//! existing 6502 dialect (ca65 / ACME shaped) is the project goal, tracked in
//! `decisions/` — not yet reached. Notable gaps, marked `TODO` below:
//! arithmetic in expressions, string/char escapes, scoped labels, macros.

use std::collections::BTreeMap;
use std::fmt;

/// The result of a successful assembly: where it loads and the bytes to load.
#[derive(Debug, Clone)]
pub struct Assembly {
    /// Load address (first `.org`, or 0 if none given).
    pub origin: u16,
    /// Assembled machine code, contiguous from `origin`.
    pub bytes: Vec<u8>,
    /// Resolved labels, for listings and debugging.
    pub symbols: BTreeMap<String, u16>,
}

/// An assembly error, with the 1-based source line it occurred on (0 = no
/// specific line).
#[derive(Debug, Clone)]
pub struct AsmError {
    pub line: usize,
    pub message: String,
}

impl AsmError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for AsmError {}

/// Assemble 6502 source into a flat binary.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_6502(source: &str) -> Result<Assembly, AsmError> {
    let set = &isa::mos6502::SET;
    let statements = parse(source)?;

    // Pass 1 — assign addresses to labels. Instruction sizes must not depend
    // on symbol *values*, so the zero-page-vs-absolute choice keys off whether
    // the operand is a literal constant, never off a (possibly forward) label.
    let mut symbols: BTreeMap<String, u16> = BTreeMap::new();
    let mut pc: i64 = 0;
    let mut origin: Option<i64> = None;
    for s in &statements {
        if let Some(label) = &s.label {
            if !(0..=0xFFFF).contains(&pc) {
                return Err(AsmError::new(s.line, "address out of range"));
            }
            if symbols.insert(label.clone(), pc as u16).is_some() {
                return Err(AsmError::new(s.line, format!("duplicate label `{label}`")));
            }
        }
        match &s.op {
            None => {}
            Some(Op::Org(e)) => {
                let v = e.eval(&symbols, s.line)?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(AsmError::new(s.line, ".org address out of range"));
                }
                pc = v;
                origin.get_or_insert(v);
            }
            Some(Op::Bytes(items)) => pc += items.len() as i64,
            Some(Op::Words(items)) => pc += 2 * items.len() as i64,
            Some(Op::Insn { mnemonic, operand }) => {
                let insn = instruction(set, mnemonic, s.line)?;
                let (mode, _) = resolve_mode(insn, operand, s.line)?;
                pc += form(insn, mode, s.line)?.len() as i64;
            }
        }
    }
    let origin = origin.unwrap_or(0);

    // Pass 2 — emit.
    let mut bytes: Vec<u8> = Vec::new();
    for s in &statements {
        match &s.op {
            None => {}
            Some(Op::Org(e)) => {
                let target = e.eval(&symbols, s.line)?;
                let cur = origin + bytes.len() as i64;
                if target < cur {
                    return Err(AsmError::new(s.line, "cannot move .org backwards"));
                }
                bytes.resize(bytes.len() + (target - cur) as usize, 0);
            }
            Some(Op::Bytes(items)) => {
                for e in items {
                    let v = e.eval(&symbols, s.line)?;
                    bytes.push(to_byte(v, s.line)?);
                }
            }
            Some(Op::Words(items)) => {
                for e in items {
                    let v = e.eval(&symbols, s.line)?;
                    push_word(&mut bytes, v, s.line)?;
                }
            }
            Some(Op::Insn { mnemonic, operand }) => {
                let insn = instruction(set, mnemonic, s.line)?;
                let (mode, expr) = resolve_mode(insn, operand, s.line)?;
                let f = form(insn, mode, s.line)?;
                let next_addr = origin + bytes.len() as i64 + f.len() as i64;
                bytes.extend_from_slice(f.opcode);
                for operand_slot in f.operands {
                    let e = expr
                        .ok_or_else(|| AsmError::new(s.line, "internal: missing operand value"))?;
                    let v = e.eval(&symbols, s.line)?;
                    match operand_slot.kind {
                        isa::OperandKind::Immediate => bytes.push(to_byte(v, s.line)?),
                        isa::OperandKind::Address => match operand_slot.bytes {
                            1 => bytes.push(to_byte(v, s.line)?),
                            2 => push_word(&mut bytes, v, s.line)?,
                            other => {
                                return Err(AsmError::new(
                                    s.line,
                                    format!("unsupported address width {other}"),
                                ));
                            }
                        },
                        isa::OperandKind::RelativePc => {
                            let offset = v - next_addr;
                            if !(-128..=127).contains(&offset) {
                                return Err(AsmError::new(
                                    s.line,
                                    format!(
                                        "branch target out of range ({offset} bytes; must be -128..=127)"
                                    ),
                                ));
                            }
                            bytes.push(offset as i8 as u8);
                        }
                    }
                }
            }
        }
    }

    if origin + bytes.len() as i64 > 0x1_0000 {
        return Err(AsmError::new(0, "program exceeds the 64K address space"));
    }

    Ok(Assembly {
        origin: origin as u16,
        bytes,
        symbols,
    })
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Expr {
    Num(i64),
    Sym(String),
    /// Low byte of the inner value (`<expr`).
    Lo(Box<Expr>),
    /// High byte of the inner value (`>expr`).
    Hi(Box<Expr>),
}

impl Expr {
    fn eval(&self, symbols: &BTreeMap<String, u16>, line: usize) -> Result<i64, AsmError> {
        Ok(match self {
            Expr::Num(n) => *n,
            Expr::Sym(s) => i64::from(
                *symbols
                    .get(s)
                    .ok_or_else(|| AsmError::new(line, format!("undefined symbol `{s}`")))?,
            ),
            Expr::Lo(e) => e.eval(symbols, line)? & 0xFF,
            Expr::Hi(e) => (e.eval(symbols, line)? >> 8) & 0xFF,
        })
    }
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Index {
    X,
    Y,
}

/// Operand syntax as parsed, before it is resolved to an addressing mode.
#[derive(Debug, Clone)]
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

enum Op {
    Org(Expr),
    Bytes(Vec<Expr>),
    Words(Vec<Expr>),
    Insn {
        mnemonic: String,
        operand: OperandSyntax,
    },
}

struct Stmt {
    line: usize,
    label: Option<String>,
    op: Option<Op>,
}

// ---------------------------------------------------------------------------
// Mode resolution (dialect → spec)
// ---------------------------------------------------------------------------

fn instruction<'a>(
    set: &'a isa::InstructionSet,
    mnemonic: &str,
    line: usize,
) -> Result<&'a isa::Instruction, AsmError> {
    set.instruction(mnemonic)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))
}

fn form<'a>(
    insn: &'a isa::Instruction,
    mode: &str,
    line: usize,
) -> Result<&'a isa::Form, AsmError> {
    insn.form(mode).ok_or_else(|| {
        AsmError::new(
            line,
            format!("`{}` has no {mode} addressing mode", insn.mnemonic),
        )
    })
}

fn resolve_mode<'a>(
    insn: &isa::Instruction,
    operand: &'a OperandSyntax,
    line: usize,
) -> Result<(&'static str, Option<&'a Expr>), AsmError> {
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
            (pick_zp_abs(insn, e, "zeropage,x", "absolute,x"), Some(e))
        }
        OperandSyntax::Indexed(e, Index::Y) => {
            (pick_zp_abs(insn, e, "zeropage,y", "absolute,y"), Some(e))
        }
        OperandSyntax::Direct(e) => {
            // A bare operand on a branch instruction is a relative target.
            if insn.form("relative").is_some() {
                ("relative", Some(e))
            } else {
                (pick_zp_abs(insn, e, "zeropage", "absolute"), Some(e))
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
// Emission helpers
// ---------------------------------------------------------------------------

fn to_byte(v: i64, line: usize) -> Result<u8, AsmError> {
    if (0..=0xFF).contains(&v) {
        Ok(v as u8)
    } else if (-128..=-1).contains(&v) {
        Ok(v as i8 as u8)
    } else {
        Err(AsmError::new(
            line,
            format!("value {v} does not fit in a byte"),
        ))
    }
}

fn push_word(bytes: &mut Vec<u8>, v: i64, line: usize) -> Result<(), AsmError> {
    if !(0..=0xFFFF).contains(&v) {
        return Err(AsmError::new(
            line,
            format!("value {v} does not fit in a word"),
        ));
    }
    bytes.push((v & 0xFF) as u8); // little-endian: low byte first
    bytes.push(((v >> 8) & 0xFF) as u8);
    Ok(())
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse(source: &str) -> Result<Vec<Stmt>, AsmError> {
    let mut out = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        // Strip line comments. TODO: a `;` inside a char literal would be cut
        // here; acceptable until char literals in operands are exercised.
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
            Some(Op::Insn {
                mnemonic: mnemonic.to_ascii_uppercase(),
                operand: parse_operand(remainder, line)?,
            })
        };

        out.push(Stmt { line, label, op });
    }
    Ok(out)
}

fn split_label(line: &str, number: usize) -> Result<(Option<String>, &str), AsmError> {
    if let Some(colon) = line.find(':') {
        let candidate = line[..colon].trim();
        if is_ident(candidate) {
            return Ok((Some(candidate.to_string()), line[colon + 1..].trim()));
        }
        return Err(AsmError::new(
            number,
            format!("invalid label `{candidate}`"),
        ));
    }
    Ok((None, line))
}

fn parse_directive(directive: &str, line: usize) -> Result<Op, AsmError> {
    let (name, rest) = split_first_word(directive);
    match name.to_ascii_lowercase().as_str() {
        "org" => Ok(Op::Org(parse_value(rest, line)?)),
        "byte" | "db" => Ok(Op::Bytes(parse_list(rest, line)?)),
        "word" | "dw" => Ok(Op::Words(parse_list(rest, line)?)),
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
            return Ok(OperandSyntax::Indirect(parse_value(
                &t[1..t.len() - 1],
                line,
            )?));
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
                return Err(AsmError::new(
                    line,
                    format!("expected `,X` or `,Y` in `{raw}`"),
                ));
            }
        };
        return Ok(OperandSyntax::Indexed(
            parse_value(&t[..comma], line)?,
            index,
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_countdown_loop() {
        let source = "\
            ; count down X, storing A across a page\n\
                    .org $0200\n\
            start:  lda #$00\n\
                    ldx #$08\n\
            loop:   sta $0400,x\n\
                    dex\n\
                    bne loop\n\
                    rts\n";
        let a = assemble_6502(source).expect("assembles");
        assert_eq!(a.origin, 0x0200);
        assert_eq!(
            a.bytes,
            vec![
                0xA9, 0x00, 0xA2, 0x08, 0x9D, 0x00, 0x04, 0xCA, 0xD0, 0xFA, 0x60
            ]
        );
        assert_eq!(a.symbols.get("start"), Some(&0x0200));
        assert_eq!(a.symbols.get("loop"), Some(&0x0204));
    }

    #[test]
    fn chooses_zero_page_over_absolute() {
        let zp = assemble_6502("lda $10").expect("zp");
        assert_eq!(zp.bytes, vec![0xA5, 0x10]);
        let abs = assemble_6502("lda $1234").expect("abs");
        assert_eq!(abs.bytes, vec![0xAD, 0x34, 0x12]); // little-endian
    }

    #[test]
    fn indexed_and_immediate() {
        assert_eq!(
            assemble_6502("sta $00,x").expect("zpx").bytes,
            vec![0x95, 0x00]
        );
        assert_eq!(
            assemble_6502("lda #'A'").expect("char").bytes,
            vec![0xA9, 0x41]
        );
        assert_eq!(
            assemble_6502("lda #%00001111").expect("bin").bytes,
            vec![0xA9, 0x0F]
        );
    }

    #[test]
    fn high_low_byte_operators() {
        // `<` takes the low byte, `>` the high byte.
        assert_eq!(
            assemble_6502("lda #<$1234").expect("lo").bytes,
            vec![0xA9, 0x34]
        );
        assert_eq!(
            assemble_6502("ldx #>$1234").expect("hi").bytes,
            vec![0xA2, 0x12]
        );
    }

    #[test]
    fn rejects_oversized_immediate() {
        let err = assemble_6502("lda #$1234").expect_err("immediate too big");
        assert!(err.message.contains("byte"), "unexpected: {err}");
    }

    #[test]
    fn reports_unknown_instruction_with_line() {
        let err = assemble_6502("\n    frob $10\n").expect_err("unknown mnemonic");
        assert_eq!(err.line, 2);
    }
}
