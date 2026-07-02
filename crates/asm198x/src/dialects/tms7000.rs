//! The Texas Instruments TMS7000 dialect front-end (asl syntax).
//!
//! Assembles against [`isa::tms7000`] and produces a flat binary at the `org`.
//! Numbers are Intel `H`-suffix hex (shared with the 8080 dialect). Operands are
//! classified by prefix: `A`/`B` the accumulators, `%n` an immediate, `Rn` a
//! register-file byte, `Pn` a peripheral byte, `@nnnn` a direct address, `*Rn`
//! an indirect register, and `@nnnn(B)` / `%nnnn(B)` indexed. The classified
//! source/destination pair selects the spec form (its mode label), and the
//! operand bytes follow in order.
//!
//! Relative offsets (jumps, `BTJO`/`DJNZ`, …) are standard 8-bit signed from the
//! following instruction, so everything is a fixed-slot [`Operation::Instruction`]
//! — only `TRAP n` (opcode `0xFF - n`) is a single computed byte. Validated
//! byte-identical against `asl` (`cpu TMS70C00`).

use std::collections::BTreeMap;

use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};

/// The Texas Instruments TMS7000 dialect.
pub(crate) struct Tms7000;

impl Dialect for Tms7000 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::tms7000::SET
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
                parse_op(set, rest, line)?
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
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        "cpu" | "end" | "title" | "page" | "name" | "sect" | "text" => return Ok(None),
        "org" | "aorg" | "rorg" => Operation::Org(value(args, line)?),
        "db" | "byte" | "dc" => Operation::Bytes(byte_list(args, line)?),
        "dw" | "word" | "data" => Operation::Words(value_list(args, line)?),
        "ds" | "bss" | "block" => parse_ds(args, line)?,
        _ => resolve(set, &word.to_ascii_uppercase(), args, line)?,
    };
    Ok(Some(op))
}

fn parse_ds(args: &str, line: usize) -> Result<Operation, AsmError> {
    let count = fold_const(&value(args.trim(), line)?, &BTreeMap::new(), line)?;
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
    // `asl` spells the location counter `$` (hex here is Intel `H`-suffix, so
    // `$` is free for the PC; `*` is the indirect prefix, not the PC).
    if raw.trim() == "$" {
        return Ok(Expr::Pc);
    }
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

/// A classified operand.
enum Op {
    A,
    B,
    Imm(Expr),      // %n
    Reg(Expr),      // Rn
    Perip(Expr),    // Pn
    Direct(Expr),   // @nnnn
    Indirect(Expr), // *Rn
    IdxDir(Expr),   // @nnnn(B)
    IdxImm(Expr),   // %nnnn(B)
    Bare,           // an unrecognised token (e.g. the `ST` of `PUSH ST`)
}

fn classify(tok: &str, line: usize) -> Result<Op, AsmError> {
    let t = tok.trim();
    let lower = t.to_ascii_lowercase();
    if lower == "a" {
        return Ok(Op::A);
    }
    if lower == "b" {
        return Ok(Op::B);
    }
    // Indexed forms end in `(b)`.
    let indexed = lower.ends_with("(b)");
    let core = if indexed {
        t[..t.len() - 3].trim_end()
    } else {
        t
    };
    if let Some(imm) = core.strip_prefix('%') {
        let e = value(imm, line)?;
        return Ok(if indexed { Op::IdxImm(e) } else { Op::Imm(e) });
    }
    if let Some(addr) = core.strip_prefix('@') {
        let e = value(addr, line)?;
        return Ok(if indexed {
            Op::IdxDir(e)
        } else {
            Op::Direct(e)
        });
    }
    if let Some(reg) = t.strip_prefix('*') {
        return Ok(Op::Indirect(reg_number(reg, line)?));
    }
    if let Some(n) = reg_index(t, 'r') {
        return Ok(Op::Reg(value(n, line)?));
    }
    if let Some(n) = reg_index(t, 'p') {
        return Ok(Op::Perip(value(n, line)?));
    }
    Ok(Op::Bare)
}

/// The `r`/`p` register/peripheral index, e.g. `r5` → `"5"`; `None` if `tok`
/// isn't `<letter><number>`.
fn reg_index(tok: &str, letter: char) -> Option<&str> {
    let rest = tok
        .strip_prefix(letter)
        .or_else(|| tok.strip_prefix(letter.to_ascii_uppercase()))?;
    (!rest.is_empty() && rest.as_bytes()[0].is_ascii_digit()).then_some(rest)
}

/// Parse an `Rn` register operand (after a `*`), yielding its number expression.
fn reg_number(tok: &str, line: usize) -> Result<Expr, AsmError> {
    match reg_index(tok.trim(), 'r') {
        Some(n) => value(n, line),
        None => Err(AsmError::new(
            line,
            format!("expected a register, got `{tok}`"),
        )),
    }
}

/// Emit a fixed-slot instruction for a resolved `(mode, operands)`.
fn instr(mn: &str, mode: &'static str, operands: Vec<Expr>) -> Operation {
    Operation::Instruction {
        mnemonic: mn.to_string(),
        mode,
        operands,
    }
}

fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    args: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    // Condition-code aliases.
    let mn = match mn {
        "JLT" => "JN",
        "JEQ" => "JZ",
        "JHS" => "JC",
        "JGT" => "JP",
        "JGE" => "JPZ",
        "JNE" => "JNZ",
        "JL" => "JNC",
        other => other,
    };

    // TRAP n → opcode 0xFF - n (n = 0..23), a single computed byte.
    if mn == "TRAP" {
        let n = fold_const(&value(args.trim(), line)?, &BTreeMap::new(), line)?;
        if !(0..=23).contains(&n) {
            return Err(AsmError::new(line, "TRAP number must be 0..23"));
        }
        return Ok(Operation::Encoded(vec![Piece::Lit(0xFF - n as u8)]));
    }

    let ops: Vec<&str> = if args.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level(args, ',')
    };

    // Implied and single-operand jump-like mnemonics resolve by group.
    match mn {
        // Implied (no operand).
        "NOP" | "IDLE" | "EINT" | "DINT" | "SETC" | "CLRC" | "STSP" | "LDSP" | "RETS" | "RETI"
        | "TSTA" | "TSTB" => return Ok(instr(mn, "", vec![])),
        // Relative jumps: one target.
        "JMP" | "JN" | "JZ" | "JC" | "JP" | "JPZ" | "JNZ" | "JNC" => {
            let target = one(&ops, line)?;
            return Ok(instr(mn, "", vec![value(target, line)?]));
        }
        _ => {}
    }

    // Extended-addressing ops: a single addressing operand.
    if matches!(mn, "LDA" | "STA" | "BR" | "CALL" | "CMPA") {
        return extended(mn, one(&ops, line)?, line);
    }

    // Single-register ops (+ DJNZ, PUSH/POP with status).
    if let Some(op) = single_reg(mn, &ops, line)? {
        return Ok(op);
    }

    // MOVD: the 16-bit-immediate / register-pair / indexed double move.
    if mn == "MOVD" {
        return movd(&ops, line);
    }

    // Peripheral ops.
    if matches!(mn, "MOVP" | "ANDP" | "ORP" | "XORP" | "BTJOP" | "BTJZP") {
        return peripheral(mn, &ops, line);
    }

    // Everything else is a dual-operand ALU op (MOV/AND/…/DSB, BTJO/BTJZ).
    dual(set, mn, &ops, line)
}

fn one<'a>(ops: &[&'a str], line: usize) -> Result<&'a str, AsmError> {
    match ops {
        [a] => Ok(a.trim()),
        _ => Err(AsmError::new(line, "expected one operand")),
    }
}

/// `LDA`/`STA`/`BR`/`CALL`/`CMPA`: `@nnnn` / `*Rn` / `@nnnn(B)`.
fn extended(mn: &str, arg: &str, line: usize) -> Result<Operation, AsmError> {
    match classify(arg, line)? {
        Op::Direct(e) => Ok(instr(mn, "@", vec![e])),
        Op::Indirect(e) => Ok(instr(mn, "*", vec![e])),
        Op::IdxDir(e) => Ok(instr(mn, "@(b)", vec![e])),
        _ => Err(AsmError::new(
            line,
            format!("`{mn}` needs @nnnn, *Rn, or @nnnn(B)"),
        )),
    }
}

/// The single-register op families, keyed by mnemonic. Returns `None` if `mn`
/// isn't one of them (so the caller falls through to the dual-operand path).
fn single_reg(mn: &str, ops: &[&str], line: usize) -> Result<Option<Operation>, AsmError> {
    let plain = matches!(
        mn,
        "DEC" | "INC" | "INV" | "CLR" | "XCHB" | "SWAP" | "DECD" | "RR" | "RRC" | "RL" | "RLC"
    );
    if plain {
        let op = match classify(one(ops, line)?, line)? {
            Op::A => instr(mn, "a", vec![]),
            Op::B => instr(mn, "b", vec![]),
            Op::Reg(e) => instr(mn, "rn", vec![e]),
            _ => return Err(AsmError::new(line, format!("`{mn}` needs A, B, or Rn"))),
        };
        return Ok(Some(op));
    }
    if matches!(mn, "PUSH" | "POP") {
        return match classify(one(ops, line)?, line)? {
            Op::A => Ok(Some(instr(mn, "a", vec![]))),
            Op::B => Ok(Some(instr(mn, "b", vec![]))),
            Op::Reg(e) => Ok(Some(instr(mn, "rn", vec![e]))),
            Op::Bare if one(ops, line)?.eq_ignore_ascii_case("st") => {
                Ok(Some(instr(mn, "st", vec![])))
            }
            _ => Err(AsmError::new(line, format!("`{mn}` needs A, B, Rn, or ST"))),
        };
    }
    if mn == "DJNZ" {
        return match ops {
            [reg, target] => {
                let t = value(target.trim(), line)?;
                match classify(reg, line)? {
                    Op::A => Ok(Some(instr(mn, "a", vec![t]))),
                    Op::B => Ok(Some(instr(mn, "b", vec![t]))),
                    Op::Reg(e) => Ok(Some(instr(mn, "rn", vec![e, t]))),
                    _ => Err(AsmError::new(line, "DJNZ needs A, B, or Rn")),
                }
            }
            _ => Err(AsmError::new(line, "DJNZ needs a register and a target")),
        };
    }
    Ok(None)
}

/// `MOVD`: `%nnnn,Rn` / `Rn,Rn` / `%nnnn(B),Rn`.
fn movd(ops: &[&str], line: usize) -> Result<Operation, AsmError> {
    let [src, dst] = two(ops, line)?;
    let d = match classify(dst, line)? {
        Op::Reg(e) => e,
        Op::A => Expr::Num(0),
        Op::B => Expr::Num(1),
        _ => return Err(AsmError::new(line, "MOVD destination must be a register")),
    };
    match classify(src, line)? {
        Op::Imm(e) => Ok(instr("MOVD", "%n,rn", vec![e, d])),
        Op::IdxImm(e) => Ok(instr("MOVD", "%n(b),rn", vec![e, d])),
        Op::Reg(e) => Ok(instr("MOVD", "rn,rn", vec![e, d])),
        _ => Err(AsmError::new(
            line,
            "MOVD source must be %nnnn, %nnnn(B), or Rn",
        )),
    }
}

/// The peripheral ops: `MOVP` (both directions) and the write/bit-test ops.
fn peripheral(mn: &str, ops: &[&str], line: usize) -> Result<Operation, AsmError> {
    // BTJOP/BTJZP carry a trailing jump target.
    let (pair, target) = if matches!(mn, "BTJOP" | "BTJZP") {
        match ops {
            [a, b, t] => ([*a, *b], Some(value(t.trim(), line)?)),
            _ => {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` needs src, Pn, and a target"),
                ));
            }
        }
    } else {
        (two(ops, line)?, None)
    };
    let src = classify(pair[0], line)?;
    let dst = classify(pair[1], line)?;
    let push = |mode: &'static str, mut v: Vec<Expr>| {
        if let Some(t) = target.clone() {
            v.push(t);
        }
        instr(mn, mode, v)
    };
    // MOVP additionally reads a peripheral into A/B.
    if mn == "MOVP" {
        if let (Op::Perip(p), Op::A) = (&src, &dst) {
            return Ok(instr(mn, "pn,a", vec![p.clone()]));
        }
        if let (Op::Perip(p), Op::B) = (&src, &dst) {
            return Ok(instr(mn, "pn,b", vec![p.clone()]));
        }
    }
    match (src, dst) {
        (Op::A, Op::Perip(p)) => Ok(push("a,pn", vec![p])),
        (Op::B, Op::Perip(p)) => Ok(push("b,pn", vec![p])),
        (Op::Imm(i), Op::Perip(p)) => Ok(push("%n,pn", vec![i, p])),
        _ => Err(AsmError::new(
            line,
            format!("`{mn}` operands must be A/B/%n and Pn"),
        )),
    }
}

/// The dual-operand ALU ops (and the special `MOV A,B` / `A,Rn` / `B,Rn` forms).
fn dual(
    set: &'static isa::InstructionSet,
    mn: &str,
    ops: &[&str],
    line: usize,
) -> Result<Operation, AsmError> {
    // BTJO/BTJZ carry a relative target as a third operand.
    let (src_s, dst_s, target) = if matches!(mn, "BTJO" | "BTJZ") {
        match ops {
            [s, d, t] => (*s, *d, Some(value(t.trim(), line)?)),
            _ => {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` needs src, dst, and a target"),
                ));
            }
        }
    } else {
        let [s, d] = two(ops, line)?;
        (s, d, None)
    };
    let src = classify(src_s, line)?;
    let dst = classify(dst_s, line)?;

    // The MOV mnemonic carries three forms the dual grid lacks.
    if mn == "MOV" {
        match (&src, &dst) {
            (Op::A, Op::B) => return Ok(instr(mn, "a,b", vec![])),
            (Op::A, Op::Reg(r)) => return Ok(instr(mn, "a,rn", vec![r.clone()])),
            (Op::B, Op::Reg(r)) => return Ok(instr(mn, "b,rn", vec![r.clone()])),
            _ => {}
        }
    }

    let with_t = |mut v: Vec<Expr>| {
        if let Some(t) = target.clone() {
            v.push(t);
        }
        v
    };

    let (mode, vals): (&'static str, Vec<Expr>) = match (src, dst) {
        (Op::Reg(s), Op::A) => ("rn,a", vec![s]),
        (Op::Imm(i), Op::A) => ("%n,a", vec![i]),
        (Op::Reg(s), Op::B) => ("rn,b", vec![s]),
        (Op::Reg(s), Op::Reg(d)) => ("rn,rn", vec![s, d]),
        (Op::Imm(i), Op::B) => ("%n,b", vec![i]),
        (Op::B, Op::A) => ("b,a", vec![]),
        (Op::Imm(i), Op::Reg(d)) => ("%n,rn", vec![i, d]),
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{mn}`: unsupported operand combination"),
            ));
        }
    };
    let _ = set;
    Ok(instr(mn, mode, with_t(vals)))
}

fn two<'a>(ops: &[&'a str], line: usize) -> Result<[&'a str; 2], AsmError> {
    match ops {
        [a, b] => Ok([a.trim(), b.trim()]),
        _ => Err(AsmError::new(line, "expected two operands")),
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_tms7000 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn dual_operand_alu() {
        assert_eq!(bytes(" mov r5,a\n"), vec![0x12, 0x05]);
        assert_eq!(bytes(" mov %42h,a\n"), vec![0x22, 0x42]);
        assert_eq!(bytes(" mov r5,r6\n"), vec![0x42, 0x05, 0x06]);
        assert_eq!(bytes(" mov b,a\n"), vec![0x62]);
        assert_eq!(bytes(" mov %42h,r5\n"), vec![0x72, 0x42, 0x05]);
        assert_eq!(bytes(" add r5,a\n"), vec![0x18, 0x05]);
        assert_eq!(bytes(" cmp b,a\n"), vec![0x6D]);
    }

    #[test]
    fn special_mov_and_single_register() {
        assert_eq!(bytes(" mov a,b\n"), vec![0xC0]);
        assert_eq!(bytes(" mov a,r5\n"), vec![0xD0, 0x05]);
        assert_eq!(bytes(" mov b,r5\n"), vec![0xD1, 0x05]);
        assert_eq!(bytes(" dec a\n"), vec![0xB2]);
        assert_eq!(bytes(" inc b\n"), vec![0xC3]);
        assert_eq!(bytes(" clr r200\n"), vec![0xD5, 0xC8]);
        assert_eq!(bytes(" rlc b\n"), vec![0xCF]);
        assert_eq!(bytes(" tstb\n"), vec![0xC1]);
    }

    #[test]
    fn peripheral_and_extended() {
        assert_eq!(bytes(" movp p6,a\n"), vec![0x80, 0x06]);
        assert_eq!(bytes(" movp a,p6\n"), vec![0x82, 0x06]);
        assert_eq!(bytes(" andp %0fh,p6\n"), vec![0xA3, 0x0F, 0x06]);
        assert_eq!(bytes(" lda @1234h\n"), vec![0x8A, 0x12, 0x34]);
        assert_eq!(bytes(" lda *r5\n"), vec![0x9A, 0x05]);
        assert_eq!(bytes(" br @1234h(b)\n"), vec![0xAC, 0x12, 0x34]);
        assert_eq!(bytes(" call *r5\n"), vec![0x9E, 0x05]);
        assert_eq!(bytes(" movd %1234h,r4\n"), vec![0x88, 0x12, 0x34, 0x04]);
        assert_eq!(bytes(" movd r2,r4\n"), vec![0x98, 0x02, 0x04]);
    }

    #[test]
    fn jumps_and_bit_tests() {
        // JMP to self at org 0: offset = 0 - 2 = -2 = 0xFE.
        assert_eq!(bytes(" jmp $\n"), vec![0xE0, 0xFE]);
        assert_eq!(bytes(" jeq $\n"), vec![0xE2, 0xFE]); // alias for JZ
        assert_eq!(bytes(" btjo %1,a,$\n"), vec![0x26, 0x01, 0xFD]);
        assert_eq!(bytes(" btjz r5,a,$\n"), vec![0x17, 0x05, 0xFD]);
        assert_eq!(bytes(" btjop a,p6,$\n"), vec![0x86, 0x06, 0xFD]);
        assert_eq!(bytes(" djnz a,$\n"), vec![0xBA, 0xFE]);
        assert_eq!(bytes(" djnz r5,$\n"), vec![0xDA, 0x05, 0xFD]);
    }

    #[test]
    fn implied_and_trap() {
        assert_eq!(bytes(" nop\n"), vec![0x00]);
        assert_eq!(bytes(" eint\n"), vec![0x05]);
        assert_eq!(bytes(" push st\n"), vec![0x0E]);
        assert_eq!(bytes(" pop st\n"), vec![0x08]);
        assert_eq!(bytes(" trap 0\n"), vec![0xFF]);
        assert_eq!(bytes(" trap 23\n"), vec![0xE8]);
    }
}
