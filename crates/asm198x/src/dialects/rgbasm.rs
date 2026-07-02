//! The rgbasm (RGBDS) dialect front-end for the SM83 (Game Boy) CPU.
//!
//! rgbasm is the canonical Game Boy assembler. This dialect assembles against
//! [`isa::sm83`] and produces a flat binary at the section's origin — the
//! `Dialect`/engine path the other flat assemblers use. Encoding is the spec's;
//! only rgbasm's surface lives here: `SECTION`, `db`/`dw`/`ds`, `EQU`/`=`
//! constants, `name:` globals and `.local` labels, and the operand syntax
//! (`[hl]`, `[hl+]`, `ldh [$ff00+n]`, `sp+e`).
//!
//! ## Resolving operands to spec mode labels
//!
//! Like the Z80 front-end, an operand is classified then written into one or
//! more candidate mode-label tokens; the cartesian product of the operands'
//! alternatives is probed against the spec until a form matches (so `ld a,$05`
//! finds `a,N` and `add sp,$05` finds `sp,D`, without hardcoding per-mnemonic
//! tables). Registers/conditions are lower-case literals; immediates become the
//! upper-case `N`/`NN`/`E`/`D` placeholders the spec uses. Opcode-embedded
//! operands (`rst` target, `bit`/`res`/`set` number) contribute a literal token
//! and emit no byte.
//!
//! Output is validated byte-identical against `rgbasm`/`rgblink` (RGBDS).

use std::collections::BTreeMap;

use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, split_top_level, string_literal,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The rgbasm (SM83) dialect.
pub(crate) struct Rgbasm;

impl Dialect for Rgbasm {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::sm83::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let set = self.instruction_set();
        let mut out = Vec::new();
        let mut consts: BTreeMap<String, i64> = BTreeMap::new();
        let mut global = String::new();

        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            let code = strip_comment(raw);
            if code.trim().is_empty() {
                continue;
            }
            // `SECTION "name", TYPE[$addr]` — only the origin matters for a flat
            // binary; a section with no address assembles at the current PC.
            if code
                .trim_start()
                .to_ascii_uppercase()
                .starts_with("SECTION")
            {
                if let Some(org) = section_origin(code.trim(), line)? {
                    out.push(Statement {
                        line,
                        label: None,
                        op: Some(Operation::Org(org)),
                    });
                }
                continue;
            }
            // `NAME EQU expr` / `NAME = expr` — a constant.
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

            let (label, rest) = split_label(code, line)?;
            if let Some(name) = &label
                && !name.starts_with('.')
            {
                global = name.clone();
            }
            let label = label.map(|n| qualify(&n, &global));
            let op = if rest.is_empty() {
                None
            } else {
                parse_op(set, rest, &consts, &global, line)?
            };
            if label.is_some() || op.is_some() {
                out.push(Statement { line, label, op });
            }
        }
        Ok(out)
    }
}

/// Strip a `;` comment, ignoring `;` inside a `"..."` string.
fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    for (i, b) in line.bytes().enumerate() {
        match b {
            b'"' => in_str = !in_str,
            b';' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// `SECTION "name", TYPE[$addr]` → the origin, if the section pins one.
fn section_origin(code: &str, line: usize) -> Result<Option<Expr>, AsmError> {
    match (code.find('['), code.rfind(']')) {
        (Some(a), Some(b)) if a < b => Ok(Some(value(code[a + 1..b].trim(), line)?)),
        _ => Ok(None),
    }
}

/// `NAME EQU expr` or `NAME = expr` (redefinable). Returns the name and value.
fn constant(code: &str, line: usize) -> Result<Option<(String, Expr)>, AsmError> {
    // `NAME EQU expr` / `NAME EQUS ...` — the keyword form.
    let (first, rest) = split_first_word(code);
    if !rest.is_empty() {
        let (kw, tail) = split_first_word(rest);
        if kw.eq_ignore_ascii_case("equ") && is_ident(first) {
            return Ok(Some((first.to_string(), value(tail, line)?)));
        }
    }
    // `NAME = expr` — a lone `=`.
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

/// Split a leading label from the line. rgbasm labels are `name:`/`name::` or a
/// leading-`.` local; a bare column-0 word with no colon is the mnemonic.
fn split_label(code: &str, line: usize) -> Result<(Option<String>, &str), AsmError> {
    if code.starts_with([' ', '\t']) {
        return Ok((None, code.trim()));
    }
    let trimmed = code.trim();
    let (word, rest) = split_first_word(trimmed);
    let name = word.trim_end_matches(':');
    if word.ends_with(':') && is_local_or_ident(name) {
        return Ok((Some(name.to_string()), rest));
    }
    // A leading-`.` local label may appear without a colon.
    if word.starts_with('.') && is_local_or_ident(word) && rest.is_empty() {
        return Ok((Some(word.to_string()), ""));
    }
    if word.starts_with('.') && is_local_or_ident(word) {
        return Ok((Some(word.to_string()), rest));
    }
    // Otherwise the whole line is an operation (mnemonic/directive).
    let _ = line;
    Ok((None, trimmed))
}

fn is_local_or_ident(s: &str) -> bool {
    s.strip_prefix('.').map_or_else(|| is_ident(s), is_ident)
}

/// Qualify a leading-`.` local label under the current global scope.
fn qualify(name: &str, global: &str) -> String {
    if name.starts_with('.') {
        format!("{global}{name}")
    } else {
        name.to_string()
    }
}

/// Parse the operation part of a line: a directive or an instruction.
fn parse_op(
    set: &'static isa::InstructionSet,
    rest: &str,
    consts: &BTreeMap<String, i64>,
    global: &str,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    let op = match word.to_ascii_lowercase().as_str() {
        "db" => Operation::Bytes(byte_list(args, line)?),
        "dw" => Operation::Words(value_list(args, line)?),
        "ds" => parse_ds(args, consts, line)?,
        _ => {
            let mn = word.to_ascii_uppercase();
            let (mode, operands) = resolve(set, &mn, args, consts, line)?;
            Operation::Instruction {
                mnemonic: mn,
                mode,
                operands,
            }
        }
    };
    Ok(Some(qualify_op(op, global)))
}

/// `ds count [, fill]` — reserve `count` bytes of `fill` (default 0).
fn parse_ds(
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let mut parts = split_top_level(args, ',');
    let count = fold_const(&value(parts.remove(0), line)?, consts, line)?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`ds` count must be a non-negative constant"))?;
    let fill = match parts.first() {
        None => 0,
        Some(v) => {
            let n = fold_const(&value(v, line)?, consts, line)?;
            u8::try_from(n & 0xFF).unwrap_or(0)
        }
    };
    Ok(Operation::Bytes(vec![Expr::Num(i64::from(fill)); count]))
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
            at_is_pc: true,
        },
    )
}

// ---------------------------------------------------------------------------
// Operand resolution (rgbasm syntax -> spec mode label)
// ---------------------------------------------------------------------------

/// One classified operand.
enum Cls {
    /// A register-indirect or other memory token that can only be a register
    /// (`[hl]`, `[c]`) — a fixed lower-case token, never a label.
    Fixed(String),
    /// A bare word that names a register/condition **but could also be a label**
    /// (register `l` vs a label `l`). Both interpretations are offered and the
    /// spec picks: a register form wins if one exists, else it is an address.
    RegOrLabel(String, Expr),
    /// A value: a bare immediate, or a `[expr]` memory reference (`paren`).
    Value { expr: Expr, paren: bool },
    /// A `sp+e` / `sp-e` stack displacement.
    SpDisp(Expr),
}

/// One label token an operand can contribute, and the bytes it emits.
type Alternative = (String, Vec<Expr>);

fn resolve(
    set: &'static isa::InstructionSet,
    mn: &str,
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let pieces = if args.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level(args, ',')
    };
    let mut per_operand: Vec<Vec<Alternative>> = Vec::new();
    // One-operand ALU ops carry an implicit accumulator destination: rgbasm reads
    // `sub b` as `sub a,b`. The spec only holds the two-operand `a,X` forms.
    if pieces.len() == 1
        && matches!(
            mn,
            "ADD" | "ADC" | "SUB" | "SBC" | "AND" | "XOR" | "OR" | "CP"
        )
    {
        per_operand.push(vec![("a".to_string(), vec![])]);
    }
    for (idx, piece) in pieces.iter().enumerate() {
        per_operand.push(alternatives(mn, idx, piece, consts, line)?);
    }

    for combo in product(&per_operand) {
        let label = combo
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if let Some(f) = set.instruction(mn).and_then(|i| i.form(&label)) {
            let emitted = combo.into_iter().flat_map(|(_, v)| v).collect();
            return Ok((f.mode, emitted));
        }
    }
    Err(AsmError::new(
        line,
        format!("`{mn}` has no form for operands `{}`", args.trim()),
    ))
}

fn alternatives(
    mn: &str,
    idx: usize,
    piece: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Vec<Alternative>, AsmError> {
    Ok(match classify(piece, line)? {
        Cls::Fixed(t) => vec![(t, vec![])],
        Cls::SpDisp(e) => vec![("sp+D".to_string(), vec![e])],
        // A bare register word: prefer the register token, but also offer it as
        // an address so a like-named label (`jr nz, l`) still resolves.
        Cls::RegOrLabel(t, e) => {
            let mut alts = vec![(t, vec![])];
            alts.extend(
                emitted_tokens(mn, false)
                    .into_iter()
                    .map(|tok| (tok, vec![e.clone()])),
            );
            alts
        }
        Cls::Value { expr, paren } => {
            if let Some(t) = embedded_token(mn, idx, &expr, consts, line)? {
                vec![(t, vec![])]
            } else if mn == "LDH" && paren {
                // High-page load: the operand byte is the low byte of $FF00+n.
                vec![("[$ff00+N]".to_string(), vec![Expr::Lo(Box::new(expr))])]
            } else {
                emitted_tokens(mn, paren)
                    .into_iter()
                    .map(|t| (t, vec![expr.clone()]))
                    .collect()
            }
        }
    })
}

fn classify(piece: &str, line: usize) -> Result<Cls, AsmError> {
    let t = piece.trim();
    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let compact = inner.replace([' ', '\t'], "").to_ascii_lowercase();
        let fixed = match compact.as_str() {
            "hl" => Some("[hl]"),
            "bc" => Some("[bc]"),
            "de" => Some("[de]"),
            "hl+" | "hli" => Some("[hl+]"),
            "hl-" | "hld" => Some("[hl-]"),
            "c" | "$ff00+c" => Some("[c]"),
            _ => None,
        };
        return Ok(match fixed {
            Some(tok) => Cls::Fixed(tok.to_string()),
            None => Cls::Value {
                expr: value(inner, line)?,
                paren: true,
            },
        });
    }
    let lower = t.to_ascii_lowercase();
    // `sp+e` / `sp-e`.
    if let Some(rest) = lower.strip_prefix("sp+") {
        return Ok(Cls::SpDisp(value(&t[t.len() - rest.len()..], line)?));
    }
    if let Some(rest) = lower.strip_prefix("sp-") {
        let e = value(&t[t.len() - rest.len()..], line)?;
        return Ok(Cls::SpDisp(Expr::Neg(Box::new(e))));
    }
    if is_reg_or_cond(&lower) {
        return Ok(Cls::RegOrLabel(lower, Expr::Sym(t.to_string())));
    }
    Ok(Cls::Value {
        expr: value(t, line)?,
        paren: false,
    })
}

/// Registers and condition codes that are fixed opcode tokens.
fn is_reg_or_cond(s: &str) -> bool {
    matches!(
        s,
        "a" | "b"
            | "c"
            | "d"
            | "e"
            | "h"
            | "l"
            | "af"
            | "bc"
            | "de"
            | "hl"
            | "sp"
            | "z"
            | "nz"
            | "nc"
    )
}

/// A token embedded in the opcode (RST target, BIT/RES/SET bit number): emits no
/// byte. `None` for operands that become bytes.
fn embedded_token(
    mn: &str,
    idx: usize,
    expr: &Expr,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<String>, AsmError> {
    let lit = || {
        fold_const(expr, consts, line).map_err(|_| {
            AsmError::new(
                line,
                "operand must be a constant here (a number or a value defined with `equ` above)",
            )
        })
    };
    Ok(match mn {
        "RST" => Some(format!("{:02X}", lit()?)),
        "BIT" | "RES" | "SET" if idx == 0 => Some(format!("{}", lit()?)),
        _ => None,
    })
}

/// Candidate placeholder tokens for a value that becomes bytes.
fn emitted_tokens(mn: &str, paren: bool) -> Vec<String> {
    if paren {
        return vec!["[NN]".to_string()];
    }
    match mn {
        "JR" => vec!["E".to_string()],
        // `N`/`NN` cover 8- and 16-bit immediates; `D` the signed `add sp,e`.
        _ => vec!["N".to_string(), "NN".to_string(), "D".to_string()],
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

/// Qualify leading-`.` local references in an operation under `global`.
fn qualify_op(op: Operation, global: &str) -> Operation {
    let q = |e: Expr| qualify_expr(e, global);
    match op {
        Operation::Bytes(v) => Operation::Bytes(v.into_iter().map(q).collect()),
        Operation::Words(v) => Operation::Words(v.into_iter().map(q).collect()),
        Operation::Instruction {
            mnemonic,
            mode,
            operands,
        } => Operation::Instruction {
            mnemonic,
            mode,
            operands: operands.into_iter().map(q).collect(),
        },
        Operation::Org(e) => Operation::Org(q(e)),
        Operation::Equ(e) => Operation::Equ(q(e)),
        other => other,
    }
}

fn qualify_expr(e: Expr, global: &str) -> Expr {
    match e {
        Expr::Sym(s) if s.starts_with('.') => Expr::Sym(format!("{global}{s}")),
        Expr::Sym(_) | Expr::Num(_) | Expr::Pc => e,
        Expr::Lo(b) => Expr::Lo(Box::new(qualify_expr(*b, global))),
        Expr::Hi(b) => Expr::Hi(Box::new(qualify_expr(*b, global))),
        Expr::Bank(b) => Expr::Bank(Box::new(qualify_expr(*b, global))),
        Expr::Neg(b) => Expr::Neg(Box::new(qualify_expr(*b, global))),
        Expr::Bin(op, l, r) => Expr::Bin(
            op,
            Box::new(qualify_expr(*l, global)),
            Box::new(qualify_expr(*r, global)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_rgbasm as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn loads_and_registers() {
        assert_eq!(bytes(" ld a, b\n"), vec![0x78]);
        assert_eq!(bytes(" ld a, [hl]\n"), vec![0x7E]);
        assert_eq!(bytes(" ld [hl], b\n"), vec![0x70]);
        assert_eq!(bytes(" ld a, $12\n"), vec![0x3E, 0x12]);
        assert_eq!(bytes(" ld bc, $1234\n"), vec![0x01, 0x34, 0x12]);
        assert_eq!(bytes(" ld [hl+], a\n"), vec![0x22]);
        assert_eq!(bytes(" ld a, [hl-]\n"), vec![0x3A]);
        assert_eq!(bytes(" ld [$1234], a\n"), vec![0xEA, 0x34, 0x12]);
    }

    #[test]
    fn sm83_specific() {
        assert_eq!(bytes(" ldh [$ff80], a\n"), vec![0xE0, 0x80]);
        assert_eq!(bytes(" ldh a, [$ff80]\n"), vec![0xF0, 0x80]);
        assert_eq!(bytes(" ldh [c], a\n"), vec![0xE2]);
        assert_eq!(bytes(" ld hl, sp+3\n"), vec![0xF8, 0x03]);
        assert_eq!(bytes(" ld hl, sp-2\n"), vec![0xF8, 0xFE]);
        assert_eq!(bytes(" add sp, $03\n"), vec![0xE8, 0x03]);
        assert_eq!(bytes(" swap a\n"), vec![0xCB, 0x37]);
        assert_eq!(bytes(" stop\n"), vec![0x10, 0x00]);
    }

    #[test]
    fn alu_one_and_two_operand() {
        // rgbasm accepts both `sub b` and `sub a, b`.
        assert_eq!(bytes(" sub b\n"), vec![0x90]);
        assert_eq!(bytes(" sub a, b\n"), vec![0x90]);
        assert_eq!(bytes(" add a, b\n"), vec![0x80]);
        assert_eq!(bytes(" cp $05\n"), vec![0xFE, 0x05]);
    }

    #[test]
    fn embedded_bit_and_rst() {
        assert_eq!(bytes(" bit 7, [hl]\n"), vec![0xCB, 0x7E]);
        assert_eq!(bytes(" set 0, b\n"), vec![0xCB, 0xC0]);
        assert_eq!(bytes(" res 3, a\n"), vec![0xCB, 0x9F]);
        assert_eq!(bytes(" rst $38\n"), vec![0xFF]);
        assert_eq!(bytes(" rst $00\n"), vec![0xC7]);
    }

    #[test]
    fn jumps_and_labels() {
        // jr to a local label; SECTION sets the origin.
        assert_eq!(
            bytes("SECTION \"c\", ROM0[$0]\nstart:\n.loop:\n jr .loop\n"),
            vec![0x18, 0xFE]
        );
        assert_eq!(bytes(" jp $1234\n"), vec![0xC3, 0x34, 0x12]);
        assert_eq!(bytes(" jp hl\n"), vec![0xE9]);
        // Backward conditional + unconditional jr to a label at origin 0.
        assert_eq!(
            bytes("SECTION \"c\", ROM0[$0]\nl:\n jr nz, l\n jr l\n"),
            vec![0x20, 0xFE, 0x18, 0xFC]
        );
    }

    #[test]
    fn current_pc_symbol() {
        // rgbasm spells the program counter `@`. Byte-identical to rgbasm at
        // origin 0: `jr @` self-loops (-2), `jp @`/`ld hl,@` take address 0.
        assert_eq!(bytes(" jr @\n"), vec![0x18, 0xFE]);
        assert_eq!(bytes(" jp @\n"), vec![0xC3, 0x00, 0x00]);
        assert_eq!(bytes(" ld hl, @\n"), vec![0x21, 0x00, 0x00]);
        // `@+4` from the jr at 0 (len 2) → offset +2.
        assert_eq!(bytes(" jr @+4\n nop\n nop\n"), vec![0x18, 0x02, 0x00, 0x00]);
    }

    #[test]
    fn directives() {
        assert_eq!(
            bytes(" db $01, $02, \"AB\"\n"),
            vec![0x01, 0x02, 0x41, 0x42]
        );
        assert_eq!(bytes(" dw $1234\n"), vec![0x34, 0x12]);
        assert_eq!(bytes(" ds 3\n"), vec![0x00, 0x00, 0x00]);
        assert_eq!(bytes(" ds 2, $FF\n"), vec![0xFF, 0xFF]);
    }
}
