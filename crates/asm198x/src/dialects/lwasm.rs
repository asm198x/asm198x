//! The lwasm 6809 dialect front-end.
//!
//! lwasm (part of LWTOOLS) is the de-facto modern 6809 assembler. There is no
//! 6809 curriculum yet, so this dialect is validated byte-for-byte against
//! `lwasm --6809 --raw` directly rather than against a curriculum corpus.
//!
//! The 6809 is the first CPU whose operands are not fixed-width slots: indexed
//! addressing carries a *computed postbyte* plus 0/1/2 extension bytes. So this
//! dialect does not hand the engine an `Operation::Instruction` to encode from a
//! form; it computes the bytes itself into [`Operation::Encoded`] pieces and
//! reuses the engine only for the two-pass driver, the symbol table, `org`, and
//! `equ`. Encoding facts come from [`isa::mos6809`]. The 6809 is big-endian.
//!
//! Covered so far: inherent, immediate, direct, extended, and relative
//! (short + long) addressing, plus `org`/`equ`/`fcb`/`fdb`/`rmb`. Indexed
//! addressing (the postbyte) and the register-list ops (`tfr`/`exg`/`pshs`/
//! `puls`) are the next increment.

use std::collections::BTreeMap;

use isa::mos6809::{self, Kind};

use super::mos6502::{self, BytePrec, fold_const, split_first_word};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};

/// The lwasm 6809 dialect.
pub(crate) struct Lwasm;

impl Dialect for Lwasm {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        // The engine consults this only for byte order (the 6809 computes its own
        // encoding into `Encoded` pieces); 6809 is big-endian.
        &isa::mos6809::INSTRUCTION_SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let mut out = Vec::new();
        let mut env: BTreeMap<String, i64> = BTreeMap::new();
        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            let code = strip_comment(raw);
            if code.trim().is_empty() {
                continue;
            }
            let (label, rest) = split_label(code);
            let op = if rest.is_empty() {
                None
            } else {
                parse_op(rest, &env, line)?
            };
            // Bind an `equ` value into the parse-time env so a later direct/
            // extended choice can fold it (mirrors the engine's pass-1 `equ`).
            if let (Some(name), Some(Operation::Equ(e))) = (&label, &op)
                && let Ok(v) = fold_const(e, &env, line)
            {
                env.insert(name.clone(), v);
            }
            if label.is_some() || op.is_some() {
                out.push(Statement { line, label, op });
            }
        }
        Ok(out)
    }
}

/// Strip a comment: a `*` as the first non-blank character makes the whole line
/// a comment (lwasm convention), and a `;` outside a string starts one anywhere.
fn strip_comment(line: &str) -> &str {
    if line.trim_start().starts_with('*') {
        return "";
    }
    let bytes = line.as_bytes();
    let mut in_str = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_str = !in_str,
            b';' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Split a column-0 label from the rest of the line. A line beginning with
/// whitespace has no label; otherwise the first token is the label (an optional
/// trailing `:` is dropped), and the remainder is the opcode + operand.
fn split_label(code: &str) -> (Option<String>, &str) {
    if code.starts_with([' ', '\t']) {
        return (None, code.trim());
    }
    let (word, remainder) = split_first_word(code.trim());
    let name = word.strip_suffix(':').unwrap_or(word);
    (Some(name.to_string()), remainder)
}

/// Parse the operation part (after any label): a pseudo-op or an instruction.
fn parse_op(
    rest: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (mnem, operand) = split_first_word(rest);
    let m = mnem.to_ascii_lowercase();
    match m.as_str() {
        "org" => Ok(Some(Operation::Org(value(operand, line)?))),
        "equ" => Ok(Some(Operation::Equ(value(operand, line)?))),
        "fcb" | ".byte" => Ok(Some(Operation::Bytes(list(operand, line)?))),
        "fdb" | ".word" => Ok(Some(Operation::Words(list(operand, line)?))),
        "rmb" | ".ds" => parse_rmb(operand, env, line),
        "end" => Ok(None), // marks the end of source; emits nothing
        _ => Ok(Some(parse_instruction(&m, operand, env, line)?)),
    }
}

/// `rmb count` — reserve `count` bytes, zero-filled (the flat-output behaviour).
/// `count` folds against the parse-time env so the size is known in pass one.
fn parse_rmb(
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let n = fold_const(&value(operand, line)?, env, line)?;
    let n = usize::try_from(n)
        .map_err(|_| AsmError::new(line, "`rmb` count must be a non-negative constant"))?;
    Ok(Some(Operation::Bytes(vec![Expr::Num(0); n])))
}

/// Encode one instruction into `Operation::Encoded` pieces.
fn parse_instruction(
    m: &str,
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    if let Some(insn) = mos6809::lookup(m) {
        match &insn.kind {
            Kind::Inherent(opcode) => encode_inherent(m, opcode, operand, line),
            Kind::Branch { short, .. } => encode_branch(short, 1, operand, line),
            Kind::Mem {
                imm,
                direct,
                indexed,
                extended,
                width,
            } => encode_mem(m, imm, direct, indexed, extended, *width, operand, env, line),
        }
    } else if let Some(stripped) = m.strip_prefix('l')
        && let Some(insn) = mos6809::lookup(stripped)
        && let Kind::Branch { long, .. } = &insn.kind
    {
        // `lbra`/`lbeq`/… are the long forms of the branch with their `l`
        // dropped; no `Mem`/inherent mnemonic's tail is itself a branch, so this
        // is unambiguous. The long displacement is 16-bit.
        encode_branch(long, 2, operand, line)
    } else {
        Err(AsmError::new(line, format!("unknown instruction `{m}`")))
    }
}

fn encode_inherent(
    m: &str,
    opcode: &[u8],
    operand: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    if !operand.trim().is_empty() {
        return Err(AsmError::new(line, format!("`{m}` takes no operand")));
    }
    Ok(Operation::Encoded(opcode.iter().map(|b| Piece::Lit(*b)).collect()))
}

/// A short (`bytes == 1`) or long (`bytes == 2`) PC-relative branch. The engine
/// turns the target into an offset from the following instruction.
fn encode_branch(
    opcode: &[u8],
    bytes: u8,
    operand: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let target = value(operand, line)?;
    let mut pieces: Vec<Piece> = opcode.iter().map(|b| Piece::Lit(*b)).collect();
    pieces.push(Piece::Val {
        expr: target,
        bytes,
        rel: true,
        signed: false,
    });
    Ok(Operation::Encoded(pieces))
}

/// Encode a register/memory instruction, choosing the addressing mode from the
/// operand syntax. Indexed addressing is a later increment.
#[allow(clippy::too_many_arguments)]
fn encode_mem(
    m: &str,
    imm: &[u8],
    direct: &[u8],
    indexed: &[u8],
    extended: &[u8],
    width: u8,
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let t = operand.trim();
    if t.is_empty() {
        return Err(AsmError::new(line, format!("`{m}` requires an operand")));
    }
    // Indexed addressing (`,R` / `n,R` / `[...]`) computes a postbyte — not yet.
    let _ = indexed;
    if t.starts_with('[') || mos6502::top_level_rfind(t, ',').is_some() {
        return Err(AsmError::new(
            line,
            format!("`{m}`: 6809 indexed addressing is not yet supported"),
        ));
    }
    if let Some(rest) = t.strip_prefix('#') {
        if imm.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no immediate mode")));
        }
        return Ok(encoded(imm, value(rest, line)?, width));
    }
    if let Some(rest) = t.strip_prefix('<') {
        if direct.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no direct mode")));
        }
        return Ok(encoded(direct, value(rest, line)?, 1));
    }
    if let Some(rest) = t.strip_prefix('>') {
        if extended.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no extended mode")));
        }
        return Ok(encoded(extended, value(rest, line)?, 2));
    }
    // Bare address: direct when it folds to a constant that fits in a byte and a
    // direct mode exists; otherwise extended. A forward symbol stays extended,
    // keeping the size stable across passes — lwasm's default.
    let e = value(t, line)?;
    let fits_direct =
        !direct.is_empty() && fold_const(&e, env, line).is_ok_and(|v| (0..=0xFF).contains(&v));
    if fits_direct {
        Ok(encoded(direct, e, 1))
    } else if !extended.is_empty() {
        Ok(encoded(extended, e, 2))
    } else {
        Err(AsmError::new(line, format!("`{m}` has no addressing mode for `{t}`")))
    }
}

/// Build an `Encoded` operation: the opcode literal bytes, then one unsigned
/// value of `width` bytes (an immediate, direct offset, or extended address).
fn encoded(opcode: &[u8], expr: Expr, width: u8) -> Operation {
    let mut pieces: Vec<Piece> = opcode.iter().map(|b| Piece::Lit(*b)).collect();
    pieces.push(Piece::Val {
        expr,
        bytes: width,
        rel: false,
        signed: false,
    });
    Operation::Encoded(pieces)
}

/// Parse a comma-separated list of value expressions (for `fcb`/`fdb`).
fn list(operand: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if operand.trim().is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    mos6502::split_top_level(operand, ',')
        .iter()
        .map(|p| value(p, line))
        .collect()
}

/// Parse one 6809 value expression. `$hex`/`%bin`/decimal numbers, symbols, `*`
/// for the location counter, and the bitwise/shift operators — reusing the
/// shared 6502 expression core. The `<`/`>` direct/extended forces are stripped
/// by the caller before this, so they never reach the byte-prefix paths.
fn value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    mos6502::parse_expr_opts(raw, line, mos6502::parse_number, BytePrec::Tight, true)
}

#[cfg(test)]
mod tests {
    use crate::assemble_lwasm as asm;

    #[test]
    fn inherent_and_immediate() {
        assert_eq!(asm("        nop\n").expect("nop").bytes, vec![0x12]);
        assert_eq!(asm("        rts\n").expect("rts").bytes, vec![0x39]);
        assert_eq!(asm("        lda #$42\n").expect("imm").bytes, vec![0x86, 0x42]);
        // 16-bit immediate.
        assert_eq!(
            asm("        ldx #$1234\n").expect("ldx").bytes,
            vec![0x8E, 0x12, 0x34]
        );
    }

    #[test]
    fn direct_and_extended_selection() {
        // Low constant -> direct; high constant -> extended.
        assert_eq!(asm("        lda $20\n").expect("dir").bytes, vec![0x96, 0x20]);
        assert_eq!(
            asm("        lda $1234\n").expect("ext").bytes,
            vec![0xB6, 0x12, 0x34]
        );
        // Forces: `<` direct, `>` extended.
        assert_eq!(
            asm("        lda <$20\n").expect("force dir").bytes,
            vec![0x96, 0x20]
        );
        assert_eq!(
            asm("        lda >$20\n").expect("force ext").bytes,
            vec![0xB6, 0x00, 0x20]
        );
    }

    #[test]
    fn big_endian_data() {
        assert_eq!(asm("        fcb $01,$02\n").expect("fcb").bytes, vec![0x01, 0x02]);
        // fdb is big-endian.
        assert_eq!(
            asm("        fdb $1234\n").expect("fdb").bytes,
            vec![0x12, 0x34]
        );
    }

    #[test]
    fn short_and_long_branches() {
        // bra to self+2 with a backward loop: org so the target resolves.
        let a = asm("        org $1000\nloop    bra loop\n").expect("bra");
        // bra opcode 0x20, offset = loop - (pc+2) = -2 = 0xFE.
        assert_eq!(a.bytes, vec![0x20, 0xFE]);
        let a = asm("        org $1000\nloop    lbra loop\n").expect("lbra");
        // lbra opcode 0x16, 16-bit offset = -3 = 0xFFFD.
        assert_eq!(a.bytes, vec![0x16, 0xFF, 0xFD]);
        // A conditional long branch is 0x10-prefixed.
        let a = asm("        org $1000\nloop    lbeq loop\n").expect("lbeq");
        assert_eq!(a.bytes, vec![0x10, 0x27, 0xFF, 0xFC]);
    }

    #[test]
    fn labels_and_org() {
        let a = asm("        org $2000\nstart   lda #$00\n        sta $1000\n        rts\n")
            .expect("prog");
        assert_eq!(a.origin, 0x2000);
        assert_eq!(a.bytes, vec![0x86, 0x00, 0xB7, 0x10, 0x00, 0x39]);
        assert_eq!(a.symbols.get("start"), Some(&0x2000));
    }
}
