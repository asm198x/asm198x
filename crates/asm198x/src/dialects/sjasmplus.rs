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
//! TODO: sjasmplus modules, macros, and `DUP`. `ELSEIF` and the dotted
//! conditional spellings landed 2026-08-18; colon-inline blocks and
//! conditions on forward symbols remain open under #67.

use std::collections::BTreeMap;

use crate::dialect::{Dialect, Oversize};
use crate::dialects::z80::{self, Z80Syntax};
use crate::engine::{AsmError, Operation, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The sjasmplus Z80 dialect. `z80n` selects the target instruction set
/// (sjasmplus emits Z80N when targeting the Next).
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
        z80::assemble_keyword(
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
        z80::parse_program_multi_keyword(
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
    fn expand_source(&self, source: &str) -> Result<Option<(String, Vec<usize>)>, AsmError> {
        let expanded = expand_macros(source)?;
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
// Macros (#93, first slice)
//
// Expansion is a source pre-pass: definitions are collected and removed,
// invocations are replaced by their substituted bodies, and the result goes
// through the ordinary parse unchanged. Only sjasmplus uses the keyword
// pipeline, so this reaches no other dialect.
//
// Every rule below was measured against sjasmplus 1.21.0 rather than read from
// its manual, and two of them are not what a reasonable person would guess:
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

/// The dot-prefixed local labels a macro body **defines**.
///
/// These scope to the expansion rather than to the file, which is what lets a
/// macro containing a loop be invoked more than once — most of what macros are
/// for. A *plain* label in the same position stays global and collides on the
/// second invocation; the reference reports `Duplicate label` there and so do
/// we, which is why only the dotted ones are renamed.
///
/// Only names the body defines are renamed, so a body referring to a local
/// defined outside it still refers to that one.
fn defined_locals(body: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for line in body {
        let text = without_comment(line);
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

/// A macro as collected by the pre-pass.
struct MacroDef {
    /// Parameter names, in order. Matched case-sensitively.
    params: Vec<String>,
    /// Body lines, verbatim, before substitution.
    body: Vec<String>,
}

/// Strip a trailing comment, respecting string literals so a `;` inside quotes
/// is not mistaken for one.
fn without_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == b'"' || c == b'\'' {
                    quote = Some(c);
                } else if c == b';' || (c == b'/' && bytes.get(i + 1) == Some(&b'/')) {
                    // sjasmplus takes both spellings.
                    return &line[..i];
                }
            }
        }
        i += 1;
    }
    line
}

/// `MACRO name [p1[, p2]...]` — the keyword matched case-insensitively, the
/// name kept as written.
fn macro_header(line: &str) -> Option<(String, Vec<String>)> {
    let text = without_comment(line).trim();
    let (kw, rest) = text.split_once(char::is_whitespace)?;
    if !kw.eq_ignore_ascii_case("macro") {
        return None;
    }
    let rest = rest.trim();
    let (name, params) = match rest.split_once(|c: char| c == ',' || c.is_whitespace()) {
        Some((name, tail)) => (
            name.trim(),
            tail.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
        ),
        None => (rest, Vec::new()),
    };
    (!name.is_empty()).then(|| (name.to_string(), params))
}

/// `ENDM`, alone on its line.
fn is_endm(line: &str) -> bool {
    without_comment(line).trim().eq_ignore_ascii_case("endm")
}

/// Replace whole-word occurrences of each parameter with its argument, leaving
/// string literals untouched.
fn substitute(line: &str, params: &[String], args: &[String]) -> String {
    if params.is_empty() {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.';
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            out.push(c as char);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = Some(c);
            out.push(c as char);
            i += 1;
            continue;
        }
        if word(c) && (i == 0 || !word(bytes[i - 1])) {
            let mut j = i;
            while j < bytes.len() && word(bytes[j]) {
                j += 1;
            }
            let token = &line[i..j];
            match params.iter().position(|p| p == token) {
                Some(k) => out.push_str(args.get(k).map_or("", String::as_str)),
                None => out.push_str(token),
            }
            i = j;
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// Split an invocation's argument list on commas outside strings.
fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.push(c);
            }
            None if c == ',' => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            None => current.push(c),
        }
    }
    let last = current.trim();
    if !last.is_empty() || !args.is_empty() {
        args.push(last.to_string());
    }
    args
}

/// Expanded source, plus the source line each output line should report as.
pub(crate) struct Expanded {
    pub(crate) text: String,
    /// `origins[i]` is the 1-based source line for output line `i + 1`. Lines
    /// from an expansion report their **invocation** site, which is where a
    /// reader looks first and what a diagnostic must name.
    pub(crate) origins: Vec<usize>,
}

/// Collect macro definitions and expand their invocations.
pub(crate) fn expand_macros(source: &str) -> Result<Expanded, AsmError> {
    let lines: Vec<&str> = source.lines().collect();
    let mut macros: std::collections::HashMap<String, MacroDef> = std::collections::HashMap::new();
    let mut text = String::with_capacity(source.len());
    let mut origins = Vec::with_capacity(lines.len());
    let mut expansions = 0usize;
    let mut i = 0;

    while i < lines.len() {
        let raw = lines[i];
        let line_no = i + 1;

        if let Some((name, params)) = macro_header(raw) {
            let mut body = Vec::new();
            let mut j = i + 1;
            let mut closed = false;
            while j < lines.len() {
                if is_endm(lines[j]) {
                    closed = true;
                    break;
                }
                body.push(lines[j].to_string());
                j += 1;
            }
            if !closed {
                return Err(AsmError::new(
                    line_no,
                    format!("`macro {name}` has no matching `endm`"),
                ));
            }
            macros.insert(name, MacroDef { params, body });
            i = j + 1;
            continue;
        }

        // An invocation: the first token names a macro. Case-sensitive, as
        // sjasmplus is for macro names though not for its keywords.
        let stripped = without_comment(raw).trim();
        let (head, tail) = stripped
            .split_once(char::is_whitespace)
            .unwrap_or((stripped, ""));
        if let Some(def) = macros.get(head) {
            let args = split_args(tail);
            if args.len() != def.params.len() {
                return Err(AsmError::new(
                    line_no,
                    format!(
                        "macro `{head}` takes {} argument(s), but {} given",
                        def.params.len(),
                        args.len()
                    ),
                ));
            }
            // Each expansion gets its own copy of the locals it defines.
            expansions += 1;
            let locals = defined_locals(&def.body);
            let renamed: Vec<String> = locals
                .iter()
                .map(|name| format!("{name}__{expansions}"))
                .collect();
            for body_line in &def.body {
                let with_args = substitute(body_line, &def.params, &args);
                text.push_str(&substitute(&with_args, &locals, &renamed));
                text.push('\n');
                origins.push(line_no);
            }
            i += 1;
            continue;
        }

        text.push_str(raw);
        text.push('\n');
        origins.push(line_no);
        i += 1;
    }
    Ok(Expanded { text, origins })
}

#[cfg(test)]
mod tests {
    use crate::assemble_sjasmplus as asm;

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

    /// Arity is checked, and unterminated definitions are caught where they
    /// begin rather than at end of file.
    #[test]
    fn arity_and_termination_are_checked() {
        let arity = asm(" MACRO m v\n ld a,v\n ENDM\n m\n").expect_err("too few arguments");
        assert!(arity.message.contains("takes 1 argument"), "{arity:?}");
        let open = asm(" MACRO m\n nop\n").expect_err("no endm");
        assert_eq!(open.line, 1, "reported where the definition opened");
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
}
