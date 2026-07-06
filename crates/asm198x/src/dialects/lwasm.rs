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
        // Route assembly through the semantic AST (U6): parse into a `Program`,
        // then lower to the engine's statement stream — byte-identical to the old
        // direct parse (AE1). The 6809 is the first **computed-operand** CPU to
        // migrate: its instructions carry a precomputed `Operation::Encoded`
        // (postbyte + extension bytes), which the AST holds verbatim as
        // `Item::Encoded` and the formatter re-emits via `Node::source`.
        crate::ast::lower(parse_program(source)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source)?))
    }
}

/// Parse 6809 source into the semantic [`Program`](crate::ast::Program). Each line
/// becomes a node with its (global) label, operation, verbatim source, span, and
/// comment trivia. The 6809 has no local-label scoping, so every label is a
/// [`Scope::Global`](crate::ast::Scope) symbol whose qualified name is the source
/// name. An instruction lowers to a computed [`Operation::Encoded`], carried as
/// [`Item::Encoded`](crate::ast::Item::Encoded) — the formatter re-emits it from
/// the node's source, so it round-trips byte-identical (the computed-operand path
/// U1 axis 2 proved, now exercised on production code).
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
    let mut nodes = Vec::new();
    let mut env: BTreeMap<String, i64> = BTreeMap::new();
    let mut pending_leading: Vec<Comment> = Vec::new();

    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let (code, comment) = split_comment(raw);
        if code.trim().is_empty() {
            if let Some(text) = comment {
                pending_leading.push(Comment {
                    text: text.to_string(),
                    span: Span::at(line as u32, 1),
                });
            }
            continue;
        }
        let trailing = comment.map(|text| Comment {
            text: text.to_string(),
            span: Span::at(line as u32, (code.len() + 1) as u32),
        });

        let (label, rest) = split_label(code);
        let op = if rest.is_empty() {
            None
        } else {
            parse_op(rest, &env, line)?
        };
        // Bind an `equ` value into the parse-time env so a later direct/extended
        // choice can fold it (mirrors the engine's pass-1 `equ`).
        if let (Some(name), Some(Operation::Equ(e))) = (&label, &op)
            && let Ok(v) = fold_const(e, &env, line)
        {
            env.insert(name.clone(), v);
        }
        if label.is_none() && op.is_none() {
            continue;
        }
        nodes.push(Node {
            label: label.map(|name| Symbol {
                qualified: name.clone(),
                scope: Scope::Global,
                name,
            }),
            item: op.map(crate::ast::item_from_operation),
            source: rest.trim().to_string(),
            span: Span::at(line as u32, 1),
            trivia: Trivia {
                leading: std::mem::take(&mut pending_leading),
                trailing,
            },
        });
    }

    if !pending_leading.is_empty() {
        let line = source.lines().count() as u32;
        nodes.push(Node {
            label: None,
            item: None,
            source: String::new(),
            span: Span::at(line, 1),
            trivia: Trivia {
                leading: pending_leading,
                trailing: None,
            },
        });
    }
    Ok(Program { nodes })
}

/// Split a line into its code and its comment (delimiter kept, trailing
/// whitespace trimmed) for carrying comments as AST trivia; defined via
/// [`strip_comment`] so the comment is exactly what it removes. A whole-line
/// `*` comment yields empty code and the whole line as the comment.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
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
        "fcc" => Ok(Some(parse_fcc(operand, line)?)),
        "fqb" => Ok(Some(parse_fqb(operand, line)?)),
        "rmb" | ".ds" | "zmb" => parse_rmb(operand, env, line),
        "fill" => parse_fill(operand, env, line),
        "end" => Ok(None), // marks the end of source; emits nothing
        _ => Ok(Some(parse_instruction(&m, operand, env, line)?)),
    }
}

/// `rmb count` / `zmb count` — reserve/zero `count` bytes, zero-filled (the
/// flat-output behaviour). `count` folds against the parse-time env so the size
/// is known in pass one.
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

/// `fill value,count` — `count` copies of `value` (lwasm's order is value first,
/// then count; both are required). Both fold against the parse-time env so the
/// size and fill are known in pass one.
fn parse_fill(
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let parts = mos6502::split_top_level(operand, ',');
    if parts.len() != 2 {
        return Err(AsmError::new(line, "`fill` needs `value,count`"));
    }
    let fill = fold_const(&value(parts[0].trim(), line)?, env, line)?;
    let fill = u8::try_from(fill & 0xFF).expect("masked");
    let count = fold_const(&value(parts[1].trim(), line)?, env, line)?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`fill` count must be a non-negative constant"))?;
    Ok(Some(Operation::Bytes(vec![
        Expr::Num(i64::from(fill));
        count
    ])))
}

/// `fqb value[,value…]` — "form quad byte": each value as a 32-bit big-endian
/// word. Emitted through the engine's computed-operand seam so symbolic values
/// resolve in pass two.
fn parse_fqb(operand: &str, line: usize) -> Result<Operation, AsmError> {
    let pieces = list(operand, line)?
        .into_iter()
        .map(|expr| Piece::Val {
            expr,
            bytes: 4,
            rel: false,
            signed: false,
        })
        .collect();
    Ok(Operation::Encoded(pieces))
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
            } => encode_mem(
                m, imm, direct, indexed, extended, *width, operand, env, line,
            ),
            Kind::Transfer(opcode) => encode_transfer(m, *opcode, operand, line),
            Kind::Stack { opcode, u_stack } => encode_stack(*opcode, *u_stack, operand, line),
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
    Ok(Operation::Encoded(
        opcode.iter().map(|b| Piece::Lit(*b)).collect(),
    ))
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
    if let Some(rest) = t.strip_prefix('#') {
        if imm.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no immediate mode")));
        }
        return Ok(encoded(imm, value(rest, line)?, width));
    }
    // Indexed addressing (`,R` / `n,R` / `[...]`) — a computed postbyte plus
    // 0/1/2 extension bytes. Detected before the `<`/`>` direct/extended forces,
    // since inside an indexed operand `<`/`>` size the offset, not the address.
    if t.starts_with('[') || mos6502::top_level_rfind(t, ',').is_some() {
        if indexed.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no indexed mode")));
        }
        return encode_indexed(m, indexed, t, env, line);
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
        Err(AsmError::new(
            line,
            format!("`{m}` has no addressing mode for `{t}`"),
        ))
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

// ---------------------------------------------------------------------------
// Indexed addressing — the computed postbyte
// ---------------------------------------------------------------------------

/// An auto-increment / -decrement marker on the index register.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Auto {
    None,
    Inc1,
    Inc2,
    Dec1,
    Dec2,
}

/// The chosen width of an indexed offset: embedded 5-bit, an 8-bit extension, or
/// a 16-bit extension.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OffSize {
    Bits5,
    Byte,
    Word,
}

/// Encode a 6809 indexed operand into the postbyte (+ 0/1/2 extension bytes).
fn encode_indexed(
    m: &str,
    opcode: &[u8],
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    // Indirect operands are wrapped in `[ ]`.
    let (inner, indirect) = match operand.strip_prefix('[') {
        Some(rest) => (
            rest.strip_suffix(']')
                .ok_or_else(|| AsmError::new(line, format!("unclosed `[` in `{operand}`")))?
                .trim(),
            true,
        ),
        None => (operand, false),
    };

    let mut pieces: Vec<Piece> = opcode.iter().map(|b| Piece::Lit(*b)).collect();

    // No top-level comma: only the extended-indirect form `[addr]` is valid.
    let Some(c) = mos6502::top_level_rfind(inner, ',') else {
        if indirect {
            pieces.push(Piece::Lit(0x9F));
            pieces.push(Piece::Val {
                expr: value(inner, line)?,
                bytes: 2,
                rel: false,
                signed: false,
            });
            return Ok(Operation::Encoded(pieces));
        }
        return Err(AsmError::new(
            line,
            format!("`{m}`: not an indexed operand"),
        ));
    };
    let left = inner[..c].trim();
    let reg = inner[c + 1..].trim();

    let (rbits, auto, pcr) = parse_index_reg(reg, line)?;
    if auto != Auto::None && !left.is_empty() {
        return Err(AsmError::new(
            line,
            "auto-increment/decrement takes no offset",
        ));
    }
    if indirect && matches!(auto, Auto::Inc1 | Auto::Dec1) {
        return Err(AsmError::new(
            line,
            "no indirect form for single `,R+`/`,-R`",
        ));
    }

    // The postbyte, before the indirect bit is OR-ed in.
    let mut post: u8;
    let mut ext: Option<(Expr, u8, bool)> = None; // (expr, width, rel)
    if pcr {
        // `n,PCR`: the offset is relative to the following instruction. The size
        // can't be chosen from the value (it depends on the unknown PC), so it
        // defaults to 16-bit; `<` forces 8-bit. `>` also gives 16-bit.
        let (size, expr) = sized_offset(left, env, line, false, true)?;
        post = if size == OffSize::Byte { 0x8C } else { 0x8D };
        ext = Some((expr, if size == OffSize::Byte { 1 } else { 2 }, true));
    } else {
        let rr = rbits << 5;
        post = match auto {
            Auto::Inc1 => 0x80 | rr,
            Auto::Inc2 => 0x81 | rr,
            Auto::Dec1 => 0x82 | rr,
            Auto::Dec2 => 0x83 | rr,
            Auto::None if left.is_empty() => 0x84 | rr,
            Auto::None if left.eq_ignore_ascii_case("a") => 0x86 | rr,
            Auto::None if left.eq_ignore_ascii_case("b") => 0x85 | rr,
            Auto::None if left.eq_ignore_ascii_case("d") => 0x8B | rr,
            Auto::None => {
                // A numeric/symbolic offset: 5-bit embedded, or an 8-/16-bit
                // extension. Indirect has no 5-bit form (8-bit is the minimum).
                let (size, expr) = sized_offset(left, env, line, !indirect, false)?;
                match size {
                    OffSize::Bits5 => {
                        let v = fold_const(&expr, env, line)?; // constant by construction
                        rr | (v as u8 & 0x1F)
                    }
                    OffSize::Byte => {
                        ext = Some((expr, 1, false));
                        0x88 | rr
                    }
                    OffSize::Word => {
                        ext = Some((expr, 2, false));
                        0x89 | rr
                    }
                }
            }
        };
    }

    if indirect {
        post |= 0x10;
    }
    pieces.push(Piece::Lit(post));
    if let Some((expr, width, rel)) = ext {
        pieces.push(Piece::Val {
            expr,
            bytes: width,
            rel,
            // An 8-bit offset is a signed displacement; a 16-bit one is often a
            // base address, so it is range-checked across the full width.
            signed: width == 1,
        });
    }
    Ok(Operation::Encoded(pieces))
}

/// Parse the register part of an indexed operand: the index register (`x`/`y`/
/// `u`/`s`), with any auto inc/dec marker, or the PC for `pcr`/`pc`. Returns the
/// 2-bit register field, the auto marker, and whether it is PC-relative.
fn parse_index_reg(reg: &str, line: usize) -> Result<(u8, Auto, bool), AsmError> {
    let r = reg.trim();
    if r.eq_ignore_ascii_case("pcr") || r.eq_ignore_ascii_case("pc") {
        return Ok((0, Auto::None, true));
    }
    let (name, auto) = if let Some(s) = r.strip_prefix("--") {
        (s, Auto::Dec2)
    } else if let Some(s) = r.strip_prefix('-') {
        (s, Auto::Dec1)
    } else if let Some(s) = r.strip_suffix("++") {
        (s, Auto::Inc2)
    } else if let Some(s) = r.strip_suffix('+') {
        (s, Auto::Inc1)
    } else {
        (r, Auto::None)
    };
    let rbits = mos6809::index_reg(name.trim())
        .ok_or_else(|| AsmError::new(line, format!("unknown index register `{reg}`")))?;
    Ok((rbits, auto, false))
}

/// Choose the width of an indexed offset and parse its expression. `<` forces
/// 8-bit, `>` forces 16-bit. Otherwise a constant picks the smallest fit
/// (5-bit only when `allow5`); a forward/symbolic offset defaults to 16-bit. For
/// `pcr` the value can't choose the size (it depends on the PC), so it is 16-bit
/// unless `<`-forced.
fn sized_offset(
    raw: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
    allow5: bool,
    pcr: bool,
) -> Result<(OffSize, Expr), AsmError> {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix('>') {
        return Ok((OffSize::Word, value(rest, line)?));
    }
    if let Some(rest) = t.strip_prefix('<') {
        return Ok((OffSize::Byte, value(rest, line)?));
    }
    let e = value(t, line)?;
    if pcr {
        return Ok((OffSize::Word, e));
    }
    let size = match fold_const(&e, env, line) {
        Ok(v) if allow5 && (-16..=15).contains(&v) => OffSize::Bits5,
        Ok(v) if (-128..=127).contains(&v) => OffSize::Byte,
        _ => OffSize::Word,
    };
    Ok((size, e))
}

// ---------------------------------------------------------------------------
// Register-list operations
// ---------------------------------------------------------------------------

/// `tfr`/`exg src,dst` — the opcode then a postbyte of two 4-bit register codes.
fn encode_transfer(m: &str, opcode: u8, operand: &str, line: usize) -> Result<Operation, AsmError> {
    let parts = mos6502::split_top_level(operand, ',');
    if parts.len() != 2 {
        return Err(AsmError::new(line, format!("`{m}` needs two registers")));
    }
    let reg = |p: &str| {
        mos6809::transfer_reg(p.trim())
            .ok_or_else(|| AsmError::new(line, format!("unknown register `{}`", p.trim())))
    };
    let post = (reg(parts[0])? << 4) | reg(parts[1])?;
    Ok(Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Lit(post),
    ]))
}

/// `pshs`/`puls`/`pshu`/`pulu reg,…` — the opcode then a register bitmask.
fn encode_stack(
    opcode: u8,
    u_stack: bool,
    operand: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    if operand.trim().is_empty() {
        return Err(AsmError::new(line, "push/pull needs at least one register"));
    }
    let mut mask = 0u8;
    for p in mos6502::split_top_level(operand, ',') {
        mask |= mos6809::stack_mask(p.trim(), u_stack)
            .ok_or_else(|| AsmError::new(line, format!("unknown register `{}`", p.trim())))?;
    }
    Ok(Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Lit(mask),
    ]))
}

/// `fcc` — a string with a self-chosen delimiter (`"text"`, `/text/`, …): one
/// byte per character, up to the closing delimiter.
fn parse_fcc(operand: &str, line: usize) -> Result<Operation, AsmError> {
    let t = operand.trim();
    let delim = t
        .chars()
        .next()
        .ok_or_else(|| AsmError::new(line, "`fcc` needs a string"))?;
    let rest = &t[delim.len_utf8()..];
    let end = rest
        .find(delim)
        .ok_or_else(|| AsmError::new(line, "unterminated `fcc` string"))?;
    Ok(Operation::Bytes(
        rest[..end]
            .bytes()
            .map(|b| Expr::Num(i64::from(b)))
            .collect(),
    ))
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
    mos6502::parse_expr(
        raw,
        line,
        mos6502::parse_number,
        mos6502::ExprOpts {
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: mos6502::Caret::Xor,
            at_is_pc: false,
        },
    )
}

#[cfg(test)]
mod tests {
    use crate::assemble_lwasm as asm;

    #[test]
    fn inherent_and_immediate() {
        assert_eq!(asm("        nop\n").expect("nop").bytes, vec![0x12]);
        assert_eq!(asm("        rts\n").expect("rts").bytes, vec![0x39]);
        assert_eq!(
            asm("        lda #$42\n").expect("imm").bytes,
            vec![0x86, 0x42]
        );
        // 16-bit immediate.
        assert_eq!(
            asm("        ldx #$1234\n").expect("ldx").bytes,
            vec![0x8E, 0x12, 0x34]
        );
    }

    #[test]
    fn direct_and_extended_selection() {
        // Low constant -> direct; high constant -> extended.
        assert_eq!(
            asm("        lda $20\n").expect("dir").bytes,
            vec![0x96, 0x20]
        );
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
        assert_eq!(
            asm("        fcb $01,$02\n").expect("fcb").bytes,
            vec![0x01, 0x02]
        );
        // fdb is big-endian.
        assert_eq!(
            asm("        fdb $1234\n").expect("fdb").bytes,
            vec![0x12, 0x34]
        );
    }

    #[test]
    fn fill_zmb_fqb_match_reference_bytes() {
        // Byte-for-byte against `lwasm --6809 --raw`:
        //   fill $ff,3 -> ff ff ff   (lwasm order is value,count)
        //   zmb 2      -> 00 00
        //   fqb $12345678 -> 12 34 56 78  (32-bit big-endian)
        let a = asm("        fcb $aa\n        fill $ff,3\n        zmb 2\n        fqb $12345678\n        fcb $bb\n")
            .expect("fill/zmb/fqb");
        assert_eq!(
            a.bytes,
            vec![
                0xAA, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x12, 0x34, 0x56, 0x78, 0xBB
            ]
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
        assert_eq!(a.origin, Some(0x2000));
        assert_eq!(a.bytes, vec![0x86, 0x00, 0xB7, 0x10, 0x00, 0x39]);
        assert_eq!(a.symbols.get("start"), Some(&0x2000));
    }

    #[test]
    fn indexed_offsets_pick_smallest() {
        // No offset, 5-bit, 8-bit, 16-bit — register X (opcode 0xA6).
        assert_eq!(
            asm("        lda ,x\n").expect("noff").bytes,
            vec![0xA6, 0x84]
        );
        assert_eq!(
            asm("        lda 5,x\n").expect("5bit").bytes,
            vec![0xA6, 0x05]
        );
        assert_eq!(
            asm("        lda -16,x\n").expect("5neg").bytes,
            vec![0xA6, 0x10]
        );
        assert_eq!(
            asm("        lda 16,x\n").expect("8bit").bytes,
            vec![0xA6, 0x88, 0x10]
        );
        assert_eq!(
            asm("        lda $1234,x\n").expect("16bit").bytes,
            vec![0xA6, 0x89, 0x12, 0x34]
        );
        // Other registers shift the postbyte; Y=+0x20, U=+0x40, S=+0x60.
        assert_eq!(asm("        lda ,y\n").expect("y").bytes, vec![0xA6, 0xA4]);
        assert_eq!(asm("        ldx 2,s\n").expect("s").bytes, vec![0xAE, 0x62]);
    }

    #[test]
    fn indexed_auto_and_accumulator() {
        assert_eq!(
            asm("        lda ,x+\n").expect("inc1").bytes,
            vec![0xA6, 0x80]
        );
        assert_eq!(
            asm("        lda ,x++\n").expect("inc2").bytes,
            vec![0xA6, 0x81]
        );
        assert_eq!(
            asm("        lda ,-x\n").expect("dec1").bytes,
            vec![0xA6, 0x82]
        );
        assert_eq!(
            asm("        lda ,--x\n").expect("dec2").bytes,
            vec![0xA6, 0x83]
        );
        assert_eq!(asm("        lda a,x\n").expect("a").bytes, vec![0xA6, 0x86]);
        assert_eq!(asm("        lda b,x\n").expect("b").bytes, vec![0xA6, 0x85]);
        assert_eq!(asm("        lda d,x\n").expect("d").bytes, vec![0xA6, 0x8B]);
    }

    #[test]
    fn indexed_indirect_and_pcr() {
        assert_eq!(
            asm("        lda [,x]\n").expect("ind").bytes,
            vec![0xA6, 0x94]
        );
        // Indirect has no 5-bit form: a small offset still uses the 8-bit form.
        assert_eq!(
            asm("        lda [5,x]\n").expect("ind8").bytes,
            vec![0xA6, 0x98, 0x05]
        );
        assert_eq!(
            asm("        lda [$2000]\n").expect("extind").bytes,
            vec![0xA6, 0x9F, 0x20, 0x00]
        );
        // PCR to a label: 16-bit offset relative to the next instruction.
        let a =
            asm("        org $1000\n        leax msg,pcr\n        nop\nmsg fcb 1\n").expect("pcr");
        // leax=0x30, postbyte 0x8D, offset = msg($1005) - next($1004) = 1.
        assert_eq!(a.bytes[..4], [0x30, 0x8D, 0x00, 0x01]);
    }

    #[test]
    fn transfer_and_stack() {
        assert_eq!(
            asm("        tfr a,b\n").expect("tfr").bytes,
            vec![0x1F, 0x89]
        );
        assert_eq!(
            asm("        exg x,d\n").expect("exg").bytes,
            vec![0x1E, 0x10]
        );
        assert_eq!(
            asm("        pshs a,b,x\n").expect("pshs").bytes,
            vec![0x34, 0x16]
        );
        // `d` sets both the A and B bits.
        assert_eq!(
            asm("        puls x,y,d\n").expect("puls").bytes,
            vec![0x35, 0x36]
        );
        // pshu's bit 6 is S, not U.
        assert_eq!(
            asm("        pshu a,b,s\n").expect("pshu").bytes,
            vec![0x36, 0x46]
        );
    }

    #[test]
    fn fcc_string() {
        assert_eq!(
            asm("        fcc \"AB\"\n").expect("dq").bytes,
            vec![0x41, 0x42]
        );
        assert_eq!(
            asm("        fcc /CD/\n").expect("slash").bytes,
            vec![0x43, 0x44]
        );
    }

    /// U6 — the 6809 front-end routes through the AST. Its computed-operand
    /// instructions carry `Item::Encoded`, and comments are carried as trivia
    /// (both `*` whole-line and `;` inline) without changing the bytes (AE1).
    #[test]
    fn comments_are_carried_as_trivia() {
        let src = "* header\nstart   lda #$05   ; load\n        leax 5,x\n";
        let prog = super::parse_program(src).expect("parses");
        assert!(
            prog.nodes[0]
                .trivia
                .leading
                .iter()
                .any(|c| c.text == "* header"),
            "whole-line `*` comment attaches as leading trivia"
        );
        assert!(
            prog.nodes.iter().any(|n| n
                .trivia
                .trailing
                .as_ref()
                .is_some_and(|c| c.text == "; load")),
            "same-line `;` comment attaches as trailing trivia"
        );
        // The indexed `leax 5,x` is a computed-operand instruction: its item is
        // `Item::Encoded`, proving the wrap path.
        assert!(
            prog.nodes
                .iter()
                .any(|n| matches!(n.item, Some(crate::ast::Item::Encoded(_)))),
            "a computed-operand instruction carries Item::Encoded"
        );
        assert_eq!(
            asm(src).expect("with comments").bytes,
            asm("start   lda #$05\n        leax 5,x\n")
                .expect("without")
                .bytes,
            "comments do not change bytes"
        );
    }
}
