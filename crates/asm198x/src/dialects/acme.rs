//! The ACME 6502 dialect front-end.
//!
//! ACME is the C64 curriculum's assembler. Its surface differs sharply from the
//! generic placeholder in [`super::mos6502`]: the program counter is set with
//! `*= $0801` (not `.org`); data is laid down with `!byte`/`!word`/`!fill` (not
//! `.byte`); and symbols are bound with a bare `name = value`. The 6502
//! *addressing-mode* resolution, by contrast, is dialect-agnostic — ACME and
//! ca65 write `lda #$00`, `sta $0400,x`, `(zp),y` identically — so that logic
//! lives here for now and will be lifted into a shared `mos6502` core once ca65
//! arrives (the same path pasmo → sjasmplus took for Z80).
//!
//! Encoding comes from [`isa::mos6502`]; the two-pass engine and byte emission
//! live in [`crate::engine`]. See `decisions/syntax-stance.md`.
//!
//! Implemented so far: `*=`, `name = expr`, `!byte`/`!by`/`!8`,
//! `!word`/`!wo`/`!16`, `!fill`, arithmetic expressions (`+ - * /` with C
//! precedence and parentheses), the `<`/`>` low/high-byte prefixes, `*` as the
//! program counter in value position, and anonymous `-`/`+` labels. Not yet:
//! the text directives (`!text`/`!pet`/`!scr`) and conditional assembly
//! (`!if`/`!ifdef`/`!ifndef`) — tracked in `decisions/syntax-stance.md`.

use std::collections::BTreeMap;

use crate::dialect::Dialect;
use crate::engine::{AsmError, BinOp, Expr, Operation, Statement};

/// The ACME 6502 dialect.
pub(crate) struct Acme;

impl Dialect for Acme {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::mos6502::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let set = self.instruction_set();
        // Anonymous `-`/`+` labels need a forward view (a `+` reference resolves
        // to a definition below it), so collect every definition up front.
        let anons = prescan_anons(source);
        // Split conditional braces into their own units so a recursive walk can
        // emit one branch and skip the other.
        let units = tokenize_braces(source);
        let mut out = Vec::new();
        // Symbols assigned with `name = const` so far, for `!if`/`!ifdef` tests.
        let mut env: BTreeMap<String, i64> = BTreeMap::new();
        let mut idx = 0;
        process_block(set, &anons, &mut env, &units, &mut idx, true, &mut out)?;
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Conditional assembly (`!if` / `!ifdef` / `!ifndef` … `{ }` … `else { }`)
// ---------------------------------------------------------------------------

/// One unit of source after `{`/`}` have been split out as standalone tokens.
struct Unit {
    line: usize,
    text: String,
}

/// Split the source into units, isolating each top-level `{` and `}` (outside
/// strings) so the conditional walker can treat them as block delimiters. Lines
/// without braces pass through whole, leading whitespace intact (so column-0
/// label detection still works).
fn tokenize_braces(source: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let code = strip_comment(raw);
        let mut in_char = false;
        let mut in_str = false;
        let mut start = 0;
        let bytes = code.as_bytes();
        let flush = |seg: &str, units: &mut Vec<Unit>| {
            if !seg.trim().is_empty() {
                units.push(Unit { line, text: seg.to_string() });
            }
        };
        for (j, &b) in bytes.iter().enumerate() {
            match b {
                b'\'' if !in_str => in_char = !in_char,
                b'"' if !in_char => in_str = !in_str,
                b'{' | b'}' if !in_char && !in_str => {
                    flush(&code[start..j], &mut units);
                    units.push(Unit { line, text: (b as char).to_string() });
                    start = j + 1;
                }
                _ => {}
            }
        }
        flush(&code[start..], &mut units);
    }
    units
}

/// The kind of a conditional directive and the text it tests.
enum Conditional {
    IfDef(String),
    IfNDef(String),
    If(String),
}

fn classify_conditional(text: &str) -> Option<Conditional> {
    let (word, rest) = split_first_word(text.trim());
    match word {
        "!ifdef" => Some(Conditional::IfDef(rest.trim().to_string())),
        "!ifndef" => Some(Conditional::IfNDef(rest.trim().to_string())),
        "!if" => Some(Conditional::If(rest.trim().to_string())),
        _ => None,
    }
}

/// Walk units, emitting statements. A conditional emits its taken branch and
/// recurses (with `emit = false`) through the other so braces stay balanced and
/// the skipped branch defines no symbols. Returns having consumed this block's
/// closing `}` (or at end of input for the top level).
fn process_block(
    set: &'static isa::InstructionSet,
    anons: &[AnonDef],
    env: &mut BTreeMap<String, i64>,
    units: &[Unit],
    idx: &mut usize,
    emit: bool,
    out: &mut Vec<Statement>,
) -> Result<(), AsmError> {
    while *idx < units.len() {
        let text = units[*idx].text.trim();
        let line = units[*idx].line;
        if text == "}" {
            *idx += 1;
            return Ok(());
        }
        if text == "{" || text == "else" {
            return Err(AsmError::new(line, format!("unexpected `{text}`")));
        }
        if let Some(cond) = classify_conditional(text) {
            *idx += 1;
            expect_brace(units, idx, line)?;
            // A skipped outer branch must not evaluate inner conditions (their
            // symbols may be undefined), so only test when actually emitting.
            let taken = if emit {
                match &cond {
                    Conditional::IfDef(s) => env.contains_key(s),
                    Conditional::IfNDef(s) => !env.contains_key(s),
                    Conditional::If(e) => eval_condition(anons, env, e, line)?,
                }
            } else {
                false
            };
            process_block(set, anons, env, units, idx, emit && taken, out)?;
            if *idx < units.len() && units[*idx].text.trim() == "else" {
                *idx += 1;
                expect_brace(units, idx, line)?;
                process_block(set, anons, env, units, idx, emit && !taken, out)?;
            }
            continue;
        }
        if emit {
            let (label, op) = parse_statement(set, anons, env, &units[*idx].text, line)?;
            // Record constant assignments so later conditions can test them.
            if let (Some(name), Some(Operation::Equ(e))) = (&label, &op)
                && let Ok(v) = fold_const(e, env, line)
            {
                env.insert(name.clone(), v);
            }
            if !(label.is_none() && op.is_none()) {
                out.push(Statement { line, label, op });
            }
        }
        *idx += 1;
    }
    Ok(())
}

fn expect_brace(units: &[Unit], idx: &mut usize, line: usize) -> Result<(), AsmError> {
    match units.get(*idx) {
        Some(u) if u.text.trim() == "{" => {
            *idx += 1;
            Ok(())
        }
        _ => Err(AsmError::new(line, "expected `{` after a conditional")),
    }
}

/// Evaluate an `!if` condition: a comparison (`=`, `!=`, `<=`, `>=`) of two
/// constant expressions, or a bare expression tested for non-zero. Single `<`/
/// `>` comparisons are not supported (they collide with the low/high-byte
/// prefixes); the curriculum uses only `=`.
fn eval_condition(
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    cond: &str,
    line: usize,
) -> Result<bool, AsmError> {
    let value = |s: &str| -> Result<i64, AsmError> {
        fold_const(&parse_value(anons, s, line)?, env, line)
    };
    let c = cond.trim();
    if let Some(i) = top_level_find(c, "!=") {
        return Ok(value(&c[..i])? != value(&c[i + 2..])?);
    }
    if let Some(i) = top_level_find(c, "<=") {
        return Ok(value(&c[..i])? <= value(&c[i + 2..])?);
    }
    if let Some(i) = top_level_find(c, ">=") {
        return Ok(value(&c[..i])? >= value(&c[i + 2..])?);
    }
    if let Some(i) = top_level_lone_eq(c) {
        return Ok(value(&c[..i])? == value(&c[i + 1..])?);
    }
    Ok(value(c)? != 0)
}

/// Fold an expression to a constant for conditions, resolving symbols against
/// the parse-time `env`. Errors on the location counter or an unknown symbol.
fn fold_const(e: &Expr, env: &BTreeMap<String, i64>, line: usize) -> Result<i64, AsmError> {
    let overflow = || AsmError::new(line, "arithmetic overflow in condition");
    Ok(match e {
        Expr::Num(n) => *n,
        Expr::Sym(s) => *env
            .get(s)
            .ok_or_else(|| AsmError::new(line, format!("`{s}` is not a parse-time constant")))?,
        Expr::Pc => return Err(AsmError::new(line, "`*` cannot be used in a condition")),
        Expr::Lo(b) => fold_const(b, env, line)? & 0xFF,
        Expr::Hi(b) => (fold_const(b, env, line)? >> 8) & 0xFF,
        Expr::Neg(b) => fold_const(b, env, line)?.checked_neg().ok_or_else(overflow)?,
        Expr::Bin(op, l, r) => {
            let a = fold_const(l, env, line)?;
            let b = fold_const(r, env, line)?;
            match op {
                BinOp::Add => a.checked_add(b).ok_or_else(overflow)?,
                BinOp::Sub => a.checked_sub(b).ok_or_else(overflow)?,
                BinOp::Mul => a.checked_mul(b).ok_or_else(overflow)?,
                BinOp::Div if b != 0 => a / b,
                BinOp::Div => return Err(AsmError::new(line, "division by zero in condition")),
            }
        }
    })
}

/// Find `pat` at the top level (outside parentheses and strings).
fn top_level_find(s: &str, pat: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let (mut in_char, mut in_str) = (false, false);
    for i in 0..bytes.len() {
        match bytes[i] {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b'(' if !in_char && !in_str => depth += 1,
            b')' if !in_char && !in_str => depth -= 1,
            _ if depth == 0 && !in_char && !in_str && s[i..].starts_with(pat) => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find a lone top-level `=` (ACME's equality test), skipping `==`/`<=`/`>=`/`!=`.
fn top_level_lone_eq(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let (mut in_char, mut in_str) = (false, false);
    for i in 0..bytes.len() {
        match bytes[i] {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b'(' if !in_char && !in_str => depth += 1,
            b')' if !in_char && !in_str => depth -= 1,
            b'=' if depth == 0 && !in_char && !in_str => {
                let prev = i.checked_sub(1).map(|p| bytes[p]);
                let next = bytes.get(i + 1).copied();
                if !matches!(prev, Some(b'!' | b'<' | b'>' | b'=')) && next != Some(b'=') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Strip a `;` line comment. A `;` inside a `'c'` char literal or `"..."` string
/// is left alone so it is not mistaken for a comment.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_char = false;
    let mut in_str = false;
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

// ---------------------------------------------------------------------------
// Anonymous labels (`-`/`--`/`+`/`++` …)
// ---------------------------------------------------------------------------

/// One anonymous-label definition: where it sits, its sign and level (the run
/// length, so `--` is level 2), and the unique synthetic name it binds. The
/// name carries a leading control char so it can never collide with a real
/// identifier.
struct AnonDef {
    line: usize,
    sign: char,
    level: usize,
    name: String,
}

/// A column-0 token made entirely of `-` or entirely of `+` is an anonymous
/// label. Returns its sign and level (run length).
fn anon_marker(word: &str) -> Option<(char, usize)> {
    let mut chars = word.chars();
    let first = chars.next()?;
    if (first == '-' || first == '+') && word.chars().all(|c| c == first) {
        Some((first, word.len()))
    } else {
        None
    }
}

/// Collect every anonymous-label definition in source order, assigning each a
/// unique synthetic name.
fn prescan_anons(source: &str) -> Vec<AnonDef> {
    let mut defs = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let code = strip_comment(raw);
        // A definition sits in column 0 (an indented `-`/`+` is an operator).
        if code.starts_with([' ', '\t']) {
            continue;
        }
        let (word, _) = split_first_word(code.trim());
        if let Some((sign, level)) = anon_marker(word) {
            let name = format!("\u{1}{sign}{level}#{}", defs.len());
            defs.push(AnonDef { line, sign, level, name });
        }
    }
    defs
}

/// Resolve an anonymous reference at `ref_line`: the nearest preceding `-`
/// definition (backward, same line allowed) or the nearest following `+`
/// definition (forward), at the same level.
fn resolve_anon(
    anons: &[AnonDef],
    sign: char,
    level: usize,
    ref_line: usize,
    line: usize,
) -> Result<String, AsmError> {
    let matching = anons.iter().filter(|d| d.sign == sign && d.level == level);
    let chosen = if sign == '-' {
        matching.filter(|d| d.line <= ref_line).max_by_key(|d| d.line)
    } else {
        matching.filter(|d| d.line >= ref_line).min_by_key(|d| d.line)
    };
    chosen.map(|d| d.name.clone()).ok_or_else(|| {
        let run: String = std::iter::repeat_n(sign, level).collect();
        AsmError::new(line, format!("no anonymous label `{run}` in that direction"))
    })
}

// ---------------------------------------------------------------------------
// Statement structure
// ---------------------------------------------------------------------------

/// Reduce one source line to an optional label and an optional operation.
fn parse_statement(
    set: &'static isa::InstructionSet,
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    code: &str,
    line: usize,
) -> Result<(Option<String>, Option<Operation>), AsmError> {
    let trimmed = code.trim();

    // `*= expr` (or `* = expr`) sets the program counter.
    if let Some(rest) = trimmed.strip_prefix('*') {
        let rest = rest.trim_start();
        if let Some(value) = rest.strip_prefix('=') {
            return Ok((None, Some(Operation::Org(parse_value(anons, value, line)?))));
        }
    }

    // `name = expr` binds a symbol. The `=` must be a lone assignment, not part
    // of `==`/`!=`/`<=`/`>=` (none of which start a statement here anyway).
    if let Some(eq) = assignment_split(trimmed) {
        let name = trimmed[..eq].trim();
        let value = trimmed[eq + 1..].trim();
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid symbol name `{name}`")));
        }
        return Ok((Some(name.to_string()), Some(Operation::Equ(parse_value(anons, value, line)?))));
    }

    // Otherwise: an optional column-0 label, then a directive or instruction.
    let (label, rest) = split_label(set, anons, code, line)?;
    let op = parse_op(set, anons, env, rest, line)?;
    Ok((label, op))
}

/// Find the byte index of a lone `=` used as assignment, or `None`. Skips `==`,
/// and a `*=` is handled before this is reached.
fn assignment_split(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' {
            let prev = i.checked_sub(1).map(|p| bytes[p]);
            let next = bytes.get(i + 1).copied();
            let part_of_cmp = matches!(prev, Some(b'!' | b'<' | b'>' | b'='))
                || next == Some(b'=');
            if !part_of_cmp {
                return Some(i);
            }
        }
    }
    None
}

/// Split a column-0 label from the rest. A leading-whitespace line has no label.
/// A column-0 first word that names a known mnemonic or a `!` directive is the
/// operation, not a label; an all-`-`/all-`+` run is an anonymous label.
fn split_label<'a>(
    set: &'static isa::InstructionSet,
    anons: &[AnonDef],
    code: &'a str,
    line: usize,
) -> Result<(Option<String>, &'a str), AsmError> {
    if code.starts_with([' ', '\t']) {
        return Ok((None, code.trim()));
    }
    let trimmed = code.trim();
    let (word, remainder) = split_first_word(trimmed);
    // An anonymous `-`/`+` label: bind the synthetic name for this line.
    if anon_marker(word).is_some() {
        let name = anons
            .iter()
            .find(|d| d.line == line)
            .map(|d| d.name.clone())
            .ok_or_else(|| AsmError::new(line, "internal: anonymous label not pre-scanned"))?;
        return Ok((Some(name), remainder));
    }
    // A `name:` label.
    if let Some(name) = word.strip_suffix(':') {
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid label `{name}`")));
        }
        return Ok((Some(name.to_string()), remainder));
    }
    // A column-0 mnemonic or directive is the op, not a label.
    if word.starts_with('!') || set.instruction(&word.to_ascii_uppercase()).is_some() {
        return Ok((None, trimmed));
    }
    // A bare column-0 identifier is a label; the rest (if any) is its op.
    if is_ident(word) {
        return Ok((Some(word.to_string()), remainder));
    }
    Err(AsmError::new(line, format!("cannot parse `{trimmed}`")))
}

/// Parse the operation part (after any label): a `!` directive or an instruction.
fn parse_op(
    set: &'static isa::InstructionSet,
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    if rest.is_empty() {
        return Ok(None);
    }
    if let Some(directive) = rest.strip_prefix('!') {
        return Ok(Some(parse_directive(anons, directive, line)?));
    }
    let (mnemonic, remainder) = split_first_word(rest);
    let mnemonic = mnemonic.to_ascii_uppercase();
    let operand = parse_operand(anons, remainder, line)?;
    let insn = set
        .instruction(&mnemonic)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
    let (mode, operand) = resolve_mode(insn, operand, env, line)?;
    Ok(Some(Operation::Instruction {
        mnemonic,
        mode,
        operands: operand.into_iter().collect(),
    }))
}

// ---------------------------------------------------------------------------
// Directives
// ---------------------------------------------------------------------------

fn parse_directive(anons: &[AnonDef], directive: &str, line: usize) -> Result<Operation, AsmError> {
    let (name, rest) = split_first_word(directive);
    match name.to_ascii_lowercase().as_str() {
        "byte" | "by" | "8" => Ok(Operation::Bytes(parse_list(anons, rest, line)?)),
        "word" | "wo" | "16" => Ok(Operation::Words(parse_list(anons, rest, line)?)),
        "fill" => parse_fill(anons, rest, line),
        other => Err(AsmError::new(line, format!("unsupported directive `!{other}`"))),
    }
}

/// `!fill amount [, value]` — `amount` bytes of `value` (default 0). Both must
/// fold to constants (the size has to be known at parse time).
fn parse_fill(anons: &[AnonDef], rest: &str, line: usize) -> Result<Operation, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let amount_src = parts.next().unwrap_or("").trim();
    let amount = match parse_value(anons, amount_src, line)? {
        Expr::Num(n) if n >= 0 => n as usize,
        _ => return Err(AsmError::new(line, "`!fill` needs a constant byte count")),
    };
    let value = match parts.next() {
        None => 0,
        Some(v) => match parse_value(anons, v, line)? {
            Expr::Num(n) if (0..=0xFF).contains(&n) => n as u8,
            _ => return Err(AsmError::new(line, "`!fill` value must be a constant byte")),
        },
    };
    Ok(Operation::Bytes(vec![Expr::Num(i64::from(value)); amount]))
}

fn parse_list(anons: &[AnonDef], rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    split_top_level(rest, ',')
        .iter()
        .map(|p| parse_value(anons, p, line))
        .collect()
}

// ---------------------------------------------------------------------------
// Operand syntax (parsed) and mode resolution (dialect -> spec)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Index {
    X,
    Y,
}

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

/// Resolve parsed operand syntax to a spec mode label, choosing zero-page vs
/// absolute from the literal (never a forward symbol) so the form size is
/// stable between passes.
fn resolve_mode(
    insn: &isa::Instruction,
    operand: OperandSyntax,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(&'static str, Option<Expr>), AsmError> {
    let resolved = match operand {
        OperandSyntax::None => {
            if insn.form("implied").is_some() {
                ("implied", None)
            } else if insn.form("accumulator").is_some() {
                ("accumulator", None)
            } else {
                return Err(AsmError::new(line, format!("`{}` requires an operand", insn.mnemonic)));
            }
        }
        OperandSyntax::Accumulator => ("accumulator", None),
        OperandSyntax::Immediate(e) => ("immediate", Some(e)),
        OperandSyntax::Indirect(e) => ("indirect", Some(e)),
        OperandSyntax::IndexedIndirect(e) => ("(indirect,x)", Some(e)),
        OperandSyntax::IndirectIndexed(e) => ("(indirect),y", Some(e)),
        OperandSyntax::Indexed(e, Index::X) => (pick_zp_abs(insn, &e, env, "zeropage,x", "absolute,x"), Some(e)),
        OperandSyntax::Indexed(e, Index::Y) => (pick_zp_abs(insn, &e, env, "zeropage,y", "absolute,y"), Some(e)),
        OperandSyntax::Direct(e) => {
            if insn.form("relative").is_some() {
                ("relative", Some(e))
            } else {
                (pick_zp_abs(insn, &e, env, "zeropage", "absolute"), Some(e))
            }
        }
    };
    Ok(resolved)
}

/// Choose zero-page when the operand folds to a constant that fits in a byte
/// (a literal, or a symbol already bound to a low value via `=`) and the
/// instruction has that form; otherwise absolute. Folding keys off parse-time
/// constants only — a forward or address symbol stays absolute — so the form
/// size is stable across passes. This matches ACME, which uses a zero-page
/// variable's value to pick the short form.
fn pick_zp_abs(
    insn: &isa::Instruction,
    e: &Expr,
    env: &BTreeMap<String, i64>,
    zp: &'static str,
    abs: &'static str,
) -> &'static str {
    let fits_zero_page = fold_const(e, env, 0).is_ok_and(|v| (0..=0xFF).contains(&v));
    if fits_zero_page && insn.form(zp).is_some() {
        zp
    } else {
        abs
    }
}

fn parse_operand(anons: &[AnonDef], raw: &str, line: usize) -> Result<OperandSyntax, AsmError> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(OperandSyntax::None);
    }
    if t.eq_ignore_ascii_case("a") {
        return Ok(OperandSyntax::Accumulator);
    }
    if let Some(rest) = t.strip_prefix('#') {
        return Ok(OperandSyntax::Immediate(parse_value(anons, rest, line)?));
    }
    if t.starts_with('(') {
        let upper = t.to_ascii_uppercase();
        if let Some(inner) = upper.strip_suffix(",X)") {
            return Ok(OperandSyntax::IndexedIndirect(parse_value(anons, &t[1..inner.len()], line)?));
        }
        if let Some(inner) = upper.strip_suffix("),Y") {
            return Ok(OperandSyntax::IndirectIndexed(parse_value(anons, &t[1..inner.len()], line)?));
        }
        if let Some(inner) = t.strip_suffix(')') {
            return Ok(OperandSyntax::Indirect(parse_value(anons, &inner[1..], line)?));
        }
        return Err(AsmError::new(line, format!("malformed indirect operand `{raw}`")));
    }
    // A trailing `,x`/`,y` indexes; the comma must be at the top level (not
    // inside a parenthesised sub-expression).
    if let Some(comma) = top_level_rfind(t, ',') {
        let index = match t[comma + 1..].trim() {
            i if i.eq_ignore_ascii_case("x") => Index::X,
            i if i.eq_ignore_ascii_case("y") => Index::Y,
            _ => return Err(AsmError::new(line, format!("expected `,X` or `,Y` in `{raw}`"))),
        };
        return Ok(OperandSyntax::Indexed(parse_value(anons, &t[..comma], line)?, index));
    }
    Ok(OperandSyntax::Direct(parse_value(anons, t, line)?))
}

// ---------------------------------------------------------------------------
// Expression parser: `+ - * /` with C precedence, `<`/`>` low/high-byte
// prefixes, parentheses, and `*` as the program counter in value position.
// ---------------------------------------------------------------------------

fn parse_value(anons: &[AnonDef], raw: &str, line: usize) -> Result<Expr, AsmError> {
    // A bare `-`/`--`/`+`/`++` operand is an anonymous-label reference, not
    // arithmetic — resolve it before tokenising.
    let trimmed = raw.trim();
    if let Some((sign, level)) = anon_marker(trimmed) {
        return Ok(Expr::Sym(resolve_anon(anons, sign, level, line, line)?));
    }
    let tokens = tokenize(raw, line)?;
    if tokens.is_empty() {
        return Err(AsmError::new(line, "expected a value"));
    }
    let mut parser = ExprParser { tokens, pos: 0, line };
    let expr = parser.expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(AsmError::new(line, format!("unexpected trailing tokens in `{}`", raw.trim())));
    }
    Ok(expr)
}

#[derive(Clone)]
enum Tok {
    Num(i64),
    Sym(String),
    /// `*` — disambiguated by position: the program counter as a value, or
    /// multiplication between two values.
    Star,
    Plus,
    Minus,
    Slash,
    /// `<` low-byte prefix.
    Lo,
    /// `>` high-byte prefix.
    Hi,
    LParen,
    RParen,
}

fn tokenize(raw: &str, line: usize) -> Result<Vec<Tok>, AsmError> {
    let chars: Vec<char> = raw.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ws if ws.is_whitespace() => i += 1,
            '+' => { tokens.push(Tok::Plus); i += 1; }
            '-' => { tokens.push(Tok::Minus); i += 1; }
            '*' => { tokens.push(Tok::Star); i += 1; }
            '/' => { tokens.push(Tok::Slash); i += 1; }
            '<' => { tokens.push(Tok::Lo); i += 1; }
            '>' => { tokens.push(Tok::Hi); i += 1; }
            '(' => { tokens.push(Tok::LParen); i += 1; }
            ')' => { tokens.push(Tok::RParen); i += 1; }
            '\'' => {
                if i + 2 < chars.len() && chars[i + 2] == '\'' {
                    let s: String = chars[i..=i + 2].iter().collect();
                    tokens.push(Tok::Num(parse_number(&s, line)?));
                    i += 3;
                } else {
                    return Err(AsmError::new(line, "malformed character literal"));
                }
            }
            '$' | '%' => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                tokens.push(Tok::Num(parse_number(&chars[start..i].iter().collect::<String>(), line)?));
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                tokens.push(Tok::Num(parse_number(&chars[start..i].iter().collect::<String>(), line)?));
            }
            l if l.is_ascii_alphabetic() || l == '_' || l == '.' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                tokens.push(Tok::Sym(chars[start..i].iter().collect()));
            }
            other => return Err(AsmError::new(line, format!("unexpected character `{other}` in expression"))),
        }
    }
    Ok(tokens)
}

struct ExprParser {
    tokens: Vec<Tok>,
    pos: usize,
    line: usize,
}

impl ExprParser {
    /// `<`/`>` bind loosest and apply to the whole expression to their right —
    /// `<label+1` is `<(label+1)`, matching ACME (verified against the binary).
    fn expr(&mut self) -> Result<Expr, AsmError> {
        match self.tokens.get(self.pos) {
            Some(Tok::Lo) => {
                self.pos += 1;
                Ok(Expr::Lo(Box::new(self.expr()?)))
            }
            Some(Tok::Hi) => {
                self.pos += 1;
                Ok(Expr::Hi(Box::new(self.expr()?)))
            }
            _ => self.add_sub(),
        }
    }

    fn add_sub(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.mul_div()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.mul_div()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn mul_div(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let right = self.unary()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, AsmError> {
        if matches!(self.tokens.get(self.pos), Some(Tok::Minus)) {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, AsmError> {
        let tok = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or_else(|| AsmError::new(self.line, "expected a value"))?;
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Sym(s) => Ok(Expr::Sym(s)),
            // A `*` in value position is the program counter.
            Tok::Star => Ok(Expr::Pc),
            Tok::LParen => {
                let inner = self.expr()?;
                if matches!(self.tokens.get(self.pos), Some(Tok::RParen)) {
                    self.pos += 1;
                    Ok(inner)
                } else {
                    Err(AsmError::new(self.line, "expected `)`"))
                }
            }
            _ => Err(AsmError::new(self.line, "expected a value")),
        }
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

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], s[idx..].trim()),
        None => (s, ""),
    }
}

/// Split on `sep` at the top level (outside parentheses), trimming each piece.
fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(s[start..i].trim());
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

/// The byte index of the last top-level (non-parenthesised) `sep`, if any.
fn top_level_rfind(s: &str, sep: char) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut found = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => found = Some(i),
            _ => {}
        }
    }
    found
}

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
    use crate::assemble_acme as asm;

    #[test]
    fn sets_pc_and_emits_bytes() {
        let a = asm("*= $0801\n!byte $0c,$08,$0a,$00\n").expect("byte");
        assert_eq!(a.origin, 0x0801);
        assert_eq!(a.bytes, vec![0x0C, 0x08, 0x0A, 0x00]);
    }

    #[test]
    fn star_equals_with_spaces() {
        assert_eq!(asm("* = $1000\n!byte 1\n").expect("spaced").origin, 0x1000);
    }

    #[test]
    fn symbol_assignment_binds_a_value() {
        let a = asm("border = $d020\n        lda #$00\n        sta border\n").expect("assign");
        assert_eq!(a.bytes, vec![0xA9, 0x00, 0x8D, 0x20, 0xD0]);
        assert_eq!(a.symbols.get("border"), Some(&0xD020));
    }

    #[test]
    fn addressing_modes_resolve() {
        assert_eq!(asm("lda #$01").expect("imm").bytes, vec![0xA9, 0x01]);
        assert_eq!(asm("lda $10").expect("zp").bytes, vec![0xA5, 0x10]);
        assert_eq!(asm("lda $0400").expect("abs").bytes, vec![0xAD, 0x00, 0x04]);
        assert_eq!(asm("sta $0400,x").expect("absx").bytes, vec![0x9D, 0x00, 0x04]);
        assert_eq!(asm("lda ($20),y").expect("indy").bytes, vec![0xB1, 0x20]);
        assert_eq!(asm("lda ($20,x)").expect("indx").bytes, vec![0xA1, 0x20]);
    }

    #[test]
    fn arithmetic_and_byte_operators() {
        assert_eq!(asm("lda #<$1234+1").expect("lo").bytes, vec![0xA9, 0x35]);
        assert_eq!(asm("lda #>$1234+1").expect("hi").bytes, vec![0xA9, 0x12]);
        assert_eq!(asm("lda #1+2*3").expect("prec").bytes, vec![0xA9, 0x07]);
        assert_eq!(asm("lda #(1+2)*3").expect("parens").bytes, vec![0xA9, 0x09]);
    }

    #[test]
    fn star_is_the_program_counter() {
        let a = asm("*= $0801\n        ldx #<*\n        lda #2*3\n").expect("pc");
        assert_eq!(a.bytes, vec![0xA2, 0x01, 0xA9, 0x06]);
    }

    #[test]
    fn fill_reserves_bytes() {
        assert_eq!(asm("!fill 3").expect("fill0").bytes, vec![0, 0, 0]);
        assert_eq!(asm("!fill 2, $ff").expect("fillv").bytes, vec![0xFF, 0xFF]);
    }

    #[test]
    fn forward_pc_gap_is_zero_filled() {
        let a = asm("*= $1000\n!byte 1\n*= $1003\n!byte 2\n").expect("gap");
        assert_eq!(a.bytes, vec![0x01, 0x00, 0x00, 0x02]);
    }

    #[test]
    fn anonymous_labels_resolve_by_direction() {
        // `bne -` branches back to the `-` above; `jmp +` jumps forward to the
        // `+` below. Byte layout verified against the acme binary.
        let a = asm(
            "*= $1000\n\
             \x20       ldx #0\n\
             -      inx\n\
             \x20       bne -\n\
             \x20       jmp +\n\
             \x20       nop\n\
             +      rts\n",
        )
        .expect("anon");
        assert_eq!(a.bytes, vec![0xA2, 0x00, 0xE8, 0xD0, 0xFD, 0x4C, 0x09, 0x10, 0xEA, 0x60]);
    }

    #[test]
    fn nested_anonymous_levels_are_distinct() {
        // `--` is a different pool from `-`; `beq --` reaches the `--` def.
        let a = asm(
            "*= $1000\n\
             -      lda #1\n\
             \x20       bne -\n\
             --     lda #2\n\
             \x20       beq --\n",
        )
        .expect("nested");
        // lda#1(2) bne -(-> -4 = $FC) lda#2(2) beq --(-> -4 = $FC)
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xD0, 0xFC, 0xA9, 0x02, 0xF0, 0xFC]);
    }

    #[test]
    fn self_referencing_backward_label() {
        // `- jmp -` on one line is the classic "loop forever" (jump to self).
        let a = asm("*= $1000\n-      jmp -\n").expect("selfloop");
        assert_eq!(a.bytes, vec![0x4C, 0x00, 0x10]);
    }

    #[test]
    fn ifdef_skips_undefined_block() {
        // SCREENSHOT_MODE is undefined, so the guarded block is dropped.
        let a = asm(
            "*= $1000\n\
             \x20       lda #1\n\
             !ifdef SCREENSHOT_MODE {\n\
             \x20       lda #2\n\
             }\n\
             \x20       lda #3\n",
        )
        .expect("ifdef");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xA9, 0x03]);
    }

    #[test]
    fn ifndef_inline_block_runs_and_defines() {
        // The classic ACME default: define a flag if it isn't already.
        let a = asm(
            "!ifndef DEBUG { DEBUG = 0 }\n\
             *= $1000\n\
             !if DEBUG = 1 {\n\
             \x20       lda #$ff\n\
             } else {\n\
             \x20       lda #$00\n\
             }\n",
        )
        .expect("ifndef+if-else");
        // DEBUG defaulted to 0, so the else branch emits `lda #$00`.
        assert_eq!(a.bytes, vec![0xA9, 0x00]);
        assert_eq!(a.symbols.get("DEBUG"), Some(&0x0000));
    }

    #[test]
    fn if_true_takes_then_branch() {
        let a = asm(
            "FLAG = 1\n*= $1000\n\
             !if FLAG = 1 {\n        lda #$11\n} else {\n        lda #$22\n}\n",
        )
        .expect("if-true");
        assert_eq!(a.bytes, vec![0xA9, 0x11]);
    }

    #[test]
    fn nested_conditionals() {
        let a = asm(
            "A = 1\nB = 0\n*= $1000\n\
             !if A = 1 {\n\
             \x20  !if B = 1 {\n        lda #$01\n\x20  } else {\n        lda #$02\n\x20  }\n\
             }\n",
        )
        .expect("nested");
        assert_eq!(a.bytes, vec![0xA9, 0x02]);
    }
}
