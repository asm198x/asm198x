//! The National SC/MP (INS8060) dialect front-end (asl syntax).
//!
//! Assembles against [`isa::scmp`] and produces a flat binary at the `org`.
//! Numbers are C-style (`0x..` hex, `0b..` binary, decimal), matching `asl`'s
//! SC/MP mode. Operand resolution dispatches on syntax:
//!
//! - no operand → an **inherent** form;
//! - `disp(ptr)` / `@disp(ptr)` → a **memory reference**: the pointer 0..3 and
//!   the optional `@` auto-index select the form (mode `"0"`..`"3"`/`"@1"`..
//!   `"@3"`), the signed displacement is emitted as the following byte. The
//!   literal `e` is the E-register index (displacement byte `0x80`);
//! - a bare operand → an **immediate** byte if the instruction has one
//!   (`LDI`/`ANI`/…/`DLY`), otherwise a **pointer number** 0..3 embedded in the
//!   opcode (the `XPAL`/`XPAH`/`XPPC` exchanges, no following byte).
//!
//! Every form is fixed-slot; no engine seam. Output is validated byte-identical
//! against `asl` (`cpu SC/MP`).

use std::collections::BTreeMap;

use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The National SC/MP (INS8060) dialect.
pub(crate) struct Scmp;

impl Dialect for Scmp {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::scmp::SET
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
        _ => resolve(set, &word.to_ascii_uppercase(), args, consts, line)?,
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
        parse_number_c,
        ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: Caret::Xor,
            at_is_pc: false,
        },
    )
}

/// C-style number lexer: `0x`/`0X` hex, `0b`/`0B` binary, `'c'` char, decimal.
fn parse_number_c(tok: &str, line: usize) -> Result<i64, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("invalid number `{tok}`"));
    if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
        return t.chars().nth(1).map(|c| c as i64).ok_or_else(bad);
    }
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).map_err(|_| bad());
    }
    if let Some(bin) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2).map_err(|_| bad());
    }
    t.parse::<i64>().map_err(|_| bad())
}

/// Resolve an instruction by its operand syntax.
fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let insn = set
        .instruction(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    let instr = |mode: &'static str, operands: Vec<Expr>| Operation::Instruction {
        mnemonic: mn.to_string(),
        mode,
        operands,
    };
    let t = args.trim();

    if t.is_empty() {
        return match insn.form("") {
            Some(f) => Ok(instr(f.mode, vec![])),
            None => Err(AsmError::new(line, format!("`{mn}` requires an operand"))),
        };
    }

    // Memory reference: `disp(ptr)` or `@disp(ptr)`.
    if let Some(open) = t.find('(') {
        let close = t
            .rfind(')')
            .ok_or_else(|| AsmError::new(line, "missing `)` in operand"))?;
        let ptr = fold_const(&value(t[open + 1..close].trim(), line)?, consts, line)?;
        if !(0..=3).contains(&ptr) {
            return Err(AsmError::new(line, "pointer register must be 0..3"));
        }
        let mut disp = t[..open].trim();
        let at = disp.starts_with('@');
        if at {
            disp = disp[1..].trim();
        }
        let label = format!("{}{ptr}", if at { "@" } else { "" });
        let f = insn.form(&label).ok_or_else(|| {
            AsmError::new(line, format!("`{mn}` has no `{}` addressing mode", label))
        })?;
        // The literal `e` is the E-register index (displacement byte 0x80).
        let disp = if disp.eq_ignore_ascii_case("e") {
            Expr::Num(0x80)
        } else {
            value(disp, line)?
        };
        return Ok(instr(f.mode, vec![disp]));
    }

    // A bare operand: an immediate byte where the instruction has one, else a
    // pointer number embedded in the opcode.
    if let Some(f) = insn.form("imm") {
        return Ok(instr(f.mode, vec![value(t, line)?]));
    }
    let n = fold_const(&value(t, line)?, consts, line)?;
    if !(0..=3).contains(&n) {
        return Err(AsmError::new(line, "pointer register must be 0..3"));
    }
    match insn.form(&n.to_string()) {
        Some(f) => Ok(instr(f.mode, vec![])),
        None => Err(AsmError::new(line, format!("`{mn}` takes no operand"))),
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_scmp as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn inherent_and_pointer_exchange() {
        assert_eq!(bytes(" nop\n"), vec![0x08]);
        assert_eq!(bytes(" cae\n"), vec![0x78]);
        assert_eq!(bytes(" xpal 1\n"), vec![0x31]);
        assert_eq!(bytes(" xppc 3\n"), vec![0x3F]);
    }

    #[test]
    fn memory_reference() {
        assert_eq!(bytes(" ld 5(1)\n"), vec![0xC1, 0x05]);
        assert_eq!(bytes(" ld @5(2)\n"), vec![0xC6, 0x05]);
        assert_eq!(bytes(" ld -1(1)\n"), vec![0xC1, 0xFF]);
        assert_eq!(bytes(" st @1(1)\n"), vec![0xCD, 0x01]);
        assert_eq!(bytes(" ld e(1)\n"), vec![0xC1, 0x80]);
        assert_eq!(bytes(" cad 7(1)\n"), vec![0xF9, 0x07]);
        assert_eq!(bytes(" jmp 0(0)\n"), vec![0x90, 0x00]);
        assert_eq!(bytes(" ild 2(1)\n"), vec![0xA9, 0x02]);
    }

    #[test]
    fn immediates_c_hex() {
        assert_eq!(bytes(" ldi 0x42\n"), vec![0xC4, 0x42]);
        assert_eq!(bytes(" ani 0x0f\n"), vec![0xD4, 0x0F]);
        assert_eq!(bytes(" xri 0b10101010\n"), vec![0xE4, 0xAA]);
        assert_eq!(bytes(" dly 5\n"), vec![0x8F, 0x05]);
    }
}
