//! Shared Z80 syntax core for the pasmo-family and sjasmplus dialects.
//!
//! The Z80's mnemonic/operand syntax is the same across assemblers — `ld a,b`,
//! `ld (ix+5),$0a`, `bit 7,(hl)` are written identically — so the bulk of a
//! Z80 front-end (operand classification, the mode-label probe against the
//! [`isa`] spec, the expression parser, the register/condition vocabulary, the
//! common directives) is shared here. A dialect supplies only the two things
//! that genuinely differ via the [`Z80Syntax`] trait: **comment style** and
//! **number formats**. Everything else is reused, so adding a dialect is a
//! handful of lines (see `pasmo.rs`, `sjasmplus.rs`).
//!
//! ## Resolving operands to spec mode labels
//!
//! The Z80 packs registers and conditions into the opcode, so a form's mode is
//! its operand signature (see [`isa::z80`]). Each parsed operand is classified
//! as a *fixed* token (register/condition/indirect), a *value* (immediate or
//! `(nn)` address), or an *indexed* `(IX+d)`. Candidate signature strings are
//! built and probed against the instruction's forms — so `ld a,c` finds form
//! `A,C` while `jr c,loop` finds `C,e`, with no need to pre-judge whether `C`
//! is a register or a flag. Operand width is settled by which form exists.

use std::collections::BTreeMap;

use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
use crate::dialects::macros::{
    self, Expand, LineOrigin, expanded_text, expansion, line_origins, place_nodes, remap_lines,
};
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, BinOp, Expr, Operation, Statement, Warning};
use crate::source::{MAX_INCLUDE_DEPTH, SourceLoader, SourceMap};
use crate::span::FileId;

/// The per-dialect surface: the parts of Z80 syntax that actually differ
/// between assemblers. Everything else in this module is shared.
pub(crate) trait Z80Syntax {
    /// A directive that changes instruction parsing for following statements.
    ///
    /// Kept as a dialect hook because this is lexical state, not a Z80
    /// instruction: SjASMPlus's `OPT --zxnext` adopts it, while Pasmo has no
    /// corresponding word.
    fn lexical_option(
        &self,
        word: &str,
        args: &str,
        line: usize,
    ) -> Result<Option<LexicalOption>, AsmError> {
        let _ = (word, args, line);
        Ok(None)
    }

    /// Rewrite source before parsing, returning the new text and, per output
    /// line, where it came from.
    ///
    /// `None` — the default — means the dialect has nothing to expand, and the
    /// source is parsed as written. pasmo overrides it for macros, which
    /// expand file by file; sjasmplus did (#93) until its macros went live
    /// (#557, [`expand_live`](Self::expand_live)). Because every entry point
    /// funnels through `parse_program_keyword`, one hook covers the
    /// single-source, AST and include-capable paths alike.
    fn expand_source(&self, _source: &str) -> Result<Option<(String, Vec<LineOrigin>)>, AsmError> {
        Ok(None)
    }

    /// Register or expand macros **live**, as the evaluation walk reaches
    /// them, against a namespace that spans the whole assembly (#557).
    ///
    /// `None` — the default — means the dialect's macros are not live: they
    /// were dealt with before the parse, by [`expand_source`](Self::expand_source),
    /// or the dialect has none. A dialect that returns `Some` has its
    /// definitions and invocations left in place by the parse; the walk hands
    /// a completed definition here to enter the namespace, and an invocation
    /// here to expand against it. That is what makes a macro visible from its
    /// definition to the end of the assembly and nowhere earlier, whichever
    /// files define and invoke it, and leaves one in an untaken branch
    /// undefined — sjasmplus's measured rules (#557).
    fn expand_live(
        &self,
        source: &str,
        file: FileId,
        line: usize,
        state: &mut macros::MacroState,
    ) -> Result<Option<macros::Expanded>, AsmError> {
        let _ = (source, file, line, state);
        Ok(None)
    }

    /// Strip a line comment, returning the code before it.
    fn strip_comment<'a>(&self, line: &'a str) -> &'a str;

    /// Split a line into its code and its comment (with the delimiter, trailing
    /// whitespace trimmed), for carrying comments as AST trivia (U4). Defined in
    /// terms of [`strip_comment`](Self::strip_comment), which returns the code
    /// prefix, so the comment is exactly what it removed — no behaviour change.
    fn split_comment<'a>(&self, line: &'a str) -> (&'a str, Option<&'a str>) {
        let code = self.strip_comment(line);
        let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
        (code, comment)
    }

    /// Parse a numeric literal token (the dialect's hex/binary/char forms).
    fn parse_number(&self, tok: &str, line: usize) -> Result<i64, AsmError>;

    /// Whether a leading-`.` label is *local* — scoped under the most recent
    /// global (non-`.`) label, so the same `.loop` may recur in different
    /// scopes (sjasmplus). Defaults off: a leading-`.` name is then an ordinary
    /// global identifier (pasmo), and reusing it is a duplicate-label error.
    fn scopes_locals(&self) -> bool {
        false
    }

    /// Which end of a module block this word is, in this dialect's spelling.
    /// Defaulted to "none": modules are sjasmplus's, and a dialect without
    /// them must not silently accept the spelling.
    fn module_keyword(&self, word: &str) -> Option<ModuleKw> {
        let _ = word;
        None
    }

    /// Which end of a `STRUCT` block this word is (#477). Defaulted to
    /// "none" for the same reason as [`module_keyword`](Self::module_keyword):
    /// structures are sjasmplus's, and a dialect without them must not
    /// silently accept the spelling.
    fn struct_keyword(&self, word: &str) -> Option<StructKw> {
        let _ = word;
        None
    }

    /// Whether `word` binds a label to a value on the same line, so the
    /// formatter renders `name: equ …` inline rather than putting the label on
    /// a line of its own.
    ///
    /// A hook rather than a literal because sjasmplus spells it `.equ` as well
    /// (#93's dotted rule), and the formatter breaking a binding apart is not
    /// a layout preference — the result does not assemble.
    fn is_equ_word(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case("equ")
    }

    /// Whether this word is the directive that ends the whole assembly.
    fn is_end_word(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case("end")
    }

    /// Whether a condition may name a symbol defined later in the file,
    /// resolved by running the walk more than once (#99).
    ///
    /// Defaults off, which is the same parse-time-constant rule `ds` and
    /// `incbin` arguments follow. sjasmplus turns it on because it does three
    /// passes and its source relies on them; pasmo does not.
    fn resolves_forward_conditions(&self) -> bool {
        false
    }

    /// Whether `:` separates statements as well as terminating a label, so one
    /// source line may hold several (#98). Defaults off: pasmo has no such
    /// form, and splitting on a character it treats as ordinary would invent a
    /// dialect.
    fn splits_on_colon(&self) -> bool {
        false
    }

    /// Whether `MODULE` scoping is live — the module stack prefixes label
    /// definitions and references, and a leading `@` escapes it. Defaults off,
    /// which also leaves `@` an invalid character everywhere else.
    fn scopes_modules(&self) -> bool {
        false
    }

    /// Whether `word` names a directive. Defaults to the common set.
    fn is_directive(&self, word: &str) -> bool {
        is_common_directive(word)
    }

    /// Whether `word` reserves space (`ds`/`defs`, plus a dialect's own
    /// spellings). Under [`Self::resolves_forward_conditions`] the count
    /// resolves across passes rather than at parse time (#528).
    fn is_reserve(&self, word: &str) -> bool {
        is_common_reserve(word)
    }

    /// Whether `^` is the bitwise-XOR operator. sjasmplus has it; pasmo does
    /// not (and rejects `^`), so it defaults off to match pasmo.
    fn has_xor_operator(&self) -> bool {
        false
    }

    /// Whether a lone operand on `ADD`/`ADC`/`SBC` means `A,<operand>` (#533).
    /// sjasmplus reads `add (hl)` as `add a,(hl)`, with or without
    /// `--syntax=abfw`; pasmo rejects the spelling, so it defaults off.
    fn implicit_accumulator(&self) -> bool {
        false
    }

    /// Whether column 0 is the label column without exception (#551).
    /// sjasmplus binds a label there even when the word is a mnemonic or a
    /// directive — `.end` in column 0 is the local label `top.end`, not the
    /// dotted `END`. pasmo reads a column-0 mnemonic or directive as the
    /// operation, so it defaults off.
    fn column_zero_is_a_label(&self) -> bool {
        false
    }

    /// Whether decimal names in the label column define repeatable temporary
    /// labels, reached as `<digits>F`/`<digits>B` from jump targets.
    fn temporary_labels(&self) -> bool {
        false
    }

    /// Whether `word` is this dialect's include directive (language-surface
    /// U2).
    ///
    /// Off by default. sjasmplus overrides it for `INCLUDE`; pasmo does not
    /// An include is walk-handled — a verbatim item in the single-source parse,
    /// a lazy load in the multi-file walk — never an [`Operation`].
    fn is_include(&self, word: &str) -> bool {
        let _ = word;
        false
    }

    /// This dialect's own declared directives, on top of
    /// [`COMMON_DIRECTIVES`].
    ///
    /// Dispatch reads it for one thing the shared code cannot know: whether a
    /// word is declared [`Category::KnownUnsupported`], and so must be refused
    /// as a directive we do not implement rather than as a word that is not one.
    /// Deriving that from the declaration keeps a single source — the
    /// alternative is a second list of "words to refuse specially", which is
    /// the drift the declared surface exists to remove.
    fn own_directives(&self) -> &'static [crate::directives::Directive] {
        &[]
    }

    /// What this line does to a macro definition, given the names defined so
    /// far — the only thing a walk needs to know to copy one through untouched.
    ///
    /// Defaulted rather than required, and delegated to the dialect's own
    /// [`MacroSyntax`](crate::dialects::macros::MacroSyntax) where there is
    /// one. Making `Z80Syntax` a subtrait of that would oblige a future Z80
    /// dialect with no macros to implement a grammar for them.
    fn macro_line(&self, line: &str, known: &dyn Fn(&str) -> bool) -> macros::MacroLine {
        let _ = (line, known);
        macros::MacroLine::None
    }

    /// Which conditional keyword this word is, in **this dialect's** spelling.
    ///
    /// Defaulted to "none": a dialect has no conditionals until it adopts them,
    /// which is a per-dialect, demand-gated decision recorded in
    /// `decisions/conditional-assembly-framework.md`. The default is what keeps
    /// the keyword pipeline from handing every Z80 dialect sjasmplus's
    /// vocabulary the moment it routes through [`parse_program_keyword`] —
    /// pasmo has no `IFDEF`, no `IFNDEF` and no `ELSEIF`, and accepting them
    /// would be inventing a dialect.
    fn cond_keyword(&self, word: &str) -> Option<CondKw> {
        let _ = word;
        None
    }

    /// Which end of a repetition block this word is, in this dialect's
    /// spelling. Defaulted to "none", for the reason above.
    fn repeat_keyword(&self, word: &str) -> Option<RepeatKw> {
        let _ = word;
        None
    }

    /// Whether this word opens a `DEFINE` (textual substitution). Defaulted to
    /// "no": it is sjasmplus's, and a dialect without it must not accept the
    /// spelling merely for sharing a pipeline.
    fn is_define_word(&self, word: &str) -> bool {
        let _ = word;
        false
    }

    /// How this dialect's diagnostics name the ways a constant can be bound —
    /// the tail of "`n` must be a constant here (…)".
    ///
    /// Stated rather than derived, because the failure it prevents is a
    /// message that tells the reader to reach for a directive their assembler
    /// does not have. Two dialects share this condition parser and only one of
    /// them has `DEFINE`.
    fn constant_sources(&self) -> &'static str {
        "a value defined with `equ` above"
    }

    /// Whether `word` is this dialect's binary-inclusion directive
    /// (language-surface U3). Off by default; sjasmplus and pasmo override for
    /// `INCBIN`. Like an include, an incbin is walk-handled: a verbatim item
    /// in the single-source parse (so `--fmt` never opens the asset), a lazy
    /// binary load in the multi-file walk.
    fn is_incbin(&self, word: &str) -> bool {
        let _ = word;
        false
    }

    /// Whether this dialect's incbin takes the `,offset[,length]` tail.
    /// sjasmplus does (probe-pinned, incl. the negative from-the-end forms);
    /// pasmo does not — its reference rejects a comma after the file name
    /// (`End line expected but ','found`), so the tail stays a parse error.
    fn incbin_offset_length(&self) -> bool {
        false
    }

    /// Whether `<file>` is a quote form for the incbin file name. sjasmplus
    /// accepts it (as its INCLUDE does); pasmo takes the token verbatim — it
    /// looks for a file literally named `<file>` (probe-pinned).
    fn incbin_angle_quotes(&self) -> bool {
        false
    }

    /// Which comparison spellings this dialect takes in an **expression**, and
    /// what it answers for true. Defaults to none, which is what a dialect
    /// whose reference has no comparison operators wants.
    ///
    /// Conditions are unaffected: the `IF` lexer has always admitted the
    /// comparison operators and keeps doing so, whatever this says. See
    /// `docs/comparison-operators.md` for the probed table.
    fn compare(&self) -> crate::dialects::mos6502::Compare {
        crate::dialects::mos6502::Compare::default()
    }

    /// Parse a directive into an operation (`None` for ones that emit nothing,
    /// like `end`). Defaults to the common set. `consts` holds the `equ` values
    /// known so far, so a directive like `ds` can fold a constant-expression
    /// count (`ds MAX*2`) at parse time.
    fn parse_directive(
        &self,
        word: &str,
        args: &str,
        line: usize,
        consts: &BTreeMap<String, i64>,
    ) -> Result<Option<Operation>, AsmError>
    where
        Self: Sized,
    {
        common_directive(self, word, args, line, consts)
    }
}

/// The bounded SjASMPlus option state the shared keyword walk needs to carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LexicalOption {
    /// `--syntax=abfw`; for the source forms Asm198x accepts, the relevant
    /// strict parsing rules are already its ordinary rules.
    SyntaxAbfw,
    /// Enable Spectrum Next instructions, optionally with CSpect's two
    /// emulator-only fake instructions.
    ZxNext { cspect: bool },
}

/// Stamp `file` onto a per-line parse error: the line-oriented helpers below
/// (`split_label`, `parse_op`, the expression parser) know their line but not
/// their file, so the walk supplies it at the one per-line boundary.
fn stamp_file(mut e: AsmError, file: FileId) -> AsmError {
    match &mut e.span {
        Some(span) => span.file = file,
        None if e.line != 0 => e.span = Some(Span::in_file(file, e.line as u32, 0)),
        None => {}
    }
    e
}

/// As [`stamp_file`], but only when the error has no span yet — the
/// conditional walk's variant (U8): a nested include's errors arrive already
/// stamped with the *inner* file, which must not be overwritten by the
/// includer's frame.
fn stamp_missing_file(mut e: AsmError, file: FileId) -> AsmError {
    if e.span.is_none() && e.line != 0 {
        e.span = Some(Span::in_file(file, e.line as u32, 0));
    }
    e
}

/// The file name of an include directive: `"file"`, `<file>`, or a bare
/// token, matching the reference's accepted spellings (probe-pinned). Text
/// after a closing quote/bracket is ignored, as the reference does.
fn include_request(args: &str, line: usize) -> Result<String, AsmError> {
    let t = args.trim();
    let inner = if let Some(rest) = t.strip_prefix('"') {
        let end = rest
            .find('"')
            .ok_or_else(|| AsmError::new(line, "unterminated include file name"))?;
        &rest[..end]
    } else if let Some(rest) = t.strip_prefix('<') {
        let end = rest
            .find('>')
            .ok_or_else(|| AsmError::new(line, "unterminated include file name"))?;
        &rest[..end]
    } else {
        t.split_whitespace().next().unwrap_or("")
    };
    if inner.is_empty() {
        return Err(AsmError::new(line, "`include` needs a file name"));
    }
    Ok(inner.to_string())
}

/// Parse an incbin's arguments: the file name, then — where the dialect
/// supports it ([`Z80Syntax::incbin_offset_length`]) — an optional
/// `,offset[,length]` tail of parse-time constant expressions (they set the
/// statement's size, so like a `ds` count they must fold now; sjasmplus's
/// multi-pass acceptance of a *forward* constant is a known divergence).
/// Name spellings are probe-pinned: `"file"` and a bare token everywhere;
/// `<file>` only where [`Z80Syntax::incbin_angle_quotes`] says so (sjasmplus —
/// pasmo reads `<file>` as a literal file name). A bare name stops at
/// whitespace or a comma, so `incbin data.bin,2` still parses.
fn incbin_args<S: Z80Syntax>(
    syntax: &S,
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<(String, Option<i64>, Option<i64>), AsmError> {
    let t = args.trim();
    let (name, rest) = if let Some(inner) = t.strip_prefix('"') {
        let end = inner
            .find('"')
            .ok_or_else(|| AsmError::new(line, "unterminated incbin file name"))?;
        (&inner[..end], &inner[end + 1..])
    } else if syntax.incbin_angle_quotes()
        && let Some(inner) = t.strip_prefix('<')
    {
        let end = inner
            .find('>')
            .ok_or_else(|| AsmError::new(line, "unterminated incbin file name"))?;
        (&inner[..end], &inner[end + 1..])
    } else {
        let end = t
            .find(|c: char| c.is_whitespace() || c == ',')
            .unwrap_or(t.len());
        (&t[..end], &t[end..])
    };
    if name.is_empty() {
        return Err(AsmError::new(line, "`incbin` needs a file name"));
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((name.to_string(), None, None));
    }
    if !syntax.incbin_offset_length() {
        // pasmo's reference posture: nothing may follow the file name.
        return Err(AsmError::new(
            line,
            format!("`incbin` takes only a file name here (unexpected `{rest}`)"),
        ));
    }
    let Some(tail) = rest.strip_prefix(',') else {
        return Err(AsmError::new(
            line,
            format!("expected `,offset[,length]` after the incbin file name, found `{rest}`"),
        ));
    };
    let mut pieces = split_operands(tail);
    if pieces.len() > 2 {
        return Err(AsmError::new(
            line,
            "`incbin` takes at most a file name, an offset, and a length",
        ));
    }
    let fold = |what: &str, piece: &str| -> Result<i64, AsmError> {
        let expr = parse_value(syntax, piece, line)?;
        eval_const(&expr, consts).ok_or_else(|| {
            AsmError::new(
                line,
                format!(
                    "incbin {what} must be a constant here (a number, an expression of \
                     constants, or a value defined with `equ` above)"
                ),
            )
        })
    };
    let offset = fold("offset", pieces.remove(0))?;
    let length = pieces.pop().map(|p| fold("length", p)).transpose()?;
    Ok((name.to_string(), Some(offset), length))
}

/// Apply an incbin's offset/length to the loaded asset — sjasmplus semantics,
/// probe-pinned: a negative offset counts back from EOF; a negative length
/// means "all but the last |n| of the remaining bytes"; any window falling
/// outside the file is the reference's `file too short` error (offset *at*
/// EOF is legal and empty). `Err` carries the message body; the caller wraps
/// it with the request name and the directive's span.
fn slice_incbin(data: &[u8], offset: Option<i64>, length: Option<i64>) -> Result<Vec<u8>, String> {
    let len = data.len() as i64;
    let off = offset.unwrap_or(0);
    let off = if off < 0 { len + off } else { off };
    if !(0..=len).contains(&off) {
        return Err(format!(
            "file too short (offset {off} of a {len}-byte file)"
        ));
    }
    let remaining = len - off;
    let take = match length {
        None => remaining,
        Some(l) if l < 0 => remaining + l,
        Some(l) => l,
    };
    if !(0..=remaining).contains(&take) {
        return Err(format!(
            "file too short (length {take} with {remaining} byte(s) after offset {off})"
        ));
    }
    Ok(data[off as usize..(off + take) as usize].to_vec())
}

// ---------------------------------------------------------------------------
// Keyword conditionals + DEFINE — the sjasmplus adoption (language-surface U8)
//
// The decision record's four-step recipe
// (`decisions/conditional-assembly-framework.md`), applied to the z80 family's
// first keyword dialect: a **structure parse** recognises `IF`/`IFDEF`/
// `IFNDEF` … `ELSE` … `ENDIF` into the shared [`Item::Conditional`] tree
// (bodies kept verbatim, never parsed eagerly — probe p31: the reference
// accepts arbitrary garbage in an untaken branch), and [`SjasmEval`]
// implements [`CondEval`](crate::ast::CondEval) so the shared
// [`evaluate`](crate::ast::evaluate) walk prunes branches and threads the
// environment. Every line lowers at **evaluation** time with the live
// environment — an `equ` inside a taken branch feeds a later `bit`/`ds` form
// choice (probe p38), a `DEFINE` in a skipped branch defines nothing (probe
// p10), and an include inside an untaken branch never loads (probe p14,
// KTD1's proof). pasmo keeps the eager [`Walker`] pipeline untouched; a
// dialect opts in by calling these entry points (the [`Z80Syntax`] gate).
//
// Reference semantics, probe-pinned (sjasmplus 1.21.0, scratch u8-probes/):
// keywords spell all-lower or all-upper only (`If` is an ordinary identifier,
// probes p9/p11/p34); at column 0 a keyword is a *label*, so conditionals are
// written in the operation field (probe p33); `IFDEF`/`IFNDEF` test the
// case-sensitive DEFINE namespace only — not labels or `equ` constants
// (probes p3/p22) — on the first token, ignoring the rest (probe p48);
// `ELSE` ignores trailing text, `ENDIF` rejects it (probes p40/p35); nesting
// is tracked while skipping (probe p42); a conditional block never spans an
// include boundary (probes p12/p13 — both directions rejected); `DEFINE NAME
// value` is *textual substitution* at identifier boundaries, outside string/
// char literals, chained to a fixed point at use (probes p4/p5/p20/p21/p24),
// and a duplicate `DEFINE` is an error (probe p23). `ELSEIF` chains and the
// dotted spellings are adopted (#67, 2026-08-18); colon-inline blocks and
// conditions on forward symbols are not.
// ---------------------------------------------------------------------------

/// The keyword-conditional vocabulary. The reference accepts only the
/// all-lowercase and all-uppercase spellings (probes p9/p11); anything else
/// falls through to ordinary identifier handling, exactly as it does there.
///
/// Every keyword also has a **dotted** spelling — `.IF`/`.ENDIF`/`.ELSE` and
/// kin (#67, re-probed 2026-08-18). The dot is a bare prefix: it does not relax
/// the case rule (`.If` is rejected exactly as `If` is), and dotted and
/// undotted spellings mix freely within one block, so `.IF … ENDIF` assembles.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CondKw {
    If,
    IfDef,
    IfNDef,
    Else,
    ElseIf,
    EndIf,
}

pub(crate) fn cond_keyword(word: &str) -> Option<CondKw> {
    // The dot is an optional prefix on every spelling, and strips before the
    // case test — so `.If` stays as unacceptable as `If`.
    let word = word.strip_prefix('.').unwrap_or(word);
    Some(match word {
        "if" | "IF" => CondKw::If,
        "ifdef" | "IFDEF" => CondKw::IfDef,
        "ifndef" | "IFNDEF" => CondKw::IfNDef,
        "else" | "ELSE" => CondKw::Else,
        "elseif" | "ELSEIF" => CondKw::ElseIf,
        "endif" | "ENDIF" => CondKw::EndIf,
        _ => return None,
    })
}

/// Which end of a repetition block a word is, if either.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatKw {
    Open,
    Close,
}

/// `DUP`/`REPT` and `EDUP`/`ENDR`, under the same strict case rule the
/// conditionals follow: all-lower or all-upper, never mixed — the reference
/// rejects `Dup` as an unrecognised instruction.
///
/// The two spellings interchange: a block opened with `DUP` may be closed by
/// `ENDR`, which the reference accepts and so do we.
pub(crate) fn repeat_keyword(word: &str) -> Option<RepeatKw> {
    Some(match word {
        "dup" | "DUP" | "rept" | "REPT" => RepeatKw::Open,
        "edup" | "EDUP" | "endr" | "ENDR" => RepeatKw::Close,
        _ => return None,
    })
}

/// Which end of a `MODULE` block a word is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ModuleKw {
    Open,
    Close,
}

/// Which end of a `STRUCT` block a word is (#477).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StructKw {
    Open,
    Close,
}

/// The sjasmplus spelling, mirrored on [`module_keyword`]'s posture: the
/// walk asks the dialect, and a dialect without structures answers none.
pub(crate) fn struct_keyword(word: &str) -> Option<StructKw> {
    if word.eq_ignore_ascii_case("struct") {
        Some(StructKw::Open)
    } else if word.eq_ignore_ascii_case("ends") {
        Some(StructKw::Close)
    } else {
        None
    }
}

/// sjasmplus's module spelling: `MODULE` opens; `ENDMODULE` and `ENDMOD` both
/// close. The same strict case rule the conditionals and repetition follow —
/// all-lower or all-upper, never mixed: the reference answers `Module foo`
/// with `Unrecognized instruction` (probe m26).
pub(crate) fn module_keyword(word: &str) -> Option<ModuleKw> {
    Some(match word {
        "module" | "MODULE" => ModuleKw::Open,
        "endmodule" | "ENDMODULE" | "endmod" | "ENDMOD" => ModuleKw::Close,
        _ => return None,
    })
}

/// The `DEFINE` directive's spellings (the same strict case rule — probe p34).
pub(crate) fn is_define_word(word: &str) -> bool {
    matches!(word, "define" | "DEFINE")
}

/// How one leg of [`KwCx::parse_block`] ended.
#[derive(PartialEq, Eq)]
enum KwClose {
    Eof,
    Else,
    /// The block ended at an `ELSEIF`, carrying its verbatim head (`ELSEIF 1`)
    /// and line. The head keeps its keyword so the chain leg round-trips: the
    /// evaluator reads it like an `IF`, and the formatter renders it back as an
    /// `ELSEIF` rather than the nested `IF` it lowers to (#67).
    ElseIf(String, usize),
    EndIf,
}

/// The keyword-conditional structure parse cursor: line-oriented, no brace
/// matching — `IF`/`ELSE`/`ENDIF` are recognised in the operation field and
/// bodies collect as **verbatim** nodes (only an `equ` keeps its item, for the
/// formatter's inline `name: equ …` rendering). No evaluation happens here:
/// no environment, no DEFINE table — [`SjasmEval`] supplies those on the live
/// walk.
/// A `MODULE` that has been opened and not yet closed.
/// How a condition's forward references are answered, and what that cost.
///
/// The reference runs three passes and reads an as-yet-undefined symbol as
/// zero in the first, warning that it did. Later passes answer from the
/// previous pass's addresses. That is reproduced rather than improved on:
/// converging further than the reference would mean emitting different bytes
/// from it, which is the opposite of the point.
struct Forward {
    /// Label and `equ` values from the previous pass. Empty on pass 1, which
    /// is why a forward symbol reads as zero there.
    seed: BTreeMap<String, i64>,
    /// Set when a condition read a symbol `consts` could not answer — so the
    /// driver knows this program depends on a later pass, and a program that
    /// does not pay for one.
    used: std::cell::Cell<bool>,
    /// One advisory per condition that reached forward. Collected from pass 1,
    /// where the symbol was genuinely unknown.
    warnings: std::cell::RefCell<Vec<Warning>>,
    /// Whether this is the pass the reference stops at. A symbol still
    /// unknown there was never defined, and the reference says so as an
    /// error (`Label not found`) rather than reading zero a third time.
    last: bool,
}

impl Forward {
    fn new(seed: BTreeMap<String, i64>, last: bool) -> Self {
        Self {
            seed,
            used: std::cell::Cell::new(false),
            warnings: std::cell::RefCell::new(Vec::new()),
            last,
        }
    }

    /// Answer a symbol the constant table could not, and remember that we had
    /// to. Zero is the reference's answer for a symbol no pass has reached.
    /// A condition that reaches forward is advised of it, as the reference
    /// advises (`warning[fwdref]`).
    fn lookup(&self, name: &str, line: usize, file: FileId) -> Result<i64, AsmError> {
        let (value, reached) = self.resolve(name, line)?;
        if !reached {
            self.warnings.borrow_mut().push(Warning {
                line,
                message: format!("forward reference of symbol `{name}`"),
                file,
                kind: crate::engine::WarningKind::Advisory,
            });
        }
        Ok(value)
    }

    /// The same answer without the advisory, and whether a pass had reached
    /// the symbol. A `DS` count that reaches forward is answered silently: the
    /// reference resolves it across its passes and says nothing (#528). On
    /// the last pass an unreached symbol is the reference's `Label not
    /// found` error, for a condition and a count alike.
    fn resolve(&self, name: &str, line: usize) -> Result<(i64, bool), AsmError> {
        self.used.set(true);
        match self.seed.get(name) {
            Some(v) => Ok((*v, true)),
            None if self.last => Err(AsmError::new(line, format!("label not found: `{name}`"))),
            None => Ok((0, false)),
        }
    }
}

/// One statement's worth of source, with where it came from.
///
/// The parse used to index its lines and derive the number from the position,
/// which held while a line was a statement. A `:` line is several (#98), so the
/// number travels with the text instead — which also removes the re-basing a
/// nested block needed when it parsed a slice of its parent's lines.
#[derive(Clone, Copy)]
struct Src<'a> {
    /// 1-based line in the (post-expansion) text.
    line: u32,
    /// 1-based column the statement starts at. Always 1 for a whole line.
    col: u32,
    text: &'a str,
}

/// Cut `text` into statements: one per line, or several where a dialect lets
/// `:` separate them.
///
/// A `:` is a statement separator *except* when it terminates a label, and
/// telling those apart is positional: the colon closing a label is the first
/// one in its statement and has nothing but an identifier before it. So
/// `lbl: ld a,1 : ld b,2` is two statements, the first of which keeps its
/// label. `::` — sjasmplus's export form — closes a label as one token.
///
/// A colon inside `"…"` or `'…'` separates nothing, and neither does one in a
/// comment: the comment is found first and travels with the statement it
/// trails, so `ld a,1 ; a:b` stays whole.
fn split_statements<'a, S: Z80Syntax>(syntax: &S, text: &'a str) -> Vec<Src<'a>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i as u32 + 1;
        if !syntax.splits_on_colon() {
            out.push(Src {
                line,
                col: 1,
                text: raw,
            });
            continue;
        }
        let (code, _) = syntax.split_comment(raw);
        let bytes = code.as_bytes();
        let (mut start, mut labelled) = (0usize, false);
        let (mut in_str, mut in_char) = (false, false);
        let mut cut = Vec::new();
        for i in 0..bytes.len() {
            match bytes[i] {
                b'"' if !in_char => in_str = !in_str,
                b'\'' if !in_str => in_char = !in_char,
                b':' if !in_str && !in_char => {
                    let head = code[start..i].trim();
                    if !labelled && !head.is_empty() && is_ident(head) {
                        labelled = true;
                        continue;
                    }
                    // A second colon straight after the label's is `::`, not
                    // an empty statement.
                    if labelled && i > 0 && bytes[i - 1] == b':' && code[start..i - 1].trim() == ""
                    {
                        continue;
                    }
                    cut.push(i);
                    start = i + 1;
                    labelled = false;
                }
                _ => {}
            }
        }
        if cut.is_empty() {
            out.push(Src {
                line,
                col: 1,
                text: raw,
            });
            continue;
        }
        // The comment trails the whole line, so it rides with the last
        // statement — where a reader wrote it.
        let mut from = 0usize;
        for &at in &cut {
            out.push(Src {
                line,
                col: from as u32 + 1,
                text: &code[from..at],
            });
            from = at + 1;
        }
        out.push(Src {
            line,
            col: from as u32 + 1,
            text: &raw[from..],
        });
    }
    out
}

struct KwCx<'a, S: Z80Syntax> {
    syntax: &'a S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    file: FileId,
    lines: Vec<Src<'a>>,
    /// The next line to read (0-based).
    pos: usize,
    /// Own-line comments since the last node, attached as leading trivia.
    pending: Vec<Comment>,
    /// Inside a macro definition, whose lines are copied and never read.
    in_macro: bool,
    /// Inside a `STRUCT` (#477): members are copied verbatim for the same
    /// reason macro bodies are — the member name sits in the label column,
    /// where the formatter would peel it onto a line of its own and the
    /// member would stop being one. The assembling reader splits the name
    /// back off the copied line.
    in_struct: bool,
    /// The macros defined so far, so an invocation is copied too.
    macro_names: std::collections::BTreeSet<String>,
    /// The structures defined so far, so an instantiation (`p1 Pt`) is
    /// copied too — its label is the instance name, which the re-layout
    /// would peel off.
    struct_names: std::collections::BTreeSet<String>,
    /// Braces still open from an instantiation's initialiser list: while
    /// positive, the lines that follow are the rest of that list and are
    /// copied, not read (#552).
    struct_init_depth: i32,
}

/// Parse one file of a keyword-conditional program into the source-preserving
/// tree: conditional blocks as [`Item::Conditional`](crate::ast::Item) (the
/// `Keyword` style), every other line a verbatim node. Used for `--fmt`
/// (`parse_ast`) and as the front half of [`assemble_keyword`] /
/// [`parse_program_multi_keyword`].
pub(crate) fn parse_program_keyword<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    file: FileId,
    source: &str,
    mode: Expand,
) -> Result<Program, AsmError> {
    // A dialect may rewrite source before it is parsed (sjasmplus macros,
    // #93). Line numbers are mapped back afterwards, so a diagnostic always
    // names a line the author wrote.
    let expanded = expansion(mode, source, |s| syntax.expand_source(s))?;
    let text = expanded_text(&expanded, source);
    let origins = line_origins(&expanded);
    parse_text_keyword(syntax, set, ext, file, text, origins)
}

/// The parse proper, over text that may already be a rewrite: every node and
/// error is placed back on the line `origins` says it came from, when there
/// is a map. Split from [`parse_program_keyword`] so live macro expansion
/// (#557) can parse an invocation's body through the same grammar.
fn parse_text_keyword<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    file: FileId,
    text: &str,
    origins: Option<&[LineOrigin]>,
) -> Result<Program, AsmError> {
    let mut cx = KwCx {
        syntax,
        set,
        ext,
        file,
        lines: split_statements(syntax, text),
        pos: 0,
        pending: Vec::new(),
        in_macro: false,
        in_struct: false,
        macro_names: std::collections::BTreeSet::new(),
        struct_names: std::collections::BTreeSet::new(),
        struct_init_depth: 0,
    };
    let (mut nodes, close) = cx
        .parse_block(false)
        .map_err(|e| remap_lines(stamp_file(e, file), origins))?;
    debug_assert!(close == KwClose::Eof, "top level only ends at EOF");
    // Flush a trailing comment block so the formatter keeps it.
    if !cx.pending.is_empty() {
        let last = cx.lines.len() as u32;
        nodes.push(Node {
            operand_span: None,
            label: None,
            item: None,
            source: String::new(),
            span: Span::in_file(file, last, 1),
            trivia: Trivia {
                leading: std::mem::take(&mut cx.pending),
                trailing: None,
            },
        });
    }
    place_nodes(&mut nodes, origins);
    Ok(Program { nodes })
}

impl<S: Z80Syntax> KwCx<'_, S> {
    /// Parse lines until `ELSE`/`ENDIF` (inside a block) or EOF, collecting
    /// nodes. `in_block` is false only at the top level, where a stray
    /// `ELSE`/`ENDIF` is an error (probe p43b's posture).
    /// Collect a `DUP`/`REPT` block: its body is parsed as a block of its own
    /// so nesting works, and its nodes keep the line numbers they had in the
    /// file rather than restarting inside the body.
    ///
    /// The body is found by scanning for the matching close at depth zero, so
    /// an inner block's `EDUP` does not end the outer one.
    fn parse_repeat(
        &mut self,
        nodes: &mut Vec<Node>,
        rest: &str,
        label: Option<String>,
        line: usize,
    ) -> Result<(), AsmError> {
        let start = self.pos;
        let mut depth = 1usize;
        let mut end = None;
        let mut k = start;
        while k < self.lines.len() {
            let (code, _) = self.syntax.split_comment(self.lines[k].text);
            let (word, _) = split_first_word(code.trim());
            match self.syntax.repeat_keyword(word) {
                Some(RepeatKw::Open) => depth += 1,
                Some(RepeatKw::Close) => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some((k, code.trim().to_string()));
                        break;
                    }
                }
                None => {}
            }
            k += 1;
        }
        let Some((end, close)) = end else {
            let (opener, _) = split_first_word(rest);
            return Err(AsmError::new(
                line,
                format!("`{opener}` block is never closed"),
            ));
        };

        let mut sub = KwCx {
            syntax: self.syntax,
            set: self.set,
            ext: self.ext,
            file: self.file,
            lines: self.lines[start..end].to_vec(),
            pos: 0,
            pending: Vec::new(),
            in_macro: false,
            in_struct: false,
            macro_names: self.macro_names.clone(),
            struct_names: self.struct_names.clone(),
            struct_init_depth: 0,
        };
        // No re-basing: a statement carries its own line number now, so the
        // slice a nested block parses is already numbered absolutely.
        let (body, _) = sub.parse_block(false)?;
        // A label before the block becomes its own node, exactly as it does
        // before a conditional, so the block itself carries none.
        let mut leading = std::mem::take(&mut self.pending);
        if let Some(name) = label {
            nodes.push(Node {
                operand_span: None,
                label: Some(Symbol {
                    qualified: name.clone(),
                    scope: Scope::Global,
                    name,
                }),
                item: None,
                source: String::new(),
                span: Span::in_file(self.file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut leading),
                    trailing: None,
                },
            });
        }
        nodes.push(Node {
            operand_span: None,
            label: None,
            item: Some(crate::ast::Item::Repeat {
                head: rest.to_string(),
                body,
                close: close.to_string(),
                style: crate::ast::CondStyle::Keyword,
            }),
            source: rest.to_string(),
            span: Span::in_file(self.file, line as u32, 1),
            trivia: Trivia {
                leading,
                trailing: None,
            },
        });
        self.pos = end + 1;
        Ok(())
    }

    fn parse_block(&mut self, in_block: bool) -> Result<(Vec<Node>, KwClose), AsmError> {
        let mut nodes = Vec::new();
        while self.pos < self.lines.len() {
            let src = self.lines[self.pos];
            let raw = src.text;
            let line = src.line as usize;
            let col = src.col;
            self.pos += 1;
            let (code, comment) = self.syntax.split_comment(raw);
            if code.trim().is_empty() {
                if let Some(text) = comment {
                    self.pending.push(Comment {
                        text: text.to_string(),
                        span: Span::in_file(self.file, line as u32, col),
                    });
                }
                continue;
            }
            // The rest of an initialiser list that opened a brace on an
            // earlier line is copied line by line until the brace closes;
            // the text is values, not statements (#552).
            if self.struct_init_depth > 0 {
                self.struct_init_depth += brace_depth(code);
                nodes.push(Node {
                    operand_span: None,
                    label: None,
                    item: Some(crate::ast::Item::Verbatim),
                    source: code.trim_end().to_string(),
                    span: Span::in_file(self.file, line as u32, col),
                    trivia: self.trivia(comment, code, line),
                });
                continue;
            }
            // A macro definition is copied, not read — and copied verbatim,
            // because a dialect may spell one `name MACRO` with the name in the
            // *label* column. See `Item::Verbatim`; this is the same rule the
            // eager walk follows, and it must run before the keyword checks
            // below: pasmo closes a repetition with `ENDM`, the same word that
            // closes a macro, and the body has to be consumed first for that
            // not to collide.
            //
            // The formatter's parse reaches here with a definition intact, and
            // so does the assembling parse of a dialect whose macros are live
            // (#557): its walk registers the copied lines when it gets to them.
            // A dialect that expands before the parse has none left by here.
            {
                let what = self
                    .syntax
                    .macro_line(code, &|word: &str| self.macro_names.contains(word));
                let copy = match what {
                    macros::MacroLine::Opens(name) => {
                        self.macro_names.insert(name);
                        self.in_macro = true;
                        true
                    }
                    macros::MacroLine::Closes if self.in_macro => {
                        self.in_macro = false;
                        true
                    }
                    macros::MacroLine::Invokes => true,
                    _ => self.in_macro,
                };
                // A `STRUCT` body is copied for the same reason (#477); the
                // keyword test runs on the first word so an indented opener
                // is seen and a column-0 spelling stays a label, exactly as
                // the reference reads it.
                let copy = copy
                    || if self.in_macro {
                        false
                    } else {
                        let indented = code.starts_with(|c: char| c.is_whitespace());
                        let (w, _) = split_first_word(code);
                        match self.syntax.struct_keyword(w) {
                            Some(StructKw::Open) if indented => {
                                self.in_struct = true;
                                let (_, rest) = split_first_word(code);
                                let name = rest.split(',').next().unwrap_or("").trim();
                                if !name.is_empty() {
                                    self.struct_names.insert(name.to_string());
                                }
                                true
                            }
                            Some(StructKw::Close) if indented && self.in_struct => {
                                self.in_struct = false;
                                true
                            }
                            _ if self.in_struct => true,
                            _ => {
                                // An instantiation: `label Name`, the name in
                                // the operation column, or the unlabelled
                                // ` Name`, followed by nothing or by an
                                // initialiser list, braced or not (#548). A
                                // brace left open carries the list on to the
                                // lines that follow (#552).
                                let instance = if indented {
                                    self.struct_names.contains(w)
                                } else {
                                    let (_, rest) = split_first_word(code);
                                    let (n, _) = split_first_word(rest);
                                    self.struct_names.contains(n)
                                };
                                if instance {
                                    self.struct_init_depth = brace_depth(code).max(0);
                                }
                                instance
                            }
                        }
                    };
                if copy {
                    nodes.push(Node {
                        operand_span: None,
                        label: None,
                        item: Some(crate::ast::Item::Verbatim),
                        source: code.trim_end().to_string(),
                        span: Span::in_file(self.file, line as u32, col),
                        trivia: self.trivia(comment, code, line),
                    });
                    continue;
                }
            }

            // A label-split failure is deferred, not raised: an untaken
            // branch may hold anything (probe p31), so the whole line becomes
            // verbatim op source — a *live* line still errors when it lowers.
            let (label, rest) = match split_label(self.syntax, self.set, self.ext, code, col, line)
            {
                Ok(v) => v,
                Err(_) => (None, code.trim()),
            };
            let (word, args) = split_first_word(rest);
            if let Some(kw) = self.syntax.repeat_keyword(word) {
                match kw {
                    RepeatKw::Open => {
                        self.parse_repeat(&mut nodes, rest, label, line)?;
                        continue;
                    }
                    RepeatKw::Close => {
                        return Err(AsmError::new(
                            line,
                            format!("`{word}` closes a repetition block that was never opened"),
                        ));
                    }
                }
            }
            match self.syntax.cond_keyword(word) {
                Some(CondKw::If | CondKw::IfDef | CondKw::IfNDef) => {
                    self.parse_conditional(&mut nodes, raw, rest, word, label, comment, line)?;
                }
                Some(CondKw::Else) => {
                    if !in_block {
                        return Err(AsmError::new(line, "`ELSE` without a matching `IF`"));
                    }
                    if label.is_some() {
                        return Err(AsmError::new(line, "a label cannot precede `ELSE`"));
                    }
                    // The reference ignores text after `ELSE` (probe p40).
                    if let Some(text) = comment {
                        self.pending.push(Comment {
                            text: text.to_string(),
                            span: Span::in_file(self.file, line as u32, col),
                        });
                    }
                    return Ok((nodes, KwClose::Else));
                }
                Some(CondKw::EndIf) => {
                    if !in_block {
                        return Err(AsmError::new(line, "`ENDIF` without a matching `IF`"));
                    }
                    if label.is_some() {
                        return Err(AsmError::new(line, "a label cannot precede `ENDIF`"));
                    }
                    if !args.trim().is_empty() {
                        // The reference rejects text after `ENDIF` (probe p35).
                        return Err(AsmError::new(
                            line,
                            format!("unexpected text after `ENDIF`: `{}`", args.trim()),
                        ));
                    }
                    if let Some(text) = comment {
                        self.pending.push(Comment {
                            text: text.to_string(),
                            span: Span::in_file(self.file, line as u32, col),
                        });
                    }
                    return Ok((nodes, KwClose::EndIf));
                }
                Some(CondKw::ElseIf) => {
                    if !in_block {
                        return Err(AsmError::new(line, "`ELSEIF` without a matching `IF`"));
                    }
                    if label.is_some() {
                        return Err(AsmError::new(line, "a label cannot precede `ELSEIF`"));
                    }
                    if let Some(text) = comment {
                        self.pending.push(Comment {
                            text: text.to_string(),
                            span: Span::in_file(self.file, line as u32, col),
                        });
                    }
                    return Ok((nodes, KwClose::ElseIf(rest.to_string(), line)));
                }
                None => {
                    // A plain line: verbatim op source. Only `equ` keeps an
                    // item, so the formatter renders `name: equ …` inline as
                    // the eager parse did; lowering re-parses from source.
                    let item = if label.is_some() && self.syntax.is_equ_word(word) {
                        parse_value(self.syntax, args, line)
                            .ok()
                            .map(|e| crate::ast::item_from_operation(Operation::Equ(e)))
                    } else {
                        None
                    };
                    let symbol = label.map(|name| Symbol {
                        qualified: name.clone(),
                        scope: Scope::Global,
                        name,
                    });
                    let operand_span =
                        crate::ast::operand_span(raw, rest, line as u32).map(|mut s| {
                            s.file = self.file;
                            s
                        });
                    nodes.push(Node {
                        operand_span,
                        label: symbol,
                        item,
                        source: rest.to_string(),
                        span: Span::in_file(self.file, line as u32, col),
                        trivia: self.trivia(comment, code, line),
                    });
                }
            }
        }
        Ok((nodes, KwClose::Eof))
    }

    /// Parse one `IF`/`IFDEF`/`IFNDEF` block: recurse for the then-branch, an
    /// optional `ELSE` branch, and require the `ENDIF`. A label on the head
    /// line binds at the block's address (probe p27), as its own node — the
    /// shared walk never reads a conditional node's label.
    #[allow(clippy::too_many_arguments)]
    fn parse_conditional(
        &mut self,
        nodes: &mut Vec<Node>,
        _raw: &str,
        rest: &str,
        word: &str,
        label: Option<String>,
        comment: Option<&str>,
        line: usize,
    ) -> Result<(), AsmError> {
        let mut leading = std::mem::take(&mut self.pending);
        if let Some(name) = label {
            nodes.push(Node {
                operand_span: None,
                label: Some(Symbol {
                    qualified: name.clone(),
                    scope: Scope::Global,
                    name,
                }),
                item: None,
                source: String::new(),
                span: Span::in_file(self.file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut leading),
                    trailing: None,
                },
            });
        }
        self.parse_cond_chain(nodes, rest.to_string(), word, line, leading, comment)
    }

    /// Build one leg of a conditional chain and, recursively, the legs after it.
    /// An `ELSEIF` lowers to a nested conditional in the else-branch — the shape
    /// the shared evaluator already walks, and the workaround this dialect used
    /// to tell people to write by hand. The leg keeps its verbatim `ELSEIF …`
    /// head so the formatter can render the chain back rather than the nesting.
    fn parse_cond_chain(
        &mut self,
        nodes: &mut Vec<Node>,
        head: String,
        word: &str,
        line: usize,
        leading: Vec<Comment>,
        comment: Option<&str>,
    ) -> Result<(), AsmError> {
        let (then_body, first) = self.parse_block(true)?;
        let else_body = match first {
            KwClose::EndIf => None,
            KwClose::Else => {
                let (body, second) = self.parse_block(true)?;
                match second {
                    KwClose::EndIf => Some(body),
                    // The reference tolerates an `ELSEIF` after `ELSE` by
                    // discarding it and everything to the `ENDIF`. Silently
                    // dropping source is worse than saying so, and no real
                    // program means it.
                    KwClose::ElseIf(_, at) => {
                        return Err(AsmError::new(
                            at,
                            "`ELSEIF` cannot follow `ELSE` — the chain is already closed",
                        ));
                    }
                    _ => {
                        return Err(AsmError::new(
                            line,
                            format!("`{word}` has no matching `ENDIF`"),
                        ));
                    }
                }
            }
            KwClose::ElseIf(next_head, at) => {
                let mut nested = Vec::new();
                self.parse_cond_chain(&mut nested, next_head, word, at, Vec::new(), None)?;
                Some(nested)
            }
            KwClose::Eof => {
                return Err(AsmError::new(
                    line,
                    format!("`{word}` has no matching `ENDIF`"),
                ));
            }
        };
        nodes.push(Node {
            operand_span: None,
            label: None,
            item: Some(crate::ast::Item::Conditional {
                close: String::new(),
                head,
                then_body,
                else_body,
                inline: false,
                style: crate::ast::CondStyle::Keyword,
            }),
            source: String::new(),
            span: Span::in_file(self.file, line as u32, 1),
            trivia: Trivia {
                leading,
                trailing: comment.map(|text| Comment {
                    text: text.to_string(),
                    span: Span::in_file(self.file, line as u32, 1),
                }),
            },
        });
        Ok(())
    }

    /// Trivia for a plain node: the pending own-line comments plus this
    /// line's trailing comment.
    fn trivia(&mut self, comment: Option<&str>, code: &str, line: usize) -> Trivia {
        Trivia {
            leading: std::mem::take(&mut self.pending),
            trailing: comment.map(|text| Comment {
                text: text.to_string(),
                span: Span::in_file(self.file, line as u32, (code.len() + 1) as u32),
            }),
        }
    }
}

/// The multi-file context of a keyword-conditional walk: the source map that
/// owns `FileId` allocation and the include graph, the loader seam, and the
/// active include stack for cycle detection (the acme `MultiCx` precedent).
struct MultiCx<'a> {
    map: &'a mut SourceMap,
    loader: &'a dyn SourceLoader,
    /// The files currently open, root first. Cycle detection is membership —
    /// a file may be included twice *sequentially* but never while open.
    stack: Vec<FileId>,
}

/// The z80 family's keyword [`CondEval`](crate::ast::CondEval) — **sjasmplus
/// is its first consumer** (U8, `decisions/conditional-assembly-framework.md`).
/// It owns the walk environment: the `equ` constants a later condition or
/// form choice folds against, the `DEFINE` substitution table, and the
/// current global label for `.local` scoping. `eval` tests a conditional head
/// against that environment; `lower` re-parses one **live** line from its
/// verbatim source with the environment as of that point — so a skipped
/// branch defines nothing and an include-defined constant feeds later form
/// selection, exactly as the eager walker did for unconditional programs.
///
/// With a [`MultiCx`] wired in, `INCLUDE`/`INCBIN` resolve *inside* this walk
/// (an untaken branch's include never loads — KTD1); without one, they error
/// with a pointer at the multi-file entry points.
struct SjasmEval<'a, S: Z80Syntax> {
    syntax: &'a S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    /// Whether CSpect's `BREAK`/`EXIT` fake instructions are live.
    cspect: bool,
    /// SjASMPlus's recommended `--syntax=abfw` parser posture.
    syntax_abfw: bool,
    /// `equ` constants as lowered, keyed by qualified name (the walker's rule).
    consts: BTreeMap<String, i64>,
    /// `DEFINE` bindings: name → verbatim replacement text (may be empty for
    /// the bare flag form). Case-sensitive (probe p22).
    defines: BTreeMap<String, String>,
    /// The scoped symbol environment (#484): the current-global anchor, the
    /// open `MODULE`s, and the qualified→bare fallbacks. This dialect
    /// declares the events — a plain label re-anchors, `MODULE`/`ENDMODULE`
    /// open and close — and the shared environment owns what they mean for
    /// lookup (`decisions/scoped-symbol-environment.md`).
    scopes: crate::scopes::ScopeEnv,
    /// The `STRUCT` being read, if one is open (#477): every line until
    /// `ENDS` declares a member rather than a statement.
    building: Option<StructBuild>,
    /// An instantiation whose initialiser list left a brace open, waiting
    /// for the lines that close it (#552).
    pending_init: Option<PendingInit>,
    /// The live macro namespace (#557), for a dialect whose
    /// [`Z80Syntax::expand_live`] is live: one per pass, filled in the
    /// walk's textual order across every file, so a definition is visible
    /// from where the walk registered it to the end of the assembly.
    macro_state: macros::MacroState,
    /// A definition being gathered line by line until its closer, for the
    /// namespace above.
    macro_capture: Option<MacroCapture>,
    /// Present on a dialect that resolves conditions across passes (#99), and
    /// the reason a `SjasmEval` is built once per pass rather than once.
    forward: Option<Forward>,
    /// Where the location counter stands, so a label binds to its address as
    /// it is defined and a *backward* reference in a condition is answered by
    /// `consts` — never by the forward path, which would warn about a symbol
    /// the walk had already seen. `None` when the counter cannot be followed;
    /// the forward path then covers it, one pass later.
    pc: Option<i64>,
    multi: Option<MultiCx<'a>>,
    /// The file the walk is currently inside — stamps condition-evaluation
    /// errors, which the shared walk raises without node context.
    current_file: FileId,
    /// Set by a live `END`. The shared evaluator consults it at every tree
    /// boundary, so an `END` reached through an include or repeated block also
    /// stops the includer and every enclosing block.
    terminated: bool,
    /// Number of definitions reached for each sjasmplus temporary-label name.
    /// Includes and live macro expansions share this textual counter.
    temporary_labels: BTreeMap<String, usize>,
    /// Forward references, retained for the reference's specific missing-label
    /// diagnostic once the complete textual walk is known.
    temporary_forwards: Vec<(String, String, usize, FileId)>,
}

impl<'a, S: Z80Syntax> SjasmEval<'a, S> {
    fn new(
        syntax: &'a S,
        set: &'static isa::InstructionSet,
        ext: Option<&'static isa::InstructionSet>,
        multi: Option<MultiCx<'a>>,
    ) -> Self {
        Self {
            syntax,
            set,
            ext,
            cspect: false,
            syntax_abfw: false,
            consts: BTreeMap::new(),
            defines: BTreeMap::new(),
            scopes: crate::scopes::ScopeEnv::new(),
            building: None,
            pending_init: None,
            macro_state: macros::MacroState::default(),
            macro_capture: None,
            forward: None,
            pc: Some(0),
            multi,
            current_file: FileId(0),
            terminated: false,
            temporary_labels: BTreeMap::new(),
            temporary_forwards: Vec::new(),
        }
    }

    fn temporary_name(number: &str, occurrence: usize) -> String {
        format!("__asm198x_sjasmplus_temporary_{number}_{occurrence}")
    }

    fn define_temporary(&mut self, number: &str) -> String {
        let occurrence = self.temporary_labels.entry(number.to_string()).or_default();
        *occurrence += 1;
        Self::temporary_name(number, *occurrence)
    }

    /// Rewrite the last operand of a jump/call only. In every other expression
    /// `1B` remains a binary literal and `1F` remains an invalid number.
    fn rewrite_temporary_target(
        &mut self,
        mnemonic: &str,
        args: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<String>, AsmError> {
        if !self.syntax.temporary_labels()
            || !matches!(
                mnemonic.to_ascii_uppercase().as_str(),
                "JR" | "JP" | "DJNZ" | "CALL"
            )
        {
            return Ok(None);
        }
        let mut operands = split_operands(args);
        let Some(target) = operands.last().copied() else {
            return Ok(None);
        };
        let Some((number, direction)) = temporary_reference(target) else {
            return Ok(None);
        };
        let seen = self.temporary_labels.get(number).copied().unwrap_or(0);
        let occurrence = if direction.eq_ignore_ascii_case("B") {
            if seen == 0 {
                return Err(AsmError::new(
                    line,
                    format!("Temporary label not found: {number}{direction}"),
                ));
            }
            seen
        } else {
            let occurrence = seen + 1;
            self.temporary_forwards.push((
                Self::temporary_name(number, occurrence),
                format!("{number}{direction}"),
                line,
                file,
            ));
            occurrence
        };
        let replacement = format!("@{}", Self::temporary_name(number, occurrence));
        *operands.last_mut().expect("target exists") = &replacement;
        // `operands` borrows its entries, so build while `replacement` lives.
        Ok(Some(operands.join(",")))
    }

    /// Resolve a label's defined name with the live environment: a `DEFINE`'d
    /// name renames the label (probe p26 — single-identifier replacements
    /// only, the smallest slice byte-identity needs), then the walker's scope
    /// rule applies — a leading-`.` local qualifies under the current global,
    /// a plain name opens a new scope.
    fn resolve_label(&mut self, name: &str, line: usize) -> Result<String, AsmError> {
        if self.syntax.temporary_labels()
            && let Some(number) = numeric_label(name)
        {
            return Ok(self.define_temporary(number));
        }
        let name = if self.defines.contains_key(name) {
            let expanded = substitute_defines(name, &self.defines, line)?;
            let expanded = expanded.trim().to_string();
            if !is_ident(&expanded) {
                return Err(AsmError::new(
                    line,
                    format!("DEFINE `{name}` does not expand to a label name (got `{expanded}`)"),
                ));
            }
            expanded
        } else {
            name.to_string()
        };
        Ok(self.scopes.define(
            name,
            self.syntax.scopes_locals(),
            self.syntax.scopes_modules(),
        ))
    }

    /// Move the counter over `op`, or give up on knowing where it is. The
    /// width rule is [`crate::engine::next_pc`], shared with the engine's own
    /// address pass — see `decisions/acme-zero-page.md` for why it is not
    /// copied. Giving up costs a warning, not a wrong answer: the symbol falls
    /// to the forward path and the next pass resolves it.
    fn advance(&mut self, op: Option<&Operation>, line: usize) {
        let Some(op) = op else { return };
        if let Operation::Org(e) = op {
            self.pc = eval_const(e, &self.consts);
            return;
        }
        let Some(pc) = self.pc else { return };
        self.pc = crate::engine::next_pc(op, pc, self.set, self.ext, 1, line).ok();
    }

    /// The forward-resolution state paired with the file a condition is being
    /// read in, so an advisory names the right one.
    fn fwd(&self) -> Option<(&Forward, FileId)> {
        self.forward.as_ref().map(|f| (f, self.current_file))
    }

    /// Fold a count the statement's size depends on — a `DS` length, a
    /// `STRUCT` member's `DS` length — against the constants known so far,
    /// then against the previous pass for anything defined later (#528).
    /// Without a forward pass the parse-time-constant rule stands. `$` is the
    /// counter where the walk can follow it.
    fn fold_count(&self, expr: &Expr, line: usize) -> Result<i64, AsmError> {
        let Some(fwd) = &self.forward else {
            return literal(expr, &self.consts, line);
        };
        // `eval_with` cannot carry the last-pass error out of its resolver,
        // so an unreached symbol is caught after the fold, on that pass only.
        let unreached = std::cell::Cell::new(None);
        let folded = expr.eval_with(
            &|s| {
                self.consts
                    .get(s)
                    .copied()
                    .or_else(|| match fwd.resolve(s, line) {
                        Ok((v, _)) => Some(v),
                        Err(e) => {
                            unreached.set(Some(e));
                            None
                        }
                    })
            },
            self.pc,
            line,
        );
        match unreached.into_inner() {
            Some(e) => Err(e),
            None => folded,
        }
    }

    /// `DS`/`DEFS`/`BLOCK count[, fill]` where the count may name a symbol
    /// defined later. The reference resolves it across its three passes
    /// (probed: `DS COUNT * 2` above `COUNT EQU 3` reserves six; `DS later+1`
    /// above `later:` settles on three with the pass-3 advisory). A negative
    /// count makes it warn `Negative BLOCK?` and move the counter backwards;
    /// because sjasmplus raw output is append-only, that is represented by an
    /// `ORG` to the wrapped logical address and emits no bytes.
    fn lower_reserve(&self, args: &str, line: usize) -> Result<Operation, AsmError> {
        let (count, fill) = match args.split_once(',') {
            Some((count, fill)) => (count, Some(fill)),
            None => (args, None),
        };
        let count = self.fold_count(&parse_value(self.syntax, count.trim(), line)?, line)?;
        if count < 0 {
            if let Some(fwd) = &self.forward {
                fwd.warnings.borrow_mut().push(Warning {
                    line,
                    message: "Negative BLOCK?".to_string(),
                    file: self.current_file,
                    kind: crate::engine::WarningKind::Advisory,
                });
            }
            let target = self
                .pc
                .unwrap_or(0)
                .saturating_add(count)
                .rem_euclid(0x1_0000);
            return Ok(Operation::Org(Expr::Num(target)));
        }
        let count = usize::try_from(count).expect("non-negative reserve count");
        let fill = match fill {
            Some(text) => parse_value(self.syntax, text.trim(), line)?,
            None => Expr::Num(0),
        };
        Ok(Operation::Bytes(vec![fill; count]))
    }

    /// The innermost module left open at the end of the walk, if any, named by
    /// its full dotted path — the reference reports one advisory naming that,
    /// not one per open module.
    fn unclosed_module(&self) -> Option<Warning> {
        let (path, line, file) = self.scopes.unclosed()?;
        Some(Warning {
            line,
            message: format!("`ENDMODULE` missing for module `{path}`"),
            file,
            kind: crate::engine::WarningKind::Advisory,
        })
    }

    /// Open or close a module scope. Nothing is emitted: a module is a naming
    /// rule, not an operation.
    fn lower_module(&mut self, kw: ModuleKw, args: &str, line: usize) -> Result<(), AsmError> {
        let file = self.current_file;
        match kw {
            ModuleKw::Open => {
                let name = args.trim();
                if name.is_empty() {
                    return Err(AsmError::new(line, "`MODULE` needs a name"));
                }
                // The reference rejects a dotted name rather than reading it as
                // a nesting shorthand (probe m29).
                if name.contains('.') {
                    return Err(AsmError::new(
                        line,
                        format!("dots are not allowed in the module name `{name}`"),
                    ));
                }
                if !is_ident(name) {
                    return Err(AsmError::new(line, format!("bad module name `{name}`")));
                }
                self.scopes.open(name, line, file);
            }
            ModuleKw::Close => {
                if !self.scopes.close() {
                    return Err(AsmError::new(line, "`ENDMODULE` without `MODULE`"));
                }
            }
        }
        Ok(())
    }

    /// Settle names that need the complete textual statement stream.
    ///
    /// Module references choose their qualified or bare candidate. Temporary
    /// references become addresses and their internal definition names are
    /// removed, so neither reaches symbol/debug output.
    fn finish(&mut self, mut out: Vec<Statement>) -> Result<Vec<Statement>, AsmError> {
        if self.scopes.has_aliases() {
            let mut defined: std::collections::BTreeSet<String> =
                out.iter().filter_map(|s| s.label.clone()).collect();
            defined.extend(self.consts.keys().cloned());
            let fix: BTreeMap<&str, &str> = self.scopes.alias_repair(&defined);
            if !fix.is_empty() {
                for st in &mut out {
                    if let Some(op) = st.op.take() {
                        st.op = Some(crate::ast::map_syms(op, &mut |s| {
                            crate::engine::Expr::Sym(
                                fix.get(s.as_str()).map_or(s, |bare| (*bare).to_string()),
                            )
                        }));
                    }
                }
            }
        }

        if self.syntax.temporary_labels() {
            let symbols = pass_symbols(&out, self.set, self.ext, |_, _, _| {});
            for (name, source, line, file) in &self.temporary_forwards {
                if !symbols.contains_key(name) {
                    return Err(stamp_file(
                        AsmError::new(*line, format!("Temporary label not found: {source}")),
                        *file,
                    ));
                }
            }
            for st in &mut out {
                if let Some(op) = st.op.take() {
                    st.op = Some(crate::ast::map_syms(op, &mut |s| match symbols.get(&s) {
                        Some(value) if s.starts_with("__asm198x_sjasmplus_temporary_") => {
                            Expr::Num(*value)
                        }
                        _ => Expr::Sym(s),
                    }));
                }
                if st
                    .label
                    .as_ref()
                    .is_some_and(|s| s.starts_with("__asm198x_sjasmplus_temporary_"))
                {
                    st.label = None;
                }
            }
        }
        Ok(out)
    }

    /// Bind a directive line's label at the current address as a label-only
    /// statement (an include point, a `DEFINE` line).
    fn push_label(
        &mut self,
        name: &str,
        node: &Node,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let qualified = self.resolve_label(name, line)?;
        out.push(Statement {
            line,
            file: node.span.file,
            label: Some(qualified),
            op: None,
            operand_span: None,
            xor_mask: 0,
            instruction_set: None,
            extension_set: None,
        });
        Ok(())
    }

    /// Lower one live line (the body of [`CondEval::lower`]; the caller
    /// stamps span-less errors with the node's file).
    fn lower_line(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        // Inside an open `STRUCT`, every line is a member or the closer —
        // the reference refuses anything else, and so does the reader.
        if self.building.is_some() {
            return self.lower_struct_line(node, out);
        }
        // Inside an open initialiser list, a line is more of the list; the
        // brace that closes it lays the instance down (#552).
        if let Some(pending) = self.pending_init.as_mut() {
            let src = substitute_defines(&node.source, &self.defines, line)?;
            pending.items.push(InitItem::LineEnd);
            lex_struct_init(self.syntax, &src, line, &mut pending.items)?;
            pending.depth += brace_depth(&src);
            pending.last_line = line;
            if pending.depth > 0 {
                return Ok(());
            }
            let p = self.pending_init.take().expect("checked above");
            return self.emit_instance(p.line, p.file, p.instance, &p.word, p.items, out);
        }
        if self.lower_macro_line(node, out)? {
            return Ok(());
        }
        let (word0, args0) = split_first_word(&node.source);
        // `DEFINE` is handled before substitution, so the name being defined
        // is never itself expanded; chained values expand at use (probe p24).
        if self.syntax.is_define_word(word0) {
            if let Some(sym) = &node.label {
                let name = sym.name.clone();
                self.push_label(&name, node, out)?;
            }
            let (name, value) = split_first_word(args0);
            if name.is_empty() {
                return Err(AsmError::new(line, "`DEFINE` needs a name"));
            }
            if !is_ident(name) {
                return Err(AsmError::new(line, format!("bad `DEFINE` name `{name}`")));
            }
            if self.defines.contains_key(name) {
                // The reference errors on a duplicate DEFINE (probe p23).
                return Err(AsmError::new(line, format!("duplicate `DEFINE` `{name}`")));
            }
            self.defines.insert(name.to_string(), value.to_string());
            return Ok(());
        }
        // Textual DEFINE substitution: identifier-boundary, string-aware,
        // chained to a fixed point (probes p4/p5/p20/p21/p24).
        let src = substitute_defines(&node.source, &self.defines, line)?;
        let (word, args) = split_first_word(&src);
        let ends_assembly = self.syntax.is_end_word(word);
        if let Some(option) = self.syntax.lexical_option(word, args, line)? {
            match option {
                LexicalOption::SyntaxAbfw => self.syntax_abfw = true,
                LexicalOption::ZxNext { cspect } => {
                    self.ext = Some(&isa::z80::NEXT);
                    self.cspect = cspect;
                }
            }
            return Ok(());
        }
        if let Some(kw) = self.syntax.module_keyword(word) {
            return self.lower_module(kw, args, line);
        }
        match self.syntax.struct_keyword(word) {
            Some(StructKw::Open) => return self.open_struct(args, node, out),
            Some(StructKw::Close) => {
                return Err(AsmError::new(line, "`ENDS` without `STRUCT`"));
            }
            None => {}
        }
        // `label Name` where `Name` is a structure defined above: an
        // instantiation, not an instruction (probed), with an optional
        // initialiser list after the name — `{ v0, v1 }` or the bare
        // `v0, v1` (#548). Two spellings arrive: the ordinary parse split
        // the label off, and a verbatim copy kept the whole line — the
        // instance name then leads the source.
        if self.lookup_struct(word).is_some() {
            let label = node.label.as_ref().map(|s| s.name.clone());
            let init = args.trim().to_string();
            return self.lower_instance(node, label, word, &init, out);
        }
        if matches!(node.item, Some(crate::ast::Item::Verbatim))
            && !node.source.starts_with(|c: char| c.is_whitespace())
        {
            let (second, after) = split_first_word(args);
            if !second.is_empty() && self.lookup_struct(second).is_some() {
                let instance = word.trim_end_matches(':').to_string();
                let second = second.to_string();
                let init = after.trim().to_string();
                return self.lower_instance(node, Some(instance), &second, &init, out);
            }
        }
        if self.syntax.is_include(word) {
            return self.lower_include(node, args, out);
        }
        if self.syntax.is_incbin(word) {
            return self.lower_incbin(node, args, out);
        }
        // Under syntax `a`, the comma is no longer the legacy
        // multi-instruction delimiter. SjASMPlus consequently accepts the
        // explicit accumulator on the one-operand ALU family and lowers it to
        // the ordinary implicit-A form (`SUB A,B` is `SUB B`).
        // The mirror (#533): on `ADD`/`ADC`/`SBC`, whose 8-bit forms carry
        // the accumulator explicitly, sjasmplus lets it go unsaid — a lone
        // operand is `A,<operand>`. One operand, not the first: `add hl,de`
        // is untouched, and a lone `hl` is left to fail as it does there.
        let normalized;
        let rest = if self.syntax_abfw
            && matches!(
                word.to_ascii_lowercase().as_str(),
                "sub" | "and" | "xor" | "or" | "cp"
            )
            && args
                .split_once(',')
                .is_some_and(|(first, _)| first.trim().eq_ignore_ascii_case("a"))
        {
            let (_, operand) = args.split_once(',').expect("checked above");
            normalized = format!("{word} {}", operand.trim());
            normalized.as_str()
        } else if self.syntax.implicit_accumulator()
            && matches!(word.to_ascii_lowercase().as_str(), "add" | "adc" | "sbc")
            && split_operands(args).len() == 1
        {
            normalized = format!("{word} a,{}", args.trim());
            normalized.as_str()
        } else {
            src.trim()
        };
        let (rest_word, rest_args) = split_first_word(rest);
        let temporary_label = node
            .label
            .as_ref()
            .filter(|_| self.syntax.temporary_labels())
            .and_then(|sym| numeric_label(&sym.name));
        if let Some(number) = temporary_label
            && self.syntax.is_equ_word(rest_word)
        {
            return Err(AsmError::new(
                line,
                format!(
                    "Number labels are allowed as address labels only, not for DEFL/=/EQU: {}",
                    number
                ),
            ));
        }
        let resolved_temporary_label = temporary_label.map(|n| self.define_temporary(n));
        let rewritten_args = self.rewrite_temporary_target(rest_word, rest_args, line, file)?;
        let rewritten_rest = rewritten_args
            .as_ref()
            .map(|args| format!("{rest_word} {args}"));
        let rest = rewritten_rest.as_deref().unwrap_or(rest);

        let mut op = if self.cspect && word.eq_ignore_ascii_case("break") && args.trim().is_empty()
        {
            Some(Operation::Bytes(vec![Expr::Num(0xFD), Expr::Num(0x00)]))
        } else if self.cspect && word.eq_ignore_ascii_case("exit") && args.trim().is_empty() {
            Some(Operation::Bytes(vec![Expr::Num(0xDD), Expr::Num(0x00)]))
        } else if rest.is_empty() {
            None
        } else if self.syntax.resolves_forward_conditions() && self.syntax.is_reserve(word) {
            Some(self.lower_reserve(args, line)?)
        } else {
            parse_op(self.syntax, self.set, self.ext, rest, line, &self.consts)?
        };
        let label = match (&resolved_temporary_label, &node.label) {
            (Some(name), _) => Some(name.clone()),
            (None, Some(sym)) => Some(self.resolve_label(&sym.name, line)?),
            (None, None) => None,
        };
        // Locals under the current global, then module qualification wrapping
        // the result, matching the definition side: `.loc` under `glob`
        // inside `foo` is `foo.glob.loc` — the environment owns both rules.
        op = op.map(|o| {
            self.scopes
                .qualify_op(o, self.syntax.scopes_locals(), self.syntax.scopes_modules())
        });
        // `equ` binds its (qualified) label to a parse-time constant.
        if let (Some(q), Some(Operation::Equ(e))) = (&label, &op)
            && let Some(v) = eval_const(e, &self.consts)
        {
            self.consts.insert(q.clone(), v);
        }
        // A plain label names the address the counter is standing on, so a
        // condition below it folds against a value rather than reaching
        // forward. Bound before the counter moves, as the label is.
        if let (Some(q), Some(at)) = (&label, self.pc)
            && !matches!(op, Some(Operation::Equ(_)))
        {
            self.consts.insert(q.clone(), at);
        }
        self.advance(op.as_ref(), line);
        if label.is_none() && op.is_none() {
            self.terminated = ends_assembly;
            return Ok(());
        }
        out.push(Statement {
            line,
            file,
            label,
            op,
            operand_span: node.operand_span.clone(),
            xor_mask: 0,
            // Carry the live pair together: the engine treats an extension as
            // an override only when its base set is stated too.
            instruction_set: Some(self.set),
            extension_set: self.ext,
        });
        self.terminated = ends_assembly;
        Ok(())
    }

    /// Resolve an `INCLUDE` live (KTD1): the target loads only when the walk
    /// reaches the directive in a taken branch, its tree parses in its own
    /// `FileId`, and it evaluates through `self` — the environment (`equ`
    /// constants, DEFINEs, the current global) threads in and back out. A
    /// conditional block can never span the boundary: each file parses its
    /// own structure, so an unbalanced `IF`/`ENDIF` errors in the file that
    /// carries it (the reference rejects both directions — probes p12/p13).
    fn lower_include(
        &mut self,
        node: &Node,
        args: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        let at = node
            .operand_span
            .clone()
            .unwrap_or_else(|| node.span.clone());
        let request = include_request(args, line)?;
        if let Some(sym) = &node.label {
            let name = sym.name.clone();
            self.push_label(&name, node, out)?;
        }
        let Some(mcx) = self.multi.as_mut() else {
            return Err(AsmError::at(
                at,
                format!(
                    "cannot resolve `include \"{request}\"` here — the single-source \
                     API assembles one file; use the multi-file entry point \
                     (the CLI resolves includes automatically)"
                ),
            ));
        };
        if mcx.stack.len() >= MAX_INCLUDE_DEPTH {
            return Err(AsmError::at(
                at,
                format!("includes nested more than {MAX_INCLUDE_DEPTH} levels deep"),
            ));
        }
        let id = mcx
            .map
            .load(mcx.loader, &request, file, line as u32)
            .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
        if mcx.stack.contains(&id) {
            let chain = mcx
                .stack
                .iter()
                .chain(std::iter::once(&id))
                .map(|f| mcx.map.path(*f).unwrap_or("?"))
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(AsmError::at(at, format!("include cycle: {chain}")));
        }
        let contents = mcx.map.contents(id).unwrap_or_default().to_owned();
        mcx.stack.push(id);
        let program =
            parse_program_keyword(self.syntax, self.set, self.ext, id, &contents, Expand::Yes)?;
        let saved = self.current_file;
        self.current_file = id;
        let walked = crate::ast::evaluate(self, &program.nodes, true, out);
        self.current_file = saved;
        if let Some(mcx) = self.multi.as_mut() {
            mcx.stack.pop();
        }
        walked
    }

    /// Resolve an `INCBIN` live: the asset loads only when the directive is
    /// reached in a taken branch, the offset/length fold against the live
    /// constants, and the payload rides one statement at the directive's span
    /// (the walker's semantics, unchanged — KTD8: no `FileId` for binaries).
    fn lower_incbin(
        &mut self,
        node: &Node,
        args: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        let at = node
            .operand_span
            .clone()
            .unwrap_or_else(|| node.span.clone());
        let (request, offset, length) = incbin_args(self.syntax, args, line, &self.consts)?;
        let label = match &node.label {
            Some(sym) => {
                let name = sym.name.clone();
                Some(self.resolve_label(&name, line)?)
            }
            None => None,
        };
        let Some(mcx) = self.multi.as_mut() else {
            return Err(AsmError::at(
                at,
                format!(
                    "cannot resolve `incbin \"{request}\"` here — the single-source \
                     API assembles one file; use the multi-file entry point \
                     (the CLI resolves binary inclusions automatically)"
                ),
            ));
        };
        let from = mcx.map.path(file).map(str::to_owned);
        let data = mcx
            .loader
            .load_binary(&request, from.as_deref())
            .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
        let payload = slice_incbin(&data, offset, length)
            .map_err(|msg| AsmError::at(at, format!("`{request}`: {msg}")))?;
        out.push(Statement {
            line,
            file,
            label,
            op: Some(Operation::Binary(payload)),
            operand_span: node.operand_span.clone(),
            xor_mask: 0,
            instruction_set: None,
            extension_set: None,
        });
        Ok(())
    }
}

/// A `STRUCT` being read (#477): the cursor walks member by member, and
/// `ENDS` turns the accumulation into a [`crate::scopes::StructDef`] plus the
/// flat constants the engine resolves references against.
struct StructBuild {
    /// The name as written, for diagnostics and the post-`ENDS` re-anchor.
    name: String,
    /// The name the environment defined — module-prefixed where one is open.
    qualified: String,
    line: usize,
    /// The next member's offset; `STRUCT name, n` starts it at `n`, and the
    /// size answered at `ENDS` includes that initial offset (probed).
    cursor: i64,
    names: Vec<(String, i64)>,
    leaves: Vec<crate::scopes::StructLeaf>,
}

impl<'a, S: Z80Syntax> SjasmEval<'a, S> {
    /// The structure `word` names here, if any: the module-qualified spelling
    /// first, the bare one second — the same two candidates a reference tries.
    fn lookup_struct(&self, word: &str) -> Option<&crate::scopes::StructDef> {
        let prefixed = format!("{}{word}", self.scopes.prefix());
        self.scopes
            .struct_def(&prefixed)
            .or_else(|| self.scopes.struct_def(word))
    }

    /// Open a `STRUCT name[, offset]` (#477).
    fn open_struct(
        &mut self,
        args: &str,
        node: &Node,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        if let Some(sym) = &node.label {
            let name = sym.name.clone();
            self.push_label(&name, node, out)?;
        }
        let (name, offset) = match args.split_once(',') {
            Some((name, off)) => {
                let expr = parse_value(self.syntax, off.trim(), line)?;
                let off = eval_const(&expr, &self.consts).ok_or_else(|| {
                    AsmError::new(line, "the `STRUCT` offset must be a constant here")
                })?;
                (name.trim(), off)
            }
            None => (args.trim(), 0),
        };
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("bad `STRUCT` name `{name}`")));
        }
        // Defined like any global — the module prefix applies — but without
        // re-anchoring: the anchor moves to the structure's name at `ENDS`
        // (probed: a `.local` after `ENDS` binds under the structure).
        let qualified = format!("{}{name}", self.scopes.prefix());
        self.building = Some(StructBuild {
            name: name.to_string(),
            qualified,
            line,
            cursor: offset,
            names: Vec::new(),
            leaves: Vec::new(),
        });
        Ok(())
    }

    /// One line inside an open `STRUCT`: a member, or the `ENDS` that closes
    /// it. Anything else is refused the way the reference refuses it
    /// (probed: `[STRUCT] Unexpected: ld a,1`).
    fn lower_struct_line(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let src = substitute_defines(&node.source, &self.defines, line)?;
        // The member name sits in the label column of the copied line: a
        // column-0 word names the member, an indented line declares an
        // anonymous one (probed: it reserves without binding).
        let (member, rest) = if src.starts_with(|c: char| c.is_whitespace()) {
            (None, src.trim())
        } else {
            let (w, r) = split_first_word(&src);
            (Some(w.trim_end_matches(':').to_string()), r.trim())
        };
        let (word, args) = split_first_word(rest);
        match self.syntax.struct_keyword(word) {
            Some(StructKw::Close) => return self.close_struct(out),
            Some(StructKw::Open) => {
                return Err(AsmError::new(
                    line,
                    "`STRUCT` inside a `STRUCT` is not taken — close the open one first",
                ));
            }
            None => {}
        }
        let b = self.building.as_mut().expect("caller checked");
        // The member's default bytes, little-endian where multi-byte — what
        // an instantiation emits (probed: `WORD $1234` emits 34 12).
        let init = |args: &str, syntax: &'a S, consts: &BTreeMap<String, i64>| {
            if args.trim().is_empty() {
                return Ok(0i64);
            }
            let expr = parse_value(syntax, args.trim(), line)?;
            eval_const(&expr, consts).ok_or_else(|| {
                AsmError::new(
                    line,
                    "a `STRUCT` member initialiser must be a constant here",
                )
            })
        };
        // `slot`: whether an instantiation's `{ … }` list fills the member.
        let (size, bytes, slot): (i64, Vec<u8>, bool) = match word.to_ascii_uppercase().as_str() {
            "BYTE" | "DB" | "DEFB" => {
                let v = init(args, self.syntax, &self.consts)?;
                (1, vec![v as u8], true)
            }
            "WORD" | "DW" | "DEFW" => {
                let v = init(args, self.syntax, &self.consts)?;
                (2, (v as u16).to_le_bytes().to_vec(), true)
            }
            "D24" => {
                let v = init(args, self.syntax, &self.consts)?;
                (3, (v as u32).to_le_bytes()[..3].to_vec(), true)
            }
            "DWORD" => {
                let v = init(args, self.syntax, &self.consts)?;
                (4, (v as u32).to_le_bytes().to_vec(), true)
            }
            "DS" | "BLOCK" => {
                // Reaches forward like a `DS` outside the structure (probed:
                // `x DS N` above `N EQU 4` sizes the structure at 4). It
                // takes no initialiser slot (probed, #548).
                let expr = parse_value(self.syntax, args.trim(), line)?;
                let n = self.fold_count(&expr, line)?;
                if !(0..=0x1_0000).contains(&n) {
                    return Err(AsmError::new(line, format!("bad `DS` length {n}")));
                }
                (n, vec![0u8; n as usize], false)
            }
            _ => {
                // An embedded structure member (probed: its size lands here,
                // its members flatten to dotted paths under this member).
                let b_cursor = b.cursor;
                let embedded = {
                    let prefixed = format!("{}{word}", self.scopes.prefix());
                    self.scopes
                        .struct_def(&prefixed)
                        .or_else(|| self.scopes.struct_def(word))
                };
                let Some(def) = embedded else {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "`{word}` is not a member this `STRUCT` reader takes — \
                             BYTE/WORD/D24/DWORD/DS, or a structure defined above"
                        ),
                    ));
                };
                let b = self.building.as_mut().expect("caller checked");
                if let Some(m) = &member {
                    for (path, off) in &def.names {
                        b.names.push((format!("{m}.{path}"), b_cursor + off));
                    }
                }
                // The member's own edges bracket its copied leaves, so an
                // instantiation can bind the member's name and match a
                // nested `{ … }` group to it (#552).
                let edge = |group| crate::scopes::StructLeaf {
                    path: (group == crate::scopes::Group::Open)
                        .then(|| member.clone())
                        .flatten(),
                    bytes: Vec::new(),
                    slot: false,
                    group: Some(group),
                };
                b.leaves.push(edge(crate::scopes::Group::Open));
                for leaf in &def.leaves {
                    let path = match (&member, &leaf.path) {
                        (Some(m), Some(p)) => Some(format!("{m}.{p}")),
                        _ => None,
                    };
                    b.leaves.push(crate::scopes::StructLeaf {
                        path,
                        bytes: leaf.bytes.clone(),
                        slot: leaf.slot,
                        group: leaf.group,
                    });
                }
                b.leaves.push(edge(crate::scopes::Group::Close));
                let b = self.building.as_mut().expect("caller checked");
                if let Some(m) = member {
                    b.names.push((m, b_cursor));
                }
                b.cursor += def.size;
                return Ok(());
            }
        };
        let b = self.building.as_mut().expect("caller checked");
        if let Some(m) = &member {
            b.names.push((m.clone(), b.cursor));
        }
        b.leaves.push(crate::scopes::StructLeaf {
            path: member,
            bytes,
            slot,
            group: None,
        });
        b.cursor += size;
        Ok(())
    }

    /// `ENDS`: bind the structure and export its names as the flat constants
    /// references resolve against — forward references then work exactly as
    /// they do for any symbol, which is the measured posture.
    fn close_struct(&mut self, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let b = self.building.take().expect("caller checked");
        let mut export = |label: String, value: i64, out: &mut Vec<Statement>| {
            self.consts.insert(label.clone(), value);
            out.push(Statement {
                line: b.line,
                file: self.current_file,
                label: Some(label),
                op: Some(Operation::Equ(Expr::Num(value))),
                operand_span: None,
                xor_mask: 0,
                instruction_set: None,
                extension_set: None,
            });
        };
        export(b.qualified.clone(), b.cursor, out);
        for (path, off) in &b.names {
            export(format!("{}.{path}", b.qualified), *off, out);
        }
        self.scopes.bind_struct(
            b.qualified.clone(),
            crate::scopes::StructDef {
                size: b.cursor,
                names: b.names,
                leaves: b.leaves,
            },
        );
        // The structure's name is the anchor now (probed: a `.local` after
        // `ENDS` binds under it).
        self.scopes.set_anchor(b.name);
        Ok(())
    }

    /// `label Name [v0, v1, …]`: lay the structure down here, binding
    /// the instance and each named member to its address (probed). A listed
    /// value replaces the member's default at the member's width; a slot
    /// left empty, or off the end of the list, keeps the default (#548). A
    /// brace the line leaves open carries the list on to the lines that
    /// follow, and the instance waits for the one that closes it (#552).
    fn lower_instance(
        &mut self,
        node: &Node,
        label: Option<String>,
        word: &str,
        init: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        let instance = match label {
            Some(name) => Some(self.resolve_label(&name, line)?),
            None => None,
        };
        let mut items = Vec::new();
        lex_struct_init(self.syntax, init, line, &mut items)?;
        let depth = brace_depth(init);
        if depth > 0 {
            self.pending_init = Some(PendingInit {
                line,
                file,
                instance,
                word: word.to_string(),
                items,
                depth,
                last_line: line,
            });
            return Ok(());
        }
        self.emit_instance(line, file, instance, word, items, out)
    }

    /// Walk the structure's leaves against the initialiser list's items,
    /// the way the reference does: a member takes the value at the cursor
    /// if there is one, then a comma if one follows on the same line; an
    /// embedded structure's edges take a `{` and its `}`; anything the
    /// member cannot take is left for the next (probed, #548/#552).
    fn emit_instance(
        &mut self,
        line: usize,
        file: FileId,
        instance: Option<String>,
        word: &str,
        items: Vec<InitItem>,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let def = self
            .lookup_struct(word)
            .expect("caller matched a structure");
        let given = items
            .iter()
            .filter(|i| matches!(i, InitItem::Value(_)))
            .count();
        let mut cursor = InitCursor { items, pos: 0 };
        let mut depth = 0;
        if cursor.take_here(Punct::Open) {
            depth += 1;
        }
        let mut first = true;
        for leaf in &def.leaves {
            let (path, bytes) = (&leaf.path, &leaf.bytes);
            let label = match (&instance, path) {
                (Some(i), Some(p)) => Some(format!("{i}.{p}")),
                _ => None,
            };
            // The instance's own label rides the first statement, at the
            // same address as its first member.
            let label = if first {
                first = false;
                match (&instance, label) {
                    (Some(i), Some(member)) => {
                        out.push(Statement {
                            line,
                            file,
                            label: Some(i.clone()),
                            op: None,
                            operand_span: None,
                            xor_mask: 0,
                            instruction_set: None,
                            extension_set: None,
                        });
                        Some(member)
                    }
                    (Some(i), None) => Some(i.clone()),
                    (None, member) => member,
                }
            } else {
                label
            };
            let value = match leaf.group {
                Some(crate::scopes::Group::Open) => {
                    if cursor.take(Punct::Open) {
                        depth += 1;
                    }
                    None
                }
                Some(crate::scopes::Group::Close) => {
                    if depth > 0 && cursor.take(Punct::Close) {
                        depth -= 1;
                        cursor.take_here(Punct::Comma);
                    }
                    None
                }
                None if leaf.slot => match cursor.peek() {
                    Some(InitItem::Punct(Punct::Comma)) => {
                        cursor.pos += 1;
                        None
                    }
                    Some(InitItem::Value(_)) => {
                        let v = cursor.value();
                        cursor.take_here(Punct::Comma);
                        Some(v)
                    }
                    _ => None,
                },
                None => None,
            };
            if bytes.is_empty() && label.is_none() {
                continue;
            }
            // A listed value lands at the member's width, the way the data
            // directive of that width would lay it down.
            let op = match (value, bytes.len()) {
                (Some(v), 1) => Some(Operation::Bytes(vec![v])),
                (Some(v), 2) => Some(Operation::Words(vec![v])),
                (Some(v), width) => Some(Operation::Encoded(vec![crate::engine::Piece::Val {
                    expr: v,
                    bytes: width as u8,
                    rel: false,
                    signed: false,
                }])),
                (None, _) => (!bytes.is_empty()).then(|| {
                    Operation::Bytes(bytes.iter().map(|b| Expr::Num(i64::from(*b))).collect())
                }),
            };
            out.push(Statement {
                line,
                file,
                label,
                op,
                operand_span: None,
                xor_mask: 0,
                instruction_set: None,
                extension_set: None,
            });
        }
        if depth > 0 && cursor.take(Punct::Close) {
            depth -= 1;
        }
        // Text the members did not take: the reference's `closing } missing`
        // then `[STRUCT] Syntax error - too many arguments?`; the one
        // message here names the first thing left over.
        match cursor.peek() {
            Some(InitItem::Punct(Punct::Open)) => {
                return Err(AsmError::new(
                    line,
                    format!(
                        "a `{{ … }}` group in the `{word}` initialiser list is only read \
                         where the member is an embedded structure"
                    ),
                ));
            }
            Some(_) => {
                let slots = def.leaves.iter().filter(|l| l.slot).count();
                return Err(AsmError::new(
                    line,
                    format!(
                        "`{word}` has {slots} member(s) to initialise, and this list gives \
                         {given} — too many arguments"
                    ),
                ));
            }
            None => {}
        }
        if depth != 0 {
            // The reference: `closing } missing`.
            return Err(AsmError::new(
                line,
                "closing } missing on the `STRUCT` initialiser list",
            ));
        }
        Ok(())
    }
}

/// A macro definition the walk is gathering for the live namespace (#557):
/// the lines so far, and the header's position for the diagnostics.
struct MacroCapture {
    source: String,
    name: String,
    line: usize,
    file: FileId,
}

impl<S: Z80Syntax> SjasmEval<'_, S> {
    /// The live macro step (#557), for a dialect whose macros are live:
    /// gather a definition until its closer and register it; expand an
    /// invocation against what is registered so far and evaluate the body
    /// in place. `true` when the line was one of those and is dealt with.
    ///
    /// Two shapes of node arrive. The parse copies a definition, and an
    /// invocation of a name it saw defined in the same file, as verbatim
    /// whole lines; an invocation of a name defined in *another* file was an
    /// ordinary line to it, so its label is already split off and is put
    /// back before the expander reads the line — the expander binds a
    /// leading label to the expansion's first address.
    fn lower_macro_line(
        &mut self,
        node: &Node,
        out: &mut Vec<Statement>,
    ) -> Result<bool, AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        if let Some(mut capture) = self.macro_capture.take() {
            capture.source.push_str(&node.source);
            capture.source.push('\n');
            let what = self.syntax.macro_line(&node.source, &|_| false);
            if matches!(what, macros::MacroLine::Closes) {
                // Registers the definition; a header-to-closer text expands
                // to nothing.
                self.syntax.expand_live(
                    &capture.source,
                    capture.file,
                    capture.line,
                    &mut self.macro_state,
                )?;
            } else {
                self.macro_capture = Some(capture);
            }
            return Ok(true);
        }
        let whole;
        let text = match &node.label {
            Some(sym) if !matches!(node.item, Some(crate::ast::Item::Verbatim)) => {
                whole = format!("{} {}", sym.name, node.source);
                whole.as_str()
            }
            _ => node.source.as_str(),
        };
        let state = &self.macro_state;
        match self.syntax.macro_line(text, &|w| state.defines(w)) {
            macros::MacroLine::Opens(name) => {
                // Probed: `Duplicate macroname`, at the second header.
                if self.macro_state.defines(&name) {
                    return Err(AsmError::new(
                        line,
                        format!("duplicate macro name `{name}`"),
                    ));
                }
                self.macro_capture = Some(MacroCapture {
                    source: format!("{text}\n"),
                    name,
                    line,
                    file,
                });
                Ok(true)
            }
            macros::MacroLine::Invokes => {
                let Some(expanded) =
                    self.syntax
                        .expand_live(text, file, line, &mut self.macro_state)?
                else {
                    return Ok(false);
                };
                let program = parse_text_keyword(
                    self.syntax,
                    self.set,
                    self.ext,
                    file,
                    &expanded.text,
                    Some(&expanded.origins),
                )?;
                crate::ast::evaluate(self, &program.nodes, true, out)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

/// An instantiation waiting for the line that closes its initialiser list
/// (#552).
struct PendingInit {
    line: usize,
    file: FileId,
    /// The instance name, already resolved under the scopes open where the
    /// list began.
    instance: Option<String>,
    /// The structure's name.
    word: String,
    /// The list so far, lines joined by [`InitItem::LineEnd`].
    items: Vec<InitItem>,
    /// Braces still open.
    depth: i32,
    /// The last line read, where an unclosed list is reported.
    last_line: usize,
}

/// One item of an initialiser list's text, as [`lex_struct_init`] cuts it.
enum InitItem {
    Punct(Punct),
    /// The boundary between two source lines: a comma is only taken on the
    /// line its value ends on (probed: `{ $11` / `, $22 }` leaves the second
    /// member at its default, where `{ $11,` / `$22 }` fills it).
    LineEnd,
    Value(Expr),
}

/// The punctuation of an initialiser list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Punct {
    Open,
    Close,
    Comma,
}

/// A cursor over an initialiser list's items.
struct InitCursor {
    items: Vec<InitItem>,
    pos: usize,
}

impl InitCursor {
    /// The next item, reading past line ends to reach it.
    fn peek(&mut self) -> Option<&InitItem> {
        while matches!(self.items.get(self.pos), Some(InitItem::LineEnd)) {
            self.pos += 1;
        }
        self.items.get(self.pos)
    }

    /// Take `p` if it is next, reading past line ends to reach it.
    fn take(&mut self, p: Punct) -> bool {
        if matches!(self.peek(), Some(InitItem::Punct(q)) if *q == p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Take `p` if it is next on the same line.
    fn take_here(&mut self, p: Punct) -> bool {
        if matches!(self.items.get(self.pos), Some(InitItem::Punct(q)) if *q == p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Take the value at the cursor; the caller has peeked one.
    fn value(&mut self) -> Expr {
        let at = self.pos;
        self.pos += 1;
        match std::mem::replace(&mut self.items[at], InitItem::LineEnd) {
            InitItem::Value(v) => v,
            _ => unreachable!("caller peeked a value"),
        }
    }
}

impl<S: Z80Syntax> crate::ast::CondEval for SjasmEval<'_, S> {
    fn terminated(&self) -> bool {
        self.terminated
    }

    /// A repetition count folds exactly as a condition does — DEFINEs
    /// substitute, then the expression folds against the `equ` constants. That
    /// is what lets `DUP n+1` work where `n` is a constant, and why repetition
    /// cannot be resolved before symbols exist.
    /// Neither Z80 dialect's repetition names a variable: `DUP`, `REPT` and
    /// pasmo's `REPT` all take a count and nothing else.
    fn iteration(&self, head: &str, line: u32) -> Result<crate::ast::Iteration, AsmError> {
        let line = line as usize;
        let (_, args) = split_first_word(head);
        let expr = substitute_defines(args, &self.defines, line)?;
        eval_condition_keyword(self.syntax, &expr, line, &self.consts, self.fwd())
            .map(crate::ast::Iteration::Times)
    }

    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        let (word, args) = split_first_word(head);
        let taken = match self.syntax.cond_keyword(word) {
            Some(kw @ (CondKw::IfDef | CondKw::IfNDef)) => {
                // The reference tests the FIRST token, ignoring the rest
                // (probe p48); the namespace is the case-sensitive DEFINE
                // table only — never labels or `equ` constants (probes
                // p3/p22).
                match args.split_whitespace().next() {
                    Some(name) => {
                        let defined = self.defines.contains_key(name);
                        Ok(if kw == CondKw::IfDef {
                            defined
                        } else {
                            !defined
                        })
                    }
                    None => Err(AsmError::new(line, format!("`{word}` needs a name"))),
                }
            }
            Some(CondKw::If | CondKw::ElseIf) => {
                // An `ELSEIF` leg tests its condition exactly as an `IF` does;
                // it reaches here only as the head of a chain leg (#67).
                // DEFINEs substitute into the condition (probe p25) before it
                // folds against the `equ` constants.
                substitute_defines(args, &self.defines, line).and_then(|cond| {
                    eval_condition_keyword(self.syntax, &cond, line, &self.consts, self.fwd())
                        .map(|v| v != 0)
                })
            }
            _ => Err(AsmError::new(
                line,
                format!("internal error: `{head}` is not a conditional head"),
            )),
        };
        // The shared walk raises condition errors without node context, so a
        // failure inside an included file is stamped here.
        taken.map_err(|e| stamp_file(e, self.current_file))
    }

    fn lower(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        // Per-line helpers know their line but not their file; stamp at this
        // one boundary — but only span-less errors. An error from a *nested*
        // walk (an include's lines, reached through `lower_include`) was
        // stamped by its own frame and must keep the inner file.
        self.lower_line(node, out)
            .map_err(|e| stamp_missing_file(e, node.span.file))
    }
}

/// Assemble keyword-conditional source (single file): the structure parse,
/// then the shared conditional walk over [`SjasmEval`] with no loader — an
/// `INCLUDE`/`INCBIN` reached live errors with a pointer at the multi-file
/// entry points.
/// Where each label and `equ` lands, given one pass's statements — the seed
/// the next pass answers forward references from.
///
/// This is the engine's address pass in miniature, over the same
/// [`crate::engine::next_pc`] rule, and it is deliberately forgiving: a
/// statement whose width or origin cannot be computed yet stops the counter
/// rather than failing the pass. A pass that cannot place everything still
/// places something, and the pass after it does better.
fn pass_symbols(
    stmts: &[Statement],
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mut at: impl FnMut(&str, usize, FileId),
) -> BTreeMap<String, i64> {
    let mut symbols = BTreeMap::new();
    let mut pc = Some(0i64);
    for s in stmts {
        let (statement_set, statement_ext) = match s.instruction_set {
            Some(statement_set) => (statement_set, s.extension_set),
            None => (set, ext),
        };
        if let (Some(name), Some(Operation::Equ(e))) = (&s.label, &s.op) {
            if let Some(v) = eval_const(e, &symbols) {
                symbols.insert(name.clone(), v);
                at(name, s.line, s.file);
            }
            continue;
        }
        if let (Some(name), Some(here)) = (&s.label, pc) {
            symbols.insert(name.clone(), here);
            at(name, s.line, s.file);
        }
        let Some(op) = &s.op else { continue };
        pc = match op {
            Operation::Org(e) => eval_const(e, &symbols),
            _ => pc.and_then(|at| {
                crate::engine::next_pc(op, at, statement_set, statement_ext, 1, s.line).ok()
            }),
        };
    }
    symbols
}

/// Run the walk until forward references settle, or until the pass the
/// reference stops at.
///
/// Three passes, because that is what sjasmplus does — it prints "Pass 3
/// complete" on every file — and matching it is the whole point. A program
/// whose first pass reached no forward symbol is finished there: the later
/// passes would produce the same statements, and every program that does not
/// use the feature would otherwise pay three times over.
///
/// **Convergence is not promised, because the reference does not promise it.**
/// Given `IF later < 2` … `later:`, emitting the body moves `later` past 2 and
/// the condition that admitted the body is false by the end. sjasmplus warns
/// and ships that binary; so do we, with the same two warnings. Refusing would
/// be defensible and would not be *the reference*.
fn run_passes<'a, S: Z80Syntax>(
    syntax: &'a S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    program: &Program,
    mut multi: Option<MultiCx<'a>>,
) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
    const PASSES: usize = 3;
    let mut warnings = Vec::new();
    let mut seed = BTreeMap::new();
    let mut previous: Option<BTreeMap<String, i64>> = None;
    let mut result = Vec::new();
    for pass in 1..=PASSES {
        let mut eval = SjasmEval::new(syntax, set, ext, multi.take());
        if syntax.resolves_forward_conditions() {
            eval.forward = Some(Forward::new(std::mem::take(&mut seed), pass == PASSES));
        }
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        // The reference errors at end of source with a structure still open
        // (probed: `[STRUCT] Unexpected end of structure`).
        if let Some(b) = &eval.building {
            return Err(AsmError::new(
                b.line,
                format!("`STRUCT {}` is never closed", b.name),
            ));
        }
        // Likewise an initialiser list still open (probed: `closing }
        // missing`, reported where the source ran out).
        if let Some(p) = &eval.pending_init {
            return Err(AsmError::new(
                p.last_line,
                "closing } missing on the `STRUCT` initialiser list",
            ));
        }
        // And a macro definition still open — the wording the expander's
        // own collector uses for the same fault.
        if let Some(c) = &eval.macro_capture {
            return Err(AsmError::new(
                c.line,
                format!("`macro {}` has no matching `endm`", c.name),
            ));
        }
        result = eval.finish(out)?;
        let mut defined_at: BTreeMap<String, (usize, FileId)> = BTreeMap::new();
        let symbols = pass_symbols(&result, set, ext, |name, line, file| {
            defined_at.insert(name.to_string(), (line, file));
        });
        let reached_forward = eval.forward.as_ref().is_some_and(|f| f.used.get());
        if pass == 1 {
            // The advisories belong to this pass: it is the one where the
            // symbol was genuinely unknown and read as zero.
            if let Some(f) = eval.forward.as_ref() {
                warnings.append(&mut f.warnings.borrow_mut());
            }
            // A module left open at end of file. The reference warns and
            // assembles; so do we, now that there is somewhere to say it.
            // Raised on pass 1 because the module structure does not change
            // between passes, and raising it on each would say it three times.
            warnings.extend(eval.unclosed_module());
            if !reached_forward {
                return Ok((result, warnings));
            }
        }
        multi = eval.multi;
        if pass == PASSES {
            // A label that still moved between the last two passes never
            // settled, and the bytes below it describe a layout no pass
            // agreed with. The reference says so rather than failing, and
            // names both values.
            if let Some(previous) = &previous {
                for (name, now) in &symbols {
                    if let Some(before) = previous.get(name)
                        && before != now
                    {
                        let (line, file) = defined_at.get(name).copied().unwrap_or((0, FileId(0)));
                        warnings.push(Warning {
                            line,
                            message: format!(
                                "label `{name}` has a different value in pass {PASSES}: \
                                 previous value {before} not equal {now}"
                            ),
                            file,
                            kind: crate::engine::WarningKind::Advisory,
                        });
                    }
                }
            }
            break;
        }
        previous = Some(symbols.clone());
        seed = symbols;
    }
    Ok((result, warnings))
}

pub(crate) fn assemble_keyword<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    source: &str,
) -> Result<Vec<Statement>, AsmError> {
    Ok(assemble_keyword_warned(syntax, set, ext, source)?.0)
}

/// [`assemble_keyword`], keeping the advisories the passes raised (#99).
pub(crate) fn assemble_keyword_warned<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    source: &str,
) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
    let program = parse_program_keyword(syntax, set, ext, FileId(0), source, Expand::Yes)?;
    run_passes(syntax, set, ext, &program, None)
}

/// Parse a multi-file keyword-conditional program to the engine's statement
/// stream: the structure parse per file, includes resolving lazily *inside*
/// the conditional walk (an untaken include never loads — KTD1).
pub(crate) fn parse_program_multi_keyword<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<Vec<Statement>, AsmError> {
    Ok(parse_program_multi_keyword_warned(syntax, set, ext, map, loader)?.0)
}

/// [`parse_program_multi_keyword`], keeping the advisories the passes raised.
///
/// Re-walking is safe across an include boundary because [`SourceMap`] dedups
/// by canonical path: a later pass resolves the same request to the same
/// `FileId` and reads nothing from the backing store a second time.
pub(crate) fn parse_program_multi_keyword_warned<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
    let root = map.contents(FileId(0)).unwrap_or_default().to_owned();
    let program = parse_program_keyword(syntax, set, ext, FileId(0), &root, Expand::Yes)?;
    run_passes(
        syntax,
        set,
        ext,
        &program,
        Some(MultiCx {
            map,
            loader,
            stack: vec![FileId(0)],
        }),
    )
}

/// Expand `DEFINE` names in one line of source, per the probe-pinned
/// reference semantics: identifier tokens replace at exact boundaries (`NN`
/// is not an occurrence of `N` — probe p20); `"…"` strings and `'c'` char
/// literals are untouched (probe p21); number tokens — digit-led runs and
/// `$`/`%`/`#`-sigil runs — are skipped whole, so a define named `FF` never
/// rewrites `$FF`. Chained defines expand pass by pass to a fixed point
/// (probe p24), with a depth cap against recursive definitions.
fn substitute_defines(
    s: &str,
    defines: &BTreeMap<String, String>,
    line: usize,
) -> Result<String, AsmError> {
    if defines.is_empty() {
        return Ok(s.to_string());
    }
    let mut cur = s.to_string();
    for _ in 0..32 {
        let (next, changed) = substitute_once(&cur, defines);
        if !changed || next == cur {
            return Ok(next);
        }
        cur = next;
        // Mutually-recursive definitions (`DEFINE A B+B` / `DEFINE B A+A`)
        // grow geometrically within the pass cap, so the working line is
        // size-bounded too: past 64K it can only be a runaway expansion.
        if cur.len() > 64 * 1024 {
            break;
        }
    }
    Err(AsmError::new(
        line,
        "`DEFINE` expansion did not terminate (recursive DEFINE?)",
    ))
}

/// One substitution pass over `s`; `true` if anything was replaced.
fn substitute_once(s: &str, defines: &BTreeMap<String, String>) -> (String, bool) {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut changed = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            // Copy a string literal whole (or to end-of-line if unterminated).
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                let closed = chars[i] == '"';
                i += 1;
                if closed {
                    break;
                }
            }
        } else if c == '\'' && i + 2 < chars.len() && chars[i + 2] == '\'' {
            // A `'c'` char literal (the tokenizer's shape); a lone `'`
            // (`af'`) copies below.
            out.push(chars[i]);
            out.push(chars[i + 1]);
            out.push(chars[i + 2]);
            i += 3;
        } else if c == '$' || c == '%' || c == '#' {
            // A number sigil: copy it and its alphanumeric run untouched.
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                out.push(chars[i]);
                i += 1;
            }
        } else if c.is_ascii_digit() {
            // A digit-led number (incl. `10h`/`0x10` forms): copy whole.
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                out.push(chars[i]);
                i += 1;
            }
        } else if c.is_ascii_alphabetic() || c == '_' || c == '.' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            match defines.get(&ident) {
                Some(replacement) => {
                    out.push_str(replacement);
                    changed = true;
                }
                None => out.push_str(&ident),
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    (out, changed)
}

/// Evaluate a keyword `IF` condition to a value (nonzero = taken) against the
/// parse-time constants. The grammar mirrors the operand expression grammar
/// and adds the condition operators the reference accepts (probes p2/p45):
/// `||`/`&&` (loosest), one comparison (`=`/`==`/`!=`/`<`/`>`/`<=`/`>=`),
/// unary `!`, and parentheses that enclose the full condition grammar.
/// Trailing text after a complete condition is ignored, as the reference does
/// (probe p50). Symbols resolve against `equ` constants (DEFINEs substituted
/// before this) — a label-valued or forward symbol, and the location counter
/// `$`, are the reference's multi-pass territory and error here (#67).
fn eval_condition_keyword<S: Z80Syntax>(
    syntax: &S,
    s: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
    forward: Option<(&Forward, FileId)>,
) -> Result<i64, AsmError> {
    let tokens = tokenize_cond(syntax, s, line)?;
    if tokens.is_empty() {
        return Err(AsmError::new(line, "`IF` needs a condition"));
    }
    CondParser {
        tokens,
        pos: 0,
        line,
        consts,
        forward,
        sources: syntax.constant_sources(),
    }
    .or_expr()
}

/// The condition parser: precedence-climbing over [`Tok`]s, folding to `i64`
/// immediately (a condition must be decidable when its `IF` is reached).
struct CondParser<'a> {
    tokens: Vec<Tok>,
    pos: usize,
    line: usize,
    consts: &'a BTreeMap<String, i64>,
    /// Where a symbol the constant table cannot answer is resolved, on a
    /// dialect that resolves conditions across passes. `None` keeps the
    /// parse-time-constant rule and the diagnostic that explains it.
    forward: Option<(&'a Forward, FileId)>,
    /// This dialect's phrasing for where a constant may come from
    /// ([`Z80Syntax::constant_sources`]).
    sources: &'static str,
}

impl CondParser<'_> {
    fn or_expr(&mut self) -> Result<i64, AsmError> {
        let mut v = self.and_expr()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::OrOr)) {
            self.pos += 1;
            let r = self.and_expr()?;
            v = i64::from(v != 0 || r != 0);
        }
        Ok(v)
    }

    fn and_expr(&mut self) -> Result<i64, AsmError> {
        let mut v = self.cmp_expr()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::AndAnd)) {
            self.pos += 1;
            let r = self.cmp_expr()?;
            v = i64::from(v != 0 && r != 0);
        }
        Ok(v)
    }

    /// At most one comparison, non-chaining (`a = b = c` is not a condition
    /// the reference's sources write).
    fn cmp_expr(&mut self) -> Result<i64, AsmError> {
        let a = self.bit_or()?;
        let tok = match self.tokens.get(self.pos) {
            Some(t @ (Tok::Eq | Tok::Ne | Tok::Lt | Tok::Gt | Tok::Le | Tok::Ge)) => t.clone(),
            _ => return Ok(a),
        };
        self.pos += 1;
        let b = self.bit_or()?;
        Ok(i64::from(match tok {
            Tok::Eq => a == b,
            Tok::Ne => a != b,
            Tok::Lt => a < b,
            Tok::Gt => a > b,
            Tok::Le => a <= b,
            Tok::Ge => a >= b,
            _ => unreachable!("matched above"),
        }))
    }

    fn bit_or(&mut self) -> Result<i64, AsmError> {
        let mut v = self.bit_xor()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Or)) {
            self.pos += 1;
            v |= self.bit_xor()?;
        }
        Ok(v)
    }

    fn bit_xor(&mut self) -> Result<i64, AsmError> {
        let mut v = self.bit_and()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Xor)) {
            self.pos += 1;
            v ^= self.bit_and()?;
        }
        Ok(v)
    }

    fn bit_and(&mut self) -> Result<i64, AsmError> {
        let mut v = self.shift()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::And)) {
            self.pos += 1;
            v &= self.shift()?;
        }
        Ok(v)
    }

    fn shift(&mut self) -> Result<i64, AsmError> {
        let mut v = self.add_sub()?;
        loop {
            let left = match self.tokens.get(self.pos) {
                Some(Tok::Shl) => true,
                Some(Tok::Shr) => false,
                _ => return Ok(v),
            };
            self.pos += 1;
            let by = self.add_sub()?;
            if !(0..64).contains(&by) {
                return Err(AsmError::new(
                    self.line,
                    "shift amount out of range in condition",
                ));
            }
            v = if left { v << by } else { v >> by };
        }
    }

    fn add_sub(&mut self) -> Result<i64, AsmError> {
        let mut v = self.mul_div()?;
        loop {
            let add = match self.tokens.get(self.pos) {
                Some(Tok::Plus) => true,
                Some(Tok::Minus) => false,
                _ => return Ok(v),
            };
            self.pos += 1;
            let r = self.mul_div()?;
            v = if add {
                v.wrapping_add(r)
            } else {
                v.wrapping_sub(r)
            };
        }
    }

    fn mul_div(&mut self) -> Result<i64, AsmError> {
        let mut v = self.unary()?;
        loop {
            let mul = match self.tokens.get(self.pos) {
                Some(Tok::Star) => true,
                Some(Tok::Slash) => false,
                _ => return Ok(v),
            };
            self.pos += 1;
            let r = self.unary()?;
            if mul {
                v = v.wrapping_mul(r);
            } else if r == 0 {
                return Err(AsmError::new(self.line, "division by zero in condition"));
            } else {
                v = v.wrapping_div(r);
            }
        }
    }

    fn unary(&mut self) -> Result<i64, AsmError> {
        match self.tokens.get(self.pos) {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(self.unary()?.wrapping_neg())
            }
            Some(Tok::Not) => {
                self.pos += 1;
                Ok(i64::from(self.unary()? == 0))
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Result<i64, AsmError> {
        let tok = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or_else(|| AsmError::new(self.line, "expected a value in condition"))?;
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(n),
            Tok::Sym(s) => match self.consts.get(&s).copied() {
                Some(v) => Ok(v),
                None => match self.forward {
                    Some((fwd, file)) => fwd.lookup(&s, self.line, file),
                    None => {
                        let sources = self.sources;
                        Err(AsmError::new(
                            self.line,
                            format!(
                                "`{s}` must be a constant here (a number, an expression of \
                                 constants, or {sources})"
                            ),
                        ))
                    }
                },
            },
            Tok::Pc => Err(AsmError::new(
                self.line,
                "the location counter `$` cannot be tested in a conditional here",
            )),
            Tok::LParen => {
                let v = self.or_expr()?;
                if matches!(self.tokens.get(self.pos), Some(Tok::RParen)) {
                    self.pos += 1;
                    Ok(v)
                } else {
                    Err(AsmError::new(self.line, "expected `)` in condition"))
                }
            }
            _ => Err(AsmError::new(self.line, "expected a value in condition")),
        }
    }
}

// ---------------------------------------------------------------------------
// Line structure
// ---------------------------------------------------------------------------

/// Split a (comment-stripped) line into its optional label and the remainder.
/// A `name:` token is always a label; otherwise a label sits in column 0 and
/// instructions are indented. A column-0 first word that names a known mnemonic
/// or directive is the operation, not a label — unless the dialect says column
/// 0 is a label without exception ([`Z80Syntax::column_zero_is_a_label`]).
///
/// `col` is where the statement starts in its line: past 1 it follows a `:`
/// separator, which puts it in the operation field whatever its text —
/// `nop : foo nop` is `Unrecognized instruction: foo nop` in sjasmplus, and
/// so is `nop : foo: nop` (probed, #551).
fn split_label<'a, S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    code: &'a str,
    col: u32,
    line: usize,
) -> Result<(Option<String>, &'a str), AsmError> {
    let trimmed = code.trim();
    if col > 1 {
        return Ok((None, trimmed));
    }
    if let Some(colon) = trimmed.find(':') {
        let before = &trimmed[..colon];
        if !before.contains(char::is_whitespace) {
            if !is_label_ident(syntax, before.trim())
                && !(syntax.temporary_labels() && numeric_label(before.trim()).is_some())
            {
                return Err(AsmError::new(
                    line,
                    format!("invalid label `{}`", before.trim()),
                ));
            }
            return Ok((Some(before.trim().to_string()), trimmed[colon + 1..].trim()));
        }
    }
    if code.starts_with([' ', '\t']) {
        return Ok((None, trimmed));
    }
    let (word, remainder) = split_first_word(trimmed);
    if !syntax.column_zero_is_a_label()
        && (has_mnemonic(set, ext, &word.to_ascii_uppercase()) || syntax.is_directive(word))
    {
        return Ok((None, trimmed));
    }
    if !is_label_ident(syntax, word)
        && !(syntax.temporary_labels() && numeric_label(word).is_some())
    {
        return Err(AsmError::new(line, format!("invalid label `{word}`")));
    }
    Ok((Some(word.to_string()), remainder))
}

fn parse_op<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    rest: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<Option<Operation>, AsmError> {
    if rest.is_empty() {
        return Ok(None);
    }
    let (word, args) = split_first_word(rest);
    if syntax.is_directive(word) {
        return syntax.parse_directive(word, args, line, consts);
    }
    // A directive this dialect declares and does not implement. Refusing it as
    // an unknown mnemonic would tell the reader their source is invalid, when
    // the reference assembler takes it and the gap is ours.
    if let Some(entry) = crate::directives::lookup(syntax.own_directives(), word)
        && let crate::directives::Category::RefusedByReference(rule) = entry.category
    {
        return Err(AsmError::new(
            line,
            // No dialect on this path declares one yet, so the tool is named
            // generically rather than adding a trait hook for an arm nothing
            // reaches — name it here when one does.
            crate::directives::refused_by_reference("the reference assembler", word, rule),
        ));
    }
    if let Some(entry) = crate::directives::lookup(syntax.own_directives(), word)
        && entry.category == crate::directives::Category::KnownUnsupported
    {
        return Err(AsmError::new(
            line,
            format!(
                "`{word}` is a directive this dialect has and asm198x does not \
                 implement — the source is valid and the gap is here, not in it"
            ),
        ));
    }
    let mnemonic = word.to_ascii_uppercase();
    if !has_mnemonic(set, ext, &mnemonic) {
        return Err(AsmError::new(
            line,
            format!("unknown instruction `{mnemonic}`"),
        ));
    }
    let (mode, operands) = resolve(syntax, set, ext, &mnemonic, args, line, consts)?;
    Ok(Some(Operation::Instruction {
        mnemonic,
        mode,
        operands,
    }))
}

// ---------------------------------------------------------------------------
// Common directives
// ---------------------------------------------------------------------------

/// The directives pasmo and sjasmplus share.
///
/// A **base**, not either dialect's full surface. Each dialect composes its
/// own file-inclusion and conditional vocabulary on top of it.
pub(crate) const COMMON_DIRECTIVES: &[Directive] = &[
    Directive {
        id: "org",
        pattern: Pattern::Exact(&["org"]),
        category: Category::Operation,
    },
    Directive {
        id: "equ",
        pattern: Pattern::Exact(&["equ"]),
        category: Category::Operation,
    },
    Directive {
        id: "bytes",
        pattern: Pattern::Exact(&["defb", "db", "defm", "dm"]),
        category: Category::Operation,
    },
    Directive {
        id: "words",
        pattern: Pattern::Exact(&["defw", "dw"]),
        category: Category::Operation,
    },
    Directive {
        id: "reserve",
        pattern: Pattern::Exact(&["defs", "ds"]),
        category: Category::Operation,
    },
    Directive {
        id: "end",
        pattern: Pattern::Exact(&["end"]),
        category: Category::Operation,
    },
];

/// Whether `word` is the common reserve directive (`ds`/`defs`).
pub(crate) fn is_common_reserve(word: &str) -> bool {
    lookup(COMMON_DIRECTIVES, word).is_some_and(|d| d.id == "reserve")
}

/// Whether `word` is one of them.
///
/// Reads the declaration rather than repeating it: this predicate and
/// [`common_directive`] used to carry the same eleven spellings separately, so
/// adding one meant remembering both.
pub(crate) fn is_common_directive(word: &str) -> bool {
    lookup(COMMON_DIRECTIVES, word).is_some()
}

/// Parse a common directive. `defs`/`ds` reserve a constant-folded number of
/// zero bytes (a literal or an expression of `equ` constants).
pub(crate) fn common_directive<S: Z80Syntax>(
    syntax: &S,
    word: &str,
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<Option<Operation>, AsmError> {
    let Some(entry) = lookup(COMMON_DIRECTIVES, word) else {
        return Err(AsmError::new(line, format!("unknown directive `{word}`")));
    };
    Ok(match entry.id {
        "org" => Some(Operation::Org(parse_value(syntax, args, line)?)),
        "equ" => Some(Operation::Equ(parse_value(syntax, args, line)?)),
        "bytes" => Some(Operation::Bytes(parse_list(syntax, args, line)?)),
        "words" => Some(Operation::Words(parse_list(syntax, args, line)?)),
        "reserve" => {
            // The count must be known at parse time (it sets the statement's
            // size), but it need not be a bare literal — fold any expression of
            // `equ` constants, e.g. `ds MAX_TORCHES * 2`.
            let count = literal(&parse_value(syntax, args, line)?, consts, line)?;
            let count = usize::try_from(count).map_err(|_| {
                AsmError::new(line, "`ds`/`defs` count must be a non-negative constant")
            })?;
            Some(Operation::Bytes(vec![Expr::Num(0); count]))
        }
        // `end [addr]` marks the entry point. A flat binary ignores it, but a
        // `.sna` snapshot needs the start address — capture it when given.
        "end" if args.trim().is_empty() => None,
        "end" => Some(Operation::Entry(parse_value(syntax, args, line)?)),
        other => {
            return Err(AsmError::new(
                line,
                format!("`{other}` is declared but not dispatched"),
            ));
        }
    })
}

// ---------------------------------------------------------------------------
// Instruction-set lookup (primary + optional Z80N extension)
// ---------------------------------------------------------------------------

fn find_form(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
    mode: &str,
) -> Option<&'static isa::Form> {
    set.find_form(mnemonic, mode)
        .or_else(|| ext.and_then(|e| e.find_form(mnemonic, mode)))
}

fn has_mnemonic(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
) -> bool {
    set.has_mnemonic(mnemonic) || ext.is_some_and(|e| e.has_mnemonic(mnemonic))
}

// ---------------------------------------------------------------------------
// Operand resolution (dialect syntax -> spec mode label)
// ---------------------------------------------------------------------------

/// One classified operand.
enum Operand {
    /// A register, condition, or register-indirect — a fixed signature token.
    Fixed(String),
    /// A value: an immediate or a `(nn)` address. `paren` marks the memory form.
    Value { expr: Expr, paren: bool },
    /// An indexed operand `(IX+d)` / `(IY+d)`. `disp` is `None` for a bare
    /// `(IX)` — either register-indirect (`JP (IX)`) or `(IX+0)`, by which form
    /// exists.
    Indexed {
        reg: &'static str,
        disp: Option<Expr>,
    },
}

/// One way an operand can be written into a mode label: the token it
/// contributes, and the value(s) it emits as bytes (empty if consumed into the
/// opcode, e.g. a BIT bit-number).
type Alternative = (String, Vec<Expr>);

fn resolve<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let pieces = split_operands(args);
    let mut per_operand: Vec<Vec<Alternative>> = Vec::new();
    for (idx, piece) in pieces.iter().enumerate() {
        per_operand.push(alternatives(syntax, mnemonic, idx, piece, consts, line)?);
    }

    for combo in product(&per_operand) {
        let label = combo
            .iter()
            .map(|(token, _)| token.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if let Some(f) = find_form(set, ext, mnemonic, &label) {
            let emitted = combo.into_iter().flat_map(|(_, values)| values).collect();
            return Ok((f.mode, emitted));
        }
    }
    Err(AsmError::new(
        line,
        format!("`{mnemonic}` has no form for operands `{}`", args.trim()),
    ))
}

fn alternatives<S: Z80Syntax>(
    syntax: &S,
    mnemonic: &str,
    idx: usize,
    piece: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Vec<Alternative>, AsmError> {
    Ok(match classify(syntax, piece, line)? {
        Operand::Fixed(token) => vec![(token, vec![])],
        Operand::Indexed { reg, disp } => match disp {
            Some(d) => vec![(format!("({reg}+d)"), vec![d])],
            None => vec![
                (format!("({reg})"), vec![]),
                (format!("({reg}+d)"), vec![Expr::Num(0)]),
            ],
        },
        Operand::Value { expr, paren } => {
            if let Some(token) = embedded_token(mnemonic, paren, idx, &expr, consts, line)? {
                vec![(token, vec![])] // consumed into the opcode
            } else {
                emitted_tokens(mnemonic, paren)
                    .into_iter()
                    .map(|token| (token, vec![expr.clone()]))
                    .collect()
            }
        }
    })
}

fn classify<S: Z80Syntax>(syntax: &S, piece: &str, line: usize) -> Result<Operand, AsmError> {
    let t = piece.trim();
    if let Some(inner) = strip_parens(t) {
        let inner = inner.trim();
        if let Some((reg, rest)) = index_register(inner) {
            let disp = if rest.is_empty() {
                None
            } else if let Some(after_plus) = rest.strip_prefix('+') {
                Some(parse_value(syntax, after_plus, line)?)
            } else {
                Some(parse_value(syntax, rest, line)?) // '-': unary minus
            };
            return Ok(Operand::Indexed { reg, disp });
        }
        let up = inner.to_ascii_uppercase();
        if is_indirect_reg(&up) {
            return Ok(Operand::Fixed(format!("({up})")));
        }
        return Ok(Operand::Value {
            expr: parse_value(syntax, inner, line)?,
            paren: true,
        });
    }
    let up = t.to_ascii_uppercase();
    if is_reg_or_cond(&up) {
        return Ok(Operand::Fixed(up));
    }
    Ok(Operand::Value {
        expr: parse_value(syntax, t, line)?,
        paren: false,
    })
}

/// If `inner` names an index register with an optional displacement, return the
/// canonical register and the rest. Guards against symbols starting with
/// "ix"/"iy" by requiring the next char to be `+`, `-`, or nothing.
fn index_register(inner: &str) -> Option<(&'static str, &str)> {
    for reg in ["IX", "IY"] {
        if inner.len() >= 2 && inner[..2].eq_ignore_ascii_case(reg) {
            let rest = inner[2..].trim_start();
            if rest.is_empty() || rest.starts_with('+') || rest.starts_with('-') {
                return Some((reg, rest));
            }
        }
    }
    None
}

/// For an operand encoded *in the opcode* (RST target, IM mode, BIT/RES/SET bit
/// number), return its mode-label token. `None` for operands that become bytes.
fn embedded_token(
    mnemonic: &str,
    paren: bool,
    index: usize,
    expr: &Expr,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<String>, AsmError> {
    if paren {
        return Ok(None);
    }
    let token = match mnemonic {
        "RST" => format!("{:02X}", literal(expr, consts, line)?),
        "IM" => format!("{}", literal(expr, consts, line)?),
        "BIT" | "RES" | "SET" if index == 0 => format!("{}", literal(expr, consts, line)?),
        _ => return Ok(None),
    };
    Ok(Some(token))
}

/// Candidate tokens for a value operand that becomes bytes. Width is left
/// ambiguous (both offered) except for relative branches.
fn emitted_tokens(mnemonic: &str, paren: bool) -> Vec<String> {
    if paren {
        return vec!["(n)".to_string(), "(nn)".to_string()];
    }
    match mnemonic {
        "JR" | "DJNZ" => vec!["e".to_string()],
        _ => vec!["n".to_string(), "nn".to_string()],
    }
}

/// Resolve an opcode-embedded operand to a parse-time constant (a number, an
/// expression of constants, or an `equ` value above — but not a label).
pub(crate) fn literal(
    expr: &Expr,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<i64, AsmError> {
    eval_const(expr, consts).ok_or_else(|| {
        AsmError::new(
            line,
            "operand must be a constant here (a number, an expression of \
             constants, or a value defined with `equ` above)",
        )
    })
}

/// Fold an expression to a constant, resolving symbols only against `equ`
/// constants. `None` if it references an unknown symbol or overflows. `$` (the
/// location counter) is unknown until the engine's emit pass, so it never folds
/// here (the parse-time-constant context passes no PC).
pub(crate) fn eval_const(expr: &Expr, consts: &BTreeMap<String, i64>) -> Option<i64> {
    expr.eval_with(&|s| consts.get(s).copied(), None, 0).ok()
}

// Local qualification — `jr .loop` under global `start` → `start.loop` — is
// the shared [`crate::ast::qualify_locals`] (language-surface U7): z80 and
// rgbasm ran provably identical copies, so the mangle lives in one place; the
// *when* (only under `Z80Syntax::scopes_locals()`, so pasmo never scopes)
// stays here.

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

// ---------------------------------------------------------------------------
// Register / condition vocabulary
// ---------------------------------------------------------------------------

fn is_indirect_reg(up: &str) -> bool {
    matches!(up, "HL" | "BC" | "DE" | "SP" | "C")
}

/// Register or condition tokens (used verbatim in a mode label). `C` is both a
/// register and the carry condition; the form lookup disambiguates by mnemonic.
fn is_reg_or_cond(up: &str) -> bool {
    matches!(
        up,
        "A" | "B"
            | "C"
            | "D"
            | "E"
            | "H"
            | "L"
            | "I"
            | "R"
            | "AF"
            | "AF'"
            | "BC"
            | "DE"
            | "HL"
            | "SP"
            | "IX"
            | "IY"
            | "IXH"
            | "IXL"
            | "IYH"
            | "IYL"
            | "NZ"
            | "Z"
            | "NC"
            | "PO"
            | "PE"
            | "P"
            | "M"
    )
}

// ---------------------------------------------------------------------------
// Tokenising and the expression parser
// ---------------------------------------------------------------------------

/// Cut one line of a `STRUCT` instance's initialiser list into its items:
/// the braces, the commas, and the values between them — several to a run
/// where no comma separates them, since the reference reads an expression
/// and then an *optional* comma (probed: `{ $11 $22 }` fills two members).
/// Every shape here was probed against SjASMPlus 1.21.0 (#548, #552).
fn lex_struct_init<S: Z80Syntax>(
    syntax: &S,
    text: &str,
    line: usize,
    out: &mut Vec<InitItem>,
) -> Result<(), AsmError> {
    let mut run = String::new();
    let mut quote: Option<char> = None;
    let mut parens = 0i32;
    let flush = |run: &mut String, out: &mut Vec<InitItem>| -> Result<(), AsmError> {
        if !run.trim().is_empty() {
            let tokens = tokenize(syntax, run, line)?;
            let mut parser = ExprParser {
                tokens,
                pos: 0,
                line,
            };
            while parser.pos < parser.tokens.len() {
                out.push(InitItem::Value(parser.expr()?));
            }
        }
        run.clear();
        Ok(())
    };
    for ch in text.chars() {
        if let Some(q) = quote {
            run.push(ch);
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                run.push(ch);
            }
            '(' => {
                parens += 1;
                run.push(ch);
            }
            ')' => {
                parens -= 1;
                run.push(ch);
            }
            '{' | '}' | ',' if parens == 0 => {
                flush(&mut run, out)?;
                out.push(InitItem::Punct(match ch {
                    '{' => Punct::Open,
                    '}' => Punct::Close,
                    _ => Punct::Comma,
                }));
            }
            _ => run.push(ch),
        }
    }
    flush(&mut run, out)
}

/// The braces `code` opens minus the ones it closes, quoted text skipped —
/// positive when an initialiser list runs on past this line (#552).
fn brace_depth(code: &str) -> i32 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for ch in code.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth
}

/// Split operand text on top-level commas (commas inside parentheses are kept).
fn split_operands(args: &str) -> Vec<&str> {
    let args = args.trim();
    if args.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(args[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(args[start..].trim());
    out
}

fn strip_parens(t: &str) -> Option<&str> {
    let t = t.trim();
    t.strip_prefix('(')?.strip_suffix(')')
}

/// Parse a `defb`/`defw` value list. A `"..."` string expands to one byte per
/// character. TODO: escape sequences in strings.
fn parse_list<S: Z80Syntax>(syntax: &S, rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.chars().map(|c| Expr::Num(c as i64)));
        } else {
            out.push(parse_value(syntax, piece, line)?);
        }
    }
    Ok(out)
}

/// Split a data list on commas not inside a `"..."` string.
pub(crate) fn split_data_items(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_string = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            ',' if !in_string => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

pub(crate) fn string_literal(piece: &str) -> Option<&str> {
    let p = piece.trim();
    (p.len() >= 2 && p.starts_with('"') && p.ends_with('"')).then(|| &p[1..p.len() - 1])
}

/// Parse an operand value: an arithmetic expression over numbers, symbols, and
/// `+`/`-`/`*`/`/` with C-style precedence, unary `~`, and parentheses. Number literals are
/// lexed by the dialect's [`Z80Syntax::parse_number`].
pub(crate) fn parse_value<S: Z80Syntax>(
    syntax: &S,
    raw: &str,
    line: usize,
) -> Result<Expr, AsmError> {
    let tokens = tokenize(syntax, raw, line)?;
    if tokens.is_empty() {
        return Err(AsmError::new(line, "expected a value"));
    }
    let mut parser = ExprParser {
        tokens,
        pos: 0,
        line,
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
    /// The location counter `$` (statement-start address).
    Pc,
    Plus,
    Minus,
    Star,
    Slash,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    LParen,
    RParen,
    // Condition-only tokens (keyword conditionals, U8): produced only by
    // [`tokenize_cond`], so operand expressions keep rejecting them exactly
    // as before.
    /// `=` or `==` — the reference treats both as equality (probe p2).
    Eq,
    /// `!=`.
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    AndAnd,
    OrOr,
    /// Unary logical not `!`.
    Not,
    /// Unary bitwise complement `~` (accepted by both reference dialects).
    BitNot,
}

/// Lex an operand expression (see [`tokenize_impl`]).
fn tokenize<S: Z80Syntax>(syntax: &S, raw: &str, line: usize) -> Result<Vec<Tok>, AsmError> {
    tokenize_impl(syntax, raw, line, false)
}

/// Lex a keyword `IF` condition: the operand lexer plus the condition
/// operators (`=`/`==`/`!=`/`<`/`>`/`<=`/`>=`/`&&`/`||`/`!`) the reference's
/// conditions accept (probes p2/p45).
fn tokenize_cond<S: Z80Syntax>(syntax: &S, raw: &str, line: usize) -> Result<Vec<Tok>, AsmError> {
    tokenize_impl(syntax, raw, line, true)
}

/// Lex an expression. The number *extent* (a `$`/`%`/`#`/digit start then an
/// alphanumeric run) is shared; the dialect's `parse_number` interprets it,
/// which is where hex/binary format differences live. `cond` admits the
/// condition-only operators; operand expressions (`cond = false`) reject them
/// unchanged.
fn tokenize_impl<S: Z80Syntax>(
    syntax: &S,
    raw: &str,
    line: usize,
    cond: bool,
) -> Result<Vec<Tok>, AsmError> {
    let chars: Vec<char> = raw.chars().collect();
    // Inside a condition every comparison spelling is admitted, as it always
    // was; outside one the dialect's table decides.
    let cmp = if cond {
        crate::dialects::mos6502::Compare::default()
    } else {
        syntax.compare()
    };
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
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
            '/' => {
                tokens.push(Tok::Slash);
                i += 1;
            }
            '&' if cond && chars.get(i + 1) == Some(&'&') => {
                tokens.push(Tok::AndAnd);
                i += 2;
            }
            '&' => {
                tokens.push(Tok::And);
                i += 1;
            }
            '|' if cond && chars.get(i + 1) == Some(&'|') => {
                tokens.push(Tok::OrOr);
                i += 2;
            }
            '|' => {
                tokens.push(Tok::Or);
                i += 1;
            }
            // sjasmplus has `^` (XOR); pasmo does not, so it falls through to the
            // unknown-character error there.
            '^' if syntax.has_xor_operator() => {
                tokens.push(Tok::Xor);
                i += 1;
            }
            '~' => {
                tokens.push(Tok::BitNot);
                i += 1;
            }
            // Conditions: `=` and `==` are both equality (probe p2), `!=` is
            // inequality, a bare `!` is logical not.
            '=' if cond => {
                tokens.push(Tok::Eq);
                i += if chars.get(i + 1) == Some(&'=') { 2 } else { 1 };
            }
            '!' if cond => {
                if chars.get(i + 1) == Some(&'=') {
                    tokens.push(Tok::Ne);
                    i += 2;
                } else {
                    tokens.push(Tok::Not);
                    i += 1;
                }
            }
            '<' if chars.get(i + 1) == Some(&'<') => {
                tokens.push(Tok::Shl);
                i += 2;
            }
            // Outside a condition the dialect's own table decides, because the
            // spellings are not the same: sjasmplus refuses `<>`, pasmo
            // refuses `==` and `<>` (`docs/comparison-operators.md`).
            '=' if cmp.eq_eq && chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Eq);
                i += 2;
            }
            '=' if cmp.eq => {
                tokens.push(Tok::Eq);
                i += 1;
            }
            '!' if cmp.ne_bang && chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Ne);
                i += 2;
            }
            '<' if cmp.ne_angle && chars.get(i + 1) == Some(&'>') => {
                tokens.push(Tok::Ne);
                i += 2;
            }
            '<' if cmp.ordered_eq && chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Le);
                i += 2;
            }
            '<' if cmp.relational => {
                tokens.push(Tok::Lt);
                i += 1;
            }
            // `>>` is the shift and its arm sits below these, so exclude it
            // here — otherwise a shift lexes as two comparisons.
            '>' if cmp.ordered_eq
                && chars.get(i + 1) == Some(&'=')
                && chars.get(i + 1) != Some(&'>') =>
            {
                tokens.push(Tok::Ge);
                i += 2;
            }
            '>' if cmp.relational && chars.get(i + 1) != Some(&'>') => {
                tokens.push(Tok::Gt);
                i += 1;
            }
            '<' if cond => {
                if chars.get(i + 1) == Some(&'=') {
                    tokens.push(Tok::Le);
                    i += 2;
                } else {
                    tokens.push(Tok::Lt);
                    i += 1;
                }
            }
            '>' if chars.get(i + 1) == Some(&'>') => {
                tokens.push(Tok::Shr);
                i += 2;
            }
            '>' if cond => {
                if chars.get(i + 1) == Some(&'=') {
                    tokens.push(Tok::Ge);
                    i += 2;
                } else {
                    tokens.push(Tok::Gt);
                    i += 1;
                }
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
                    tokens.push(Tok::Num(syntax.parse_number(&s, line)?));
                    i += 3;
                } else {
                    return Err(AsmError::new(line, "malformed character literal"));
                }
            }
            // A bare `$` is the location counter; `$` followed by hex digits is
            // a number. Disambiguate on the next character.
            '$' if !chars.get(i + 1).is_some_and(|c| c.is_ascii_alphanumeric()) => {
                tokens.push(Tok::Pc);
                i += 1;
            }
            // A number: a prefix sigil ($/%/#) or a digit, then an alnum run.
            '$' | '%' | '#' => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Tok::Num(syntax.parse_number(&s, line)?));
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Tok::Num(syntax.parse_number(&s, line)?));
            }
            // An identifier: letters, digits, `_`, `.` (not starting with a
            // digit), and — where modules are live — a leading `@`, which
            // names the global scope (probes m9/m30).
            l if l.is_ascii_alphabetic()
                || l == '_'
                || l == '.'
                || (l == '@' && syntax.scopes_modules()) =>
            {
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                tokens.push(Tok::Sym(chars[start..i].iter().collect()));
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

/// Precedence-climbing parser: `add_sub` over `mul_div` over `unary` over
/// `atom`, so `*`/`/` bind tighter than `+`/`-`.
struct ExprParser {
    tokens: Vec<Tok>,
    pos: usize,
    line: usize,
}

impl ExprParser {
    fn expr(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.bit_or()?;
        while let Some(op) = self.compare_op() {
            self.pos += 1;
            let right = self.bit_or()?;
            let cmp = Expr::Bin(op, Box::new(left), Box::new(right));
            // Both Z80 dialects answer `$FF` for true, so the negation is
            // unconditional here rather than a knob.
            left = Expr::Neg(Box::new(cmp));
        }
        Ok(left)
    }

    /// The comparison at the cursor, if there is one.
    fn compare_op(&self) -> Option<BinOp> {
        Some(match self.tokens.get(self.pos)? {
            Tok::Eq => BinOp::Eq,
            Tok::Ne => BinOp::Ne,
            Tok::Lt => BinOp::Lt,
            Tok::Gt => BinOp::Gt,
            Tok::Le => BinOp::Le,
            Tok::Ge => BinOp::Ge,
            _ => return None,
        })
    }

    // Bitwise and shift operators, C-style: `|` loosest, then `^`, `&`, then the
    // shifts, all looser than `+`/`-` (so `1+2<<1` is `(1+2)<<1`). This matches
    // sjasmplus; pasmo binds its shifts tighter than additive, a divergence that
    // only shows on unparenthesised mixed expressions.
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
        let mut left = self.add_sub()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Shl) => BinOp::Shl,
                Some(Tok::Shr) => BinOp::Shr,
                _ => break,
            };
            self.pos += 1;
            let right = self.add_sub()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
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
        if matches!(self.tokens.get(self.pos), Some(Tok::BitNot)) {
            self.pos += 1;
            return Ok(Expr::BitNot(Box::new(self.unary()?)));
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
            Tok::Pc => Ok(Expr::Pc),
            Tok::Sym(s) => Ok(Expr::Sym(s)),
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

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], s[idx..].trim()),
        None => (s, ""),
    }
}

/// An identifier: letters, digits, `_`, and `.` (the last so local-style labels
/// like `.loop` read as ordinary names), not starting with a digit.
/// A label name as this dialect spells it: [`is_ident`], plus the leading `@`
/// that escapes module scoping where modules are live (probe m4).
fn is_label_ident<S: Z80Syntax>(syntax: &S, s: &str) -> bool {
    let s = s.trim();
    match s.strip_prefix('@') {
        Some(rest) if syntax.scopes_modules() => is_ident(rest),
        _ => is_ident(s),
    }
}

fn numeric_label(s: &str) -> Option<&str> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then_some(s)
}

fn temporary_reference(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    for direction in ["F", "f", "B", "b"] {
        if let Some(number) = s.strip_suffix(direction)
            && numeric_label(number).is_some()
        {
            return Some((number, direction));
        }
    }
    None
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
