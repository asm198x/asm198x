//! The Fairchild F8 (3850) dialect front-end (asl syntax).
//!
//! Assembles against [`isa::f8`] and produces a flat binary at the `org`.
//! Numbers are Intel `H`-suffix hex (shared with the 8080 dialect via
//! [`super::i8080::parse_number_intel`]), matching `asl`'s F8 mode. Operand
//! resolution dispatches on the mnemonic:
//!
//! - **branches** (`BT`/`BF` with a mask, the named `BR`/`BP`/`BC`/`BZ`/`BM`/
//!   `BNC`/`BNZ`/`BNO`, and `BR7`) go through the **computed-operand seam**
//!   ([`Operation::Encoded`]). The F8 measures the signed offset from the
//!   address of the offset byte itself — one past the opcode — so the offset is
//!   emitted as `target + 1` with the engine's `rel` (end-of-instruction) base,
//!   which nets to the F8 base. The opcode is a `Lit`; no engine change;
//! - **`LR`** builds the spec's `dest,src` mode label from the two register
//!   operands (`S`/`I`/`D` fold to 12/13/14 for the scratchpad positions);
//! - the **scratchpad ops** (`DS`/`AS`/`ASD`/`XS`/`NS`) take one register
//!   (0–15, `S`/`I`/`D`) selecting mode `"0"`..`"15"`;
//! - the **immediate-nibble** loads/ports (`LIS`/`INS`/`OUTS` 0–15,
//!   `LISU`/`LISL` 0–7) pack a value into the opcode via the mode label;
//! - `LI`/…/`CI` and `IN`/`OUT` take an immediate byte; `PI`/`JMP`/`DCI` a
//!   16-bit big-endian address; `SR`/`SL` an optional shift count (1 or 4);
//! - `CLR` is an alias for `LIS 0` (opcode `0x70`);
//! - everything else is inherent (mode `""`).
//!
//! Output is validated byte-identical against `asl` (`cpu F3850`).

use std::collections::BTreeMap;

use super::asl::{self, AslChip};
use super::i8080::parse_number_intel;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Piece, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The Fairchild F8 (3850) dialect.
pub(crate) struct F8;

impl Dialect for F8 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::f8::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (0b straggler migration): parse
        // into a `Program`, then lower to the engine's statement stream —
        // byte-identical to the old direct parse (AE1).
        crate::ast::lower(parse_program(source)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source)?))
    }

    /// The include-capable parse (language-surface U4): the shared asl-family
    /// walk, resolving `include`/`binclude` lazily through the loader — see
    /// [`parse_program_multi`].
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        crate::ast::lower(parse_program_multi(map, loader)?)
    }

    /// Intel-style `equ` takes no colon on its label (`name equ …`); the keyword
    /// disambiguates it, and a colon would fail to reassemble.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse Fairchild-F8 source into the semantic [`Program`](crate::ast::Program)
/// via the shared asl-family walk ([`asl::parse_single`]): each line becomes
/// a node with its (global) label, operation, verbatim source, span, and
/// comment trivia — [`lower`](crate::ast::lower) reproduces the old
/// statements exactly. An `include`/`binclude` stays an unresolved item — the
/// target is never opened here (U4, KTD1).
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    asl::parse_single(Chip, source)
}

/// Parse a multi-file Fairchild-F8 program (language-surface U4): the shared
/// asl-family interleaved walk with asl's probe-pinned semantics — see
/// [`asl::parse_multi_files`].
///
/// # Errors
/// Any per-line parse failure (stamped with its file), a missing target, an
/// include cycle, a bad `binclude` window, or the depth backstop — all at the
/// directive's span.
pub(crate) fn parse_program_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<crate::ast::Program, AsmError> {
    asl::parse_multi_files(Chip, map, loader, &asl::SEMANTICS)
}

/// The Fairchild-F8's hooks into the shared asl-family walk (its own comment
/// scanner, constant recogniser, label split, number lexer, and operation
/// parse).
struct Chip;

impl AslChip for Chip {
    fn split_comment<'a>(&self, line: &'a str) -> (&'a str, Option<&'a str>) {
        split_comment(line)
    }

    fn constant(
        &self,
        code: &str,
        line: usize,
    ) -> Result<Option<(String, Expr, String)>, AsmError> {
        constant(code, line)
    }

    fn split_label<'a>(&self, code: &'a str) -> (Option<String>, &'a str) {
        split_label(code)
    }

    fn parse_op(
        &mut self,
        rest: &str,
        consts: &BTreeMap<String, i64>,
        line: usize,
    ) -> Result<Option<Operation>, AsmError> {
        parse_op(&isa::f8::SET, rest, consts, line)
    }

    fn value(&self, raw: &str, line: usize) -> Result<Expr, AsmError> {
        value(raw, line)
    }
}

/// Split a line into its code and its `;` comment (leading `;` and whitespace
/// trimmed) for carrying comments as AST trivia. Defined via [`strip_comment`] so
/// the comment is exactly what it removes — no behaviour change to assembly.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
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

/// `NAME EQU expr` or `NAME = expr`. Returns the name, the value expression, and
/// the operation's source text (`EQU expr` / `= expr`) so the formatter can
/// re-emit `NAME <source>` with the label kept on the same line.
fn constant(code: &str, line: usize) -> Result<Option<(String, Expr, String)>, AsmError> {
    let (first, rest) = split_first_word(code);
    if !rest.is_empty() {
        let (kw, tail) = split_first_word(rest);
        if kw.eq_ignore_ascii_case("equ") && is_ident(first) {
            return Ok(Some((
                first.to_string(),
                value(tail, line)?,
                rest.trim().to_string(),
            )));
        }
    }
    if let Some(eq) = mos6502::assignment_split(code) {
        let name = code[..eq].trim();
        if is_ident(name) {
            return Ok(Some((
                name.to_string(),
                value(code[eq + 1..].trim(), line)?,
                code[eq..].trim().to_string(),
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
        // `ds` is the F8 "decrement scratchpad" mnemonic (matching `asl`), so
        // reserve space with `rmb`/`dfs`, not `ds`.
        "rmb" | "dfs" => parse_ds(args, consts, line)?,
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
        parse_number_intel,
        ExprOpts {
            bang_is_or: false,
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: Caret::Xor,
            at_is_pc: false,
        },
    )
}

/// A `+1` correction on a branch target: the F8 measures the signed offset from
/// the offset byte, one before the end-of-instruction base the engine's `rel`
/// uses. Adding 1 to the target nets the two out.
fn target_plus_one(target: Expr) -> Expr {
    Expr::Bin(BinOp::Add, Box::new(target), Box::new(Expr::Num(1)))
}

/// Emit a branch: the fixed opcode byte, then the signed offset via the seam.
fn branch(opcode: u8, target: Expr) -> Operation {
    Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Val {
            expr: target_plus_one(target),
            bytes: 1,
            rel: true,
            signed: true,
        },
    ])
}

/// Map a named-branch mnemonic to its `(spec mnemonic, mode)`; `None` for the
/// generic `BT`/`BF` (which carry an explicit mask) and non-branches.
fn named_branch(mn: &str) -> Option<(&'static str, &'static str)> {
    Some(match mn {
        "BR" => ("BF", "0"),
        "BM" => ("BF", "1"),
        "BNC" => ("BF", "2"),
        "BNZ" => ("BF", "4"),
        "BNO" => ("BF", "8"),
        "BP" => ("BT", "1"),
        "BC" => ("BT", "2"),
        "BZ" => ("BT", "4"),
        "BR7" => ("BR7", ""),
        _ => return None,
    })
}

/// Resolve a scratchpad register operand to a mode label: `S`/`I`/`D` fold to
/// 12/13/14, otherwise a constant 0..15.
fn reg_mode(tok: &str, consts: &BTreeMap<String, i64>, line: usize) -> Result<String, AsmError> {
    Ok(match tok.trim().to_ascii_lowercase().as_str() {
        "s" => "12".to_string(),
        "i" => "13".to_string(),
        "d" => "14".to_string(),
        _ => {
            let n = fold_const(&value(tok, line)?, consts, line)?;
            if !(0..=15).contains(&n) {
                return Err(AsmError::new(line, "scratchpad register must be 0..15"));
            }
            n.to_string()
        }
    })
}

/// Resolve one side of an `LR` operand pair to its label token: a fixed register
/// name passes through, `S`/`I`/`D` fold to 12/13/14, else a constant 0..15.
fn lr_token(tok: &str, consts: &BTreeMap<String, i64>, line: usize) -> Result<String, AsmError> {
    let t = tok.trim().to_ascii_lowercase();
    if is_lr_reg(&t) {
        return Ok(t);
    }
    reg_mode(&t, consts, line)
}

/// The fixed `LR` register names (everything that is not a scratchpad number).
fn is_lr_reg(s: &str) -> bool {
    matches!(
        s,
        "a" | "ku" | "kl" | "qu" | "ql" | "k" | "p" | "is" | "p0" | "q" | "dc" | "h" | "w" | "j"
    )
}

/// A value-nibble mnemonic (`LIS`/`INS`/`OUTS` 0..15, `LISU`/`LISL` 0..7):
/// resolve the operand to a mode label in range.
fn value_nibble(
    args: &str,
    hi: i64,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<String, AsmError> {
    let n = fold_const(&value(args.trim(), line)?, consts, line)?;
    if !(0..=hi).contains(&n) {
        return Err(AsmError::new(line, format!("value must be 0..{hi}")));
    }
    Ok(n.to_string())
}

/// Resolve an instruction by its mnemonic and operand syntax.
fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let t = args.trim();
    let instr = |mode: &'static str, operands: Vec<Expr>| Operation::Instruction {
        mnemonic: mn.to_string(),
        mode,
        operands,
    };
    // Look up a form's opcode from the spec (single-sourcing the branch bytes).
    let opcode = |m: &str, mode: &str| -> Result<u8, AsmError> {
        set.find_form(m, mode)
            .map(|f| f.opcode[0])
            .ok_or_else(|| AsmError::new(line, format!("`{m}` has no `{mode}` form")))
    };

    // --- branches (computed-operand seam) ---
    if let Some((spec_mn, mode)) = named_branch(mn) {
        let op = opcode(spec_mn, mode)?;
        return Ok(branch(op, value(t, line)?));
    }
    if mn == "BT" || mn == "BF" {
        let (mask, tgt) = t
            .split_once(',')
            .ok_or_else(|| AsmError::new(line, format!("`{mn}` needs a mask and a target")))?;
        let mode = value_nibble(mask, if mn == "BT" { 7 } else { 15 }, consts, line)?;
        let op = opcode(mn, &mode)?;
        return Ok(branch(op, value(tgt, line)?));
    }

    // --- `CLR` alias for `LIS 0` ---
    if mn == "CLR" {
        return Ok(Operation::Instruction {
            mnemonic: "LIS".to_string(),
            mode: "0",
            operands: vec![],
        });
    }

    // --- LR: build the dest,src mode label ---
    if mn == "LR" {
        let (d, s) = t
            .split_once(',')
            .ok_or_else(|| AsmError::new(line, "`LR` needs two registers"))?;
        let label = format!(
            "{},{}",
            lr_token(d, consts, line)?,
            lr_token(s, consts, line)?
        );
        let form = set
            .find_form("LR", &label)
            .ok_or_else(|| AsmError::new(line, format!("`LR {t}` is not a valid register pair")))?;
        return Ok(instr(form.mode, vec![]));
    }

    // --- scratchpad ops: one register operand ---
    if matches!(mn, "DS" | "AS" | "ASD" | "XS" | "NS") {
        let mode = reg_mode(t, consts, line)?;
        let form = set
            .find_form(mn, &mode)
            .ok_or_else(|| AsmError::new(line, format!("`{mn}` has no `{mode}` form")))?;
        return Ok(instr(form.mode, vec![]));
    }

    // --- shifts: optional count, 1 (default) or 4 ---
    if matches!(mn, "SR" | "SL") {
        let mode: &'static str = if t.is_empty() {
            "1"
        } else {
            match fold_const(&value(t, line)?, consts, line)? {
                1 => "1",
                4 => "4",
                _ => return Err(AsmError::new(line, "shift count must be 1 or 4")),
            }
        };
        return Ok(instr(mode, vec![]));
    }

    // --- immediate-nibble loads / ports (mode is in range, so the form exists) ---
    if matches!(mn, "LIS" | "INS" | "OUTS" | "LISU" | "LISL") {
        let hi = if matches!(mn, "LISU" | "LISL") { 7 } else { 15 };
        let mode = value_nibble(t, hi, consts, line)?;
        let form = set
            .find_form(mn, &mode)
            .ok_or_else(|| AsmError::new(line, format!("`{mn}` has no `{mode}` form")))?;
        return Ok(instr(form.mode, vec![]));
    }

    // --- immediate byte / port / 16-bit address ---
    if matches!(mn, "LI" | "NI" | "OI" | "XI" | "AI" | "CI") {
        return Ok(instr("imm", vec![value(t, line)?]));
    }
    if matches!(mn, "IN" | "OUT") {
        return Ok(instr("port", vec![value(t, line)?]));
    }
    if matches!(mn, "PI" | "JMP" | "DCI") {
        return Ok(instr("abs", vec![value(t, line)?]));
    }

    // --- inherent (no operand) ---
    let insn = set
        .instruction(mn)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mn}`")))?;
    match insn.form("") {
        Some(f) if f.operands.is_empty() => Ok(instr(f.mode, vec![])),
        _ => Err(AsmError::new(line, format!("`{mn}` requires an operand"))),
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_f8 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn registers_and_scratchpad() {
        assert_eq!(bytes(" lr a,ku\n"), vec![0x00]);
        assert_eq!(bytes(" lr h,dc\n"), vec![0x11]);
        assert_eq!(bytes(" lr a,3\n"), vec![0x43]);
        assert_eq!(bytes(" lr a,s\n"), vec![0x4C]);
        assert_eq!(bytes(" lr 12,a\n"), vec![0x5C]);
        assert_eq!(bytes(" as 1\n"), vec![0xC1]);
        assert_eq!(bytes(" ns d\n"), vec![0xFE]);
        assert_eq!(bytes(" ds 15\n"), vec![0x3F]);
    }

    #[test]
    fn immediates_and_nibbles() {
        assert_eq!(bytes(" li 42h\n"), vec![0x20, 0x42]);
        assert_eq!(bytes(" ni 0fh\n"), vec![0x21, 0x0F]);
        assert_eq!(bytes(" in 10h\n"), vec![0x26, 0x10]);
        assert_eq!(bytes(" lis 5\n"), vec![0x75]);
        assert_eq!(bytes(" clr\n"), vec![0x70]);
        assert_eq!(bytes(" lisu 3\n"), vec![0x63]);
        assert_eq!(bytes(" ins 4\n"), vec![0xA4]);
        assert_eq!(bytes(" sl 4\n"), vec![0x15]);
        assert_eq!(bytes(" sr\n"), vec![0x12]);
    }

    #[test]
    fn absolute_is_big_endian() {
        assert_eq!(bytes(" dci 1234h\n"), vec![0x2A, 0x12, 0x34]);
        assert_eq!(bytes(" jmp 1234h\n"), vec![0x29, 0x12, 0x34]);
        assert_eq!(bytes(" pi 1234h\n"), vec![0x28, 0x12, 0x34]);
    }

    #[test]
    fn branches_offset_from_the_offset_byte() {
        // BR at org 0 to target 8: offset = 8 - 1 = 7.
        assert_eq!(bytes(" br 8\n"), vec![0x90, 0x07]);
        assert_eq!(bytes(" bt 1,8\n"), vec![0x81, 0x07]);
        assert_eq!(bytes(" bf 6,8\n"), vec![0x96, 0x07]);
        assert_eq!(bytes(" bp 8\n"), vec![0x81, 0x07]);
        assert_eq!(bytes(" bnz 8\n"), vec![0x94, 0x07]);
        assert_eq!(bytes(" br7 8\n"), vec![0x8F, 0x07]);
        // Branch to self at org 10h: offset = 10h - 11h = -1 = 0FFh.
        assert_eq!(bytes(" org 10h\nl: br l\n"), vec![0x90, 0xFF]);
    }

    #[test]
    fn forward_branch_label() {
        // BR at 0 (2 bytes) then two NOPs; `there` is at 4, offset = 4 - 1 = 3.
        assert_eq!(
            bytes(" org 0\n br there\n nop\n nop\nthere: nop\n"),
            vec![0x90, 0x03, 0x2B, 0x2B, 0x2B]
        );
    }
}
