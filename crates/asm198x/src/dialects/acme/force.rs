//! ACME's **Force Bits**: the addressing width a value demands whatever its
//! size (`AddrModes.txt` in ACME's documentation). Every rule here was
//! measured against ACME 0.97 with `acme -f plain`; see
//! `decisions/acme-zero-page.md` for where this sits in mode selection.
//!
//! - **A literal's written width forces it**, but only while the value would
//!   fit the narrower width. Hex (`$` or `0x`) with more than 2 digits forces
//!   16 bits when the value is under `$100`, and with more than 4 forces 24
//!   when it is under `$10000`: `$0070` is 16, `$00070` is 24, and `$0100`
//!   carries no force at all. Binary takes 8 and 16 digits as its thresholds,
//!   octal 3 and 6. Decimal never forces.
//! - **A symbol keeps the force of the expression that defined it.** A label
//!   has none; a loop variable has none.
//! - **Across an expression the widest force wins**, through every operator
//!   except the comparisons, whose result carries none.
//! - **`<`, `>` and `^` force 8 bits over all they cover**, which is the whole
//!   expression to their right ([`super::BytePrec::Loose`]): `<NR + $0001` is
//!   8 bits, `$0001 + <NR` is 16.
//!
//! The operand's text is scanned here rather than its parsed tree, because the
//! tree has already folded a literal's digits into a number. The shared parser
//! stays as it is; this reads the same tokens it does, and only the ones that
//! matter.

use std::collections::BTreeMap;

use super::super::mos6502::{self, Index, OperandSyntax};
use super::evaluate::AcmeTarget;
use super::scope::anon_marker;
use super::{AsmError, Expr, lone_string_value};

/// A width the Force Bits can demand. Ordered so that the widest of several is
/// their `max`, which is how ACME combines them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Force {
    Bits8,
    Bits16,
    Bits24,
}

impl Force {
    const ALL: [Force; 3] = [Force::Bits8, Force::Bits16, Force::Bits24];

    fn bits(self) -> u32 {
        match self {
            Force::Bits8 => 8,
            Force::Bits16 => 16,
            Force::Bits24 => 24,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The force an expression carries, kept open over the symbols it names, so a
/// symbol defined after the expression still counts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ForceTerms {
    /// The widest force among its literals and byte operators.
    pub(super) base: Option<Force>,
    /// The symbols outside any byte operator, zone-qualified, whose own force
    /// counts too.
    pub(super) symbols: Vec<String>,
}

impl ForceTerms {
    pub(super) fn fixed(force: Option<Force>) -> Self {
        ForceTerms {
            base: force,
            symbols: Vec::new(),
        }
    }

    /// Both sets of terms at once: a symbol that is defined twice keeps every
    /// force either definition gave it, as ACME's flags do.
    pub(super) fn absorb(&mut self, other: ForceTerms) {
        self.base = self.base.max(other.base);
        for s in other.symbols {
            if !self.symbols.contains(&s) {
                self.symbols.push(s);
            }
        }
    }
}

/// Every symbol's force, by qualified name. A label is present with none:
/// presence is what says a symbol has been defined yet.
pub(super) type ForceMap = BTreeMap<String, ForceTerms>;

/// What `terms` comes to against `map`: the widest force, and whether every
/// symbol it rests on has been defined yet.
pub(super) fn resolve(terms: &ForceTerms, map: &ForceMap) -> (Option<Force>, bool) {
    resolve_within(terms, map, 0)
}

fn resolve_within(terms: &ForceTerms, map: &ForceMap, depth: usize) -> (Option<Force>, bool) {
    // A symbol defined in terms of itself is the engine's error to report;
    // this only has to stop.
    const DEPTH: usize = 64;
    let mut force = terms.base;
    let mut complete = true;
    for name in &terms.symbols {
        match map.get(name) {
            Some(inner) if depth < DEPTH => {
                let (f, c) = resolve_within(inner, map, depth + 1);
                force = force.max(f);
                complete &= c;
            }
            Some(_) => {}
            None => complete = false,
        }
    }
    (force, complete)
}

/// `name+1`, `name+2` or `name+3`: a mnemonic or a symbol's name carrying
/// ACME's postfix, and the width it sets. The postfix is written against the
/// name — `S + 2 = $70` is ACME's "Syntax error".
pub(super) fn split_postfix(word: &str) -> Option<(&str, Force)> {
    let (base, digit) = word.rsplit_once('+')?;
    let force = match digit {
        "1" => Force::Bits8,
        "2" => Force::Bits16,
        "3" => Force::Bits24,
        _ => return None,
    };
    (!base.is_empty() && !base.ends_with(char::is_whitespace)).then_some((base, force))
}

// ---------------------------------------------------------------------------
// Scanning an expression
// ---------------------------------------------------------------------------

/// One token, as far as the Force Bits care.
enum Tok {
    /// A value with the force its spelling gives it.
    Value(Option<Force>),
    Sym(String),
    /// `<`, `>` or `^` where a value may begin.
    Byte,
    /// A comparison, whose result carries no force.
    Compare,
    Open,
    Close,
    /// Any other operator: it passes the force of both sides through.
    Op,
}

/// The force terms of one ACME expression, as `parse_value` reads it.
/// Leading-`.` names qualify into `zone`, as `parse_value` qualifies them.
pub(super) fn scan(text: &str, zone: &str) -> ForceTerms {
    let trimmed = text.trim();
    if anon_marker(trimmed).is_some() || lone_string_value(trimmed, 0).is_some() {
        return ForceTerms::default();
    }
    let toks = tokens(trimmed, zone);
    let mut pos = 0;
    group(&toks, &mut pos)
}

/// The terms of one parenthesised group, or of the whole expression: up to
/// its `)`, which is left for the caller.
fn group(toks: &[Tok], pos: &mut usize) -> ForceTerms {
    let mut terms = ForceTerms::default();
    let mut compared = false;
    while let Some(tok) = toks.get(*pos) {
        match tok {
            Tok::Close => break,
            Tok::Open => {
                *pos += 1;
                let inner = group(toks, pos);
                if matches!(toks.get(*pos), Some(Tok::Close)) {
                    *pos += 1;
                }
                terms.absorb(inner);
            }
            // A byte operator covers the rest of its group, whatever force
            // that carries.
            Tok::Byte => {
                let mut depth = 0usize;
                while let Some(t) = toks.get(*pos) {
                    match t {
                        Tok::Open => depth += 1,
                        Tok::Close if depth == 0 => break,
                        Tok::Close => depth -= 1,
                        _ => {}
                    }
                    *pos += 1;
                }
                terms.base = terms.base.max(Some(Force::Bits8));
            }
            Tok::Compare => {
                compared = true;
                *pos += 1;
            }
            Tok::Value(f) => {
                terms.base = terms.base.max(*f);
                *pos += 1;
            }
            Tok::Sym(name) => {
                if !terms.symbols.contains(name) {
                    terms.symbols.push(name.clone());
                }
                *pos += 1;
            }
            Tok::Op => *pos += 1,
        }
    }
    if compared {
        ForceTerms::default()
    } else {
        terms
    }
}

/// Tokenise as the shared expression reader does with ACME's options. Text
/// only reaches here once that reader has accepted it, so nothing is refused.
fn tokens(text: &str, zone: &str) -> Vec<Tok> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let after_value = matches!(out.last(), Some(Tok::Value(_) | Tok::Sym(_) | Tok::Close));
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let word_end = |from: usize| {
            let mut j = from;
            while j < chars.len() && chars[j].is_ascii_alphanumeric() {
                j += 1;
            }
            j
        };
        match c {
            ws if ws.is_whitespace() => i += 1,
            '(' => {
                out.push(Tok::Open);
                i += 1;
            }
            ')' => {
                out.push(Tok::Close);
                i += 1;
            }
            '<' | '>' if next == Some(c) => {
                out.push(Tok::Op);
                i += 2;
            }
            '<' | '>' if after_value => {
                out.push(Tok::Compare);
                i += if matches!(next, Some('=' | '>')) {
                    2
                } else {
                    1
                };
            }
            '<' | '>' => {
                out.push(Tok::Byte);
                i += 1;
            }
            '^' if !after_value => {
                out.push(Tok::Byte);
                i += 1;
            }
            '=' => {
                out.push(Tok::Compare);
                i += 1;
            }
            '*' if !after_value => {
                // The program counter, which carries no force.
                out.push(Tok::Value(None));
                i += 1;
            }
            '$' | '%' | '&'
                if !after_value && (c != '&' || next.is_some_and(|d| d.is_digit(8))) =>
            {
                let end = word_end(i + 1);
                let digits: String = chars[i + 1..end].iter().collect();
                let (radix, word, long) = match c {
                    '$' => (16, 2, 4),
                    '%' => (2, 8, 16),
                    _ => (8, 3, 6),
                };
                out.push(Tok::Value(literal(&digits, radix, word, long)));
                i = end;
            }
            '\'' => {
                out.push(Tok::Value(None));
                i += 3;
            }
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                out.push(Tok::Value(None));
                i += 1;
            }
            d if d.is_ascii_digit() => {
                let end = word_end(i);
                let token: String = chars[i..end].iter().collect();
                let force = if let Some(hex) = token.strip_prefix("0x") {
                    literal(hex, 16, 2, 4)
                } else if let Some(bin) = token.strip_prefix("0b") {
                    literal(bin, 2, 8, 16)
                } else {
                    None
                };
                out.push(Tok::Value(force));
                i = end;
            }
            l if l.is_ascii_alphabetic() || l == '_' || l == '.' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if word.eq_ignore_ascii_case("xor") || word.eq_ignore_ascii_case("eor") {
                    out.push(Tok::Op);
                } else if word.starts_with('.') && !zone.is_empty() {
                    out.push(Tok::Sym(format!("{zone}{word}")));
                } else {
                    out.push(Tok::Sym(word));
                }
            }
            _ => {
                out.push(Tok::Op);
                i += 1;
            }
        }
    }
    out
}

/// The force a literal's digits give it: past `word` digits it forces 16 bits
/// while the value is under `$100`, past `long` it forces 24 while the value
/// is under `$10000`. A value already that wide needs no force to be so.
fn literal(digits: &str, radix: u32, word: usize, long: usize) -> Option<Force> {
    let value = i64::from_str_radix(digits, radix).ok()?;
    let n = digits.len();
    if n > long {
        (value < 0x1_0000).then_some(Force::Bits24)
    } else if n > word {
        (value < 0x100).then_some(Force::Bits16)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Choosing a mode
// ---------------------------------------------------------------------------

/// How an instruction's operand was sized.
pub(super) struct Sizing {
    pub(super) mode: &'static str,
    pub(super) operand: Option<Expr>,
    /// The width the operand took, when it had a choice to make at all.
    pub(super) width: Option<Force>,
    /// The forms the instruction offers this operand, by width.
    pub(super) offers: [Option<&'static str>; 3],
    /// Whether the value decided, with no force or postfix to say otherwise.
    pub(super) by_value: bool,
    /// The mnemonic the chosen form belongs to, when it is not the one written.
    pub(super) renamed: Option<&'static str>,
}

/// Pick the addressing mode for an instruction under ACME's Force Bits.
///
/// A postfix on the mnemonic decides first, and must name a width the
/// instruction has. Failing that, the operand's force picks the narrowest
/// width at least that wide, and "Number out of range" when there is none.
/// With neither, the value decides, as `decisions/acme-zero-page.md`
/// describes — unless the operand is `tainted`: it names a symbol that was
/// read before it was defined, so its value is not allowed to decide.
#[allow(clippy::too_many_arguments)]
pub(super) fn choose_mode(
    target: AcmeTarget,
    mnemonic: &str,
    operand: OperandSyntax,
    env: &BTreeMap<String, i64>,
    tainted: bool,
    postfix: Option<Force>,
    force: Option<Force>,
    line: usize,
) -> Result<Sizing, AsmError> {
    let has = |mode: &str| {
        target.set.find_form(mnemonic, mode).is_some()
            || target
                .ext
                .is_some_and(|set| set.find_form(mnemonic, mode).is_some())
    };
    let by_value_in = |operand: OperandSyntax,
                       offers: [Option<&'static str>; 3],
                       env: &BTreeMap<String, i64>|
     -> Result<Sizing, AsmError> {
        let (mode, operand) = mos6502::resolve_mode_in_sets(
            target.set, target.ext, mnemonic, operand, env, false, line,
        )?;
        Ok(Sizing {
            mode,
            operand,
            width: Force::ALL
                .into_iter()
                .find(|f| offers[f.index()] == Some(mode)),
            offers,
            by_value: true,
            renamed: None,
        })
    };
    let by_value = |operand: OperandSyntax, offers: [Option<&'static str>; 3]| {
        by_value_in(operand, offers, env)
    };
    let is_65816 = target.ext.is_some_and(|set| set.has_mnemonic("XCE"));

    // Branches, implied instructions and the zero-page bit instructions have
    // no width to choose. ACME reads no postfix after them and ignores a
    // force: `rmb0 $0070` is `07 70`.
    if !sized(target, mnemonic) {
        if postfix.is_some() {
            return Err(AsmError::new(
                line,
                format!("`{mnemonic}` takes no postfix"),
            ));
        }
        return by_value(operand, [None; 3]);
    }

    let named = |modes: [&'static str; 3]| modes.map(|m| (!m.is_empty() && has(m)).then_some(m));
    // ACME's 65816 `jsr` to a 24-bit address is `jsl`: `jsr+3 $70` is
    // `22 70 00 00`.
    let jsr_long = is_65816 && mnemonic == "JSR";
    let offers = match &operand {
        OperandSyntax::Direct(_) if jsr_long => {
            let mut offers = named(["zeropage", "absolute", ""]);
            offers[Force::Bits24.index()] = Some("long");
            offers
        }
        OperandSyntax::Direct(_) => named(["zeropage", "absolute", "long"]),
        OperandSyntax::Indexed(_, Index::X) => named(["zeropage,x", "absolute,x", "long,x"]),
        OperandSyntax::Indexed(_, Index::Y) => named(["zeropage,y", "absolute,y", ""]),
        OperandSyntax::None | OperandSyntax::Accumulator => {
            if let Some(p) = postfix {
                return Err(illegal_postfix(mnemonic, p, line));
            }
            return by_value(operand, [None; 3]);
        }
        // The 65816 sizes an immediate by its register width, and ACME
        // ignores the operand's force there: `lda #$0070` is `a9 70`.
        OperandSyntax::Immediate(_) if is_65816 => {
            return match postfix {
                None | Some(Force::Bits8) => by_value(operand, [None; 3]),
                Some(Force::Bits16) => Err(AsmError::new(
                    line,
                    format!(
                        "`{mnemonic}+2` asks for a 16-bit immediate, which ACME assembles on \
                         the 65816 and this assembler does not yet — the gap is ours"
                    ),
                )),
                Some(Force::Bits24) => Err(illegal_postfix(mnemonic, Force::Bits24, line)),
            };
        }
        // Every other shape has exactly one mode, and the mode its width.
        _ => {
            let (mode, _) = mos6502::resolve_mode_in_sets(
                target.set,
                target.ext,
                mnemonic,
                operand.clone(),
                env,
                false,
                line,
            )?;
            let width = match mode {
                "indirect" | "(absolute,x)" | "immediate16" => Force::Bits16,
                _ => Force::Bits8,
            };
            let mut offers = [None; 3];
            offers[width.index()] = Some(mode);
            offers
        }
    };

    // ACME's own rule when nothing forces a width: an instruction with only
    // one width takes it, whatever the value is or whether it is known yet.
    // So `stx p,y` is `96 70` with `p` defined later: STX has no absolute,y.
    let only = {
        let mut widths = Force::ALL
            .into_iter()
            .filter(|w| offers[w.index()].is_some());
        match (widths.next(), widths.next()) {
            (Some(w), None) => Some(w),
            _ => None,
        }
    };
    let width = match (postfix, force) {
        (None, None) if let Some(w) = only => w,
        (Some(p), _) => {
            if offers[p.index()].is_none() {
                return Err(illegal_postfix(mnemonic, p, line));
            }
            p
        }
        (None, Some(f)) => Force::ALL[f.index()..]
            .iter()
            .copied()
            .find(|w| offers[w.index()].is_some())
            .ok_or_else(|| {
                AsmError::new(
                    line,
                    format!(
                        "number out of range: the operand is forced to {} bits, wider than \
                         any `{mnemonic}` form here takes",
                        f.bits()
                    ),
                )
            })?,
        // ACME's NUMBER_EVER_UNDEFINED: a symbol read before it was defined
        // stays tainted for the rest of the assembly, and a tainted operand
        // takes the form it would take if its value were still unknown. So
        // `lda p` after `sta p` and `p = $70` is `AD 70 00`, with ACME's
        // oversized-mode warning.
        (None, None) if tainted => return by_value_in(operand, offers, &BTreeMap::new()),
        (None, None) => return by_value(operand, offers),
    };
    let Some(mode) = offers[width.index()] else {
        unreachable!("a chosen width is one the instruction offers")
    };
    let expr = match operand {
        OperandSyntax::Direct(e)
        | OperandSyntax::Indexed(e, _)
        | OperandSyntax::Immediate(e)
        | OperandSyntax::Indirect(e)
        | OperandSyntax::IndexedIndirect(e)
        | OperandSyntax::IndirectIndexed(e)
        | OperandSyntax::IndirectIndexedZ(e)
        | OperandSyntax::StackIndirectIndexedY(e) => e,
        OperandSyntax::None | OperandSyntax::Accumulator => {
            unreachable!("an operand-less instruction returned above")
        }
    };
    // A value a forced byte cannot hold: `lda+1 $1234`. ACME takes -128 to
    // 255, so `lda+1 -1` is `a5 ff`.
    if width == Force::Bits8
        && mode.starts_with("zeropage")
        && let Ok(v) = mos6502::fold_const(&expr, env, line)
        && !(-128..=0xFF).contains(&v)
    {
        return Err(AsmError::new(
            line,
            format!("number out of range: {v} does not fit the 8 bits `{mnemonic}` is forced to"),
        ));
    }
    Ok(Sizing {
        mode,
        operand: Some(expr),
        width: Some(width),
        offers,
        by_value: false,
        renamed: (jsr_long && width == Force::Bits24).then_some("JSL"),
    })
}

/// Whether ACME gives this instruction a width to choose: any form beyond the
/// implied, relative and zero-page-only ones.
fn sized(target: AcmeTarget, mnemonic: &str) -> bool {
    let fixed = |mode: &str| {
        matches!(
            mode,
            "implied"
                | "accumulator"
                | "relative"
                | "relative16"
                | "zeropage"
                | "zeropage,relative"
        )
    };
    [Some(target.set), target.ext]
        .into_iter()
        .flatten()
        .filter_map(|set| set.instruction(mnemonic))
        .any(|inst| inst.forms.iter().any(|f| !fixed(f.mode)))
}

/// ACME 0.97 of 1 Aug 2025's words. The 2020 build said "Illegal
/// combination of command and postfix" for the same refusal.
fn illegal_postfix(mnemonic: &str, postfix: Force, line: usize) -> AsmError {
    AsmError::new(
        line,
        format!(
            "CPU does not support this postfix for this mnemonic: `{mnemonic}` has no {}-bit \
             form for this operand",
            postfix.bits()
        ),
    )
}

/// A force found only once later symbols were defined, checked against the
/// width the walk already gave the operand. `None` when they agree.
///
/// A forward reference sizes absolute, as ACME's first pass does; when it
/// turns out to need 24 bits, ACME widens it where a long form exists and
/// refuses it where none does.
pub(super) fn late_force_error(
    width: Force,
    offers: [Option<&'static str>; 3],
    force: Option<Force>,
    line: usize,
) -> Option<AsmError> {
    let force = force.filter(|f| *f > width)?;
    Some(if offers[force.index()..].iter().any(Option::is_some) {
        AsmError::new(
            line,
            format!(
                "a symbol defined later forces this operand to {} bits; ACME widens the \
                 instruction to fit, which this assembler does not do after sizing it — \
                 the gap is ours",
                force.bits()
            ),
        )
    } else {
        AsmError::new(
            line,
            format!(
                "number out of range: a symbol defined later forces this operand to {} bits, \
                 wider than the instruction takes",
                force.bits()
            ),
        )
    })
}
