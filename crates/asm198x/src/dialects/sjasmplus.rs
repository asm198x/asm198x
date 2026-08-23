//! The sjasmplus Z80 dialect.
//!
//! A thin surface over the shared Z80 core in [`crate::dialects::z80`]. The Z80
//! instruction/operand syntax is identical to pasmo's; sjasmplus differs only
//! in its surface, which is all that lives here:
//!
//! - **Comments**: `;` *and* `//`.
//! - **Numbers**: a superset — `$hex`, `0xhex`, `NNh`; `%binary`, `0bbinary`,
//!   `NNb`; decimal; `'c'` char.
//!
//! Directives and operand resolution are shared. sjasmplus also targets the
//! Spectrum Next, so it carries the same `z80n` target flag as pasmo. Unlike
//! pasmo, a leading-`.` label is *local*, scoped under the most recent global
//! label (so `.loop` may recur) — see [`Z80Syntax::scopes_locals`].
//!
//! **Conditional assembly** (language-surface U8): sjasmplus is the first
//! keyword-style adopter of the shared `ast::CondEval`/`ast::evaluate`
//! framework — `IF`/`IFDEF`/`IFNDEF`/`ELSE`/`ENDIF` plus `DEFINE` (textual
//! substitution, probe-pinned). All three entry points route through the
//! z80 keyword pipeline (`z80::parse_program_keyword` + the `SjasmEval`
//! walk), so every line lowers with the live environment and an include in
//! an untaken branch never loads. pasmo stays on the eager walker — its
//! conditional adoption is demand-gated
//! (`decisions/conditional-assembly-framework.md`).
//!
//! **Macros** (#93): `MACRO`/`ENDM` with dot-prefixed locals scoped per
//! expansion, over the shared expander in [`crate::dialects::macros`] — this
//! module supplies only the grammar, governed by
//! `decisions/macro-expansion-framework.md`. Repetition (`DUP`/`REPT`) is a
//! conditional-framework item rather than a macro one, because its count is an
//! expression over the environment.
//!
//! **Modules** (#93): `MODULE`/`ENDMODULE` prefix the names defined inside
//! them, and a leading `@` escapes to the global scope. A reference has two
//! candidates — the fully-qualified name and the bare global one, with no
//! walk-up between — which is why the choice is repaired after the walk rather
//! than made as each line is read. See
//! `docs/plans/2026-08-23-001-feat-sjasmplus-modules-plan.md`.
//!
//! TODO: macros across include boundaries, and the name-first `name MACRO`
//! spelling the reference also accepts (#205). `ELSEIF` and the dotted
//! conditional spellings landed 2026-08-18; colon-inline blocks and conditions
//! on forward symbols remain open under #67.

use std::collections::BTreeMap;

use crate::dialect::{Dialect, Oversize};
use crate::dialects::macros::{self, Expand};
use crate::dialects::z80::{self, Z80Syntax};
use crate::directives::{Category, Directive, Pattern};
use crate::engine::{AsmError, Operation, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The sjasmplus Z80 dialect. `z80n` selects the target instruction set
/// (sjasmplus emits Z80N when targeting the Next).
/// What sjasmplus accepts beyond the shared Z80 base.
///
/// `bytes` overrides the base entry rather than adding a second one:
/// sjasmplus spells `db` four ways and adds `byte`, and two entries claiming
/// one concept would show as two rows in a matrix.
///
/// `include` is here and is not in pasmo's list, which is the difference this
/// declaration exists to make visible.
pub const DIRECTIVES: &[Directive] = &[
    Directive {
        id: "bytes",
        pattern: Pattern::Exact(&["defb", "db", "defm", "dm", "byte"]),
        category: Category::Operation,
    },
    Directive {
        id: "include",
        pattern: Pattern::Exact(&["include"]),
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Exact(&["incbin"]),
        category: Category::Operation,
    },
    // Scanner- and expander-handled, like acme's conditionals: these never
    // reach `parse_op`, because the macro expander and the conditional walk
    // consume them first. Declared all the same — the surface describes the
    // dialect, and a matrix showing sjasmplus with no macros and no `IF` would
    // be describing whichever parser happened to read the line.
    //
    // **One entry per construct, named by its opener.** `ENDM`, `EDUP`, `ELSE`
    // and `ENDIF` are parts of a block rather than vocabulary of their own, the
    // same call the plan already made for acme's `}`. A matrix answering "does
    // this dialect have macros" wants one row, not two.
    Directive {
        id: "macro",
        pattern: Pattern::Exact(&["macro"]),
        category: Category::Operation,
    },
    Directive {
        id: "repeat",
        pattern: Pattern::Exact(&["dup", "rept"]),
        category: Category::Operation,
    },
    Directive {
        id: "conditional",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["if", "ifdef", "ifndef"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "define",
        pattern: Pattern::Exact(&["define"]),
        category: Category::Operation,
    },
    // Named by its opener, like the blocks above: `ENDMODULE`/`ENDMOD` are
    // parts of the block rather than vocabulary of their own.
    //
    // Deliberately **not** in `is_directive`. The reference reads a column-0
    // `MODULE` as a label and the name after it as an instruction (probe m27),
    // so treating it as a directive would accept source sjasmplus rejects.
    Directive {
        id: "module",
        pattern: Pattern::Exact(&["module"]),
        category: Category::Operation,
    },
];

pub(crate) struct Sjasmplus {
    pub(crate) z80n: bool,
}

impl Dialect for Sjasmplus {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z80::SET
    }
    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        self.z80n.then_some(&isa::z80::NEXT)
    }
    /// Assembly routes through the keyword-conditional pipeline (U8): the
    /// structure parse builds the shared conditional tree, and the
    /// `ast::evaluate` walk lowers each live line with the environment as of
    /// that point (an `equ` in a taken branch feeds later form selection).
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        Ok(self.parse_warned(source)?.0)
    }
    /// The advisories are sjasmplus's own: a condition that reached forward,
    /// and a label that never settled (#99).
    fn parse_warned(
        &self,
        source: &str,
    ) -> Result<(Vec<Statement>, Vec<crate::engine::Warning>), AsmError> {
        z80::assemble_keyword_warned(
            &SjasmplusSyntax,
            self.instruction_set(),
            self.extension_set(),
            source,
        )
    }
    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(z80::parse_program_keyword(
            &SjasmplusSyntax,
            self.instruction_set(),
            self.extension_set(),
            crate::span::FileId(0),
            source,
            // The formatter must not expand: it lays source out, and would
            // otherwise write the expansions back in place of the macro.
            Expand::No,
        )?))
    }
    /// The include-capable parse (language-surface U2, conditional-aware
    /// since U8): includes resolve lazily *inside* the conditional walk, so
    /// a guarded include in an untaken branch never loads (KTD1) and the
    /// environment threads through the boundary in both directions.
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        Ok(self.parse_multi_warned(map, loader)?.0)
    }
    fn parse_multi_warned(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<(Vec<Statement>, Vec<crate::engine::Warning>), AsmError> {
        z80::parse_program_multi_keyword_warned(
            &SjasmplusSyntax,
            self.instruction_set(),
            self.extension_set(),
            map,
            loader,
        )
    }
    /// sjasmplus truncates an over-range byte to its low 8 bits and warns.
    fn oversized_byte_policy(&self) -> Oversize {
        Oversize::TruncateWarn
    }
}

/// sjasmplus's surface syntax.
struct SjasmplusSyntax;

impl Z80Syntax for SjasmplusSyntax {
    /// sjasmplus is the dialect the shared keyword vocabulary was measured
    /// against, so its adoption is the free functions unchanged.
    fn cond_keyword(&self, word: &str) -> Option<z80::CondKw> {
        z80::cond_keyword(word)
    }

    fn repeat_keyword(&self, word: &str) -> Option<z80::RepeatKw> {
        z80::repeat_keyword(word)
    }

    fn module_keyword(&self, word: &str) -> Option<z80::ModuleKw> {
        z80::module_keyword(word)
    }

    /// The formatter must copy a macro definition rather than re-lay it out.
    /// It survived without this while every spelling was indented — the
    /// definition simply looked like an unrecognised line and came through
    /// unchanged. The name-first spelling (#205) puts the name in the *label*
    /// column, where the formatter peels it onto a line of its own and the
    /// definition stops being one.
    fn macro_line(&self, line: &str, known: &dyn Fn(&str) -> bool) -> macros::MacroLine {
        macros::macro_line(self, line, known)
    }

    /// sjasmplus scopes names under the open `MODULE`s (#93's third item).
    fn scopes_modules(&self) -> bool {
        true
    }

    /// sjasmplus takes `:` as a statement separator as well as a label
    /// terminator (#98) — ` ld a,1 : ld b,2` is two instructions, and it is
    /// how hand-written Spectrum source is often laid out.
    fn splits_on_colon(&self) -> bool {
        true
    }

    /// sjasmplus resolves a condition against a symbol defined later in the
    /// file, across its three passes (#99).
    fn resolves_forward_conditions(&self) -> bool {
        true
    }

    fn is_define_word(&self, word: &str) -> bool {
        z80::is_define_word(word)
    }

    fn constant_sources(&self) -> &'static str {
        "a value defined with `equ` or `DEFINE` above"
    }

    fn strip_comment<'a>(&self, line: &'a str) -> &'a str {
        // The earlier of `;` and `//` starts the comment.
        let semi = line.find(';');
        let slashes = line.find("//");
        let cut = match (semi, slashes) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        cut.map_or(line, |i| &line[..i])
    }

    /// sjasmplus has the `^` bitwise-XOR operator (pasmo does not).
    fn has_xor_operator(&self) -> bool {
        true
    }

    /// sjasmplus adds `byte` as a spelling of `db` (pasmo has neither), plus
    /// `include` (U2) and `incbin` (U3) — listed here so a column-0 spelling
    /// reads as an operation, not a label; the walk intercepts both before
    /// directive parsing.
    fn is_directive(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case("byte")
            || self.is_include(word)
            || self.is_incbin(word)
            || z80::is_common_directive(word)
    }

    /// sjasmplus's include directive (language-surface U2), walk-handled.
    fn is_include(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case("include")
    }

    /// sjasmplus's binary-inclusion directive (language-surface U3),
    /// walk-handled like `include`.
    fn is_incbin(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case("incbin")
    }

    fn own_directives(&self) -> &'static [crate::directives::Directive] {
        DIRECTIVES
    }

    /// sjasmplus's `INCBIN "file"[,offset[,length]]` takes the full tail,
    /// including the probe-pinned negative from-the-end forms.
    fn incbin_offset_length(&self) -> bool {
        true
    }

    /// sjasmplus accepts `<file>` for the incbin name (as its INCLUDE does).
    fn incbin_angle_quotes(&self) -> bool {
        true
    }

    /// `byte` is `db`; everything else is the shared common set.
    fn parse_directive(
        &self,
        word: &str,
        args: &str,
        line: usize,
        consts: &BTreeMap<String, i64>,
    ) -> Result<Option<Operation>, AsmError> {
        let word = if word.eq_ignore_ascii_case("byte") {
            "db"
        } else {
            word
        };
        z80::common_directive(self, word, args, line, consts)
    }

    /// sjasmplus scopes leading-`.` labels under the most recent global label.
    /// Macros expand before parsing (#93). Returning the map lets the shared
    /// pipeline report every line against its source rather than against a
    /// line that only existed inside the expander.
    fn expand_source(
        &self,
        source: &str,
    ) -> Result<Option<(String, Vec<macros::LineOrigin>)>, AsmError> {
        let expanded = macros::expand(&SjasmplusSyntax, source)?;
        Ok(Some((expanded.text, expanded.origins)))
    }

    fn scopes_locals(&self) -> bool {
        true
    }

    /// sjasmplus numbers: hex (`$`/`0x`/`#` prefix, `h` suffix), binary (`%`/`0b`
    /// prefix, `b` suffix), `'c'` char, decimal.
    fn parse_number(&self, tok: &str, line: usize) -> Result<i64, AsmError> {
        let t = tok.trim();
        let bad = || AsmError::new(line, format!("invalid number `{tok}`"));

        if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
            return t.chars().nth(1).map(|c| c as i64).ok_or_else(bad);
        }
        // Hex: $, 0x, or # prefix, or h suffix.
        if let Some(hex) = t
            .strip_prefix('$')
            .or_else(|| t.strip_prefix("0x"))
            .or_else(|| t.strip_prefix("0X"))
            .or_else(|| t.strip_prefix('#'))
        {
            return i64::from_str_radix(hex, 16).map_err(|_| bad());
        }
        if let Some(hex) = t.strip_suffix(['h', 'H'])
            && let Ok(v) = i64::from_str_radix(hex, 16)
        {
            return Ok(v);
        }
        // Binary: % or 0b prefix, or b suffix.
        if let Some(bin) = t
            .strip_prefix('%')
            .or_else(|| t.strip_prefix("0b"))
            .or_else(|| t.strip_prefix("0B"))
        {
            return i64::from_str_radix(bin, 2).map_err(|_| bad());
        }
        if let Some(bin) = t.strip_suffix(['b', 'B'])
            && let Ok(v) = i64::from_str_radix(bin, 2)
        {
            return Ok(v);
        }
        t.parse::<i64>().map_err(|_| bad())
    }
}

// ---------------------------------------------------------------------------
// Macros (#93)
//
// The mechanics live in [`crate::dialects::macros`]; this is sjasmplus's
// grammar. Every rule below was measured against sjasmplus 1.21.0 rather than
// read from its manual, and two of them are not what a reasonable person would
// guess:
//
//   * the `MACRO`/`ENDM` **keyword** is case-insensitive, but a macro **name**
//     is case-sensitive — defining `mac` and calling `MAC` is an error;
//   * substitution respects word boundaries (a parameter `v` leaves the symbol
//     `val` alone) and does **not** reach inside string literals, so
//     `db "v"` emits the letter, not the argument.
//
// Substitution is textual and happens before expression evaluation, which is
// why `val*2` with `val = 5` assembles to `ld a,10`.
// ---------------------------------------------------------------------------

/// Split a macro header's tail into its leading word and the rest — the name
/// and its parameters in the keyword-first form, the keyword and the
/// parameters in the name-first one.
fn split_macro_name(rest: &str) -> (&str, &str) {
    let rest = rest.trim();
    match rest.split_once(char::is_whitespace) {
        Some((word, tail)) => (word.trim(), tail.trim()),
        None => (rest, ""),
    }
}

impl macros::MacroSyntax for SjasmplusSyntax {
    /// Two spellings, both the reference's (#205):
    ///
    /// ```text
    ///     MACRO name [p1[, p2]...]     indented — the keyword leads
    /// name[:] MACRO [p1[, p2]...]      column 0 — the name leads
    /// ```
    ///
    /// The keyword matches case-insensitively; the name is kept as written and
    /// stays case-sensitive at the call site.
    ///
    /// **Indentation decides which is which, and a line in the wrong column is
    /// not a definition at all.** At column 0 the reference reads `MACRO` as a
    /// label and the name after it as an instruction (probe n9); indented,
    /// `mk MACRO a` is an unrecognised instruction rather than a definition
    /// (probe n8). Both are errors there, and returning `None` makes them
    /// errors here.
    ///
    /// Parameters are comma-separated in both forms, and a comma may not stand
    /// between the keyword and the name in either (probes n10, n11) — which
    /// the name check below rejects, where the previous grammar allowed it.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        let text = macros::without_comment(line);
        let indented = text.starts_with(char::is_whitespace);
        let (first, rest) = text.trim().split_once(char::is_whitespace)?;
        let (name, tail) = if first.eq_ignore_ascii_case("macro") {
            if !indented {
                return None;
            }
            split_macro_name(rest)
        } else {
            if indented {
                return None;
            }
            let (kw, tail) = split_macro_name(rest);
            if !kw.eq_ignore_ascii_case("macro") {
                return None;
            }
            (first.trim_end_matches(':'), tail)
        };
        if name.is_empty() || name.contains(',') {
            return None;
        }
        let params = tail
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        Some((name.to_string(), params))
    }

    /// `ENDM`, alone on its line.
    fn is_end(&self, line: &str) -> bool {
        macros::without_comment(line)
            .trim()
            .eq_ignore_ascii_case("endm")
    }

    fn end_keyword(&self) -> &'static str {
        "endm"
    }

    /// sjasmplus rejects any mismatch, and says which way round it went.
    fn fit_arguments(
        &self,
        name: &str,
        params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        match args.len().cmp(&params.len()) {
            std::cmp::Ordering::Greater => Err(format!("too many arguments for macro `{name}`")),
            std::cmp::Ordering::Less => Err(format!("not enough arguments for macro `{name}`")),
            std::cmp::Ordering::Equal => Ok(args),
        }
    }

    /// The dot-prefixed labels a macro body **defines**.
    ///
    /// These scope to the expansion rather than to the file, which is what lets
    /// a macro containing a loop be invoked more than once — most of what macros
    /// are for. A *plain* label in the same position stays global and collides
    /// on the second invocation; the reference reports `Duplicate label` there
    /// and so do we, which is why only the dotted ones are renamed.
    ///
    /// Only names the body defines are renamed, so a body referring to a local
    /// defined outside it still refers to that one.
    fn locals(&self, body: &[String]) -> Vec<String> {
        let mut names = Vec::new();
        for line in body {
            let text = macros::without_comment(line);
            if text.starts_with(char::is_whitespace) {
                continue;
            }
            let token = text.split_whitespace().next().unwrap_or("");
            let name = token.trim_end_matches(':');
            if name.starts_with('.') && name.len() > 1 && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_sjasmplus as asm;

    /// A module left open at end of file assembles, and now says so. The
    /// reference warns once, naming the *innermost* module by its full dotted
    /// path — not one advisory per open module (probe m19).
    #[test]
    fn an_unclosed_module_is_reported() {
        let r = asm("    MODULE foo\nbar: db 1\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x01], "it still assembles");
        assert_eq!(r.warnings.len(), 1);
        assert!(
            r.warnings[0]
                .message
                .contains("`ENDMODULE` missing for module `foo`")
        );
        assert_eq!(
            r.warnings[0].line, 1,
            "reported against the line that opened it"
        );

        let r = asm("    MODULE foo\n    MODULE baz\nbar: db 1\n").expect("assemble");
        assert_eq!(r.warnings.len(), 1, "one advisory, not one per module");
        assert!(r.warnings[0].message.contains("`foo.baz`"));

        assert!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n")
                .expect("assemble")
                .warnings
                .is_empty()
        );
    }

    // -----------------------------------------------------------------------
    // Forward-referenced conditions (#99,
    // `decisions/forward-conditions-and-passes.md`). Probed against SjASMPlus
    // 1.21.0, 2026-08-23 — bytes *and* warnings.
    // -----------------------------------------------------------------------

    /// A condition may name a symbol defined below it. Pass 1 reads it as
    /// zero and says so; later passes answer from the pass before.
    #[test]
    fn a_condition_may_reach_forward() {
        let r = asm(" IF later\n ld a,1\n ENDIF\nlater: nop\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x00]);
        assert_eq!(r.warnings.len(), 1);
        assert!(
            r.warnings[0]
                .message
                .contains("forward reference of symbol `later`")
        );
        assert_eq!(r.warnings[0].line, 1);
    }

    /// A backward reference is not a forward one, and must not say it is —
    /// the walk binds each label to its address as it passes, so this folds
    /// against a value and warns about nothing.
    #[test]
    fn a_backward_condition_needs_no_pass() {
        let r = asm("later: nop\n IF later\n ld a,1\n ENDIF\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x00]);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    /// The case #99 was really about. Emitting the body moves `later` past 2,
    /// so the condition that admitted the body is false by the end — and the
    /// body is in the binary. The reference ships that and warns twice; so do
    /// we, rather than converging further than it does or refusing what it
    /// builds.
    #[test]
    fn a_condition_that_never_settles_warns_and_ships() {
        let r = asm(" IF later < 2\n ld a,1\n ENDIF\nlater: nop\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x3E, 0x01, 0x00]);
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        assert!(
            r.warnings[1]
                .message
                .contains("has a different value in pass 3")
        );
        assert!(
            r.warnings[1]
                .message
                .contains("previous value 0 not equal 2")
        );
        assert_eq!(r.warnings[1].line, 4, "reported on the label's line");
    }

    /// pasmo shares the walk and must not gain the behaviour through it: it
    /// keeps the parse-time-constant rule and the diagnostic that explains it.
    #[test]
    fn pasmo_still_requires_a_constant_condition() {
        assert!(
            crate::assemble_pasmo(" IF later\n ld a,1\n ENDIF\nlater: nop\n").is_err(),
            "pasmo has no forward-condition adoption"
        );
    }

    // -----------------------------------------------------------------------
    // `:` as a statement separator (#98,
    // `decisions/colon-separated-statements.md`). Probes s1-s11 against
    // SjASMPlus 1.21.0, 2026-08-23.
    // -----------------------------------------------------------------------

    /// The plain case, which is what showed this was never about conditionals:
    /// instructions separated by `:` failed exactly the way the colon-inline
    /// `IF` did.
    #[test]
    fn a_colon_separates_statements() {
        assert_eq!(
            asm(" ld a,1 : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02]
        );
        assert_eq!(
            asm(" ld a,1:ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02],
            "the spaces are not what makes it one"
        );
        assert_eq!(
            asm(" ld a,1 : : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02],
            "an empty statement between two colons is nothing"
        );
    }

    /// The colon that closes a label is not a separator, and the rule that
    /// tells them apart is positional: first in its statement, nothing but an
    /// identifier before it. That covers a local label and `::` as well.
    #[test]
    fn a_labels_colon_is_not_a_separator() {
        assert_eq!(
            asm("lbl: ld a,1 : ld b,2\n djnz lbl\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x01, 0x06, 0x02, 0x10, 0xFA]
        );
        assert_eq!(
            asm("glob:\n.l: ld a,1 : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02]
        );
        assert_eq!(
            asm("gl:: ld a,1 : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02],
            "`::` closes a label as one token"
        );
    }

    /// A colon inside a literal separates nothing, and neither does one in a
    /// comment — the comment is found first and rides with its statement.
    #[test]
    fn a_colon_in_a_literal_or_comment_separates_nothing() {
        assert_eq!(
            asm(" db \":\" : db 1\n").expect("assemble").bytes,
            vec![0x3A, 0x01]
        );
        assert_eq!(
            asm(" db ':' : db 1\n").expect("assemble").bytes,
            vec![0x3A, 0x01]
        );
        assert_eq!(
            asm(" ld a,1 ; a:b\n").expect("assemble").bytes,
            vec![0x3E, 0x01]
        );
    }

    /// A block may open, fill and close inside one line's statements — which
    /// is the form #67 filed, and it falls out rather than being handled.
    #[test]
    fn a_conditional_fits_on_one_line() {
        assert_eq!(
            asm(" IF 1 : ld a,1 : ENDIF\n").expect("assemble").bytes,
            vec![0x3E, 0x01]
        );
        assert_eq!(
            asm(" IF 0 : ld a,1 : ENDIF\n ld b,2\n")
                .expect("assemble")
                .bytes,
            vec![0x06, 0x02]
        );
    }

    /// The formatter puts one statement on each line, which is what it already
    /// did to a label sharing a line with an operation. Idempotent, and the
    /// same bytes.
    #[test]
    fn the_formatter_expands_a_colon_line() {
        let out = crate::format_sjasmplus("lbl: ld a,1 : ld b,2\n djnz lbl\n").expect("format");
        assert_eq!(
            out,
            "lbl:\n        ld a,1\n        ld b,2\n        djnz lbl\n"
        );
        assert_eq!(
            crate::format_sjasmplus(&out).expect("idempotent"),
            out,
            "formatting the output changes nothing further"
        );
    }

    /// Each statement gets its own debug span, all naming the line they share.
    /// Nothing collapses, which is why the frozen wire format needed no column.
    #[test]
    fn each_statement_on_a_colon_line_gets_its_own_span() {
        let r = asm(" ld a,1 : ld b,2\n nop\n").expect("assemble");
        let spans: Vec<_> = r
            .debug
            .lines
            .iter()
            .map(|s| (s.line, s.offset, s.length))
            .collect();
        assert_eq!(spans, vec![(1, 0, 2), (1, 2, 2), (2, 4, 1)]);
    }

    /// pasmo shares the whole Z80 core and must not pick the form up through
    /// it — it has no colon separator, and splitting on a character it treats
    /// as ordinary would invent a dialect.
    #[test]
    fn pasmo_does_not_split_on_a_colon() {
        assert!(crate::assemble_pasmo(" ld a,1 : ld b,2\n").is_err());
    }

    // -----------------------------------------------------------------------
    // The two macro spellings (#205). Probes n1–n11 against SjASMPlus 1.21.0,
    // run 2026-08-23.
    // -----------------------------------------------------------------------

    /// The reference takes the definition either way round, and the name-first
    /// form carries parameters like the other (n1, n3, n4, n5).
    #[test]
    fn a_macro_may_be_defined_name_first() {
        assert_eq!(
            asm("mk MACRO a, b\n db a,b\n ENDM\n mk 1,2\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x02]
        );
        assert_eq!(
            asm("mk: MACRO a\n db a\n ENDM\n mk 3\n")
                .expect("assemble")
                .bytes,
            vec![0x03],
            "the colon form is the same definition"
        );
        assert_eq!(
            asm("mk macro a\n db a\n endm\n mk 4\n")
                .expect("assemble")
                .bytes,
            vec![0x04],
            "the keyword is case-insensitive here too"
        );
        assert_eq!(
            asm("mk MACRO\n nop\n ENDM\n mk\n").expect("assemble").bytes,
            vec![0x00],
            "no parameters"
        );
    }

    /// Which spelling a line is depends on its **column**, and a line in the
    /// wrong one is not a definition at all. The reference reads a column-0
    /// `MACRO` as a label (n9), and an indented `mk MACRO a` as an
    /// unrecognised instruction (n8). Both are errors there, and here.
    #[test]
    fn the_macro_spellings_are_told_apart_by_indentation() {
        assert!(
            asm(" mk MACRO a\n db a\n ENDM\n mk 8\n").is_err(),
            "an indented name-first header is not a definition"
        );
        assert!(
            asm("MACRO kw a\n db a\n ENDM\n kw 9\n").is_err(),
            "a column-0 keyword-first header is not a definition"
        );
    }

    /// Parameters are comma-separated in both spellings (n2, n7), and a comma
    /// may not stand between the keyword and the name in either (n10, n11).
    /// The last of these is a case the previous grammar accepted and the
    /// reference does not.
    #[test]
    fn macro_parameters_are_comma_separated() {
        assert!(
            asm("mk MACRO a b\n db a,b\n ENDM\n mk 1,2\n").is_err(),
            "space-separated parameters, name-first"
        );
        assert!(
            asm(" MACRO kw a b\n db a,b\n ENDM\n kw 5,6\n").is_err(),
            "space-separated parameters, keyword-first"
        );
        assert!(
            asm("mk MACRO, a\n db a\n ENDM\n mk 10\n").is_err(),
            "a comma after the keyword"
        );
        assert!(
            asm(" MACRO kw, a\n db a\n ENDM\n kw 11\n").is_err(),
            "a comma after the name — the reference calls it an illegal macro name"
        );
    }

    // -----------------------------------------------------------------------
    // Modules (#93's third item). Every case below is a probe against
    // SjASMPlus 1.21.0, run 2026-08-23 — the probe ids are the ones the plan
    // (`docs/plans/2026-08-23-001-feat-sjasmplus-modules-plan.md`) tabulates.
    // -----------------------------------------------------------------------

    /// The base rule: a module prefixes the labels defined inside it, and the
    /// qualified name is how the outside reaches them (m1). Nesting
    /// concatenates with `.` (m5), and `ENDMOD` closes as well as `ENDMODULE`
    /// (m7).
    #[test]
    fn a_module_prefixes_the_labels_defined_inside_it() {
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm(
                "    MODULE foo\n    MODULE baz\nbar: db 1\n    ENDMODULE\n    \
                 ENDMODULE\n    db foo.baz.bar\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMOD\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
    }

    /// A reference has **two** candidates and only two: the fully-qualified
    /// name, then the bare global one. Inside `foo`, `bar` finds `foo.bar`
    /// (m2) and `top` finds the global `top` (m13) — but outside, `bar` finds
    /// nothing (m3).
    #[test]
    fn a_reference_tries_the_qualified_name_then_the_global_one() {
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    db bar\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("top: db 9\n    MODULE foo\n    db top\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0x09, 0x00]
        );
        assert!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n    db bar\n").is_err(),
            "a module's name is not visible unqualified from outside"
        );
    }

    /// The qualified candidate wins when both exist (m31) — so a module may
    /// shadow a global of the same name without the outer one leaking in.
    #[test]
    fn the_qualified_candidate_wins_over_the_global_one() {
        assert_eq!(
            asm("x equ $AA\n    MODULE foo\nx equ $BB\n    db x\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0xBB]
        );
    }

    /// There is **no walk-up**: an inner module does not see an outer module's
    /// unqualified names, only its own and the globals (m8, m32). This is the
    /// rule that makes the two-candidate model a model rather than a shortcut,
    /// so it gets its own test.
    #[test]
    fn an_inner_module_does_not_see_the_outer_modules_names() {
        assert!(
            asm(
                "    MODULE foo\nouter: db 1\n    MODULE baz\n    db outer\n    \
                 ENDMODULE\n    ENDMODULE\n"
            )
            .is_err(),
            "`outer` is `foo.outer`; `foo.baz` reaches neither it nor a global"
        );
        assert_eq!(
            asm("g equ $CC\n    MODULE foo\n    MODULE baz\n    db g\n    \
                 ENDMODULE\n    ENDMODULE\n")
            .expect("assemble")
            .bytes,
            vec![0xCC],
            "the second candidate is the global, at any depth"
        );
    }

    /// The choice between the two candidates cannot be made as the line is
    /// read: either may be defined later (m33, m34). Both directions resolve.
    #[test]
    fn a_forward_reference_picks_the_right_candidate() {
        assert_eq!(
            asm("    MODULE foo\n    db bar\nbar equ $DD\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0xDD]
        );
        assert_eq!(
            asm("    MODULE foo\n    db g\n    ENDMODULE\ng equ $EE\n")
                .expect("assemble")
                .bytes,
            vec![0xEE]
        );
    }

    /// A leading `@` escapes module scoping — on a definition (m4, m15), on a
    /// reference (m9), and on an already-dotted name (m30).
    #[test]
    fn an_at_sign_escapes_the_module_scope() {
        assert_eq!(
            asm("    MODULE foo\n@bar: db 1\n    ENDMODULE\n    db bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm(
                "    MODULE foo\n    MODULE baz\n@bar: db 1\n    ENDMODULE\n    \
                 ENDMODULE\n    db bar\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\ntop: db 2\n    \
                 MODULE foo2\n    db @top\n    ENDMODULE\n")
            .expect("assemble")
            .bytes,
            vec![0x01, 0x02, 0x01]
        );
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n    db @foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
    }

    /// Locals compose *under* modules, not beside them: the leading-`.` rule
    /// runs first and the module prefix wraps its result, so `.loc` under
    /// `glob` inside `foo` is `foo.glob.loc` (m6, m25).
    #[test]
    fn a_local_label_inside_a_module_is_qualified_by_both() {
        assert_eq!(
            asm("    MODULE foo\nglob:\n.loc: db 1\n    db glob.loc\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\nglob:\n.loc: db 1\n    ENDMODULE\n    db foo.glob.loc\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
    }

    /// A macro is not module-scoped, but its *expansion* is: the labels a
    /// macro defines take the prefix of wherever it was invoked (m18, m23).
    #[test]
    fn a_macro_expands_into_the_module_that_invoked_it() {
        assert_eq!(
            asm(
                "    MACRO mk\nlbl: db 1\n    ENDM\n    MODULE foo\n    mk\n    \
                 ENDMODULE\n    db foo.lbl\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\n    MACRO mk\n    db 1\n    ENDM\n    ENDMODULE\n    mk\n")
                .expect("assemble")
                .bytes,
            vec![0x01],
            "the macro name itself stays global"
        );
    }

    /// `DEFINE` is not module-scoped either (m24), and `equ` is (m11).
    #[test]
    fn equ_is_module_scoped_and_define_is_not() {
        assert_eq!(
            asm("    MODULE foo\nbar equ 7\n    ENDMODULE\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x07]
        );
        assert_eq!(
            asm("    MODULE foo\n    DEFINE V 5\n    ENDMODULE\n    db V\n")
                .expect("assemble")
                .bytes,
            vec![0x05]
        );
    }

    /// Reopening a module name adds to it rather than starting again (m12).
    #[test]
    fn a_module_may_be_reopened() {
        assert_eq!(
            asm(
                "    MODULE foo\nbar: db 1\n    ENDMODULE\n    MODULE foo\nbaz: db 2\n    \
                 ENDMODULE\n    db foo.bar, foo.baz\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x02, 0x00, 0x01]
        );
    }

    /// The keyword follows the same strict case rule as the conditionals and
    /// repetition — all-lower or all-upper, never mixed (m21, m26).
    #[test]
    fn the_module_keyword_is_all_one_case() {
        assert_eq!(
            asm("    module foo\nbar: db 1\n    endmodule\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert!(
            asm("    Module foo\nbar: db 1\n    EndModule\n").is_err(),
            "the reference answers `Module` with `Unrecognized instruction`"
        );
    }

    /// The reference reads a column-0 `MODULE` as a *label* and the name after
    /// it as an instruction (m27), so `MODULE` is deliberately absent from the
    /// directive set that suppresses column-0 label parsing. Indentation is
    /// load-bearing, in the reference and here.
    #[test]
    fn a_column_zero_module_is_a_label_not_a_directive() {
        assert!(
            asm("MODULE foo\nbar: db 1\nENDMODULE\n    db foo.bar\n").is_err(),
            "`foo` is then an unknown instruction, as in the reference"
        );
    }

    /// The three malformed cases: no name (m10), a dotted name, which is not a
    /// nesting shorthand (m29), and a close with nothing open (m20).
    #[test]
    fn a_malformed_module_is_refused() {
        assert!(
            asm("    MODULE\nbar: db 1\n    ENDMODULE\n").is_err(),
            "no name"
        );
        assert!(
            asm("    MODULE foo.baz\nbar: db 1\n    ENDMODULE\n").is_err(),
            "a dotted name is rejected, not read as nesting"
        );
        assert!(
            asm("    endmodule\nx: db 1\n").is_err(),
            "close with nothing open"
        );
    }

    /// Modules are sjasmplus's alone: pasmo shares the whole Z80 core, and
    /// must not pick the spelling up through it.
    #[test]
    fn pasmo_does_not_have_modules() {
        assert!(
            crate::assemble_pasmo("    MODULE foo\nbar: db 1\n    ENDMODULE\n").is_err(),
            "pasmo has no MODULE"
        );
    }

    /// The keyword is case-insensitive but the **name** is not — measured
    /// against sjasmplus 1.21.0, and not a combination anyone would guess.
    /// Defining `mac` and calling `MAC` is an error there, so it must be here.
    #[test]
    fn the_macro_keyword_is_case_insensitive_but_the_name_is_not() {
        assert_eq!(
            asm(" macro m\n nop\n endm\n m\n").expect("assemble").bytes,
            vec![0x00]
        );
        assert!(
            asm(" MACRO mac\n nop\n ENDM\n MAC\n").is_err(),
            "a macro name is case-sensitive"
        );
    }

    /// A macro containing a loop must be usable more than once — which is most
    /// of what macros are for. The dot-local is scoped to the expansion, so the
    /// second invocation does not collide with the first.
    #[test]
    fn a_macro_local_label_is_scoped_to_its_expansion() {
        assert_eq!(
            asm(" MACRO m\n.loc djnz .loc\n ENDM\n m\n m\n m\n")
                .expect("assemble")
                .bytes,
            vec![0x10, 0xFE, 0x10, 0xFE, 0x10, 0xFE]
        );
    }

    /// Macros compose, and a macro may invoke one defined **later** — the
    /// reference resolves a name when it expands, not when it reads, which is
    /// why every definition is collected before anything expands.
    #[test]
    fn macros_nest_and_may_invoke_one_defined_later() {
        assert_eq!(
            asm(" MACRO inner v\n ld a,v\n ENDM\n MACRO outer w\n inner w\n ENDM\n outer 5\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x05]
        );
        assert_eq!(
            asm(" MACRO outer\n inner\n ENDM\n MACRO inner\n nop\n ENDM\n outer\n")
                .expect("assemble")
                .bytes,
            vec![0x00]
        );
    }

    /// Locals stay distinct through nesting: one outer expansion invoking the
    /// same inner macro twice still gets two separate labels.
    #[test]
    fn locals_stay_distinct_through_nesting() {
        assert_eq!(
            asm(" MACRO m\n.loc djnz .loc\n ENDM\n MACRO two\n m\n m\n ENDM\n two\n")
                .expect("assemble")
                .bytes,
            vec![0x10, 0xFE, 0x10, 0xFE]
        );
    }

    /// An error in generated code must say where the text came from: the
    /// failing line is nowhere in the file the reader has open. Frames are
    /// innermost first, matching the `included from` chain's order.
    #[test]
    fn an_error_in_an_expansion_carries_its_frames() {
        let err = asm(" MACRO inner\n frobnicate\n ENDM\n MACRO outer\n inner\n ENDM\n outer\n")
            .expect_err("frobnicate is not an instruction");
        let span = err.span.as_ref().expect("a span");
        let named: Vec<&str> = span
            .expansion_frames
            .iter()
            .map(|f| f.macro_name.as_str())
            .collect();
        assert_eq!(named, vec!["inner", "outer"], "innermost first");
        assert_eq!(span.line, 7, "and it points at the invocation");
    }

    /// Source with no macros is untouched — no frames, nothing to explain.
    #[test]
    fn an_error_outside_an_expansion_carries_none() {
        let err = asm(" frobnicate\n").expect_err("not an instruction");
        assert!(
            err.span
                .as_ref()
                .is_none_or(|s| s.expansion_frames.is_empty()),
            "{err:?}"
        );
    }

    /// Repetition's count is an expression over the environment, so it folds
    /// where conditions fold rather than in the macro pre-pass — `DUP n+1`
    /// with `n equ 2` repeats three times.
    #[test]
    fn dup_repeats_its_body_a_computed_number_of_times() {
        assert_eq!(
            asm(" DUP 3\n nop\n EDUP\n").expect("assemble").bytes,
            vec![0x00, 0x00, 0x00]
        );
        assert_eq!(
            asm("n equ 2\n DUP n+1\n nop\n EDUP\n")
                .expect("assemble")
                .bytes,
            vec![0x00, 0x00, 0x00]
        );
        assert!(
            asm(" DUP 0\n nop\n EDUP\n")
                .expect("assemble")
                .bytes
                .is_empty(),
            "zero repetitions emit nothing"
        );
    }

    /// `REPT`/`ENDR` is the same block, and the spellings interchange — the
    /// reference accepts a `DUP` closed by `ENDR`.
    #[test]
    fn rept_and_dup_are_the_same_block() {
        let dup = asm(" DUP 2\n nop\n EDUP\n").expect("assemble").bytes;
        assert_eq!(asm(" REPT 2\n nop\n ENDR\n").expect("assemble").bytes, dup);
        assert_eq!(asm(" DUP 2\n nop\n ENDR\n").expect("assemble").bytes, dup);
    }

    /// Blocks nest, and interleave with macros in both directions.
    #[test]
    fn repetition_nests_and_composes_with_macros() {
        assert_eq!(
            asm(" DUP 2\n DUP 2\n nop\n EDUP\n EDUP\n")
                .expect("assemble")
                .bytes,
            vec![0x00; 4]
        );
        assert_eq!(
            asm(" MACRO m\n nop\n ENDM\n DUP 2\n m\n EDUP\n")
                .expect("assemble")
                .bytes,
            vec![0x00, 0x00]
        );
        assert_eq!(
            asm(" MACRO m\n DUP 2\n nop\n EDUP\n ENDM\n m\n")
                .expect("assemble")
                .bytes,
            vec![0x00, 0x00]
        );
    }

    /// Mixed case is not a block keyword, matching the strict rule the
    /// conditionals already follow — the reference calls `Dup` an unrecognised
    /// instruction.
    #[test]
    fn a_mixed_case_repetition_keyword_is_not_one() {
        assert_eq!(
            asm(" dup 2\n nop\n edup\n").expect("assemble").bytes,
            vec![0x00, 0x00]
        );
        assert!(
            asm(" Dup 2\n nop\n Edup\n").is_err(),
            "mixed case is not `DUP`"
        );
    }

    /// A self-recursive macro segfaults sjasmplus (exit 139). We decline to
    /// reproduce that: a crash is not a verdict about anyone's source, and an
    /// assembler that dies is worse than one that explains itself.
    #[test]
    fn runaway_recursion_is_reported_not_crashed_on() {
        let err = asm(" MACRO recur\n recur\n ENDM\n recur\n").expect_err("recursive");
        assert!(err.message.contains("recur"), "names the macro: {err:?}");
        assert!(err.message.contains("recursive"), "{err:?}");
    }

    /// A *plain* label in a macro body stays global, so a second invocation
    /// collides — the reference reports `Duplicate label` for exactly this, so
    /// scoping it would diverge from the tool we claim to match.
    #[test]
    fn a_plain_label_in_a_macro_body_stays_global() {
        let err = asm(" MACRO m\nplain djnz plain\n ENDM\n m\n m\n")
            .expect_err("the second expansion redefines `plain`");
        assert!(err.message.contains("duplicate label"), "{err:?}");
    }

    /// Substitution respects word boundaries: a parameter `v` must leave the
    /// symbol `val` alone. A naive replace would assemble `ld a,5al`.
    #[test]
    fn substitution_stops_at_word_boundaries() {
        assert_eq!(
            asm(" MACRO m v\nval equ 9\n ld a,val\n ENDM\n m 5\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x09]
        );
    }

    /// And it does not reach inside string literals — `db "v"` emits the
    /// letter, not the argument.
    #[test]
    fn substitution_does_not_reach_inside_strings() {
        assert_eq!(
            asm(" MACRO m v\n db \"v\"\n ENDM\n m 5\n")
                .expect("assemble")
                .bytes,
            vec![b'v']
        );
    }

    /// Substitution is textual and happens before the expression is evaluated,
    /// so `val*2` with `val = 5` is `ld a,10`.
    #[test]
    fn a_parameter_substitutes_before_the_expression_is_evaluated() {
        assert_eq!(
            asm(" MACRO m val\n ld a,val*2\n ENDM\n m 5\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x0A]
        );
    }

    /// A diagnostic must name a line the author wrote. An error inside a macro
    /// body reports the **invocation**, which is where a reader looks first —
    /// the expanded line number never existed in their file.
    #[test]
    fn an_error_inside_an_expansion_names_the_invocation() {
        let err = asm(" nop\n MACRO bad\n frobnicate\n ENDM\n nop\n bad\n")
            .expect_err("frobnicate is not an instruction");
        assert_eq!(err.line, 6, "the invocation is on line 6: {err:?}");
    }

    /// A label may sit in front of an invocation, and binds to the address the
    /// expansion starts at — the same rule a label on an `include` line
    /// follows, and the colon is optional as everywhere else.
    ///
    /// Getting this wrong does not mis-assemble; it rejects the line, because
    /// the label is read as the mnemonic. The reference assembles it.
    #[test]
    fn a_label_may_sit_in_front_of_an_invocation() {
        for src in [
            " MACRO m1 v\n ld a,v\n ENDM\nlbl: m1 9\n ld hl,lbl\n",
            " MACRO m1 v\n ld a,v\n ENDM\nlbl m1 9\n ld hl,lbl\n",
        ] {
            // The label is at $0000: it precedes the expansion, not follows it.
            assert_eq!(
                asm(src).expect(src).bytes,
                vec![0x3E, 0x09, 0x21, 0x00, 0x00],
                "{src}"
            );
        }
    }

    /// The formatter lays source out; it does not rewrite programs. Formatting
    /// must therefore give the macro **back**, not the lines it expands to.
    ///
    /// This is a regression test with teeth: expansion is a source pre-pass, so
    /// the obvious wiring — one hook on the shared parse — silently made `fmt`
    /// inline every invocation and delete every definition. Over a file, in
    /// place. The parse the formatter asks for is deliberately not the parse
    /// assembly asks for (`z80::Expand`).
    #[test]
    fn formatting_preserves_a_macro_rather_than_expanding_it() {
        let src = " MACRO setv v\n ld a,v\n ENDM\n setv 9\n";
        let out = crate::format_sjasmplus(src).expect("formats");
        assert!(out.contains("MACRO setv v"), "definition is gone: {out}");
        assert!(out.contains("ENDM"), "terminator is gone: {out}");
        assert!(out.contains("setv 9"), "invocation is gone: {out}");
        assert!(!out.contains("ld a,9"), "expanded into the source: {out}");
        // And assembly of the same text still expands, so the two paths really
        // are different parses rather than one of them being broken.
        assert_eq!(asm(src).expect("assembles").bytes, vec![0x3E, 0x09]);
    }

    /// Arity is checked in both directions, and unterminated definitions are
    /// caught where they begin rather than at end of file.
    ///
    /// The two directions get different words because sjasmplus gives them
    /// different words, and they are different mistakes: too many arguments
    /// means the call is wrong, too few usually means the macro moved on.
    #[test]
    fn arity_and_termination_are_checked() {
        let short = asm(" MACRO m v\n ld a,v\n ENDM\n m\n").expect_err("too few arguments");
        assert!(short.message.contains("not enough arguments"), "{short:?}");
        let long = asm(" MACRO m v\n ld a,v\n ENDM\n m 1,2\n").expect_err("too many arguments");
        assert!(long.message.contains("too many arguments"), "{long:?}");
        let open = asm(" MACRO m\n nop\n").expect_err("no endm");
        assert_eq!(open.line, 1, "reported where the definition opened");
        assert!(open.message.contains("`endm`"), "{open:?}");
    }

    #[test]
    fn number_formats() {
        // All of these are $1234.
        for src in ["ld hl, $1234", "ld hl, 0x1234", "ld hl, 1234h"] {
            assert_eq!(asm(src).expect(src).bytes, vec![0x21, 0x34, 0x12], "{src}");
        }
        // All of these are %1010 = 0x0A.
        for src in ["ld a, %1010", "ld a, 0b1010", "ld a, 1010b"] {
            assert_eq!(asm(src).expect(src).bytes, vec![0x3E, 0x0A], "{src}");
        }
    }

    #[test]
    fn slash_slash_comment() {
        assert_eq!(
            asm("ld a, 5  // a comment\n").expect("//").bytes,
            vec![0x3E, 0x05]
        );
    }

    #[test]
    fn shares_instruction_syntax_with_pasmo() {
        // Identical bytes to pasmo for the shared instruction syntax.
        let src = "        org $8000\nloop:   ld a, (ix+5)\n        bit 7,(hl)\n        ldir\n        jr loop\n";
        assert_eq!(
            asm(src).expect("sjasm").bytes,
            crate::assemble_pasmo(src).expect("pasmo").bytes
        );
    }

    #[test]
    fn ds_reserves_bytes() {
        assert_eq!(asm("        ds 3\n").expect("ds").bytes, vec![0, 0, 0]);
    }

    #[test]
    fn oversized_byte_truncates_with_a_warning() {
        // sjasmplus keeps the low 8 bits and warns (byte-identical to sjasmplus:
        // `ld a,$1234` -> 3e 34, one warning).
        let a = asm("        ld a,$1234\n").expect("truncate");
        assert_eq!(a.bytes, vec![0x3E, 0x34]);
        assert_eq!(a.warnings.len(), 1);
        assert!(a.warnings[0].message.contains("truncated"));
        // In range: no warning.
        assert!(asm("        ld a,$12\n").expect("ok").warnings.is_empty());
    }

    #[test]
    fn byte_is_db() {
        // sjasmplus's `byte` behaves exactly like `db` — values and strings.
        // Byte-for-byte against `sjasmplus --raw`.
        assert_eq!(
            asm("        byte 1,2,$ff\n").expect("byte vals").bytes,
            vec![0x01, 0x02, 0xFF]
        );
        assert_eq!(
            asm("        byte \"AB\"\n").expect("byte str").bytes,
            vec![0x41, 0x42]
        );
    }

    #[test]
    fn local_labels_scope_under_the_preceding_global() {
        // The same `.loop` recurs under two globals; each `jr .loop` binds to
        // its own scope. Validated byte-for-byte against the sjasmplus binary.
        let src = "        org $8000\n\
                   start:\n.loop:  nop\n        jr .loop\n        nop\n\
                   done:\n.loop:  nop\n        jr .loop\n";
        let a = asm(src).expect("local scoping");
        assert_eq!(a.bytes, vec![0x00, 0x18, 0xFD, 0x00, 0x00, 0x18, 0xFD]);
        // The qualified names are distinct in the symbol table.
        assert_eq!(a.symbols.get("start.loop"), Some(&0x8000));
        assert_eq!(a.symbols.get("done.loop"), Some(&0x8004));
    }

    #[test]
    fn pasmo_rejects_reused_local_label() {
        // pasmo treats `.loop` as an ordinary global, so reuse is a duplicate.
        let src = "start:\n.loop:  nop\ndone:\n.loop:  nop\n";
        let err = crate::assemble_pasmo(src).expect_err("duplicate");
        assert!(err.message.contains("duplicate"), "unexpected: {err}");
    }

    #[test]
    fn location_counter_is_statement_start() {
        // `$` is the current statement's address (matches pasmo and the binary).
        let a = asm("        org $8000\n        jr $\n        ld hl,$\n").expect("pc");
        assert_eq!(a.bytes, vec![0x18, 0xFE, 0x21, 0x02, 0x80]);
    }

    // -----------------------------------------------------------------------
    // Conditional assembly + DEFINE (language-surface U8). Every byte
    // expectation below is pinned by the sjasmplus 1.21.0 probe runs (the
    // u8-probes set); the same programs ride the differential corpus.
    // -----------------------------------------------------------------------

    /// AE4 (R5): taken and untaken branches, with `ELSE`, byte-identical to
    /// the reference (probe p1).
    #[test]
    fn conditional_takes_the_live_branch() {
        let src = "        org $8000\n\
                   \x20       IF 1\n        ld a,1\n        ELSE\n        ld a,2\n        ENDIF\n\
                   \x20       IF 0\n        ld b,1\n        ELSE\n        ld b,2\n        ENDIF\n";
        assert_eq!(asm(src).expect("p1").bytes, vec![0x3E, 0x01, 0x06, 0x02]);
    }

    /// Condition grammar: comparisons (`=`/`==`/`>`/`<`/`>=`/`!=`),
    /// arithmetic truthiness, `&&`/`||`/`!` (probe p2), and the
    /// parenthesised logical forms (probe p45).
    #[test]
    fn condition_expressions_match_the_reference() {
        let src = "        org $8000\n\
                   VAL     equ 5\n\
                   \x20       IF VAL = 5\n        ld a,1\n        ENDIF\n\
                   \x20       IF VAL == 5\n        ld a,2\n        ENDIF\n\
                   \x20       IF VAL > 3\n        ld a,3\n        ENDIF\n\
                   \x20       IF VAL < 3\n        ld a,4\n        ENDIF\n\
                   \x20       IF VAL*2-10\n        ld a,5\n        ENDIF\n\
                   \x20       IF VAL && 0\n        ld a,6\n        ENDIF\n\
                   \x20       IF VAL || 0\n        ld a,7\n        ENDIF\n\
                   \x20       IF !VAL\n        ld a,8\n        ENDIF\n\
                   \x20       IF (VAL = 5) && !(VAL && 0)\n        ld a,9\n        ENDIF\n";
        assert_eq!(
            asm(src).expect("conditions").bytes,
            vec![0x3E, 1, 0x3E, 2, 0x3E, 3, 0x3E, 7, 0x3E, 9]
        );
    }

    /// `IFDEF`/`IFNDEF` test the DEFINE namespace only — a same-named `equ`
    /// constant or label is *not* "defined" (probe p3), and names are
    /// case-sensitive (probe p22).
    #[test]
    fn ifdef_namespace_is_defines_only_and_case_sensitive() {
        let src = "        org $8000\n\
                   \x20       DEFINE flag\n\
                   CONST   equ 7\n\
                   LBL:    nop\n\
                   \x20       IFDEF flag\n        ld a,1\n        ENDIF\n\
                   \x20       IFDEF FLAG\n        ld a,2\n        ENDIF\n\
                   \x20       IFDEF CONST\n        ld a,3\n        ENDIF\n\
                   \x20       IFDEF LBL\n        ld a,4\n        ENDIF\n\
                   \x20       IFNDEF NOPE\n        ld a,5\n        ENDIF\n";
        assert_eq!(asm(src).expect("ifdef").bytes, vec![0x00, 0x3E, 1, 0x3E, 5]);
    }

    /// `DEFINE NAME value` substitutes textually at identifier boundaries —
    /// operands, whole instructions, chains — but never inside strings or
    /// partial identifiers (probes p4/p5/p20/p21/p24/p26).
    #[test]
    fn define_substitutes_textually() {
        // Operand (p4) and expression (p6) positions.
        assert_eq!(
            asm("        DEFINE X 5\n        ld a,X\n")
                .expect("p4")
                .bytes,
            vec![0x3E, 5]
        );
        assert_eq!(
            asm("        DEFINE N 3\n        ld a,N+1\n        db N,N*2\n")
                .expect("p6")
                .bytes,
            vec![0x3E, 4, 3, 6]
        );
        // A whole-instruction replacement on a bare line (p5).
        assert_eq!(
            asm("        DEFINE X ld a,1\n        X\n")
                .expect("p5")
                .bytes,
            vec![0x3E, 1]
        );
        // Chained defines expand at use (p24).
        assert_eq!(
            asm("        DEFINE A1 3\n        DEFINE B1 A1+1\n        db B1\n")
                .expect("p24")
                .bytes,
            vec![4]
        );
        // A DEFINE'd name renames a label definition (p26).
        let r = asm("        org $8000\n        DEFINE L mylab\nL:      nop\n        jr mylab\n")
            .expect("p26");
        assert_eq!(r.bytes, vec![0x00, 0x18, 0xFD]);
        // Identifier boundaries: `NN` is not an occurrence of `N` (p20).
        assert!(asm("        DEFINE N 3\n        db NN\n").is_err(), "p20");
        // Strings are never rewritten (p21).
        assert_eq!(
            asm("        DEFINE N 3\n        db \"N\"\n")
                .expect("p21")
                .bytes,
            vec![0x4E]
        );
        // A duplicate DEFINE is the reference's error (p23).
        let e = asm("        DEFINE X 1\n        DEFINE X 2\n").expect_err("p23");
        assert!(e.message.contains("duplicate"), "unexpected: {e}");
    }

    /// A skipped branch defines nothing — labels, `equ` constants, and
    /// DEFINEs inside an untaken branch do not exist afterwards (probes
    /// p10/p10b), and untaken lines are never parsed at all (probe p31).
    #[test]
    fn skipped_branch_defines_nothing() {
        let src = "        org $8000\n\
                   \x20       IF 0\n\
                   skipped:  nop\n\
                   SKONST  equ 9\n\
                   \x20       DEFINE SKDEF\n\
                   \x20       ENDIF\n\
                   \x20       IFDEF SKDEF\n        ld a,1\n        ENDIF\n\
                   \x20       IFNDEF SKDEF\n        ld a,2\n        ENDIF\n";
        let r = asm(src).expect("skipped defines nothing");
        assert_eq!(r.bytes, vec![0x3E, 2]);
        assert!(!r.symbols.contains_key("skipped"), "skipped label leaked");
        // The skipped `equ` is unknown afterwards (the reference errors too).
        assert!(
            asm("        IF 0\nSK      equ 9\n        ENDIF\n        ld a,SK\n").is_err(),
            "p10b"
        );
        // Untaken lines are skipped without parsing (p31).
        assert_eq!(
            asm("        org $8000\n        IF 0\n        @@!! garbage (((\n        ENDIF\n        ld a,1\n")
                .expect("p31")
                .bytes,
            vec![0x3E, 1]
        );
    }

    /// Nested conditionals: the inner block evaluates only inside a taken
    /// outer branch, and nesting is tracked while skipping (probes p9/p42);
    /// lowercase keywords are the reference's other accepted spelling.
    #[test]
    fn conditionals_nest() {
        let src = "        org $8000\n\
                   \x20       if 1\n\
                   \x20       if 0\n        ld a,1\n        else\n        ld a,2\n        endif\n\
                   \x20       ifdef NOPE\n        ld a,3\n        endif\n\
                   \x20       endif\n";
        assert_eq!(asm(src).expect("p9").bytes, vec![0x3E, 2]);
        let src = "        org $8000\n\
                   \x20       IF 0\n\
                   \x20       IF 1\n        ld a,1\n        ENDIF\n        ld a,2\n\
                   \x20       ENDIF\n        ld a,3\n";
        assert_eq!(asm(src).expect("p42").bytes, vec![0x3E, 3]);
    }

    /// The environment threads across a conditional: an `equ` in a taken
    /// branch feeds later opcode-embedded form selection (probe p38), and a
    /// global label inside a taken branch rescopes later locals (probe p37).
    #[test]
    fn taken_branch_bindings_flow_out() {
        let src = "        org $8000\n\
                   \x20       IF 1\nBITN    equ 5\nPAD     equ 2\n        ENDIF\n\
                   \x20       bit BITN,a\n        ds PAD\n        ld a,1\n";
        assert_eq!(
            asm(src).expect("p38").bytes,
            vec![0xCB, 0x6F, 0, 0, 0x3E, 1]
        );
        let src = "        org $8000\n\
                   first:\n.l:     nop\n\
                   \x20       IF 1\nsecond:\n.l:     nop\n        jr .l\n        ENDIF\n\
                   \x20       jr .l\n";
        assert_eq!(
            asm(src).expect("p37").bytes,
            vec![0x00, 0x00, 0x18, 0xFD, 0x18, 0xFB]
        );
    }

    /// A label on the `IF` line binds at the block's address (probe p27).
    #[test]
    fn label_on_the_if_line_binds() {
        let r =
            asm("        org $8000\nlbl:    IF 1\n        ld a,1\n        ENDIF\n        jr lbl\n")
                .expect("p27");
        assert_eq!(r.bytes, vec![0x3E, 1, 0x18, 0xFC]);
        assert_eq!(r.symbols.get("lbl"), Some(&0x8000));
    }

    /// The block-structure error postures: an unterminated `IF`, a stray
    /// `ENDIF`, junk after `ENDIF` (the reference rejects it; junk after
    /// `ELSE` it ignores — probes p43/p43b/p35/p40), a stray `ELSEIF`, and an
    /// `ELSEIF` after `ELSE` all error clearly.
    #[test]
    fn block_structure_errors() {
        let e = asm("        IF 1\n        ld a,1\n").expect_err("p43");
        assert!(e.message.contains("ENDIF"), "unexpected: {e}");
        let e = asm("        ENDIF\n").expect_err("p43b");
        assert!(e.message.contains("without a matching"), "unexpected: {e}");
        let e = asm("        IF 1\n        ENDIF junk\n").expect_err("p35");
        assert!(e.message.contains("unexpected text"), "unexpected: {e}");
        // Junk after ELSE is tolerated, as the reference does (p40).
        assert_eq!(
            asm("        org $8000\n        IF 0\n        ld a,1\n        ELSE junk\n        ld a,2\n        ENDIF\n")
                .expect("p40")
                .bytes,
            vec![0x3E, 2]
        );
        let e = asm("        ELSEIF 1\n        ENDIF\n").expect_err("stray elseif");
        assert!(e.message.contains("without a matching"), "unexpected: {e}");
        // The reference tolerates an `ELSEIF` after `ELSE` by discarding it and
        // everything to the `ENDIF` (re-probed 2026-08-18). Dropping source
        // silently is worse than saying so, and no real program means it.
        let e = asm(
            "        IF 0\n        ld a,1\n        ELSE\n        ld a,2\n\
             \x20       ELSEIF 1\n        ld a,3\n        ENDIF\n",
        )
        .expect_err("elseif after else");
        assert!(e.message.contains("already closed"), "unexpected: {e}");
    }

    /// `ELSEIF` chains (#67), probed against the reference: the first true leg
    /// wins, a chain can end in `ELSE`, and none-true emits nothing.
    #[test]
    fn elseif_chains_pick_the_first_true_leg() {
        let asmb = |src: &str| asm(src).expect("chain").bytes;
        assert_eq!(
            asmb("        IF 0\n        ld a,1\n        ELSEIF 1\n        ld a,2\n        ENDIF\n"),
            vec![0x3E, 2]
        );
        assert_eq!(
            asmb("        IF 1\n        ld a,1\n        ELSEIF 1\n        ld a,2\n        ENDIF\n"),
            vec![0x3E, 1]
        );
        assert_eq!(
            asmb(
                "        IF 0\n        ld a,1\n        ELSEIF 1\n        ld a,2\n\
                  \x20       ELSEIF 1\n        ld a,3\n        ENDIF\n"
            ),
            vec![0x3E, 2],
            "the first true leg wins"
        );
        assert_eq!(
            asmb(
                "        IF 0\n        ld a,1\n        ELSEIF 0\n        ld a,2\n\
                  \x20       ELSE\n        ld a,3\n        ENDIF\n"
            ),
            vec![0x3E, 3]
        );
        assert_eq!(
            asmb(
                "        IF 0\n        ld a,1\n        ELSEIF 0\n        ld a,2\n        ENDIF\n        nop\n"
            ),
            vec![0x00],
            "no leg taken emits only what follows the block"
        );
    }

    /// Every conditional keyword also has a dotted spelling (#67). The dot does
    /// not relax the case rule, and dotted and undotted mix within one block —
    /// both probed against the reference.
    #[test]
    fn dotted_conditional_spellings() {
        let asmb = |src: &str| asm(src).expect("dotted").bytes;
        assert_eq!(
            asmb("        .IF 1\n        ld a,1\n        .ENDIF\n"),
            vec![0x3E, 1]
        );
        assert_eq!(
            asmb("        .if 1\n        ld a,1\n        .endif\n"),
            vec![0x3E, 1]
        );
        assert_eq!(
            asmb("        .IF 0\n        ld a,1\n        .ELSE\n        ld a,2\n        .ENDIF\n"),
            vec![0x3E, 2]
        );
        assert_eq!(
            asmb("        .IF 1\n        ld a,1\n        ENDIF\n"),
            vec![0x3E, 1],
            "dotted and undotted mix, as the reference allows"
        );
        assert!(
            asm("        .If 1\n        ld a,1\n        .EndIf\n").is_err(),
            "the dot does not relax the all-upper/all-lower rule"
        );
    }

    /// Keywords spell all-lower or all-upper only; a mixed-case `If` is an
    /// ordinary identifier, exactly as the reference treats it (probe p11).
    #[test]
    fn mixed_case_keywords_are_not_conditionals() {
        assert!(
            asm("        If 1\n        ld a,1\n        Endif\n").is_err(),
            "p11"
        );
    }

    /// Formatting a repetition changes the layout and not the program.
    ///
    /// This is a regression test for shipped **data loss**: `emit` had no arm
    /// for `Item::Repeat`, so the node fell through to the plain-line case,
    /// which renders its head — and the body and closer were dropped on the
    /// floor. `fmt` is documented as safe to run over source you have not
    /// read, and it was deleting loop bodies.
    #[test]
    fn a_formatted_repetition_keeps_its_body() {
        for src in [
            " DUP 3\n nop\n EDUP\n ret\n",
            " REPT 2\n inc a\n ENDR\n ret\n",
            " dup 2\n nop\n edup\n ret\n",
            " DUP 2\n DUP 3\n nop\n EDUP\n inc a\n EDUP\n ret\n",
        ] {
            let before = asm(src).expect(src).bytes;
            let formatted = crate::format_sjasmplus(src).expect(src);
            let after = asm(&formatted)
                .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
                .bytes;
            assert_eq!(
                before, after,
                "formatting changed the program:\n{formatted}"
            );

            let again = crate::format_sjasmplus(&formatted).expect("formats");
            assert_eq!(formatted, again, "{formatted}");
        }
    }

    /// The closer keeps the spelling and the case the author wrote. sjasmplus
    /// takes `EDUP` and `ENDR` for either opener, so choosing one would be the
    /// formatter rewriting a line rather than laying it out.
    #[test]
    fn a_repetitions_closer_is_not_respelled() {
        let out = crate::format_sjasmplus(" DUP 2\n nop\n ENDR\n").expect("formats");
        assert!(out.contains("ENDR"), "{out}");
        assert!(!out.contains("EDUP"), "the closer was respelled:\n{out}");
        let lower = crate::format_sjasmplus(" dup 2\n nop\n edup\n").expect("formats");
        assert!(lower.contains("edup"), "the closer was re-cased:\n{lower}");
    }
}
