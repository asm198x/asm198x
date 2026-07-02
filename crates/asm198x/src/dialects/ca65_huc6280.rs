//! The ca65 HuC6280 dialect front-end (PC Engine / TurboGrafx-16 CPU).
//!
//! The HuC6280 is a 65C02 superset, so this dialect assembles against the 6502
//! [`isa`] spec **plus** the [`isa::huc6280`] extension set — the same
//! primary-plus-extension mechanism the 65816 (`ca65_816`) and Z80N use. The
//! extension carries the 65C02 additions the chip inherits, the Rockwell bit
//! ops, and the HuC6280-specific instructions; the base 6502 set supplies the
//! rest. Encoding and the two-pass driver are the engine's; only ca65's surface
//! (directives, labels, operand syntax) lives here.
//!
//! The 6502 expression and lexer helpers are shared from [`super::mos6502`], and
//! the ordinary addressing modes reuse its operand parser. What is HuC6280-only
//! and handled here: the `bbr`/`bbs` `zeropage,relative` two-operand branch, the
//! `tst #imm, <mem>` test, and the block transfers `tii`/`tdd`/`tin`/`tia`/`tai`
//! (opcode + three 16-bit little-endian words). Mode resolution is spec-driven:
//! an ambiguous `(expr)` is settled by asking whether the mnemonic has a `jmp`
//! `indirect` form or the `(dp)` `(indirect)` form, never a hardcoded list.
//!
//! Output is a flat little-endian binary, validated byte-identical against
//! `ca65 --cpu huc6280`. There is no PC Engine curriculum yet, so (as with
//! 6809/lwasm and the 65816) the reference is the tool directly.

use std::collections::BTreeMap;

use super::mos6502::{
    self, BytePrec, Index, OperandSyntax, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, split_top_level, string_literal, top_level_rfind,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};

/// The ca65 HuC6280 dialect.
pub(crate) struct Ca65Huc6280;

impl Dialect for Ca65Huc6280 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::mos6502::SET
    }

    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        Some(&isa::huc6280::SET)
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let prim = self.instruction_set();
        let ext = &isa::huc6280::SET;
        let mut out = Vec::new();
        let mut env: BTreeMap<String, i64> = BTreeMap::new();

        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            let code = strip_comment(raw);
            if code.trim().is_empty() {
                continue;
            }
            // `name = expr` binds a named constant (a lone `=`, not a comparison).
            if let Some(eq) = mos6502::assignment_split(code.trim()) {
                let trimmed = code.trim();
                let name = trimmed[..eq].trim();
                if !is_ident(name) {
                    return Err(AsmError::new(line, format!("invalid symbol `{name}`")));
                }
                let e = value(trimmed[eq + 1..].trim(), line)?;
                if let Ok(v) = fold_const(&e, &env, line) {
                    env.insert(name.to_string(), v);
                }
                out.push(Statement {
                    line,
                    label: Some(name.to_string()),
                    op: Some(Operation::Equ(e)),
                });
                continue;
            }
            let (label, rest) = split_label(code);
            let op = if rest.is_empty() {
                None
            } else {
                parse_op(prim, ext, rest, &env, line)?
            };
            if label.is_some() || op.is_some() {
                out.push(Statement { line, label, op });
            }
        }
        Ok(out)
    }
}

/// Strip a `;` comment, ignoring `;` inside a `'c'` char or `"..."` string.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let (mut in_char, mut in_str) = (false, false);
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b';' if !in_char && !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Split a `label:` from the rest of the line. ca65 labels require a trailing
/// colon, so a colon-less column-0 word is the instruction or directive.
fn split_label(code: &str) -> (Option<String>, &str) {
    if code.starts_with([' ', '\t']) {
        return (None, code.trim());
    }
    let trimmed = code.trim();
    let (word, remainder) = split_first_word(trimmed);
    match word.strip_suffix(':') {
        Some(name) if is_ident(name) => (Some(name.to_string()), remainder),
        _ => (None, trimmed),
    }
}

/// Parse the operation part of a line: a `.directive` or an instruction.
fn parse_op(
    prim: &'static isa::InstructionSet,
    ext: &'static isa::InstructionSet,
    rest: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    if let Some(dir) = rest.strip_prefix('.') {
        return parse_directive(dir, env, line);
    }
    let (mnemonic, operand) = split_first_word(rest);
    let mn = mnemonic.to_ascii_uppercase();
    let (mode, exprs) = resolve(prim, ext, &mn, operand, env, line)?;
    Ok(Some(Operation::Instruction {
        mnemonic: mn,
        mode,
        operands: exprs,
    }))
}

/// Parse a `.directive`. The little-endian directive semantics match `ca65_816`.
fn parse_directive(
    dir: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (name, rest) = split_first_word(dir);
    match name.to_ascii_lowercase().as_str() {
        "setcpu" | "segment" | "smart" => Ok(None),
        "org" => Ok(Some(Operation::Org(value(rest, line)?))),
        "byte" | "byt" => Ok(Some(Operation::Bytes(byte_list(rest, line)?))),
        "word" | "addr" => Ok(Some(Operation::Words(value_list(rest, line)?))),
        // `.dword` — 32-bit little-endian, as computed pieces so symbols resolve
        // in pass two.
        "dword" => Ok(Some(Operation::Encoded(
            value_list(rest, line)?
                .into_iter()
                .map(|expr| Piece::Val {
                    expr,
                    bytes: 4,
                    rel: false,
                    signed: false,
                })
                .collect(),
        ))),
        // `.dbyt` — 16-bit big-endian (high byte first), independent of the CPU's
        // little-endianness.
        "dbyt" => Ok(Some(Operation::Bytes(
            value_list(rest, line)?
                .into_iter()
                .flat_map(|e| [Expr::Hi(Box::new(e.clone())), Expr::Lo(Box::new(e))])
                .collect(),
        ))),
        "asciiz" => {
            let mut out = byte_list(rest, line)?;
            out.push(Expr::Num(0));
            Ok(Some(Operation::Bytes(out)))
        }
        "res" => parse_res(rest, env, line),
        other => Err(AsmError::new(
            line,
            format!("unsupported directive `.{other}`"),
        )),
    }
}

/// `.res count [, fill]` — reserve `count` bytes (zero or `fill`).
fn parse_res(
    rest: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let count_src = parts.next().unwrap_or("").trim();
    let count = fold_const(&value(count_src, line)?, env, line)?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`.res` count must be a non-negative constant"))?;
    let fill = match parts.next() {
        None => 0,
        Some(v) => {
            let n = fold_const(&value(v.trim(), line)?, env, line)?;
            u8::try_from(n).map_err(|_| AsmError::new(line, "`.res` fill must be a byte"))?
        }
    };
    Ok(Some(Operation::Bytes(vec![
        Expr::Num(i64::from(fill));
        count
    ])))
}

fn byte_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "`.byte` needs a value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.bytes().map(|b| Expr::Num(i64::from(b))));
        } else {
            out.push(value(piece, line)?);
        }
    }
    Ok(out)
}

fn value_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "`.word` needs a value"));
    }
    split_top_level(rest, ',')
        .iter()
        .map(|p| value(p, line))
        .collect()
}

fn value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    mos6502::parse_expr(
        raw,
        line,
        parse_number,
        mos6502::ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: true,
            caret: mos6502::Caret::BankOrXor,
            at_is_pc: false,
        },
    )
}

// ---------------------------------------------------------------------------
// Operand mode resolution
// ---------------------------------------------------------------------------

/// Resolve a HuC6280 operand to its spec mode label and value expressions.
fn resolve(
    prim: &'static isa::InstructionSet,
    ext: &'static isa::InstructionSet,
    mn: &str,
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let has = |mode: &str| prim.find_form(mn, mode).is_some() || ext.find_form(mn, mode).is_some();

    // --- HuC6280-specific multi-operand forms, keyed off the spec ---
    // Block transfers: `op src, dst, len` — three 16-bit words.
    if has("block") {
        let parts = split_top_level(operand, ',');
        if parts.len() != 3 {
            return Err(AsmError::new(
                line,
                format!("`{mn}` needs source, destination, length"),
            ));
        }
        let exprs = parts
            .iter()
            .map(|p| value(p, line))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(("block", exprs));
    }
    // `tst #imm, <mem>` — an immediate mask, then a zero-page/absolute operand
    // (optionally `,x`). The first top-level comma separates them.
    if mn == "TST" {
        return resolve_tst(&has, operand, env, line);
    }
    // `bbr`/`bbs`: `op <zp>, <target>` — a zero-page byte then a relative branch.
    if has("zeropage,relative") {
        let parts = split_top_level(operand, ',');
        if parts.len() != 2 {
            return Err(AsmError::new(
                line,
                format!("`{mn}` needs a zero-page byte and a branch target"),
            ));
        }
        let zp = value(parts[0], line)?;
        let target = value(parts[1], line)?;
        return Ok(("zeropage,relative", vec![zp, target]));
    }

    // --- Ordinary 6502/65C02 addressing, extension-aware ---
    // A leading `z:`/`a:` size force applies to a direct or indexed memory
    // operand; strip it so the shared parser sees a bare expression.
    let (force, rest) = strip_force(operand);
    let syntax = mos6502::parse_operand(rest, line, &value)?;
    let (mode, expr) = match syntax {
        OperandSyntax::None => {
            if has("implied") {
                ("implied", None)
            } else if has("accumulator") {
                ("accumulator", None)
            } else {
                return Err(AsmError::new(line, format!("`{mn}` requires an operand")));
            }
        }
        OperandSyntax::Accumulator => ("accumulator", None),
        OperandSyntax::Immediate(e) => ("immediate", Some(e)),
        OperandSyntax::IndexedIndirect(e) => ("(indirect,x)", Some(e)),
        OperandSyntax::IndirectIndexed(e) => ("(indirect),y", Some(e)),
        // `(expr)` is `jmp` indirect where the mnemonic has that form, else the
        // HuC6280/65C02 `(dp)` zero-page indirect.
        OperandSyntax::Indirect(e) => {
            let mode = if has("indirect") {
                "indirect"
            } else {
                "(indirect)"
            };
            (mode, Some(e))
        }
        OperandSyntax::Indexed(e, Index::X) => (
            pick_zp_abs(&has, force, &e, env, "zeropage,x", "absolute,x"),
            Some(e),
        ),
        OperandSyntax::Indexed(e, Index::Y) => (
            pick_zp_abs(&has, force, &e, env, "zeropage,y", "absolute,y"),
            Some(e),
        ),
        // A bare operand: a relative branch target (`bra`/`bcc`/…/`bsr`) or a
        // zero-page/absolute memory reference.
        OperandSyntax::Direct(e) => {
            if has("relative") {
                ("relative", Some(e))
            } else {
                (
                    pick_zp_abs(&has, force, &e, env, "zeropage", "absolute"),
                    Some(e),
                )
            }
        }
    };
    Ok((mode, expr.into_iter().collect()))
}

/// Resolve `tst #imm, <mem>` into one of the four immediate+memory modes.
fn resolve_tst(
    has: &dyn Fn(&str) -> bool,
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    // No parentheses in a `tst` operand, so the first bare comma splits it.
    let comma = operand
        .find(',')
        .ok_or_else(|| AsmError::new(line, "`tst` needs `#imm, address`"))?;
    let imm = operand[..comma]
        .trim()
        .strip_prefix('#')
        .ok_or_else(|| AsmError::new(line, "`tst` mask must be immediate (`#`)"))?;
    let imm_expr = value(imm, line)?;

    let mem = operand[comma + 1..].trim();
    let (mem, indexed) = match top_level_rfind(mem, ',') {
        Some(c) if mem[c + 1..].trim().eq_ignore_ascii_case("x") => (mem[..c].trim(), true),
        Some(_) => return Err(AsmError::new(line, "`tst` memory index must be `,x`")),
        None => (mem, false),
    };
    let (force, base) = strip_force(mem);
    let mem_expr = value(base, line)?;
    let zp = match force {
        Force::Zp => true,
        Force::Abs => false,
        Force::None => fold_const(&mem_expr, env, line).is_ok_and(|v| (0..=0xFF).contains(&v)),
    };
    let mode = match (zp, indexed) {
        (true, false) => "immediate,zeropage",
        (false, false) => "immediate,absolute",
        (true, true) => "immediate,zeropage,x",
        (false, true) => "immediate,absolute,x",
    };
    // Fall up to absolute if the zero-page form is absent (defensive; ca65 has
    // every `tst` mode).
    let mode = if has(mode) {
        mode
    } else if mode == "immediate,zeropage" {
        "immediate,absolute"
    } else if mode == "immediate,zeropage,x" {
        "immediate,absolute,x"
    } else {
        mode
    };
    Ok((mode, vec![imm_expr, mem_expr]))
}

/// A `z:`/`a:` address-size force. ca65 has no long addressing on the 6502, so
/// there is no `f:`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Force {
    None,
    Zp,
    Abs,
}

/// Strip a leading `z:`/`a:` size-force prefix from an operand.
fn strip_force(operand: &str) -> (Force, &str) {
    let t = operand.trim();
    if let Some(r) = t.strip_prefix("z:").or_else(|| t.strip_prefix("Z:")) {
        (Force::Zp, r.trim())
    } else if let Some(r) = t.strip_prefix("a:").or_else(|| t.strip_prefix("A:")) {
        (Force::Abs, r.trim())
    } else {
        (Force::None, t)
    }
}

/// Choose zero-page when a `z:` force asks for it, or (with no force) when the
/// operand folds to a byte-sized constant and the instruction has that form; an
/// `a:` force, a forward symbol, or a large value stays absolute, matching ca65
/// and keeping form sizes stable across passes.
fn pick_zp_abs(
    has: &dyn Fn(&str) -> bool,
    force: Force,
    e: &Expr,
    env: &BTreeMap<String, i64>,
    zp: &'static str,
    abs: &'static str,
) -> &'static str {
    let fits_zp = match force {
        Force::Zp => true,
        Force::Abs => false,
        Force::None => fold_const(e, env, 0).is_ok_and(|v| (0..=0xFF).contains(&v)),
    };
    if fits_zp && has(zp) { zp } else { abs }
}

#[cfg(test)]
mod tests {
    use crate::assemble_ca65_huc6280 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn inherited_6502_and_65c02_ops() {
        // Base 6502 through the extension-aware resolver.
        assert_eq!(bytes(" lda #$12\n"), vec![0xA9, 0x12]);
        assert_eq!(bytes(" lda $12\n"), vec![0xA5, 0x12]); // zp
        assert_eq!(bytes(" lda $1234\n"), vec![0xAD, 0x34, 0x12]); // abs
        assert_eq!(bytes(" sta $10,x\n"), vec![0x95, 0x10]);
        assert_eq!(bytes(" jmp ($1234)\n"), vec![0x6C, 0x34, 0x12]); // jmp indirect
        // 65C02 additions from the extension.
        assert_eq!(bytes(" bra l\nl: rts\n"), vec![0x80, 0x00, 0x60]);
        assert_eq!(bytes(" stz $12\n"), vec![0x64, 0x12]);
        assert_eq!(bytes(" lda ($12)\n"), vec![0xB2, 0x12]); // (dp) indirect
        assert_eq!(bytes(" phx\n"), vec![0xDA]);
    }

    #[test]
    fn huc6280_implied_and_bit_ops() {
        assert_eq!(bytes(" sax\n"), vec![0x22]);
        assert_eq!(bytes(" csh\n"), vec![0xD4]);
        assert_eq!(bytes(" rmb0 $10\n"), vec![0x07, 0x10]);
        assert_eq!(bytes(" smb7 $10\n"), vec![0xF7, 0x10]);
        // bbr/bbs: zero-page byte, then a relative target (self-branch = -3).
        assert_eq!(
            bytes(".org $1000\nl: bbr0 $10, l\n"),
            vec![0x0F, 0x10, 0xFD]
        );
        assert_eq!(
            bytes(".org $1000\nl: bbs7 $10, l\n"),
            vec![0xFF, 0x10, 0xFD]
        );
    }

    #[test]
    fn huc6280_exotic_forms() {
        // st0-2 / tam / tma: opcode + one immediate.
        assert_eq!(bytes(" st0 #$aa\n"), vec![0x03, 0xAA]);
        assert_eq!(bytes(" tam #$01\n"), vec![0x53, 0x01]);
        assert_eq!(bytes(" tma #$01\n"), vec![0x43, 0x01]);
        // tst #imm, <mem> across all four modes.
        assert_eq!(bytes(" tst #$55, $10\n"), vec![0x83, 0x55, 0x10]);
        assert_eq!(bytes(" tst #$55, $1234\n"), vec![0x93, 0x55, 0x34, 0x12]);
        assert_eq!(bytes(" tst #$55, $10,x\n"), vec![0xA3, 0x55, 0x10]);
        assert_eq!(bytes(" tst #$55, $1234,x\n"), vec![0xB3, 0x55, 0x34, 0x12]);
        // bsr: relative like bra.
        assert_eq!(bytes(" bsr sub\nsub: rts\n"), vec![0x44, 0x00, 0x60]);
    }

    #[test]
    fn low_absolute_takes_a_force() {
        // A low absolute must assemble to the absolute form via `a:`, not
        // zero-page — and the disassembler must round-trip it byte-exact.
        assert_eq!(bytes(" lda a:$0080\n"), vec![0xAD, 0x80, 0x00]);
        let dis = crate::disassemble_huc6280(&[0xAD, 0x80, 0x00], 0x0000);
        assert!(dis[0].text.contains("a:$0080"), "got `{}`", dis[0].text);
        assert_eq!(
            bytes(&format!(" {}\n", dis[0].text)),
            vec![0xAD, 0x80, 0x00]
        );
    }

    #[test]
    fn multi_operand_forms_round_trip_via_disassembler() {
        // Assemble → disassemble → reassemble the tricky multi-operand shapes.
        // Assembly and disassembly both use origin 0 so branch targets align.
        for (src, expect) in [
            (" bbr0 $10, $0005\n", vec![0x0F, 0x10, 0x02]),
            (" tst #$55, a:$0080,x\n", vec![0xB3, 0x55, 0x80, 0x00]),
            (
                " tii $1234, $5678, $0010\n",
                vec![0x73, 0x34, 0x12, 0x78, 0x56, 0x10, 0x00],
            ),
        ] {
            let asm = bytes(src);
            assert_eq!(asm, expect, "assemble `{src}`");
            let dis = crate::disassemble_huc6280(&asm, 0x0000);
            let reasm = bytes(&format!(" {}\n", dis[0].text));
            assert_eq!(reasm, expect, "round-trip `{}`", dis[0].text);
        }
    }

    #[test]
    fn huc6280_block_transfers() {
        // opcode, src-lo, src-hi, dst-lo, dst-hi, len-lo, len-hi.
        assert_eq!(
            bytes(" tii $1000, $2000, $0010\n"),
            vec![0x73, 0x00, 0x10, 0x00, 0x20, 0x10, 0x00]
        );
        assert_eq!(
            bytes(" tdd $1000, $2000, $0010\n"),
            vec![0xC3, 0x00, 0x10, 0x00, 0x20, 0x10, 0x00]
        );
        assert_eq!(
            bytes(" tai $1000, $2000, $0010\n"),
            vec![0xF3, 0x00, 0x10, 0x00, 0x20, 0x10, 0x00]
        );
    }
}
