//! Shared 6502 core: operand-to-mode resolution and the expression parser used
//! by every 6502 dialect (ACME, ca65, …).
//!
//! The 6502 addressing-mode syntax is the same across assemblers — `lda #$00`,
//! `sta $0400,x`, `($20),y` are written identically — so operand classification,
//! the zero-page-vs-absolute choice, and the arithmetic expression grammar live
//! here. Each dialect keeps only what genuinely differs: its directives, label
//! and segment rules, comment and number formats, and where the `<`/`>`
//! byte-extraction operators sit in precedence ([`BytePrec`]). This mirrors the
//! `Z80Syntax` split for the Z80 dialects.

use std::cell::Cell;
use std::collections::BTreeMap;

use crate::engine::{AsmError, BinOp, Expr};

// ---------------------------------------------------------------------------
// Operand syntax (parsed) and mode resolution (dialect -> spec)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) enum Index {
    X,
    Y,
}

/// Operand syntax as parsed, before it is resolved to an addressing mode.
// `Clone` so a dialect that carries `OperandSyntax` in an AST node payload (the
// ca65 NES `Kind`) can project it back out of the tree.
#[derive(Clone)]
pub(crate) enum OperandSyntax {
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
/// absolute from a parse-time-constant operand (never a forward symbol) so the
/// form size is stable between passes.
pub(crate) fn resolve_mode(
    insn: &isa::Instruction,
    operand: OperandSyntax,
    env: &BTreeMap<String, i64>,
    force_abs: bool,
    line: usize,
) -> Result<(&'static str, Option<Expr>), AsmError> {
    let resolved = match operand {
        OperandSyntax::None => {
            if insn.form("implied").is_some() {
                ("implied", None)
            } else if insn.form("accumulator").is_some() {
                ("accumulator", None)
            } else {
                return Err(AsmError::new(
                    line,
                    format!("`{}` requires an operand", insn.mnemonic),
                ));
            }
        }
        OperandSyntax::Accumulator => ("accumulator", None),
        OperandSyntax::Immediate(e) => ("immediate", Some(e)),
        OperandSyntax::Indirect(e) => ("indirect", Some(e)),
        OperandSyntax::IndexedIndirect(e) => ("(indirect,x)", Some(e)),
        OperandSyntax::IndirectIndexed(e) => ("(indirect),y", Some(e)),
        OperandSyntax::Indexed(e, Index::X) => (
            pick_zp_abs(insn, &e, env, force_abs, "zeropage,x", "absolute,x"),
            Some(e),
        ),
        OperandSyntax::Indexed(e, Index::Y) => (
            pick_zp_abs(insn, &e, env, force_abs, "zeropage,y", "absolute,y"),
            Some(e),
        ),
        OperandSyntax::Direct(e) => {
            if insn.form("relative").is_some() {
                ("relative", Some(e))
            } else {
                (
                    pick_zp_abs(insn, &e, env, force_abs, "zeropage", "absolute"),
                    Some(e),
                )
            }
        }
    };
    Ok(resolved)
}

/// Choose zero-page when the operand folds to a constant that fits in a byte (a
/// literal, or a symbol already bound to a low value) and the instruction has
/// that form; otherwise absolute. A forward or address symbol stays absolute,
/// keeping form sizes stable across passes. `force_abs` skips the zero-page
/// pick — ACME treats a `≥3`-digit hex literal (`$0010`) as 16-bit even though
/// its value is low.
fn pick_zp_abs(
    insn: &isa::Instruction,
    e: &Expr,
    env: &BTreeMap<String, i64>,
    force_abs: bool,
    zp: &'static str,
    abs: &'static str,
) -> &'static str {
    let fits_zero_page = !force_abs && fold_const(e, env, 0).is_ok_and(|v| (0..=0xFF).contains(&v));
    if fits_zero_page && insn.form(zp).is_some() {
        zp
    } else {
        abs
    }
}

/// Fold an expression to a constant, resolving symbols against the parse-time
/// `env`. Errors on the location counter or an unknown symbol. A parse-time
/// constant context, so `*` (PC) is not available.
pub(crate) fn fold_const(
    e: &Expr,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<i64, AsmError> {
    e.eval_with(&|s| env.get(s).copied(), None, line)
}

/// Parse operand structure (immediate, indirect, indexed, direct), delegating
/// each sub-expression to the dialect's `value` parser. The 6502 operand shapes
/// are the same across dialects; only the expression contents differ.
pub(crate) fn parse_operand(
    raw: &str,
    line: usize,
    value: &dyn Fn(&str, usize) -> Result<Expr, AsmError>,
) -> Result<OperandSyntax, AsmError> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(OperandSyntax::None);
    }
    if t.eq_ignore_ascii_case("a") {
        return Ok(OperandSyntax::Accumulator);
    }
    if let Some(rest) = t.strip_prefix('#') {
        return Ok(OperandSyntax::Immediate(value(rest, line)?));
    }
    if t.starts_with('(') {
        // The three indirect forms, tolerant of spaces around `,` and `)`:
        //   `(expr)`      indirect            `(expr,x)`  indexed-indirect
        //   `(expr),y`    indirect-indexed
        let malformed = || AsmError::new(line, format!("malformed indirect operand `{raw}`"));
        if let Some(inner) = t.strip_suffix(')') {
            let inner = &inner[1..];
            if let Some(c) = top_level_rfind(inner, ',')
                && inner[c + 1..].trim().eq_ignore_ascii_case("x")
            {
                return Ok(OperandSyntax::IndexedIndirect(value(&inner[..c], line)?));
            }
            return Ok(OperandSyntax::Indirect(value(inner, line)?));
        }
        let close = t.rfind(')').ok_or_else(malformed)?;
        let after = t[close + 1..].trim();
        let idx = after.strip_prefix(',').map(str::trim);
        if idx.is_some_and(|i| i.eq_ignore_ascii_case("y")) {
            return Ok(OperandSyntax::IndirectIndexed(value(&t[1..close], line)?));
        }
        return Err(malformed());
    }
    if let Some(comma) = top_level_rfind(t, ',') {
        let index = match t[comma + 1..].trim() {
            i if i.eq_ignore_ascii_case("x") => Index::X,
            i if i.eq_ignore_ascii_case("y") => Index::Y,
            _ => {
                return Err(AsmError::new(
                    line,
                    format!("expected `,X` or `,Y` in `{raw}`"),
                ));
            }
        };
        return Ok(OperandSyntax::Indexed(value(&t[..comma], line)?, index));
    }
    Ok(OperandSyntax::Direct(value(t, line)?))
}

// ---------------------------------------------------------------------------
// Expression parser: `+ - * /` with C precedence, parentheses, `*` as the
// program counter, and `<`/`>` low/high-byte prefixes whose precedence the
// dialect selects via `BytePrec`.
// ---------------------------------------------------------------------------

/// Where the `<`/`>` byte-extraction operators sit in precedence.
///
/// - `Loose`: they apply to the whole expression to their right, so
///   `<a+1` is `<(a+1)` (ACME).
/// - `Tight`: they are unary operators binding to the next term, so
///   `<a+1` is `(<a)+1` (ca65).
///
/// Both were verified against the respective assembler binaries.
#[derive(Clone, Copy)]
pub(crate) enum BytePrec {
    Loose,
    Tight,
}

/// What a bare `^` means in a dialect's expressions.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Caret {
    /// Binary bitwise XOR only (acme, 6809, 68000).
    Xor,
    /// Binary XOR *and* a unary 65816 bank-byte prefix (ca65), the role chosen
    /// by position: a leading `^x` is the bank byte, `a^b` is XOR.
    BankOrXor,
    /// `^` is exponentiation, not XOR (ACME: `5^3` = 125). Selecting this also
    /// switches the expression grammar to ACME's — its bitwise/shift operators
    /// bind *looser* than arithmetic, and bitwise XOR is the keyword `XOR`
    /// (alias `EOR`). See [`ExprParser::acme_or`].
    Power,
}

/// One argument to an expression function: a value, or a string literal.
///
/// Strings exist here and nowhere else in an expression. A function that
/// *returns* a string — ca65's `.concat`/`.sprintf`, rgbasm's `STRCAT` — is a
/// text-substitution feature rather than an expression one, and is not reached
/// by this.
#[derive(Debug, Clone)]
pub(crate) enum ExprArg {
    Value(Expr),
    Text(String),
}

impl ExprArg {
    /// The value, for a function whose argument must be one.
    pub(crate) fn value(self, name: &str, line: usize) -> Result<Expr, AsmError> {
        match self {
            ExprArg::Value(e) => Ok(e),
            ExprArg::Text(_) => Err(AsmError::new(
                line,
                format!("`{name}` takes a value, not a string"),
            )),
        }
    }

    /// The text, for a function whose argument must be a string.
    pub(crate) fn text(self, name: &str, line: usize) -> Result<String, AsmError> {
        match self {
            ExprArg::Text(t) => Ok(t),
            ExprArg::Value(_) => Err(AsmError::new(
                line,
                format!("`{name}` takes a string, not a value"),
            )),
        }
    }
}

/// How a dialect turns an expression function call — `name(arg)` — into an
/// [`Expr`]. See [`ExprOpts::function`].
///
/// Three limits, each of which blocks a known group of reference functions and
/// none of which is worked around here:
///
/// - **No string-*returning* functions.** A string argument is fine — see
///   [`ExprArg`] — because it is consumed at parse time and yields a number.
///   A function that hands a string *back* (ca65's `.concat`/`.sprintf`,
///   rgbasm's `STRCAT`/`STRFMT`) would need an expression to evaluate to
///   something other than an `i64`, and those are a text-substitution feature
///   rather than an expression one.
/// - **No parse-position symbol knowledge.** ca65's `.defined(X)` is
///   *positional* — `0` before the definition and `1` after — and nothing is
///   defined yet when a walk parses an expression, so it cannot fold here at
///   all. A dialect whose pipeline visits statements in source order can
///   answer it later; ca65 does, by emitting a marker its projection resolves.
///   One whose symbols resolve once at the end cannot, and should refuse it
///   rather than answer `1` both times.
pub(crate) type ExprFn = fn(&str, Vec<ExprArg>, usize) -> Result<Expr, AsmError>;

/// Which comparison spellings a dialect takes, and what it answers for true.
/// Every field is probed, not inferred — see `docs/comparison-operators.md`.
#[derive(Clone, Copy, Default)]
pub(crate) struct Compare {
    /// `=` as equality. lwasm has no such operator.
    pub eq: bool,
    /// `==` as equality (rgbasm, sjasmplus).
    pub eq_eq: bool,
    /// `<>` as inequality. sjasmplus refuses it.
    pub ne_angle: bool,
    /// `!=` as inequality (rgbasm, sjasmplus).
    pub ne_bang: bool,
    /// `<` and `>` as relations. Every dialect that compares at all has these,
    /// **including** the ones where a leading `<` is the low-byte prefix: the
    /// two never occupy the same position.
    pub relational: bool,
    /// `<=` and `>=`. lwasm refuses them.
    pub ordered_eq: bool,
    /// True is `$FF` rather than `1` (vasm, sjasmplus, pasmo).
    pub minus_one: bool,
}

/// Expression-syntax knobs that vary by dialect. The bitwise/shift operators
/// `& | << >>` are available in every dialect (the engine AST supports them);
/// these knobs vary the `<`/`>` byte-prefix behaviour, what `^` means, and
/// which comparisons exist.
#[derive(Clone, Copy)]
pub(crate) struct ExprOpts {
    /// Where the `<`/`>` byte-extraction operators sit in precedence.
    pub prec: BytePrec,
    /// `<`/`>` are low/high-byte prefixes (the 6502 family). When false
    /// (68000/6809) a lone `<`/`>` is not a byte operator — though the `<<`/`>>`
    /// shifts still parse.
    pub byte_prefix: bool,
    /// What `^` means.
    pub caret: Caret,
    /// Whether `@` is the program-counter symbol (rgbasm). The 6502-family
    /// dialects spell the PC `*`; ca65 uses `@` for cheap-local labels instead,
    /// so this is off by default and only rgbasm turns it on. When on, `@`
    /// tokenises exactly like `*` (a PC atom).
    pub at_is_pc: bool,
    /// Whether `!` is a bitwise-OR operator, an alias for `|` at the same
    /// precedence (vasm). Verified against `vasmm68k_mot`: `(1<<6)!2` = 66,
    /// `6!1&3` = 7 (binds looser than `&`, like `|`). Off elsewhere — other
    /// dialects (e.g. rgbasm) give `!` a different meaning.
    pub bang_is_or: bool,
    /// How this dialect builds an expression **function call** — a symbol
    /// immediately followed by `(`, as in ca65's `.lobyte(addr)`. `None` (the
    /// default) leaves `name(...)` a parse error, which is what it has always
    /// been: no dialect gave a bare symbol a call meaning before.
    ///
    /// The builder receives the name as written and the parsed arguments, so a
    /// function whose argument is a *name* rather than a value (ca65's
    /// `.sizeof(Point)`) reads it back as an `Expr::Sym`.
    pub function: Option<ExprFn>,
    /// Whether this dialect has ca65's logical layer: `&&`/`.and`, `||`/`.or`,
    /// `.xor`, `!`/`.not` and `~`/`.bitnot`, plus the keyword spellings of the
    /// bitwise operators it already has.
    ///
    /// Off everywhere else, and it has to be: `!` is bitwise OR in vasm, and a
    /// bare `.and` is an ordinary symbol in a dialect that does not know the
    /// word.
    pub logical: bool,
    /// `::` inside a name (ca65's `scope::name`, and `::name` for the top
    /// level). Off everywhere else, where a `:` ends a name — a lone `:` is an
    /// anonymous label in several dialects, so only the doubled form is taken.
    pub scoped_names: bool,
    /// A `.` inside a number, and an optional `qN` precision suffix after it —
    /// rgbasm's fixed-point literals, where `1.0` is `$10000`. Off elsewhere,
    /// where a `.` after a digit is not part of the number.
    pub fixed_point: bool,
    /// Comparison support. `Default` is none, which is what a dialect whose
    /// reference has no comparison operators wants.
    pub compare: Compare,
}

thread_local! {
    /// The ASL-family base for an otherwise unadorned alphanumeric number
    /// token. Outside that walk, each dialect's existing callback retains
    /// complete control of its number syntax.
    static IMPLICIT_RADIX: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Run an expression-parsing operation with `radix` as the base for an
/// unadorned alphanumeric number token, restoring the caller's base afterward.
///
/// This is dynamically scoped because the expression tokenizer is shared by
/// every dialect while ASL's `RADIX` is parser state owned by one source walk.
/// The thread-local boundary keeps concurrent assemblies independent, and the
/// restore guard makes nested parsing retain the outer walk's state.
pub(crate) fn with_implicit_radix<T>(radix: u32, f: impl FnOnce() -> T) -> T {
    IMPLICIT_RADIX.with(|current| {
        struct Reset<'a> {
            current: &'a Cell<Option<u32>>,
            previous: Option<u32>,
        }

        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.current.set(self.previous);
            }
        }

        let reset = Reset {
            current,
            previous: current.replace(Some(radix)),
        };
        let result = f();
        drop(reset);
        result
    })
}

fn implicit_number(token: &str) -> Option<i64> {
    IMPLICIT_RADIX.with(|current| {
        let radix = current.get()?;
        token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() && c.is_digit(radix))
            .then(|| i64::from_str_radix(token, radix).ok())
            .flatten()
    })
}

/// Parse a value expression. `parse_number` lexes the dialect's numeric literal
/// forms; `opts` selects the dialect's operator syntax.
pub(crate) fn parse_expr(
    raw: &str,
    line: usize,
    parse_number: fn(&str, usize) -> Result<i64, AsmError>,
    opts: ExprOpts,
) -> Result<Expr, AsmError> {
    let tokens = tokenize(raw, line, parse_number, opts)?;
    if tokens.is_empty() {
        return Err(AsmError::new(line, "expected a value"));
    }
    let mut parser = ExprParser {
        tokens,
        pos: 0,
        line,
        prec: opts.prec,
        caret: opts.caret,
        function: opts.function,
        compare_opts: opts.compare,
        logical: opts.logical,
    };
    let expr = parser.expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(AsmError::new(
            line,
            format!("unexpected trailing tokens in `{}`", raw.trim()),
        ));
    }
    Ok(expr)
}

#[derive(Clone)]
enum Tok {
    Num(i64),
    Sym(String),
    Star,
    Plus,
    Minus,
    Slash,
    Lo,
    Hi,
    LParen,
    RParen,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    /// Exponentiation (`^` in ACME, where it is *not* XOR).
    Pow,
    /// A comparison, in operator position. A leading `<`/`>` is still the
    /// byte prefix where the dialect has one; these are only produced after a
    /// value, where a prefix cannot appear.
    Cmp(BinOp),
    /// A string literal. Only ever a **function argument** — see [`ExprArg`].
    /// An expression still evaluates to an `i64`, so a string never becomes a
    /// value; `.strlen("abc")` consumes it at parse time and yields a number.
    Str(String),
    /// ca65's logical operators and its bitwise complement — `&&`/`.and`,
    /// `||`/`.or`, `.xor`, `!`/`.not`, `~`/`.bitnot`. Only ca65 produces
    /// these: `!` is bitwise OR in vasm and `~` is nothing anywhere else.
    LogAnd,
    LogOr,
    LogXor,
    LogNot,
    BitNot,
    /// ca65's `.mod`. It has no symbol spelling there — `%` is a binary
    /// literal — so the keyword is the only way in.
    Mod,
    /// Argument separator inside a function call. Nothing else in an
    /// expression takes one — every caller splits its operand list on commas
    /// before an expression reaches here, and does so paren-aware, so a
    /// comma survives only inside `f(a,b)`.
    Comma,
}

/// ca65's keyword spelling of an operator, if that is what the word is.
///
/// Every one of these has a symbol twin except `.mod`, which has none — `%`
/// is a binary literal there. Mapping the keyword to the twin's token is what
/// makes `4 + 1 .bitand 1` and `4 + 1 & 1` the same expression, precedence
/// included.
fn keyword_operator(word: &str) -> Option<Tok> {
    Some(match word.to_ascii_lowercase().as_str() {
        ".bitand" => Tok::And,
        ".bitor" => Tok::Or,
        ".bitxor" => Tok::Xor,
        ".bitnot" => Tok::BitNot,
        ".shl" => Tok::Shl,
        ".shr" => Tok::Shr,
        ".mod" => Tok::Mod,
        ".and" => Tok::LogAnd,
        ".or" => Tok::LogOr,
        ".xor" => Tok::LogXor,
        ".not" => Tok::LogNot,
        _ => return None,
    })
}

fn tokenize(
    raw: &str,
    line: usize,
    parse_number: fn(&str, usize) -> Result<i64, AsmError>,
    opts: ExprOpts,
) -> Result<Vec<Tok>, AsmError> {
    let chars: Vec<char> = raw.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // Whether the token before this one ended a value. A prefix operator
        // can only appear where one did not; an infix comparison only where
        // one did. That is the whole of how `<` tells its two meanings apart.
        let after_value = matches!(
            tokens.last(),
            Some(Tok::Num(_) | Tok::Sym(_) | Tok::Star | Tok::RParen | Tok::Str(_))
        );
        let c = chars[i];
        match c {
            ws if ws.is_whitespace() => i += 1,
            '+' => {
                tokens.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Tok::Star);
                i += 1;
            }
            // rgbasm spells the program counter `@`; it tokenises as a PC atom,
            // the same node `*` produces for the other dialects.
            '@' if opts.at_is_pc => {
                tokens.push(Tok::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Tok::Slash);
                i += 1;
            }
            // `<<`/`>>` are shifts everywhere. A lone `<`/`>` is a
            // low/high-byte **prefix** where `byte_prefix` is on, and a
            // **comparison** where one follows a value — the two never occupy
            // the same position, which is why ca65 and acme have both
            // (`docs/comparison-operators.md`).
            '<' => {
                let c = opts.compare;
                if chars.get(i + 1) == Some(&'<') {
                    tokens.push(Tok::Shl);
                    i += 2;
                } else if after_value && c.ne_angle && chars.get(i + 1) == Some(&'>') {
                    tokens.push(Tok::Cmp(BinOp::Ne));
                    i += 2;
                } else if after_value && c.ordered_eq && chars.get(i + 1) == Some(&'=') {
                    tokens.push(Tok::Cmp(BinOp::Le));
                    i += 2;
                } else if after_value && c.relational {
                    tokens.push(Tok::Cmp(BinOp::Lt));
                    i += 1;
                } else if opts.byte_prefix {
                    tokens.push(Tok::Lo);
                    i += 1;
                } else {
                    return Err(AsmError::new(line, "expected `<<` (shift)"));
                }
            }
            '>' => {
                let c = opts.compare;
                if chars.get(i + 1) == Some(&'>') {
                    tokens.push(Tok::Shr);
                    i += 2;
                } else if after_value && c.ordered_eq && chars.get(i + 1) == Some(&'=') {
                    tokens.push(Tok::Cmp(BinOp::Ge));
                    i += 2;
                } else if after_value && c.relational {
                    tokens.push(Tok::Cmp(BinOp::Gt));
                    i += 1;
                } else if opts.byte_prefix {
                    tokens.push(Tok::Hi);
                    i += 1;
                } else {
                    return Err(AsmError::new(line, "expected `>>` (shift)"));
                }
            }
            '=' if opts.compare.eq_eq && chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Cmp(BinOp::Eq));
                i += 2;
            }
            '=' if opts.compare.eq => {
                tokens.push(Tok::Cmp(BinOp::Eq));
                i += 1;
            }
            '!' if opts.compare.ne_bang && chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Cmp(BinOp::Ne));
                i += 2;
            }
            '&' if opts.logical && chars.get(i + 1) == Some(&'&') => {
                tokens.push(Tok::LogAnd);
                i += 2;
            }
            '&' => {
                tokens.push(Tok::And);
                i += 1;
            }
            '|' if opts.logical && chars.get(i + 1) == Some(&'|') => {
                tokens.push(Tok::LogOr);
                i += 2;
            }
            '|' => {
                tokens.push(Tok::Or);
                i += 1;
            }
            '~' if opts.logical => {
                tokens.push(Tok::BitNot);
                i += 1;
            }
            '!' if opts.logical => {
                tokens.push(Tok::LogNot);
                i += 1;
            }
            // vasm accepts `!` as a second spelling of bitwise OR, at the same
            // precedence as `|` (verified against vasmm68k_mot).
            '!' if opts.bang_is_or => {
                tokens.push(Tok::Or);
                i += 1;
            }
            // `^` is XOR (or, in ca65, the bank-byte prefix when it leads a
            // term) in most dialects, but exponentiation in ACME.
            '^' => {
                tokens.push(match opts.caret {
                    Caret::Power => Tok::Pow,
                    Caret::Xor | Caret::BankOrXor => Tok::Xor,
                });
                i += 1;
            }
            '"' => {
                let start = i + 1;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(AsmError::new(line, "unterminated string in expression"));
                }
                tokens.push(Tok::Str(chars[start..i].iter().collect()));
                i += 1;
            }
            ',' => {
                tokens.push(Tok::Comma);
                i += 1;
            }
            '(' => {
                tokens.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Tok::RParen);
                i += 1;
            }
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
                tokens.push(Tok::Num(parse_number(
                    &chars[start..i].iter().collect::<String>(),
                    line,
                )?));
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                // `3.7`, and `3.7q8` for a precision other than the default.
                // Only a digit may follow the point: `3.foo` is a number and a
                // label, which is how a dialect without this reads every case.
                if opts.fixed_point
                    && chars.get(i) == Some(&'.')
                    && chars.get(i + 1).is_some_and(char::is_ascii_digit)
                {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                        i += 1;
                    }
                }
                let token = chars[start..i].iter().collect::<String>();
                tokens.push(Tok::Num(match implicit_number(&token) {
                    Some(value) => value,
                    None => parse_number(&token, line)?,
                }));
            }
            l if l.is_ascii_alphabetic()
                || l == '_'
                || l == '.'
                || (l == ':' && opts.scoped_names) =>
            {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric()
                        || chars[i] == '_'
                        || chars[i] == '.'
                        // `::` binds into the name; a single `:` does not, so a
                        // trailing one is left for whatever else reads it.
                        || (opts.scoped_names
                            && chars[i] == ':'
                            && chars.get(i + 1) == Some(&':')))
                {
                    i += if chars[i] == ':' { 2 } else { 1 };
                }
                let word: String = chars[start..i].iter().collect();
                if let Some(value) = implicit_number(&word) {
                    tokens.push(Tok::Num(value));
                    continue;
                }
                // ACME spells bitwise XOR as the keyword `XOR` (alias `EOR`);
                // `^` is exponentiation there. Elsewhere these are ordinary
                // symbols. (`EOR` the mnemonic never reaches here — the operand
                // text is parsed after the mnemonic is split off.)
                if opts.caret == Caret::Power
                    && (word.eq_ignore_ascii_case("xor") || word.eq_ignore_ascii_case("eor"))
                {
                    tokens.push(Tok::Xor);
                } else if let Some(op) = opts.logical.then(|| keyword_operator(&word)).flatten() {
                    // ca65 spells every operator twice: `&` and `.bitand` are
                    // one operator, and the keyword must land on the same
                    // token so it inherits the same precedence.
                    tokens.push(op);
                } else {
                    tokens.push(Tok::Sym(word));
                }
            }
            other => {
                return Err(AsmError::new(
                    line,
                    format!("unexpected character `{other}` in expression"),
                ));
            }
        }
    }
    Ok(tokens)
}

struct ExprParser {
    tokens: Vec<Tok>,
    pos: usize,
    line: usize,
    prec: BytePrec,
    caret: Caret,
    function: Option<ExprFn>,
    compare_opts: Compare,
    /// ca65's logical layer — see [`ExprOpts::logical`].
    logical: bool,
}

impl ExprParser {
    fn expr(&mut self) -> Result<Expr, AsmError> {
        self.logical_not()
    }

    // ---- ca65's logical layer, above the comparisons and loosest-first.
    //
    // Measured against ca65 V2.18, and the ordering is the reference's rather
    // than the obvious one: `.not` binds **looser than everything**, so
    // `.not 1 .or 1` is `0` — the `.or` happens first and the `.not` negates
    // the lot. A parser that treated it as an ordinary prefix would answer
    // `1`. Below it, `.or` is looser than `.and`/`.xor`, which are looser
    // than the comparisons.
    //
    // Off in every other dialect: `!` is bitwise OR in vasm, and `.and` is an
    // ordinary symbol in a dialect that has never heard of it.

    fn logical_not(&mut self) -> Result<Expr, AsmError> {
        if self.logical && matches!(self.tokens.get(self.pos), Some(Tok::LogNot)) {
            self.pos += 1;
            return Ok(Expr::LogNot(Box::new(self.logical_not()?)));
        }
        self.logical_or()
    }

    fn logical_or(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.logical_and()?;
        while self.logical && matches!(self.tokens.get(self.pos), Some(Tok::LogOr)) {
            self.pos += 1;
            let right = self.logical_and()?;
            left = Expr::Bin(BinOp::LogOr, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn logical_and(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.compare()?;
        while self.logical {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::LogAnd) => BinOp::LogAnd,
                Some(Tok::LogXor) => BinOp::LogXor,
                _ => break,
            };
            self.pos += 1;
            let right = self.compare()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The dialect's arithmetic ladder, below any comparison.
    fn ladder(&mut self) -> Result<Expr, AsmError> {
        // Loose `<`/`>` wrap the whole expression to their right.
        if matches!(self.prec, BytePrec::Loose) {
            match self.tokens.get(self.pos) {
                Some(Tok::Lo) => {
                    self.pos += 1;
                    return Ok(Expr::Lo(Box::new(self.expr()?)));
                }
                Some(Tok::Hi) => {
                    self.pos += 1;
                    return Ok(Expr::Hi(Box::new(self.expr()?)));
                }
                _ => {}
            }
        }
        // ACME's precedence differs from the vasm-style ladder used by the other
        // 6502-family dialects: its bitwise/shift operators bind *looser* than
        // arithmetic, and `^` is exponentiation (tightest). Use ACME's ladder
        // when `^` means power; otherwise the shared one.
        if self.caret == Caret::Power {
            self.acme_or()
        } else {
            self.add_sub()
        }
    }

    /// Comparisons, loosest of all — `a+1 = b*2` compares the sums. Non-
    /// associative in practice, but chaining is harmless and no reference
    /// refuses it, so the loop is left general.
    ///
    /// A dialect whose reference answers `$FF` for true gets the comparison
    /// negated here, so `Expr` evaluation stays dialect-agnostic.
    fn compare(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.ladder()?;
        while let Some(Tok::Cmp(op)) = self.tokens.get(self.pos) {
            let op = *op;
            self.pos += 1;
            let right = self.ladder()?;
            let cmp = Expr::Bin(op, Box::new(left), Box::new(right));
            left = if self.compare_opts.minus_one {
                Expr::Neg(Box::new(cmp))
            } else {
                cmp
            };
        }
        Ok(left)
    }

    // ---- ACME precedence ladder (loosest first): `|`, keyword `XOR`/`EOR`,
    // `&`, `<< >>`, `+ -`, `* /`, `^` (power). Verified against the acme binary.

    fn acme_or(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.acme_xor()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Or)) {
            self.pos += 1;
            let right = self.acme_xor()?;
            left = Expr::Bin(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn acme_xor(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.acme_and()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Xor)) {
            self.pos += 1;
            let right = self.acme_and()?;
            left = Expr::Bin(BinOp::Xor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn acme_and(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.acme_shift()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::And)) {
            self.pos += 1;
            let right = self.acme_shift()?;
            left = Expr::Bin(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn acme_shift(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.acme_add_sub()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Shl) => BinOp::Shl,
                Some(Tok::Shr) => BinOp::Shr,
                _ => break,
            };
            self.pos += 1;
            let right = self.acme_add_sub()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn acme_add_sub(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.acme_mul_div()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.acme_mul_div()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn acme_mul_div(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.acme_power()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let right = self.acme_power()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `^` exponentiation — tightest binary, right-associative (`2^3^2` =
    /// `2^(3^2)` = 512).
    fn acme_power(&mut self) -> Result<Expr, AsmError> {
        let base = self.unary()?;
        if matches!(self.tokens.get(self.pos), Some(Tok::Pow)) {
            self.pos += 1;
            let exp = self.acme_power()?;
            Ok(Expr::Bin(BinOp::Pow, Box::new(base), Box::new(exp)))
        } else {
            Ok(base)
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
        let mut left = self.bit_or()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                // ca65's `.mod` sits with `*` and `/`: `7 .mod 4 + 1` is 4.
                // The token only exists where `logical` is on, so this arm is
                // ca65's alone.
                Some(Tok::Mod) => BinOp::Mod,
                _ => break,
            };
            self.pos += 1;
            let right = self.bit_or()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    // vasm binds bitwise and shift operators tighter than `* /`: `<< >>` highest
    // among the binaries, then `&`, `^`, `|`. These tokens appear only in vasm
    // (bitwise) mode, so for 6502 each level falls straight through to `unary`.
    fn bit_or(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.bit_xor()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Or)) {
            self.pos += 1;
            let right = self.bit_xor()?;
            left = Expr::Bin(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn bit_xor(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.bit_and()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Xor)) {
            self.pos += 1;
            let right = self.bit_and()?;
            left = Expr::Bin(BinOp::Xor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn bit_and(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.shift()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::And)) {
            self.pos += 1;
            let right = self.shift()?;
            left = Expr::Bin(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn shift(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Shl) => BinOp::Shl,
                Some(Tok::Shr) => BinOp::Shr,
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
        // Tight `<`/`>` are unary operators binding to the next term.
        if matches!(self.prec, BytePrec::Tight) {
            match self.tokens.get(self.pos) {
                Some(Tok::Lo) => {
                    self.pos += 1;
                    return Ok(Expr::Lo(Box::new(self.unary()?)));
                }
                Some(Tok::Hi) => {
                    self.pos += 1;
                    return Ok(Expr::Hi(Box::new(self.unary()?)));
                }
                // In a ca65 dialect a leading `^` is the bank-byte prefix (as a
                // binary operator it is XOR, handled at `bit_xor`).
                Some(Tok::Xor) if self.caret == Caret::BankOrXor => {
                    self.pos += 1;
                    return Ok(Expr::Bank(Box::new(self.unary()?)));
                }
                _ => {}
            }
        }
        if self.logical && matches!(self.tokens.get(self.pos), Some(Tok::BitNot)) {
            self.pos += 1;
            return Ok(Expr::BitNot(Box::new(self.unary()?)));
        }
        self.atom()
    }

    /// One function argument: a string literal where the source wrote one,
    /// otherwise an ordinary expression.
    fn call_arg(&mut self) -> Result<ExprArg, AsmError> {
        if let Some(Tok::Str(t)) = self.tokens.get(self.pos) {
            let t = t.clone();
            self.pos += 1;
            return Ok(ExprArg::Text(t));
        }
        Ok(ExprArg::Value(self.expr()?))
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
            Tok::Sym(s) => {
                // `name(` is a call where the dialect has functions; anywhere
                // else it stays a plain symbol and the `(` is the next token's
                // problem.
                //
                // One argument only: the tokenizer has no comma, because every
                // caller splits its operands on commas before an expression
                // reaches here. A multi-argument function (`.max(a,b)`,
                // rgbasm's `STRFMT`) needs that token first.
                match (self.function, self.tokens.get(self.pos)) {
                    (Some(build), Some(Tok::LParen)) => {
                        self.pos += 1;
                        let mut args = vec![self.call_arg()?];
                        while matches!(self.tokens.get(self.pos), Some(Tok::Comma)) {
                            self.pos += 1;
                            args.push(self.call_arg()?);
                        }
                        if !matches!(self.tokens.get(self.pos), Some(Tok::RParen)) {
                            return Err(AsmError::new(
                                self.line,
                                format!("expected `,` or `)` in `{s}(...)`"),
                            ));
                        }
                        self.pos += 1;
                        build(&s, args, self.line)
                    }
                    _ => Ok(Expr::Sym(s)),
                }
            }
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

// ---------------------------------------------------------------------------
// Shared lexical helpers
// ---------------------------------------------------------------------------

pub(crate) fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], s[idx..].trim()),
        None => (s, ""),
    }
}

/// Split on `sep` at the top level (outside parentheses), trimming each piece.
pub(crate) fn split_top_level(s: &str, sep: char) -> Vec<&str> {
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
pub(crate) fn top_level_rfind(s: &str, sep: char) -> Option<usize> {
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

/// The 6502 numeric literal forms shared by acme and ca65: `$hex`, `%binary`,
/// `'c'` char, decimal.
pub(crate) fn parse_number(tok: &str, line: usize) -> Result<i64, AsmError> {
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

/// The byte index of a lone `=` used as a symbol assignment, or `None`. Skips
/// the comparison operators `==`/`!=`/`<=`/`>=`. (A leading `*=` is handled by
/// each dialect before this is reached.)
pub(crate) fn assignment_split(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' {
            let prev = i.checked_sub(1).map(|p| bytes[p]);
            let next = bytes.get(i + 1).copied();
            if !matches!(prev, Some(b'!' | b'<' | b'>' | b'=')) && next != Some(b'=') {
                // The left side has to be a *name* for this to be a
                // definition. Without that check `.byte 2=2` reads as defining
                // a symbol called `.byte 2`, which is what it did before `=`
                // could be a comparison and nothing had reason to notice.
                let left = trimmed[..i].trim();
                let is_name = !left.is_empty()
                    && (left == "*"
                        || left.chars().all(|c| {
                            c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | ':')
                        }));
                if is_name {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Split a data list on commas that are not inside a `"..."` string.
pub(crate) fn split_data_items(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_string = false;
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            // A comma inside parentheses separates a *function's* arguments,
            // not the list's items: `.byte .max(2,9)` is one value, and
            // splitting it here left the parser half a call to read.
            '(' if !in_string => depth += 1,
            ')' if !in_string => depth -= 1,
            ',' if !in_string && depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

/// The contents of a `"..."` string literal, or `None` if `piece` is not one.
pub(crate) fn string_literal(piece: &str) -> Option<&str> {
    let p = piece.trim();
    (p.len() >= 2 && p.starts_with('"') && p.ends_with('"')).then(|| &p[1..p.len() - 1])
}

/// An identifier: letters, digits, `_`, and `.` (so local-style labels like
/// `.loop` read as ordinary names), not starting with a digit.
pub(crate) fn is_ident(s: &str) -> bool {
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
    use super::*;

    fn num(tok: &str, line: usize) -> Result<i64, AsmError> {
        tok.strip_prefix('$')
            .map_or_else(|| tok.parse::<i64>(), |h| i64::from_str_radix(h, 16))
            .map_err(|_| AsmError::new(line, "bad number"))
    }

    fn eval(raw: &str, prec: BytePrec) -> i64 {
        let env = BTreeMap::new();
        let opts = ExprOpts {
            logical: false,
            scoped_names: false,
            fixed_point: false,
            compare: Compare::default(),
            function: None,
            prec,
            byte_prefix: true,
            caret: Caret::Xor,
            at_is_pc: false,
            bang_is_or: false,
        };
        fold_const(&parse_expr(raw, 1, num, opts).expect("parse"), &env, 1).expect("fold")
    }

    /// A data list splits on the commas *between its items*, not on the ones
    /// inside a function call. `.byte .max(2,9)` is one value, and splitting
    /// it left the expression parser half a call to read — which made every
    /// two-argument function unreachable, `.max`, `.min` and `.strat`
    /// included, though all three were written and correct.
    #[test]
    fn a_data_list_does_not_split_inside_a_call() {
        assert_eq!(split_data_items(".max(2,9)"), vec![".max(2,9)"]);
        assert_eq!(
            split_data_items("1, .max(2,9), 3"),
            vec!["1", ".max(2,9)", "3"]
        );
        assert_eq!(
            split_data_items(".max(.min(1,5), 3)"),
            vec![".max(.min(1,5), 3)"],
            "nested calls keep their own commas"
        );
        // The rules it already had still hold.
        assert_eq!(split_data_items("1,2,3"), vec!["1", "2", "3"]);
        assert_eq!(split_data_items("\"a,b\""), vec!["\"a,b\""]);
        assert_eq!(
            split_data_items("\"a,b\", .max(1,2)"),
            vec!["\"a,b\"", ".max(1,2)"]
        );
    }

    #[test]
    fn byte_operator_precedence_differs_by_dialect() {
        // Loose (ACME): `>` applies to the whole expression -> high($1235) = $12.
        assert_eq!(eval(">$1234+1", BytePrec::Loose), 0x12);
        // Tight (ca65): `>` binds to the term -> high($1234) + 1 = $13.
        assert_eq!(eval(">$1234+1", BytePrec::Tight), 0x13);
        // Arithmetic precedence is the same regardless.
        assert_eq!(eval("1+2*3", BytePrec::Loose), 7);
        assert_eq!(eval("1+2*3", BytePrec::Tight), 7);
    }
}
