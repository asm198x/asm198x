//! The ACME 6502 dialect front-end.
//!
//! ACME is the C64 curriculum's assembler. The 6502 addressing-mode and
//! expression machinery is shared in [`super::mos6502`]; this module owns ACME's
//! surface: the program counter set with `*= $0801`, data laid with
//! `!byte`/`!word`/`!fill`/`!text`/`!scr`, symbols bound with a bare
//! `name = value`, anonymous `-`/`+` labels, and conditional assembly. ACME's
//! `<`/`>` byte operators apply to the whole expression to their right
//! ([`BytePrec::Loose`]).
//!
//! Encoding comes from [`isa::mos6502`]; the two-pass engine and byte emission
//! live in [`crate::engine`]. See `decisions/syntax-stance.md`.
//!
//! Not yet covered (no curriculum use): macros and `!for`.
//! `!zone` is accepted but inert (no `.`-local scoping yet).

use std::collections::{BTreeMap, BTreeSet};

use super::mos6502::{
    self, BytePrec, assignment_split, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, string_literal, top_level_rfind,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};

/// The ACME 6502 dialect.
pub(crate) struct Acme;

impl Dialect for Acme {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::mos6502::SET
    }

    /// ACME requires `*=` before any code or data (it rejects an implicit
    /// origin), so a forgotten `*=` errors rather than assembling at `$0000`.
    fn requires_explicit_origin(&self) -> bool {
        true
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        let set = self.instruction_set();
        let anons = prescan_anons(source);
        let units = tokenize_braces(source);
        let mut out = Vec::new();
        let mut env: BTreeMap<String, i64> = BTreeMap::new();
        // Names bound by `!set` (a *reassignable* variable). They live in `env`
        // like `=` constants, but are additionally marked here so each use is
        // baked to the value current at that point (reassignment can't be a
        // single pass-2 symbol).
        let mut set_names: BTreeSet<String> = BTreeSet::new();
        let mut idx = 0;
        process_block(
            set,
            &anons,
            &mut env,
            &mut set_names,
            &units,
            &mut idx,
            true,
            &mut out,
        )?;
        Ok(out)
    }
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
        if code.starts_with([' ', '\t']) {
            continue;
        }
        let (word, _) = split_first_word(code.trim());
        if let Some((sign, level)) = anon_marker(word) {
            let name = format!("\u{1}{sign}{level}#{}", defs.len());
            defs.push(AnonDef {
                line,
                sign,
                level,
                name,
            });
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
        matching
            .filter(|d| d.line <= ref_line)
            .max_by_key(|d| d.line)
    } else {
        matching
            .filter(|d| d.line >= ref_line)
            .min_by_key(|d| d.line)
    };
    chosen.map(|d| d.name.clone()).ok_or_else(|| {
        let run: String = std::iter::repeat_n(sign, level).collect();
        AsmError::new(
            line,
            format!("no anonymous label `{run}` in that direction"),
        )
    })
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
                units.push(Unit {
                    line,
                    text: seg.to_string(),
                });
            }
        };
        for (j, &b) in bytes.iter().enumerate() {
            match b {
                b'\'' if !in_str => in_char = !in_char,
                b'"' if !in_char => in_str = !in_str,
                b'{' | b'}' if !in_char && !in_str => {
                    flush(&code[start..j], &mut units);
                    units.push(Unit {
                        line,
                        text: (b as char).to_string(),
                    });
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
    set_names: &mut BTreeSet<String>,
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
            let taken = if emit {
                match &cond {
                    Conditional::IfDef(s) => env.contains_key(s),
                    Conditional::IfNDef(s) => !env.contains_key(s),
                    Conditional::If(e) => eval_condition(anons, env, e, line)?,
                }
            } else {
                false
            };
            process_block(set, anons, env, set_names, units, idx, emit && taken, out)?;
            if *idx < units.len() && units[*idx].text.trim() == "else" {
                *idx += 1;
                expect_brace(units, idx, line)?;
                process_block(set, anons, env, set_names, units, idx, emit && !taken, out)?;
            }
            continue;
        }
        // `!set name = expr` binds or rebinds a variable, folded now against the
        // current `env`. It emits nothing; later uses are baked to this value.
        if split_first_word(text).0 == "!set" {
            if emit {
                let (name, value) = parse_set(anons, env, text, line)?;
                env.insert(name.clone(), value);
                set_names.insert(name);
            }
            *idx += 1;
            continue;
        }
        if emit {
            let (label, op) = parse_statement(set, anons, env, &units[*idx].text, line)?;
            // Bake `!set` variables to their current value (a rebindable var
            // can't be one pass-2 symbol); real labels/constants stay symbolic.
            let op = op.map(|o| bake_set_vars(o, env, set_names));
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

/// Parse `!set name = expr`, folding `expr` against the current `env`.
fn parse_set(
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    text: &str,
    line: usize,
) -> Result<(String, i64), AsmError> {
    let rest = split_first_word(text).1.trim();
    let eq =
        assignment_split(rest).ok_or_else(|| AsmError::new(line, "`!set` needs `name = value`"))?;
    let name = rest[..eq].trim();
    if !is_ident(name) {
        return Err(AsmError::new(line, format!("invalid `!set` name `{name}`")));
    }
    let value = fold_const(&parse_value(anons, &rest[eq + 1..], line)?, env, line)?;
    Ok((name.to_string(), value))
}

/// Replace every reference to a `!set` variable in `op` with its current value,
/// leaving real labels and `=` constants symbolic (resolved in pass two).
fn bake_set_vars(
    op: Operation,
    env: &BTreeMap<String, i64>,
    set_names: &BTreeSet<String>,
) -> Operation {
    if set_names.is_empty() {
        return op;
    }
    let bake = |e: Expr| bake_expr(e, env, set_names);
    match op {
        Operation::Org(e) => Operation::Org(bake(e)),
        Operation::Equ(e) => Operation::Equ(bake(e)),
        Operation::Bytes(v) => Operation::Bytes(v.into_iter().map(bake).collect()),
        Operation::Words(v) => Operation::Words(v.into_iter().map(bake).collect()),
        Operation::Instruction {
            mnemonic,
            mode,
            operands,
        } => Operation::Instruction {
            mnemonic,
            mode,
            operands: operands.into_iter().map(bake).collect(),
        },
        // acme never emits pre-encoded instructions, entry points, or aligns
        // carrying set-var expressions.
        other => other,
    }
}

/// Recursively substitute `!set` variable symbols with their current numeric
/// value; other symbols pass through.
fn bake_expr(e: Expr, env: &BTreeMap<String, i64>, set_names: &BTreeSet<String>) -> Expr {
    match e {
        Expr::Sym(s) if set_names.contains(&s) => Expr::Num(env.get(&s).copied().unwrap_or(0)),
        Expr::Lo(inner) => Expr::Lo(Box::new(bake_expr(*inner, env, set_names))),
        Expr::Hi(inner) => Expr::Hi(Box::new(bake_expr(*inner, env, set_names))),
        Expr::Bank(inner) => Expr::Bank(Box::new(bake_expr(*inner, env, set_names))),
        Expr::Neg(inner) => Expr::Neg(Box::new(bake_expr(*inner, env, set_names))),
        Expr::Bin(op, l, r) => Expr::Bin(
            op,
            Box::new(bake_expr(*l, env, set_names)),
            Box::new(bake_expr(*r, env, set_names)),
        ),
        other => other,
    }
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
/// `>` comparisons are not supported (they collide with the byte prefixes); the
/// curriculum uses only `=`.
fn eval_condition(
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    cond: &str,
    line: usize,
) -> Result<bool, AsmError> {
    let value =
        |s: &str| -> Result<i64, AsmError> { fold_const(&parse_value(anons, s, line)?, env, line) };
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

    // `name = expr` binds a symbol (a lone `=`, not `==`/`!=`/`<=`/`>=`).
    if let Some(eq) = assignment_split(trimmed) {
        let name = trimmed[..eq].trim();
        let value = trimmed[eq + 1..].trim();
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid symbol name `{name}`")));
        }
        return Ok((
            Some(name.to_string()),
            Some(Operation::Equ(parse_value(anons, value, line)?)),
        ));
    }

    // Otherwise: an optional column-0 label, then a directive or instruction.
    let (label, rest) = split_label(set, anons, code, line)?;
    let op = parse_op(set, anons, env, rest, line)?;
    Ok((label, op))
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
    if anon_marker(word).is_some() {
        let name = anons
            .iter()
            .find(|d| d.line == line)
            .map(|d| d.name.clone())
            .ok_or_else(|| AsmError::new(line, "internal: anonymous label not pre-scanned"))?;
        return Ok((Some(name), remainder));
    }
    if let Some(name) = word.strip_suffix(':') {
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid label `{name}`")));
        }
        return Ok((Some(name.to_string()), remainder));
    }
    if word.starts_with('!') || set.instruction(&word.to_ascii_uppercase()).is_some() {
        return Ok((None, trimmed));
    }
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
        return Ok(Some(parse_directive(anons, env, directive, line)?));
    }
    let (mnemonic, remainder) = split_first_word(rest);
    let mnemonic = mnemonic.to_ascii_uppercase();
    let operand = mos6502::parse_operand(remainder, line, &|s, l| parse_value(anons, s, l))?;
    let insn = set
        .instruction(&mnemonic)
        .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
    let force_abs = address_forces_absolute(remainder);
    let (mode, operand) = mos6502::resolve_mode(insn, operand, env, force_abs, line)?;
    Ok(Some(Operation::Instruction {
        mnemonic,
        mode,
        operands: operand.into_iter().collect(),
    }))
}

// ---------------------------------------------------------------------------
// Directives
// ---------------------------------------------------------------------------

fn parse_directive(
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    directive: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let (name, rest) = split_first_word(directive);
    match name.to_ascii_lowercase().as_str() {
        "byte" | "by" | "8" => Ok(Operation::Bytes(parse_list(anons, rest, line)?)),
        "word" | "wo" | "16" => Ok(Operation::Words(parse_list(anons, rest, line)?)),
        "fill" => parse_fill(anons, env, rest, line),
        "align" => parse_align(anons, env, rest, line),
        "text" | "tx" => parse_text(anons, rest, line, |c| c),
        "scr" => parse_text(anons, rest, line, screen_code),
        "pet" => parse_text(anons, rest, line, petscii),
        // `!zone [title]` starts a new local-label scope. This dialect has no
        // `.`-local labels yet, so a zone has no effect on the bytes — accept it
        // and emit nothing. (The `!zone name { … }` block form is not covered.)
        "zone" | "zn" => Ok(Operation::Bytes(Vec::new())),
        other => Err(AsmError::new(
            line,
            format!("unsupported directive `!{other}`"),
        )),
    }
}

/// `!fill amount [, value]` — `amount` bytes of `value` (default 0). Both fold
/// against the parse-time `env` (so a `= const` like `MAX_NOTES` works), because
/// the size has to be known before pass two assigns addresses.
fn parse_fill(
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let amount_src = parts.next().unwrap_or("").trim();
    let amount = fold_const(&parse_value(anons, amount_src, line)?, env, line)?;
    let amount = usize::try_from(amount)
        .map_err(|_| AsmError::new(line, "`!fill` byte count must be a non-negative constant"))?;
    let value = match parts.next() {
        None => 0,
        Some(v) => {
            let n = fold_const(&parse_value(anons, v, line)?, env, line)?;
            u8::try_from(n)
                .map_err(|_| AsmError::new(line, "`!fill` value must be a constant byte"))?
        }
    };
    Ok(Operation::Bytes(vec![Expr::Num(i64::from(value)); amount]))
}

/// `!align andmask, value [, fill]` — advance the PC to the next address where
/// `pc & andmask == value`, filling with `fill` (default `$EA`, ACME's). `andmask`
/// and `value` are required; all three fold against the parse-time `env`. The pad
/// is PC-dependent, so the count is computed by the engine (`Operation::Align`),
/// not here.
fn parse_align(
    anons: &[AnonDef],
    env: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let parts = mos6502::split_top_level(rest, ',');
    if parts.len() < 2 || parts.len() > 3 {
        return Err(AsmError::new(
            line,
            "`!align` takes `andmask, value [, fill]`",
        ));
    }
    let andmask = fold_const(&parse_value(anons, &parts[0], line)?, env, line)?;
    let value = fold_const(&parse_value(anons, &parts[1], line)?, env, line)?;
    let fill = match parts.get(2) {
        None => 0xEA, // ACME's default fill byte
        Some(v) => {
            let n = fold_const(&parse_value(anons, v, line)?, env, line)?;
            u8::try_from(n)
                .map_err(|_| AsmError::new(line, "`!align` fill must be a constant byte"))?
        }
    };
    Ok(Operation::Align {
        andmask,
        value,
        fill,
    })
}

fn parse_list(anons: &[AnonDef], rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    mos6502::split_top_level(rest, ',')
        .iter()
        .map(|p| parse_value(anons, p, line))
        .collect()
}

/// Parse a text directive: a comma list mixing `"..."` strings (one byte per
/// character, passed through `convert`) and bare values (emitted as-is). ACME's
/// `!text` passes characters through unchanged; `!scr` maps them to screen codes.
fn parse_text(
    anons: &[AnonDef],
    rest: &str,
    line: usize,
    convert: fn(u8) -> u8,
) -> Result<Operation, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "text directive needs a value"));
    }
    let mut bytes = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            bytes.extend(text.bytes().map(|b| Expr::Num(i64::from(convert(b)))));
        } else {
            bytes.push(parse_value(anons, piece, line)?);
        }
    }
    Ok(Operation::Bytes(bytes))
}

/// ACME's `!pet` conversion: ASCII to PETSCII (the default, unshifted set). The
/// two swap letter case relative to each other — ASCII `A`–`Z` become `$C1`–`$DA`
/// and ASCII `a`–`z` become `$41`–`$5A`; everything else passes through. Derived
/// from the acme binary (`!pet "ABab" -> C1 C2 41 42`).
fn petscii(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' => c + 0x80,
        b'a'..=b'z' => c - 0x20,
        _ => c,
    }
}

/// ACME's `!scr` conversion: ASCII to C64 screen codes. Lowercase maps to the
/// uppercase screen codes (1–26) — the default uppercase/graphics set — so
/// lowercase source text shows as capitals. Derived from the acme binary.
fn screen_code(c: u8) -> u8 {
    match c {
        b'@' => 0x00,
        b'A'..=b'Z' => c,
        0x5B..=0x5F => c - 0x40,
        b'`' => 0x40,
        b'a'..=b'z' => c - 0x60,
        _ => c,
    }
}

// ---------------------------------------------------------------------------
// Value parsing (ACME surface over the shared expression core)
// ---------------------------------------------------------------------------

/// Parse an ACME value: a bare `-`/`+` run is an anonymous-label reference;
/// otherwise it is an expression with `<`/`>` applying loosely.
fn parse_value(anons: &[AnonDef], raw: &str, line: usize) -> Result<Expr, AsmError> {
    let trimmed = raw.trim();
    if let Some((sign, level)) = anon_marker(trimmed) {
        return Ok(Expr::Sym(resolve_anon(anons, sign, level, line, line)?));
    }
    mos6502::parse_expr(
        raw,
        line,
        parse_number,
        mos6502::ExprOpts {
            prec: BytePrec::Loose,
            byte_prefix: true,
            // ACME's `^` is exponentiation and its XOR is the `XOR`/`EOR`
            // keyword; `Power` also selects ACME's precedence ladder (bitwise/
            // shift looser than arithmetic).
            caret: mos6502::Caret::Power,
        },
    )
}

/// ACME sizes a hex literal by its written width: a `≥3`-digit hex address
/// (`$0010`, `$0400`) is 16-bit, forcing absolute addressing even when the value
/// is low. Detect that on the operand's address part (after stripping a trailing
/// `,X`/`,Y` index); other forms decide by value.
fn address_forces_absolute(operand: &str) -> bool {
    let t = operand.trim();
    let base = match top_level_rfind(t, ',') {
        Some(c) => t[..c].trim(),
        None => t,
    };
    base.strip_prefix('$')
        .is_some_and(|hex| hex.len() >= 3 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use crate::{AsmError, Assembly, assemble_acme};

    /// Assemble ACME source, giving it a default origin when it declares none —
    /// so the byte-output tests below needn't each set `*=`. (ACME requires `*=`
    /// before code/data; a source that sets its own origin starts with `*` and
    /// passes straight through. The requirement itself is covered by
    /// `emitting_without_an_origin_is_an_error`.)
    fn asm(src: &str) -> Result<Assembly, AsmError> {
        let sets_origin = src.lines().any(|l| l.trim_start().starts_with('*'));
        if sets_origin {
            assemble_acme(src)
        } else {
            assemble_acme(&format!("*= $c000\n{src}"))
        }
    }

    #[test]
    fn emitting_without_an_origin_is_an_error() {
        // ACME rejects code or data before `*=` ("Program counter undefined").
        let err = assemble_acme(" lda #1\n").expect_err("no origin");
        assert!(err.message.contains("program counter undefined"));
        // A symbol definition alone (no emission) is fine.
        assert!(assemble_acme("border = $d020\n").is_ok());
    }

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
        assert_eq!(
            asm("sta $0400,x").expect("absx").bytes,
            vec![0x9D, 0x00, 0x04]
        );
        assert_eq!(asm("lda ($20),y").expect("indy").bytes, vec![0xB1, 0x20]);
        assert_eq!(asm("lda ($20,x)").expect("indx").bytes, vec![0xA1, 0x20]);
    }

    #[test]
    fn hex_width_forces_absolute() {
        // `$10` is zero-page; `$0010` is 16-bit (absolute), matching acme — the
        // value is the same but the written width differs.
        assert_eq!(asm("lda $10").expect("zp").bytes, vec![0xA5, 0x10]);
        assert_eq!(asm("lda $0010").expect("abs").bytes, vec![0xAD, 0x10, 0x00]);
        assert_eq!(
            asm("sta $0000,x").expect("absx").bytes,
            vec![0x9D, 0x00, 0x00]
        );
        // Decimal and symbols still decide by value.
        assert_eq!(asm("lda 16").expect("dec").bytes, vec![0xA5, 0x10]);
    }

    #[test]
    fn arithmetic_and_byte_operators() {
        // ACME `<`/`>` are loose: they apply to the whole expression.
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
        let a = asm("*= $1000\n\
             \x20       ldx #0\n\
             -      inx\n\
             \x20       bne -\n\
             \x20       jmp +\n\
             \x20       nop\n\
             +      rts\n")
        .expect("anon");
        assert_eq!(
            a.bytes,
            vec![0xA2, 0x00, 0xE8, 0xD0, 0xFD, 0x4C, 0x09, 0x10, 0xEA, 0x60]
        );
    }

    #[test]
    fn nested_anonymous_levels_are_distinct() {
        let a = asm("*= $1000\n\
             -      lda #1\n\
             \x20       bne -\n\
             --     lda #2\n\
             \x20       beq --\n")
        .expect("nested");
        assert_eq!(
            a.bytes,
            vec![0xA9, 0x01, 0xD0, 0xFC, 0xA9, 0x02, 0xF0, 0xFC]
        );
    }

    #[test]
    fn self_referencing_backward_label() {
        let a = asm("*= $1000\n-      jmp -\n").expect("selfloop");
        assert_eq!(a.bytes, vec![0x4C, 0x00, 0x10]);
    }

    #[test]
    fn ifdef_skips_undefined_block() {
        let a = asm("*= $1000\n\
             \x20       lda #1\n\
             !ifdef SCREENSHOT_MODE {\n\
             \x20       lda #2\n\
             }\n\
             \x20       lda #3\n")
        .expect("ifdef");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xA9, 0x03]);
    }

    #[test]
    fn ifndef_inline_block_runs_and_defines() {
        let a = asm("!ifndef DEBUG { DEBUG = 0 }\n\
             *= $1000\n\
             !if DEBUG = 1 {\n\
             \x20       lda #$ff\n\
             } else {\n\
             \x20       lda #$00\n\
             }\n")
        .expect("ifndef+if-else");
        assert_eq!(a.bytes, vec![0xA9, 0x00]);
        assert_eq!(a.symbols.get("DEBUG"), Some(&0x0000));
    }

    #[test]
    fn if_true_takes_then_branch() {
        let a = asm("FLAG = 1\n*= $1000\n\
             !if FLAG = 1 {\n        lda #$11\n} else {\n        lda #$22\n}\n")
        .expect("if-true");
        assert_eq!(a.bytes, vec![0xA9, 0x11]);
    }

    #[test]
    fn text_emits_raw_bytes() {
        assert_eq!(
            asm("!text \"2064\"").expect("text").bytes,
            vec![0x32, 0x30, 0x36, 0x34]
        );
    }

    #[test]
    fn pet_converts_to_petscii() {
        // Byte-for-byte against acme: !pet swaps letter case into PETSCII,
        // passing other characters through.
        assert_eq!(
            asm("!pet \"ABab@[]\"").expect("pet").bytes,
            vec![0xC1, 0xC2, 0x41, 0x42, 0x40, 0x5B, 0x5D]
        );
    }

    #[test]
    fn caret_is_exponentiation_and_xor_is_the_keyword() {
        // ACME's `^` is power (right-assoc, tighter than `* /`), and bitwise XOR
        // is the keyword `XOR`/`EOR`. All byte-identical to acme.
        assert_eq!(asm("!word 5^3\n").expect("pow").bytes, vec![125, 0]);
        assert_eq!(asm("!word 2^8\n").expect("pow16").bytes, vec![0, 1]); // 256
        assert_eq!(asm("!word 2^3^2\n").expect("rassoc").bytes, vec![0, 2]); // 512
        assert_eq!(asm("!word 2*3^2\n").expect("prec").bytes, vec![18, 0]);
        assert_eq!(asm("!word 5 XOR 1\n").expect("xor").bytes, vec![4, 0]);
        assert_eq!(asm("!word 5 eor 1\n").expect("eor lc").bytes, vec![4, 0]);
    }

    #[test]
    fn bitwise_and_shift_bind_looser_than_arithmetic() {
        // ACME binds `& | << >>` looser than `+ - * /` (unlike the vasm ladder).
        // Byte-identical to acme.
        assert_eq!(asm("!word 1 & 3 + 1\n").expect("and").bytes, vec![0, 0]); // 1&(3+1)
        assert_eq!(asm("!word 1 << 2 + 1\n").expect("shl").bytes, vec![8, 0]); // 1<<(2+1)
        assert_eq!(asm("!word 2 * 3 & 4\n").expect("mul-and").bytes, vec![4, 0]); // (2*3)&4
        // & tighter than XOR tighter than |.
        assert_eq!(
            asm("!word 6 & 3 XOR 1\n").expect("and-xor").bytes,
            vec![3, 0]
        );
        assert_eq!(
            asm("!word 1 | 2 XOR 3\n").expect("xor-or").bytes,
            vec![1, 0]
        );
    }

    #[test]
    fn set_is_a_reassignable_variable() {
        // Byte-for-byte against acme. A `!set` variable takes the value current
        // at each use, so reassignment gives each `lda #n` its own value.
        let a = asm("*= $c000\n!set n=5\n lda #n\n!set n=7\n lda #n\n").expect("reassign");
        assert_eq!(a.bytes, vec![0xA9, 0x05, 0xA9, 0x07]);
        // Folds an expression of constants at the `!set`, and bakes into data.
        let b = asm("BASE = 10\n!set n=BASE+2\n!byte n, n*2\n").expect("expr");
        assert_eq!(b.bytes, vec![0x0C, 0x18]);
        // `<`/`>` byte operators apply to a baked set-var.
        let c = asm("!set p=$1234\n lda #<p\n ldx #>p\n").expect("byte ops");
        assert_eq!(c.bytes, vec![0xA9, 0x34, 0xA2, 0x12]);
    }

    #[test]
    fn align_pads_to_boundary_with_default_and_custom_fill() {
        // Byte-for-byte against acme. After `lda #1` at $1000 (pc=$1002):
        //   !align 7,0 pads 6 bytes to $1008, default fill $EA.
        let a = asm("*= $1000\n lda #1\n!align 7,0\n nop\n").expect("align");
        assert_eq!(
            a.bytes,
            vec![0xA9, 0x01, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA]
        );
        // A custom fill byte.
        let b = asm("*= $1000\n lda #1\n!align 7,0,$ff\n nop\n").expect("align fill");
        assert_eq!(
            b.bytes,
            vec![0xA9, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xEA]
        );
        // Already aligned ((pc & 3) == 2 at $1002): no padding.
        let c = asm("*= $1000\n lda #1\n!align 3,2\n nop\n").expect("aligned");
        assert_eq!(c.bytes, vec![0xA9, 0x01, 0xEA]);
    }

    #[test]
    fn zone_emits_nothing() {
        // `!zone` (bare and titled) is inert here: it only scopes `.`-locals,
        // which this dialect does not have. Matches acme's bytes (a901 a902).
        let a = asm("*= $1000\n!zone\n        lda #1\n!zone foo\n        lda #2\n").expect("zone");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xA9, 0x02]);
    }

    #[test]
    fn scr_converts_to_screen_codes() {
        assert_eq!(
            asm("!scr \"sid\"").expect("scr").bytes,
            vec![0x13, 0x09, 0x04]
        );
        assert_eq!(
            asm("!scr \"a, z\"").expect("scr comma").bytes,
            vec![0x01, 0x2C, 0x20, 0x1A]
        );
        assert_eq!(
            asm("!scr \"@A`\"").expect("scr edge").bytes,
            vec![0x00, 0x41, 0x40]
        );
    }

    #[test]
    fn nested_conditionals() {
        let a = asm("A = 1\nB = 0\n*= $1000\n\
             !if A = 1 {\n\
             \x20  !if B = 1 {\n        lda #$01\n\x20  } else {\n        lda #$02\n\x20  }\n\
             }\n")
        .expect("nested");
        assert_eq!(a.bytes, vec![0xA9, 0x02]);
    }
}
