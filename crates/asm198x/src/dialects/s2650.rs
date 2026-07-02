//! The Signetics 2650 dialect front-end (asl syntax).
//!
//! Assembles against [`isa::s2650`] and produces a flat binary at the `org`.
//! Numbers are `$`-hex (shared with the 6800/6809 dialects). The 2650's
//! `mnemonic,suffix operand` shape glues a register (`r0`–`r3`) or condition
//! (`eq`/`gt`/`lt`/`un`) to the mnemonic with a comma; the operand follows.
//!
//! Emission is driven by the resolved spec form's operand kind:
//! - **no operand** (`Z` register forms, `NOP`, `LPSU`, …) → a fixed opcode;
//! - **immediate** (`LODI`, the mask ops `CPSU`/`TMI`, `REDE`) → opcode + byte;
//! - **relative** → the computed-operand seam: opcode + a **7-bit signed**
//!   displacement from the following instruction, bit 7 the indirect (`*`) flag;
//! - **absolute** → the seam: opcode + a **15-bit** big-endian address, bit 15
//!   indirect, bits 14-13 the index control (`,r3` / `,r3,+` / `,r3,-`).
//!   Indexing forces the register field to R3 and the operand register to R0.
//!
//! Special cases matching `asl`: `LODZ,R0` → `IORZ,R0` (`0x60`); `STRZ,R0` /
//! `ANDZ,R0` are illegal (the `NOP`/`HALT` slots); `BXA`/`BSXA` are aliases of
//! `BCFA,UN`/`BSFA,UN` (absolute); `ZBRR`/`ZBSR` share the `BCFR,UN`/`BSFR,UN`
//! opcodes but are page-zero relative (displacement = target). Validated
//! byte-identical against `asl` (`cpu 2650`).

use std::collections::BTreeMap;

use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Piece, Statement};

/// The Signetics 2650 dialect.
pub(crate) struct S2650;

impl Dialect for S2650 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::s2650::SET
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
    // The mnemonic may carry a comma-glued register/condition suffix.
    let (mnemonic, suffix) = match word.split_once(',') {
        Some((m, s)) => (m, Some(s)),
        None => (word, None),
    };
    let op = match mnemonic.to_ascii_lowercase().as_str() {
        "cpu" | "end" | "title" | "page" | "name" => return Ok(None),
        "org" => Operation::Org(value(args, line)?),
        "db" | "dc" | "byte" | "acon" => Operation::Bytes(byte_list(args, line)?),
        "dw" | "word" => Operation::Words(value_list(args, line)?),
        "ds" | "res" => parse_ds(args, line)?,
        _ => resolve(set, &mnemonic.to_ascii_uppercase(), suffix, args, line)?,
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
    mos6502::parse_expr(
        raw,
        line,
        parse_number,
        ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: Caret::Xor,
            at_is_pc: false,
        },
    )
}

/// Index control for absolute addressing (bits 14-13 of the address word).
#[derive(Clone, Copy)]
enum Index {
    Plain, // `,r3`     → 3
    Inc,   // `,r3,+`   → 1
    Dec,   // `,r3,-`   → 2
}

fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    suffix: Option<&str>,
    args: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    // ZBRR/ZBSR occupy the `BCFR,UN`/`BSFR,UN` opcodes (branch-on-false of the
    // always-true condition never branches, so the slot was repurposed) but with
    // *page-zero* relative addressing — the displacement is the target itself,
    // not a PC-relative offset. They are their own mnemonics, not aliases.
    match mn {
        "ZBRR" => return zero_relative(0x9B, args, line),
        "ZBSR" => return zero_relative(0xBB, args, line),
        _ => {}
    }
    // BXA/BSXA are the unconditional forms of BCFA/BSFA — identical absolute
    // encoding, so plain aliases.
    let (mn, suffix) = match mn {
        "BXA" => ("BCFA", Some("un")),
        "BSXA" => ("BSFA", Some("un")),
        _ => (mn, suffix),
    };

    let insn = set
        .instruction(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;

    // The register/condition selector comes from the comma suffix (with
    // condition aliases normalised: `z`/`p`/`n` and `0`-`3` → `eq`/`gt`/`lt`/
    // `un`), or — for the register forms `asl` also accepts space-separated
    // (`lodz r1`, `rrr r0`) — from a leading `r0`-`r3` operand token.
    let (mode, operand): (String, &str) = match suffix {
        Some(s) => (normalize_cc(&s.to_ascii_lowercase()), args),
        None => {
            let (first, rest) = split_first_word(args);
            let flow = first.to_ascii_lowercase();
            if matches!(flow.as_str(), "r0" | "r1" | "r2" | "r3") {
                (flow, rest)
            } else {
                (String::new(), args)
            }
        }
    };

    // LODZ,R0 is redundant; asl encodes it as IORZ,R0.
    if mn == "LODZ" && mode == "r0" {
        return Ok(Operation::Instruction {
            mnemonic: "IORZ".to_string(),
            mode: "r0",
            operands: vec![],
        });
    }

    let form = insn.form(&mode).ok_or_else(|| {
        if mode.is_empty() {
            AsmError::new(line, format!("`{mn}` needs a register or condition suffix"))
        } else {
            AsmError::new(line, format!("`{mn},{mode}` is not a valid form"))
        }
    })?;

    // Dispatch on the form's operand shape.
    match form.operands.first().map(|o| o.kind) {
        None => {
            if !operand.trim().is_empty() {
                return Err(AsmError::new(line, format!("`{mn}` takes no operand")));
            }
            Ok(Operation::Instruction {
                mnemonic: mn.to_string(),
                mode: form.mode,
                operands: vec![],
            })
        }
        Some(isa::OperandKind::Immediate) => Ok(Operation::Instruction {
            mnemonic: mn.to_string(),
            mode: form.mode,
            operands: vec![value(operand, line)?],
        }),
        Some(isa::OperandKind::RelativePc) => relative(form.opcode[0], operand, line),
        Some(isa::OperandKind::Address) => absolute(set, mn, &mode, form.opcode[0], operand, line),
        Some(_) => Err(AsmError::new(
            line,
            format!("`{mn}` operand kind unsupported"),
        )),
    }
}

/// Relative addressing: a 7-bit signed displacement from the following
/// instruction (`target - (pc + 2)`, range `-64..=63`), bit 7 the indirect flag.
fn relative(opcode: u8, args: &str, line: usize) -> Result<Operation, AsmError> {
    let (indirect, target) = strip_indirect(args);
    let target = value(target, line)?;
    // The raw displacement; the engine range-checks it before masking to 7 bits.
    let disp = Expr::Bin(
        BinOp::Sub,
        Box::new(target),
        Box::new(Expr::Bin(
            BinOp::Add,
            Box::new(Expr::Pc),
            Box::new(Expr::Num(2)),
        )),
    );
    Ok(Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Packed {
            expr: disp,
            bytes: 1,
            scale: 1,
            min: -64,
            max: 63,
            mask: 0x7F,
            or_bits: indirect_bit(indirect),
            what: "branch displacement",
        },
    ]))
}

/// Page-zero relative addressing (`ZBRR`/`ZBSR`): the displacement byte is the
/// target itself (base address 0), a 6-bit value `0..=63`, bit 7 the indirect
/// flag.
fn zero_relative(opcode: u8, args: &str, line: usize) -> Result<Operation, AsmError> {
    let (indirect, target) = strip_indirect(args);
    let target = value(target, line)?;
    Ok(Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Packed {
            expr: target,
            bytes: 1,
            scale: 1,
            min: 0,
            max: 63,
            mask: 0x7F,
            or_bits: indirect_bit(indirect),
            what: "ZBRR/ZBSR target",
        },
    ]))
}

/// The indirect flag as bit 7 of a relative displacement byte.
fn indirect_bit(indirect: bool) -> u32 {
    if indirect { 0x80 } else { 0 }
}

/// Absolute addressing: a 15-bit big-endian address, bit 15 indirect, bits
/// 14-13 the index control (which forces the register field to R3).
fn absolute(
    set: &'static isa::InstructionSet,
    mn: &str,
    mode: &str,
    opcode: u8,
    args: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let (indirect, rest) = strip_indirect(args);
    let parts = split_top_level(rest, ',');
    let (target, index) = parse_index(&parts, line)?;
    let target = value(target, line)?;

    // Indexing forces register 3 in the opcode and R0 as the operand register,
    // and is only valid for the memory-reference absolute ops (not branches).
    let (opcode, ctrl_bits) = match index {
        None => (opcode, 0u32),
        Some(ix) => {
            if !is_memref_abs(mn) {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` does not support indexed addressing"),
                ));
            }
            if mode != "r0" {
                return Err(AsmError::new(
                    line,
                    format!("`{mn}` indexed addressing requires register 0"),
                ));
            }
            let op3 = set
                .find_form(mn, "r3")
                .ok_or_else(|| AsmError::new(line, format!("`{mn}` has no indexed form")))?
                .opcode[0];
            let ctrl: u32 = match ix {
                Index::Inc => 1,
                Index::Dec => 2,
                Index::Plain => 3,
            };
            (op3, ctrl << 13)
        }
    };
    // The memory-reference ops carry a 13-bit direct address (the high two bits
    // are the index control); branches carry a full 15-bit address.
    let max: i64 = if is_memref_abs(mn) { 0x1FFF } else { 0x7FFF };
    let or_bits = ctrl_bits | if indirect { 0x8000 } else { 0 };
    Ok(Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Packed {
            expr: target,
            bytes: 2,
            scale: 1,
            min: 0,
            max,
            mask: max as u32,
            or_bits,
            what: "address",
        },
    ]))
}

/// The eight memory-reference absolute mnemonics — the only ones that support
/// `,r3` indexing (branches with an absolute target do not).
pub(crate) fn is_memref_abs(mn: &str) -> bool {
    matches!(
        mn,
        "LODA" | "STRA" | "ADDA" | "SUBA" | "ANDA" | "IORA" | "EORA" | "COMA"
    )
}

/// Normalise a condition-code suffix to its canonical form. `asl` accepts the
/// arithmetic aliases (`z`/`p`/`n`) and numeric codes (`0`-`3`) alongside
/// `eq`/`gt`/`lt`/`un`; register selectors (`r0`-`r3`) pass through unchanged.
fn normalize_cc(tok: &str) -> String {
    match tok {
        "z" | "0" => "eq",
        "p" | "1" => "gt",
        "n" | "2" => "lt",
        "3" => "un",
        other => other,
    }
    .to_string()
}

/// Strip a leading `*` indirect marker.
fn strip_indirect(args: &str) -> (bool, &str) {
    let t = args.trim();
    match t.strip_prefix('*') {
        Some(rest) => (true, rest.trim()),
        None => (false, t),
    }
}

/// Parse an optional `,r3` / `,r3,+` / `,r3,-` index suffix from the
/// comma-split absolute operand.
fn parse_index<'a>(parts: &[&'a str], line: usize) -> Result<(&'a str, Option<Index>), AsmError> {
    let bad_reg = || AsmError::new(line, "2650 index register must be r3");
    match parts.len() {
        1 => Ok((parts[0].trim(), None)),
        2 => {
            if !parts[1].trim().eq_ignore_ascii_case("r3") {
                return Err(bad_reg());
            }
            Ok((parts[0].trim(), Some(Index::Plain)))
        }
        3 => {
            if !parts[1].trim().eq_ignore_ascii_case("r3") {
                return Err(bad_reg());
            }
            let ix = match parts[2].trim() {
                "+" => Index::Inc,
                "-" => Index::Dec,
                _ => return Err(AsmError::new(line, "index auto-modify must be `+` or `-`")),
            };
            Ok((parts[0].trim(), Some(ix)))
        }
        _ => Err(AsmError::new(line, "malformed absolute operand")),
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_2650 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn register_and_immediate() {
        assert_eq!(bytes(" lodz r1\n"), vec![0x01]);
        assert_eq!(bytes(" lodi,r0 $42\n"), vec![0x04, 0x42]);
        assert_eq!(bytes(" addi,r0 $05\n"), vec![0x84, 0x05]);
        assert_eq!(bytes(" strz r3\n"), vec![0xC3]);
        assert_eq!(bytes(" iorz r0\n"), vec![0x60]);
        assert_eq!(bytes(" lodz r0\n"), vec![0x60]); // alias for IORZ,R0
        assert_eq!(bytes(" nop\n"), vec![0xC0]);
        assert_eq!(bytes(" halt\n"), vec![0x40]);
        assert_eq!(bytes(" cpsl $01\n"), vec![0x75, 0x01]);
        assert_eq!(bytes(" tmi,r0 $05\n"), vec![0xF4, 0x05]);
    }

    #[test]
    fn illegal_z_forms_rejected() {
        assert!(asm(" strz r0\n").is_err());
        assert!(asm(" andz r0\n").is_err());
    }

    #[test]
    fn absolute_and_indirect() {
        assert_eq!(bytes(" loda,r0 $1234\n"), vec![0x0C, 0x12, 0x34]);
        assert_eq!(bytes(" loda,r0 *$1234\n"), vec![0x0C, 0x92, 0x34]);
        assert_eq!(bytes(" bcta,un $1234\n"), vec![0x1F, 0x12, 0x34]);
    }

    #[test]
    fn absolute_indexed() {
        assert_eq!(bytes(" loda,r0 $1234,r3\n"), vec![0x0F, 0x72, 0x34]);
        assert_eq!(bytes(" adda,r0 $1234,r3,+\n"), vec![0x8F, 0x32, 0x34]);
        assert_eq!(bytes(" stra,r0 $1234,r3,-\n"), vec![0xCF, 0x52, 0x34]);
        assert!(asm(" loda,r1 $1234,r3\n").is_err()); // indexed needs r0
    }

    #[test]
    fn relative_signed_7bit() {
        // At org 0: target 8, base = pc+2 = 2, disp = 6.
        assert_eq!(bytes(" lodr,r0 $08\n"), vec![0x08, 0x06]);
        // Backward: branch to self, disp = 0 - 2 = -2 = 0x7E.
        assert_eq!(bytes(" org $10\nl: bctr,un l\n"), vec![0x1B, 0x7E]);
        // Indirect relative sets bit 7.
        assert_eq!(bytes(" lodr,r0 *$08\n"), vec![0x08, 0x86]);
    }

    #[test]
    fn out_of_range_operands_error() {
        // Relative displacement is 7-bit signed (-64..=63 from the following
        // instruction). At org $0100 the base is $0102.
        assert!(asm(" org $0100\n bctr,un $0141\n").is_ok()); // +63
        assert!(asm(" org $0100\n bctr,un $0142\n").is_err()); // +64
        assert!(asm(" org $0100\n bctr,un $00c2\n").is_ok()); // -64
        assert!(asm(" org $0100\n bctr,un $00c1\n").is_err()); // -65
        // ZBRR page-0 target is 0..=63.
        assert!(asm(" zbrr $3f\n").is_ok());
        assert!(asm(" zbrr $40\n").is_err());
        // Memory-reference absolute is a 13-bit direct address; branches 15-bit.
        assert!(asm(" loda,r0 $1fff\n").is_ok());
        assert!(asm(" loda,r0 $2000\n").is_err());
        assert!(asm(" bcta,un $7fff\n").is_ok());
        assert!(asm(" bcta,un $8000\n").is_err());
    }

    #[test]
    fn zero_relative_and_indexed_branch_aliases() {
        // ZBRR/ZBSR are page-0 relative: the displacement is the target itself.
        assert_eq!(bytes(" zbrr $00\n"), vec![0x9B, 0x00]);
        assert_eq!(bytes(" zbrr $10\n"), vec![0x9B, 0x10]);
        assert_eq!(bytes(" zbrr *$10\n"), vec![0x9B, 0x90]);
        assert_eq!(bytes(" zbsr $3f\n"), vec![0xBB, 0x3F]);
        // BXA/BSXA are absolute (identical to BCFA,UN / BSFA,UN).
        assert_eq!(bytes(" bxa $1234\n"), vec![0x9F, 0x12, 0x34]);
        assert_eq!(bytes(" bsxa $1234\n"), vec![0xBF, 0x12, 0x34]);
    }
}
