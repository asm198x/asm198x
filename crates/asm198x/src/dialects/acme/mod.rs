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
//! Includes and binary inclusion (language-surface U4) resolve **inside the
//! evaluation walk**: `!src`/`!source` and `!bin`/`!binary` are recognised by
//! [`AcmeEval::lower`], so an include in an untaken conditional branch never
//! loads (KTD1), the environment (`=` constants, `!set` variables, the
//! conditional bindings) threads through the included file and back out, and
//! anonymous `-`/`+` labels are collected in **spliced evaluation order**
//! across files — not by textual position over any single source string.
//! Probe-pinned semantics (acme 0.97): `!bin "file"[, [size][, [skip]]]` with
//! size *then* skip, zero-padding (never an error) when the size exceeds the
//! available data, negative skip reading from the start, and a negative size
//! rejected; a forward `+` reference never matches a definition on its own
//! line, while a backward `-` reference does.
//!
//! One deliberate deviation, on our own CLI surface rather than the
//! directive's semantics: acme resolves a quoted relative `!src`/`!bin`
//! against the **process working directory** only (then `-I`), never the
//! including file's directory. Our loader never consults the process cwd
//! (the [`crate::source::FsLoader`] contract); it anchors at the requesting
//! file's directory first, then the `-I` dirs — identical in the canonical
//! run-from-the-project-directory layout. The `<file>` library spelling
//! (acme: the `ACME` environment variable only) resolves through the same
//! loader order instead.
//!
//! Local labels are **zone-scoped** (language-surface U7, probe-pinned against
//! acme 0.97): a leading-`.` name is local to the current `!zone`, and zones
//! are the *only* delimiter — a global label does NOT end a local's scope
//! (probe z3; the acme-vs-sjasmplus divergence). Every `!zone [title]`
//! directive mints a **fresh** scope: the title is cosmetic (error-message
//! display), so re-entering a title is a new zone (probe z12b). The block form
//! `!zone [title] { … }` resumes the enclosing zone at `}` (probe z6b); the
//! line form switches for good — even from inside a taken conditional branch
//! (probe ze), while a `!zone` in an untaken branch never runs (probe zd).
//! Zone state threads through `!src` like the rest of the environment: an
//! include inherits the includer's zone, and a `!zone` inside it persists
//! after return (probes za/zb). `.name = expr` constants, `!set .name`
//! variables, and `!ifdef .name` tests are all zone-scoped (probes z16, zh6,
//! zh7). Qualification happens in the evaluation walk — zones are runtime
//! state (a conditional can skip a `!zone`), so the source-preserving tree
//! keeps source names and [`parse_value`] rewrites `.name` to its qualified
//! key via the shared [`crate::ast::qualify_expr`] (the U7 consolidation; see
//! the audit note in `crate::ast`). Qualified keys are `{title}@{ordinal}.name`
//! (`@{ordinal}.name` for untitled zones; the initial zone keeps the bare
//! `.name`, preserving zone-free programs' public symbol keys) — acme itself
//! never lists zone-locals in its symbol files, so the scheme is our own
//! surface, and `@` is not producible in an acme identifier, so keys cannot
//! collide with user globals.
//!
//! Not yet covered (no curriculum use): `@cheap` locals.

use std::collections::{BTreeMap, BTreeSet};

use super::macros;
use super::mos6502::{
    self, BytePrec, assignment_split, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, string_literal, top_level_rfind,
};
use crate::dialect::Dialect;
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, Expr, Operation, OutputFormat, Statement, Warning};
use crate::source::{MAX_INCLUDE_DEPTH, SourceLoader, SourceMap};
use crate::span::FileId;

mod evaluate;
use evaluate::{AcmeEval, AcmeTarget, ConvTable, MultiCx, strip_comment};
mod scope;
use scope::{Anons, anon_marker, anon_ref_placeholder, substitute_anon_refs};

/// The ACME 6502 dialect.
pub(crate) struct Acme;

impl Dialect for Acme {
    /// Every instruction lowers by form; the only piece-encoded emissions
    /// are data directives, so an absent cycle record means data (#497).
    fn cycle_coverage(&self) -> crate::dialect::CycleCoverage {
        crate::dialect::CycleCoverage::Full
    }
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::mos6502::SET
    }

    /// ACME requires `*=` before any code or data (it rejects an implicit
    /// origin), so a forgotten `*=` errors rather than assembling at `$0000`.
    fn requires_explicit_origin(&self) -> bool {
        true
    }

    fn org_starts_address_run(&self) -> bool {
        true
    }

    fn later_run_overwrites(&self) -> bool {
        true
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Idea 4: assemble by **evaluating the shared conditional AST** — the same
        // source-preserving tree the formatter parses — rather than a separate
        // brace preprocessor. `evaluate` walks the tree, prunes untaken branches,
        // threads `env`, bakes `!set`, and lowers each line through
        // `parse_statement`. This retires `tokenize_braces`/`process_block`; the
        // conditional now lives in the tree, not a second parse. No loader here:
        // a `!src`/`!bin` on this single-source path is an error pointing at the
        // multi-file entry points.
        let program = parse_program(source, macros::Expand::Yes)?;
        let mut eval = AcmeEval::new(self.instruction_set(), None);
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        eval.resolve_anon_refs(&mut out)?;
        Ok(out)
    }

    /// The advisories are ACME's own: an instruction that sized absolute
    /// because its operand was still unknown, and whose value turned out to
    /// fit a byte.
    fn parse_warned(&self, source: &str) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        let program = parse_program(source, macros::Expand::Yes)?;
        let mut eval = AcmeEval::new(self.instruction_set(), None);
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        eval.resolve_anon_refs(&mut out)?;
        let warnings = eval.oversized_warnings();
        Ok((out, warnings))
    }

    /// The include-capable parse (language-surface U4): the same evaluation
    /// walk as [`parse`](Self::parse), with a loader wired in — `!src` and
    /// `!bin` resolve *live* inside the walk (an untaken branch never loads,
    /// KTD1), the environment threads through included files and back out,
    /// and anonymous labels collect in spliced evaluation order.
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        let root = map
            .contents(FileId(0))
            .map(str::to_owned)
            .unwrap_or_default();
        let program = parse_program_in(FileId(0), &root, macros::Expand::No)?;
        let mut eval = AcmeEval::new(
            self.instruction_set(),
            Some(MultiCx {
                map,
                loader,
                stack: vec![FileId(0)],
            }),
        );
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        eval.resolve_anon_refs(&mut out)?;
        Ok(out)
    }

    /// [`parse_multi`](Self::parse_multi) with the same advisories.
    fn parse_multi_warned(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        let root = map
            .contents(FileId(0))
            .map(str::to_owned)
            .unwrap_or_default();
        let program = parse_program_in(FileId(0), &root, macros::Expand::No)?;
        let mut eval = AcmeEval::new(
            self.instruction_set(),
            Some(MultiCx {
                map,
                loader,
                stack: vec![FileId(0)],
            }),
        );
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        eval.resolve_anon_refs(&mut out)?;
        let warnings = eval.oversized_warnings();
        Ok((out, warnings))
    }

    /// The formatter parses through the same source-preserving front-end
    /// (`parse_program`) the assembler now uses — conditional blocks as
    /// `Item::Conditional`, every other line's verbatim operation source. `emit`
    /// reformats this tree; `parse` evaluates it.
    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        // The formatter must not expand — see `parse_program_in`.
        Ok(Some(parse_program(source, macros::Expand::No)?))
    }

    /// ACME binds constants with `name = value` (no colon), so the formatter
    /// emits the label without one — and re-aligns runs of them (the ruling).
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Split a line into its code and its `;` comment (delimiter kept, trailing
/// whitespace trimmed) for carrying comments as AST trivia; defined via
/// [`strip_comment`] so the comment is exactly what it removes.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
}

/// Whether a column-zero word can be an instruction in any ACME processor
/// profile this front-end implements. This parse runs before lexical `!cpu`
/// directives are evaluated; the active-profile check still happens later.
fn is_known_acme_mnemonic(base: &'static isa::InstructionSet, word: &str) -> bool {
    let mnemonic = word.to_ascii_uppercase();
    base.has_mnemonic(&mnemonic)
        || [
            &isa::nmos6502_undocumented::SET,
            &isa::mos65c02::SET,
            &isa::mos65c02::ROCKWELL_SET,
            &isa::mos65c02::WDC_SET,
            &isa::c64dtv2::SET,
            &isa::csg65ce02::SET,
            &isa::csg65ce02::CSG4502_SET,
            &isa::mos65816::SET,
        ]
        .iter()
        .any(|set| set.has_mnemonic(&mnemonic))
}

/// Split ACME's `:`-separated statements without mistaking a column-zero
/// label suffix or a colon inside a quoted literal for a separator.
fn split_statements(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let (mut in_char, mut in_str) = (false, false);
    let mut start = 0usize;
    let mut parts = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b':' if !in_char && !in_str => {
                let label_suffix =
                    start == 0 && !line.starts_with([' ', '\t']) && is_ident(line[..i].trim());
                if !label_suffix {
                    parts.push(&line[start..i]);
                    start = i + 1;
                }
            }
            _ => {}
        }
    }
    parts.push(&line[start..]);
    parts
}

// ---------------------------------------------------------------------------
// Source-preserving parse — the single ACME front-end (U6 / idea 4).
//
// `parse_program` keeps the source structure: conditional blocks as
// `Item::Conditional`, every other line as a flat node carrying its verbatim
// operation source. It does **not** evaluate — no branch pruning, no `!set`
// baking, no anonymous-label resolution. Both consumers run off this one tree:
// `emit` reformats it to the canonical layout (see
// `decisions/formatter-canonical-style.md`), and `evaluate` (below) assembles
// it — pruning branches and threading `env`. This is idea 4: the conditional
// lives in the tree, replacing the old brace preprocessor.
// ---------------------------------------------------------------------------

/// How a [`parse_block`](FmtCx::parse_block) ended.
#[derive(PartialEq, Eq, Clone)]
enum Closer {
    Eof,
    /// End of input reached because `!eof` said so, as distinct from running
    /// out of lines. The two are the same thing to the *file* and different
    /// things to an open block: ACME answers `!eof` inside an `!if` with
    /// "Found end-of-file instead of '}'", so the block parser has to be able
    /// to tell them apart.
    EofDirective,
    Brace,
    BraceElse,
    /// `} while <cond>` or `} until <cond>` — the tail of an ACME `!do` loop,
    /// whose condition lives on the closing line rather than the opening one.
    /// `invert` is the `until` spelling: run *until* it holds.
    BraceLoop {
        cond: String,
        invert: bool,
    },
}

/// The formatter parse cursor.
struct FmtCx<'a> {
    set: &'static isa::InstructionSet,
    /// The file every node's span points into — `FileId(0)` for the root /
    /// single-source parse, the include's own id in the multi-file walk.
    file: FileId,
    lines: Vec<&'a str>,
    pos: usize,
    /// Own-line comments seen since the last node, attached as leading trivia.
    pending: Vec<crate::ast::Comment>,
}

/// Parse ACME source into the source-preserving formatter AST (the root /
/// single-source form: spans point into `FileId(0)`).
pub(crate) fn parse_program(
    source: &str,
    mode: macros::Expand,
) -> Result<crate::ast::Program, AsmError> {
    parse_program_in(FileId(0), source, mode)
}

/// Parse one file of a multi-file ACME program: as [`parse_program`], with
/// every span minted in `file` so diagnostics and line records name the
/// include they came from (language-surface U4).
fn parse_program_in(
    file: FileId,
    source: &str,
    mode: macros::Expand,
) -> Result<crate::ast::Program, AsmError> {
    // Macros expand before parsing (#93), but only for assembly: the formatter
    // asks with `Expand::No`, because laying source out must not replace a
    // definition with its expansions.
    let expanded = expand_acme(source, mode)?;
    let text = macros::expanded_text(&expanded, source);
    let origins = macros::line_origins(&expanded);
    let mut cx = FmtCx {
        set: &isa::mos6502::SET,
        file,
        lines: text.lines().collect(),
        pos: 0,
        pending: Vec::new(),
    };
    let (mut nodes, closer) = cx
        .parse_block()
        .map_err(|e| macros::remap_lines(e, origins))?;
    if !matches!(closer, Closer::Eof | Closer::EofDirective) {
        return Err(AsmError::new(cx.pos, "unbalanced `}` in conditional block"));
    }
    // Flush a trailing comment block so the formatter keeps it.
    let last = cx.lines.len();
    cx.flush_pending(&mut nodes, last);
    macros::place_nodes(&mut nodes, origins);
    Ok(crate::ast::Program { nodes })
}

impl<'a> FmtCx<'a> {
    /// A span at `line:col` in this parse's file.
    fn at(&self, line: usize, col: u32) -> crate::ast::Span {
        crate::ast::Span::in_file(self.file, line as u32, col)
    }

    /// Stamp this parse's file onto a computed operand/token span (the shared
    /// helpers mint `FileId(0)`).
    fn patch(&self, span: Option<crate::ast::Span>) -> Option<crate::ast::Span> {
        span.map(|mut s| {
            s.file = self.file;
            s
        })
    }

    /// Parse a run of nodes until a block close (`}`, `} else {`) or EOF.
    fn parse_block(&mut self) -> Result<(Vec<crate::ast::Node>, Closer), AsmError> {
        let mut nodes = Vec::new();
        // `!zone { … }` blocks opened (and not yet closed) inside *this* block
        // level: their `}` becomes a marker node, not a block closer (U7).
        let mut zone_depth = 0usize;
        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            let line = self.pos + 1;
            let (code, comment) = split_comment(raw);
            let trimmed = code.trim();

            if trimmed.is_empty() {
                match comment {
                    // An own-line comment becomes leading trivia of the next node.
                    Some(text) => self.pending.push(crate::ast::Comment {
                        text: text.to_string(),
                        span: self.at(line, 1),
                    }),
                    // A blank line is preserved as an empty-text marker (emit
                    // renders it as a blank line), collapsing consecutive blanks
                    // to one. Preserving blanks keeps constant-run boundaries
                    // stable across re-formats (idempotence) and respects the
                    // author's visual grouping.
                    None => {
                        let last_blank =
                            matches!(self.pending.last(), Some(c) if c.text.is_empty());
                        if !last_blank {
                            self.pending.push(crate::ast::Comment {
                                text: String::new(),
                                span: self.at(line, 1),
                            });
                        }
                    }
                }
                self.pos += 1;
                continue;
            }

            // `!eof` ends this *file* here — the lines after it are not
            // parsed at all, so a malformed one is never seen. Stopping the
            // scan is the whole implementation, and it gives ACME's three
            // behaviours at once: at the top level the parse ends cleanly; an
            // included file stops while its parent carries on (each file is
            // its own parse); and inside an open `!if` the enclosing block
            // reaches end-of-input where it wanted `}`, which is the error
            // ACME reports as "Found end-of-file instead of '}'".
            {
                let word = split_first_word(trimmed).0.to_ascii_lowercase();
                if word == "!eof" || word == "!endoffile" {
                    let rest = split_first_word(trimmed).1.trim();
                    if !rest.is_empty() {
                        return Err(AsmError::new(
                            line,
                            format!("garbage data at end of statement: `{rest}`"),
                        ));
                    }
                    self.pos = self.lines.len();
                    return Ok((nodes, Closer::EofDirective));
                }
            }

            // A block close: `}`, `} else {`, `} else` — or, when a `!zone`
            // block is open at this level, its close, kept as a marker node so
            // the evaluator restores the enclosing zone (probe z6b) and the
            // formatter re-renders it.
            if let Some(rest) = trimmed.strip_prefix('}') {
                let rest = rest.trim();
                self.pos += 1;
                // Flush comments/blanks pending at the block's end into *this*
                // block, so a trailing comment stays inside the branch it closes
                // rather than leaking onto the next one (across `} else {`).
                self.flush_pending(&mut nodes, line);
                if zone_depth > 0 {
                    if !rest.is_empty() {
                        return Err(AsmError::new(
                            line,
                            format!("unexpected `{trimmed}` closing a `!zone` block"),
                        ));
                    }
                    zone_depth -= 1;
                    nodes.push(self.op_node(
                        None,
                        None,
                        "}".to_string(),
                        Vec::new(),
                        comment,
                        line,
                    ));
                    continue;
                }
                if rest.is_empty() {
                    return Ok((nodes, Closer::Brace));
                }
                if let Some(after) = rest.strip_prefix("else")
                    && (after.trim().is_empty() || after.trim() == "{")
                {
                    return Ok((nodes, Closer::BraceElse));
                }
                // `} while <cond>` / `} until <cond>`: an ACME `!do` loop
                // testing after the body, so the body always runs once.
                for (word, invert) in [("while", false), ("until", true)] {
                    if let Some(cond) = strip_word_ci(rest, word) {
                        let cond = cond.trim();
                        if cond.is_empty() {
                            return Err(AsmError::new(line, format!("`{word}` needs a condition")));
                        }
                        return Ok((
                            nodes,
                            Closer::BraceLoop {
                                cond: cond.to_string(),
                                invert,
                            },
                        ));
                    }
                }
                return Err(AsmError::new(line, format!("unexpected `{trimmed}`")));
            }

            // A `!macro` definition is copied, not read. A body is a template
            // rather than code — `.v` is a parameter and `+other` is a call, so
            // neither is an operand this parse could lay out — and acme
            // delimits one at character level, so the copy counts braces the
            // way the expander does instead of looking for a keyword. See
            // `Item::Verbatim`.
            //
            // The formatter and the multi-file assembly parse reach here with
            // a definition intact. The latter registers these copied nodes in
            // the live evaluation walk, preserving textual `!source` order.
            if is_macro_head(trimmed) {
                let mut leading = std::mem::take(&mut self.pending);
                let mut depth = 0usize;
                let mut closed = false;
                while self.pos < self.lines.len() {
                    let raw = self.lines[self.pos];
                    let line = self.pos + 1;
                    let (code, comment) = split_comment(raw);
                    // A brace inside a string closes nothing, which is why this
                    // is `close_brace` and not a byte count.
                    closed = close_brace(code, &mut depth).is_some();
                    nodes.push(self.verbatim_node(
                        code,
                        std::mem::take(&mut leading),
                        comment,
                        line,
                    ));
                    self.pos += 1;
                    if closed {
                        break;
                    }
                }
                if !closed {
                    // acme: "Found end-of-file instead of '}'".
                    return Err(AsmError::new(
                        self.pos,
                        "unterminated `!macro` definition (missing `}`)",
                    ));
                }
                continue;
            }

            // A conditional head opens a block (one-line or multi-line).
            if is_conditional_head(trimmed) {
                let node = self.parse_conditional(trimmed, comment, line)?;
                nodes.push(node);
                continue;
            }

            // A `!for` block. Unlike `!macro` its body **is** code — it
            // assembles once per iteration — so it is parsed here rather than
            // copied, and unlike `!zone` it is a repetition, so it becomes an
            // `Item::Repeat` the shared walk runs.
            if let Some(open) = for_block_open(trimmed) {
                let leading = std::mem::take(&mut self.pending);
                let head = trimmed[..open].trim().to_string();
                let after = trimmed[open + 1..].trim();
                // One-line form: `!for i, 1, 2 { !byte i }`, which acme takes.
                // Depth-matched, not first-brace: a nested `!for` on the same
                // line closes twice and the inner `}` is not this block's.
                let mut depth = 1usize;
                if let Some(close) = close_brace(after, &mut depth) {
                    let body_text = after[..close].trim();
                    let tail = after[close + 1..].trim();
                    if !tail.is_empty() {
                        return Err(AsmError::new(
                            line,
                            format!("unexpected `{tail}` after the `!for` block's `}}`"),
                        ));
                    }
                    // Parsed as a *block*, not a line: the body may itself be
                    // a one-line `!for`, and `parse_line` knows no blocks.
                    let mut body = Vec::new();
                    if !body_text.is_empty() {
                        let mut sub = FmtCx {
                            set: self.set,
                            file: self.file,
                            lines: vec![body_text],
                            pos: 0,
                            pending: Vec::new(),
                        };
                        let (mut inner, _) = sub.parse_block()?;
                        for node in &mut inner {
                            node.span.line = line as u32;
                            if let Some(span) = node.operand_span.as_mut() {
                                span.line = line as u32;
                            }
                        }
                        body = inner;
                    }
                    self.pos += 1;
                    nodes.push(self.repeat_node(head, body, leading, comment, line));
                    continue;
                }
                self.pos += 1;
                if !after.is_empty() {
                    return Err(AsmError::new(
                        line,
                        format!("unexpected `{after}` after the `!for` block's `{{`"),
                    ));
                }
                let (body, closer) = self.parse_block()?;
                if closer != Closer::Brace {
                    return Err(AsmError::new(line, "`!for` block is never closed"));
                }
                nodes.push(self.repeat_node(head, body, leading, comment, line));
                continue;
            }

            // ACME's condition loops, in all five spellings ACME takes:
            //
            //   !while c { … }        test first
            //   !do while c { … }     test first
            //   !do until c { … }     test first, inverted
            //   !do { … } while c     test after, so the body always runs
            //   !do { … } until c     test after, inverted
            //
            // The head forms close on a plain `}`; the tail forms carry their
            // condition on it, which is what `Closer::BraceLoop` is for.
            if let Some(open) = loop_block_open(trimmed) {
                let leading = std::mem::take(&mut self.pending);
                let head = trimmed[..open].trim();
                let after = trimmed[open + 1..].trim();
                if !after.is_empty() {
                    return Err(AsmError::new(
                        line,
                        format!("unexpected `{after}` after the loop's `{{`"),
                    ));
                }
                self.pos += 1;
                let (body, closer) = self.parse_block()?;
                let (cond, invert, test_first) = match loop_head_condition(head, line)? {
                    // A head condition: the closer must be a plain `}`.
                    Some((cond, invert)) => {
                        if !matches!(closer, Closer::Brace) {
                            return Err(AsmError::new(
                                line,
                                "this loop states its condition twice",
                            ));
                        }
                        (cond, invert, true)
                    }
                    // A bare `!do {`: the condition is on the closing line.
                    None => match closer {
                        Closer::BraceLoop { cond, invert } => (cond, invert, false),
                        _ => {
                            return Err(AsmError::new(
                                line,
                                "`!do {` needs a `} while <cond>` or `} until <cond>`",
                            ));
                        }
                    },
                };
                // Stored as a conditional *head* in ACME's own spelling, not
                // as a bare expression: the shared walk asks the dialect to
                // evaluate it, and every dialect spells its conditions its own
                // way. `Item::Conditional` carries its head for the same
                // reason.
                nodes.push(self.loop_node(
                    format!("!if {cond}"),
                    head.to_string(),
                    invert,
                    test_first,
                    body,
                    leading,
                    comment,
                    line,
                ));
                continue;
            }

            // A `!zone [title] {` head opens a zone block (U7, probes
            // zh1-zh3/zh8): unlike a conditional there is no branch to prune,
            // so the head and its `}` stay in the tree as verbatim marker
            // nodes (the evaluator switches/restores the zone; the formatter
            // re-renders them) with the body parsed inline between them.
            if let Some(open) = marker_block_open(trimmed) {
                let leading = std::mem::take(&mut self.pending);
                let head = trimmed[..open].trim();
                let after = trimmed[open + 1..].trim();
                // One-line form: `!zone t { body }` (probe zh1).
                if let Some(close) = find_top(after, b'}') {
                    let body_text = after[..close].trim();
                    let tail = after[close + 1..].trim();
                    if !tail.is_empty() {
                        return Err(AsmError::new(
                            line,
                            format!("unexpected `{tail}` after the block's `}}`"),
                        ));
                    }
                    nodes.push(self.op_node(None, None, format!("{head} {{"), leading, None, line));
                    if !body_text.is_empty() {
                        nodes.extend(self.parse_statements(body_text, None, line, Vec::new())?);
                    }
                    nodes.push(self.op_node(
                        None,
                        None,
                        "}".to_string(),
                        Vec::new(),
                        comment,
                        line,
                    ));
                    self.pos += 1;
                    continue;
                }
                // Multi-line: the head node, then the body until its `}`.
                nodes.push(self.op_node(None, None, format!("{head} {{"), leading, comment, line));
                if !after.is_empty() {
                    nodes.extend(self.parse_statements(after, None, line, Vec::new())?);
                }
                zone_depth += 1;
                self.pos += 1;
                continue;
            }

            // An ordinary line.
            let leading = std::mem::take(&mut self.pending);
            nodes.extend(self.parse_statements(code, comment, line, leading)?);
            self.pos += 1;
        }
        if zone_depth > 0 {
            // acme: "Found end-of-file instead of '}'" (probe zh5).
            return Err(AsmError::new(
                self.pos,
                "unterminated `!zone` block (missing `}`)",
            ));
        }
        Ok((nodes, Closer::Eof))
    }

    /// Parse a conditional block from the head line at `self.pos`. Handles the
    /// one-line guard (`!ifndef X { X = 0 }`) and the multi-line `!if … {` … `}`
    /// (with optional `} else {`).
    fn parse_conditional(
        &mut self,
        trimmed: &str,
        comment: Option<&str>,
        line: usize,
    ) -> Result<crate::ast::Node, AsmError> {
        let leading = std::mem::take(&mut self.pending);
        let open =
            find_top(trimmed, b'{').ok_or_else(|| AsmError::new(line, "conditional needs `{`"))?;
        let head = trimmed[..open].trim().to_string();
        let after = trimmed[open + 1..].trim();

        // One-line guard: `{ body }` closed on the same line.
        if let Some(close) = find_top(after, b'}') {
            let body_text = after[..close].trim();
            let then_body = if body_text.is_empty() {
                Vec::new()
            } else {
                self.parse_statements(body_text, None, line, Vec::new())?
            };
            self.pos += 1;
            return Ok(self.conditional_node(head, then_body, None, true, leading, comment, line));
        }

        // Multi-line: the body starts on the following line.
        self.pos += 1;
        let (then_body, closer) = self.parse_block()?;
        // A conditional body must be closed. Running out of input instead —
        // whether the file simply ended or `!eof` ended it — is ACME's "Found
        // end-of-file instead of '}'", and was silently accepted here before:
        // an unclosed `!if 1 {` assembled its body and emitted bytes.
        let eof_in_block = |c: &Closer| matches!(c, Closer::Eof | Closer::EofDirective);
        if eof_in_block(&closer) {
            return Err(AsmError::new(line, "found end-of-file instead of `}`"));
        }
        let else_body = if closer == Closer::BraceElse {
            let (eb, eb_closer) = self.parse_block()?;
            if eof_in_block(&eb_closer) {
                return Err(AsmError::new(line, "found end-of-file instead of `}`"));
            }
            Some(eb)
        } else {
            None
        };
        Ok(self.conditional_node(head, then_body, else_body, false, leading, comment, line))
    }

    /// Build the flat nodes from one physical line. ACME treats a top-level
    /// colon as a statement separator; leading trivia belongs to the first
    /// statement and the line comment to the last.
    fn parse_statements(
        &self,
        code: &str,
        comment: Option<&str>,
        line: usize,
        leading: Vec<crate::ast::Comment>,
    ) -> Result<Vec<crate::ast::Node>, AsmError> {
        let parts: Vec<&str> = split_statements(code)
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .collect();
        let last = parts.len().saturating_sub(1);
        let mut leading = Some(leading);
        parts
            .into_iter()
            .enumerate()
            .map(|(i, part)| {
                // A separator begins a fresh statement column. ACME accepts a
                // label here (with its usual non-leftmost warning), so leading
                // spacing after `:` must not hide it from label recognition.
                let part = if i == 0 { part } else { part.trim_start() };
                self.parse_line(
                    part,
                    (i == last).then_some(comment).flatten(),
                    line,
                    leading.take().unwrap_or_default(),
                )
            })
            .collect()
    }

    /// Build one flat node from an ordinary statement: its optional (column-0) label,
    /// its verbatim operation source, and trivia. Mirrors `parse_statement`'s
    /// label rules but keeps source rather than lowering.
    fn parse_line(
        &self,
        code: &str,
        comment: Option<&str>,
        line: usize,
        leading: Vec<crate::ast::Comment>,
    ) -> Result<crate::ast::Node, AsmError> {
        let trimmed = code.trim();
        // The original source line, so operand columns stay file-accurate even
        // when `code` is a mid-line slice (an inline conditional body). Every
        // slice below borrows from it (contract U3).
        let raw = self.lines[line - 1];
        let at_line = line as u32;

        // `*= expr` / `* = expr` — a program-counter set (no label).
        if let Some(rest) = trimmed.strip_prefix('*') {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                let src = format!("*= {}", value.trim());
                let span = self.patch(crate::ast::token_span(raw, value, at_line));
                return Ok(self.op_node(span, None, src, leading, comment, line));
            }
        }

        // `name = expr` — a constant binding (a lone `=`), kept on the label line.
        if let Some(eq) = top_level_lone_eq(trimmed) {
            let name = trimmed[..eq].trim();
            if is_ident(name) {
                let src = format!("= {}", trimmed[eq + 1..].trim());
                let span = self.patch(crate::ast::token_span(raw, &trimmed[eq + 1..], at_line));
                return Ok(self.equ_node(span, name, src, leading, comment, line));
            }
        }

        // A column-0 token may be a label; a leading-whitespace line is all op.
        if !code.starts_with([' ', '\t']) {
            let (word, rest) = split_first_word(trimmed);
            let span = self.patch(crate::ast::operand_span(raw, rest, at_line));
            if anon_marker(word).is_some() {
                return Ok(self.labeled_node(span, word, rest.trim(), leading, comment, line));
            }
            if let Some(name) = word.strip_suffix(':')
                && is_ident(name)
            {
                return Ok(self.labeled_node(span, name, rest.trim(), leading, comment, line));
            }
            if !word.starts_with('!') && !is_known_acme_mnemonic(self.set, word) && is_ident(word) {
                return Ok(self.labeled_node(span, word, rest.trim(), leading, comment, line));
            }
        }

        // No label: an instruction or `!` directive, kept verbatim.
        let span = self.patch(crate::ast::operand_span(raw, trimmed, at_line));
        Ok(self.op_node(span, None, trimmed.to_string(), leading, comment, line))
    }

    // --- node builders ------------------------------------------------------

    fn trailing(
        &self,
        comment: Option<&str>,
        line: usize,
        col: u32,
    ) -> Option<crate::ast::Comment> {
        comment.map(|text| crate::ast::Comment {
            text: text.to_string(),
            span: self.at(line, col),
        })
    }

    fn equ_node(
        &self,
        operand_span: Option<crate::ast::Span>,
        name: &str,
        source: String,
        leading: Vec<crate::ast::Comment>,
        comment: Option<&str>,
        line: usize,
    ) -> crate::ast::Node {
        crate::ast::Node {
            operand_span,
            label: Some(global(name)),
            // A placeholder value: the formatter reads only `source`; this tree is
            // never lowered (ACME assembles via its preprocessor).
            item: Some(crate::ast::item_from_operation(Operation::Equ(Expr::Num(
                0,
            )))),
            source,
            span: self.at(line, 1),
            trivia: crate::ast::Trivia {
                leading,
                trailing: self.trailing(comment, line, 1),
            },
        }
    }

    /// A line with a column-0 label and (optionally) an operation after it.
    fn labeled_node(
        &self,
        operand_span: Option<crate::ast::Span>,
        name: &str,
        op: &str,
        leading: Vec<crate::ast::Comment>,
        comment: Option<&str>,
        line: usize,
    ) -> crate::ast::Node {
        crate::ast::Node {
            operand_span,
            label: Some(global(name)),
            item: None,
            source: op.to_string(),
            span: self.at(line, 1),
            trivia: crate::ast::Trivia {
                leading,
                trailing: self.trailing(comment, line, 1),
            },
        }
    }

    /// A line copied through the formatter exactly as written — the macro
    /// case. The comment is carried as trivia so it is spaced canonically; the
    /// code keeps its own column, because a body's layout is the author's.
    fn verbatim_node(
        &self,
        code: &str,
        leading: Vec<crate::ast::Comment>,
        comment: Option<&str>,
        line: usize,
    ) -> crate::ast::Node {
        crate::ast::Node {
            operand_span: None,
            label: None,
            item: Some(crate::ast::Item::Verbatim),
            source: code.trim_end().to_string(),
            span: self.at(line, 1),
            trivia: crate::ast::Trivia {
                leading,
                trailing: self.trailing(comment, line, 1),
            },
        }
    }

    /// A repetition node in acme's brace style. The head is stored without its
    /// `{`, which `emit` puts back — the same shape a brace conditional uses.
    #[allow(clippy::too_many_arguments)]
    fn loop_node(
        &self,
        cond: String,
        head: String,
        invert: bool,
        test_first: bool,
        body: Vec<crate::ast::Node>,
        leading: Vec<crate::ast::Comment>,
        comment: Option<&str>,
        line: usize,
    ) -> crate::ast::Node {
        // The source text is rebuilt in the spelling that reads back the same
        // way, so the formatter round-trips a loop it did not see written.
        // The head exactly as it was written. `!while c` and `!do while c`
        // are the same loop and ACME takes both, so a formatter that renders
        // one as the other has changed the word rather than the layout.
        let source = head;
        crate::ast::Node {
            operand_span: None,
            label: None,
            item: Some(crate::ast::Item::Loop {
                cond,
                invert,
                test_first,
                body,
            }),
            source,
            span: self.at(line, 1),
            trivia: crate::ast::Trivia {
                leading,
                trailing: self.trailing(comment, line, 1),
            },
        }
    }

    fn repeat_node(
        &self,
        head: String,
        body: Vec<crate::ast::Node>,
        leading: Vec<crate::ast::Comment>,
        comment: Option<&str>,
        line: usize,
    ) -> crate::ast::Node {
        crate::ast::Node {
            operand_span: None,
            label: None,
            item: Some(crate::ast::Item::Repeat {
                head: head.clone(),
                body,
                close: "}".to_string(),
                style: crate::ast::CondStyle::Brace,
            }),
            source: head,
            span: self.at(line, 1),
            trivia: crate::ast::Trivia {
                leading,
                trailing: self.trailing(comment, line, 1),
            },
        }
    }

    fn op_node(
        &self,
        operand_span: Option<crate::ast::Span>,
        label: Option<crate::ast::Symbol>,
        source: String,
        leading: Vec<crate::ast::Comment>,
        comment: Option<&str>,
        line: usize,
    ) -> crate::ast::Node {
        crate::ast::Node {
            operand_span,
            label,
            item: None,
            source,
            span: self.at(line, 1),
            trivia: crate::ast::Trivia {
                leading,
                trailing: self.trailing(comment, line, 1),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn conditional_node(
        &self,
        head: String,
        then_body: Vec<crate::ast::Node>,
        else_body: Option<Vec<crate::ast::Node>>,
        inline: bool,
        leading: Vec<crate::ast::Comment>,
        comment: Option<&str>,
        line: usize,
    ) -> crate::ast::Node {
        crate::ast::Node {
            operand_span: None,
            label: None,
            item: Some(crate::ast::Item::Conditional {
                close: "}".to_string(),
                head,
                then_body,
                else_body,
                inline,
                style: crate::ast::CondStyle::Brace,
            }),
            source: String::new(),
            span: self.at(line, 1),
            trivia: crate::ast::Trivia {
                leading,
                trailing: self.trailing(comment, line, 1),
            },
        }
    }

    /// Append the pending comments/blanks as a bare node (so the formatter keeps
    /// them) when a block or the file ends; a no-op if none are pending.
    fn flush_pending(&mut self, nodes: &mut Vec<crate::ast::Node>, line: usize) {
        if !self.pending.is_empty() {
            nodes.push(crate::ast::Node {
                operand_span: None,
                label: None,
                item: None,
                source: String::new(),
                span: self.at(line, 1),
                trivia: crate::ast::Trivia {
                    leading: std::mem::take(&mut self.pending),
                    trailing: None,
                },
            });
        }
    }
}

/// A plain global symbol whose source name and qualified name are the same.
fn global(name: &str) -> crate::ast::Symbol {
    crate::ast::Symbol {
        name: name.to_string(),
        scope: crate::ast::Scope::Global,
        qualified: name.to_string(),
    }
}

/// Whether a trimmed line opens a conditional (`!if`/`!ifdef`/`!ifndef`).
fn is_conditional_head(trimmed: &str) -> bool {
    matches!(split_first_word(trimmed).0, "!if" | "!ifdef" | "!ifndef")
}

/// Whether a trimmed line opens a `!zone [title] { … }` block (U7): a
/// `!zone`/`!zn` first word (case-insensitive, as acme reads directives) with
/// a top-level `{`. Returns the `{`'s offset. A brace-less `!zone` line is an
/// ordinary node (the walk-handled line form).
/// Whether `trimmed` opens a `!macro` definition.
///
/// Only the keyword is checked, not the brace: acme wants the `{` on the header
/// line, and a header without one is a *malformed definition* rather than a
/// non-definition — the same reading [`AcmeMacros::collect`] takes.
fn is_macro_head(trimmed: &str) -> bool {
    split_first_word(trimmed).0.eq_ignore_ascii_case("!macro")
}

/// Where a `!for` block's opening brace is, if this line opens one.
fn for_block_open(trimmed: &str) -> Option<usize> {
    if split_first_word(trimmed).0.eq_ignore_ascii_case("!for") {
        find_top(trimmed, b'{')
    } else {
        None
    }
}

/// A head that opens a **marker block**: the head and its `}` stay in the
/// tree, and the evaluator pushes state on the way in and pops it on the way
/// out. Unlike a conditional there is no branch to prune, so nothing is
/// removed from the tree and the formatter re-renders both markers.
///
/// Four directives share the shape and differ in what they save: `!zone` the
/// zone name, `!xor` the mask, `!ct` the conversion table, `!pseudopc` the
/// address code claims to be at. The first three also have a no-block form
/// that mutates without saving, which is why the `{` is what decides — not
/// the word. `!pseudopc` has no such form: ACME retired it, and the bare
/// spelling is declared as its refusal.
/// A `!while …  {` or `!do … {` head, and where its `{` is.
fn loop_block_open(trimmed: &str) -> Option<usize> {
    let word = split_first_word(trimmed).0.to_ascii_lowercase();
    if matches!(word.as_str(), "!while" | "!do") {
        find_top(trimmed, b'{')
    } else {
        None
    }
}

/// The condition a loop head states, if it states one. `!while c` and
/// `!do while c` / `!do until c` do; a bare `!do` does not, and puts it on the
/// closing line instead.
fn loop_head_condition(head: &str, line: usize) -> Result<Option<(String, bool)>, AsmError> {
    let (word, rest) = split_first_word(head);
    let rest = rest.trim();
    if word.eq_ignore_ascii_case("!while") {
        if rest.is_empty() {
            return Err(AsmError::new(line, "`!while` needs a condition"));
        }
        return Ok(Some((rest.to_string(), false)));
    }
    // `!do`: either bare, or with its own `while`/`until`.
    for (w, invert) in [("while", false), ("until", true)] {
        if let Some(cond) = strip_word_ci(rest, w) {
            let cond = cond.trim();
            if cond.is_empty() {
                return Err(AsmError::new(line, format!("`{w}` needs a condition")));
            }
            return Ok(Some((cond.to_string(), invert)));
        }
    }
    if rest.is_empty() {
        return Ok(None);
    }
    Err(AsmError::new(
        line,
        format!("unexpected `{rest}` after `!do`"),
    ))
}

fn marker_block_open(trimmed: &str) -> Option<usize> {
    let word = split_first_word(trimmed).0.to_ascii_lowercase();
    if matches!(
        word.as_str(),
        "!zone" | "!zn" | "!xor" | "!ct" | "!convtab" | "!pseudopc"
    ) {
        find_top(trimmed, b'{')
    } else {
        None
    }
}

/// The first top-level occurrence of `ch` (outside `'…'`/`"…"`), for brace scans.
fn find_top(s: &str, ch: u8) -> Option<usize> {
    let (mut in_char, mut in_str) = (false, false);
    for (i, &b) in s.as_bytes().iter().enumerate() {
        match b {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            _ if b == ch && !in_char && !in_str => return Some(i),
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Conditional assembly (`!if` / `!ifdef` / `!ifndef` … `{ }` … `else { }`)
// ---------------------------------------------------------------------------

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

/// Parse `!set name = expr`, folding `expr` against the current `env`.
fn parse_set(
    anons: &Anons,
    zone: &str,
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
    let value = fold_const(&parse_value(anons, zone, &rest[eq + 1..], line)?, env, line)?;
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
        Expr::BitNot(inner) => Expr::BitNot(Box::new(bake_expr(*inner, env, set_names))),
        Expr::LogNot(inner) => Expr::LogNot(Box::new(bake_expr(*inner, env, set_names))),
        Expr::Bin(op, l, r) => Expr::Bin(
            op,
            Box::new(bake_expr(*l, env, set_names)),
            Box::new(bake_expr(*r, env, set_names)),
        ),
        other => other,
    }
}

/// Evaluate an `!if` condition: a comparison of two constant expressions, or a
/// bare expression tested for non-zero. Every operator the reference has —
/// `=`, `!=`, `<>`, `<=`, `>=`, `<`, `>`.
///
/// The single `<` and `>` are the awkward ones, because ACME spells the
/// low-byte and high-byte extracts with the same two characters (`lda #<v`).
/// They are told apart by **position**: a `<` with an expression to its left is
/// a comparison, and one with nothing to its left is a prefix. So `5 > 3`
/// compares, `<v` extracts, and `<v > 3` does both — the scan finds the `<`
/// first, sees nothing to its left, and keeps looking.
///
/// This is why they were left out originally, with the note that the
/// curriculum only used `=`. The curriculum is not the instrument: it was
/// written against what this assembler accepts, so it cannot signal demand for
/// a form the assembler rejects — the same argument #93 makes about macros.
fn eval_condition(
    anons: &Anons,
    zone: &str,
    env: &BTreeMap<String, i64>,
    cond: &str,
    line: usize,
) -> Result<bool, AsmError> {
    let value = |s: &str| -> Result<i64, AsmError> {
        fold_const(&parse_value(anons, zone, s, line)?, env, line)
    };
    // A string is a type of its own to ACME, and **no operator applies to
    // one** — `"a" = 97` is *"Cannot apply test for equality to string and
    // number"*, the same refusal as `"a"+1`. A bare `!if "a"` is fine, so this
    // guards the comparison operands rather than the condition.
    let operand = |s: &str| -> Result<i64, AsmError> {
        if lone_string_value(s.trim(), line).is_some() {
            return Err(AsmError::new(
                line,
                format!("no operator applies to the string `{}`", s.trim()),
            ));
        }
        value(s)
    };
    let c = cond.trim();
    if let Some(i) = top_level_find(c, "!=") {
        return Ok(operand(&c[..i])? != operand(&c[i + 2..])?);
    }
    if let Some(i) = top_level_find(c, "<=") {
        return Ok(operand(&c[..i])? <= operand(&c[i + 2..])?);
    }
    if let Some(i) = top_level_find(c, ">=") {
        return Ok(operand(&c[..i])? >= operand(&c[i + 2..])?);
    }
    if let Some(i) = top_level_find(c, "<>") {
        return Ok(operand(&c[..i])? != operand(&c[i + 2..])?);
    }
    if let Some(i) = top_level_lone_eq(c) {
        return Ok(operand(&c[..i])? == operand(&c[i + 1..])?);
    }
    if let Some(i) = infix_relation(c, b'<') {
        return Ok(operand(&c[..i])? < operand(&c[i + 1..])?);
    }
    if let Some(i) = infix_relation(c, b'>') {
        return Ok(operand(&c[..i])? > operand(&c[i + 1..])?);
    }
    Ok(value(c)? != 0)
}

/// The byte index of a top-level `op` used as a **comparison** rather than as a
/// byte-extract prefix — that is, one with an expression to its left.
///
/// "An expression to its left" means non-empty text that does not end in an
/// operator: `5 <` compares, `5 + <` does not, and neither does a bare `<`.
/// The two-character operators are matched before this is reached, so a `<`
/// found here is never the first half of `<=` or `<>`.
fn infix_relation(s: &str, op: u8) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let (mut in_char, mut in_str) = (false, false);
    for i in 0..bytes.len() {
        match bytes[i] {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b'(' if !in_char && !in_str => depth += 1,
            b')' if !in_char && !in_str => depth -= 1,
            b if b == op && depth == 0 && !in_char && !in_str => {
                let left = s[..i].trim_end();
                let ends_in_operator = left
                    .as_bytes()
                    .last()
                    .is_some_and(|c| b"+-*/&|^!<>=(,".contains(c));
                if !left.is_empty() && !ends_in_operator {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
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
/// `text` with a leading `word` removed, when it starts with exactly that
/// word (case-insensitively) followed by whitespace or nothing. Returns
/// `None` otherwise, so `!address` is not read as `!addr` with a stray `ess`.
fn strip_word_ci<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let head = text.get(..word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let rest = &text[word.len()..];
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(rest)
    } else {
        None
    }
}

fn parse_statement(
    target: AcmeTarget,
    anons: &Anons,
    zone: &str,
    env: &BTreeMap<String, i64>,
    conv: ConvTable,
    code: &str,
    line: usize,
) -> Result<(Option<String>, Option<Operation>), AsmError> {
    let trimmed = code.trim();

    // `*= expr` (or `* = expr`) sets the program counter.
    if let Some(rest) = trimmed.strip_prefix('*') {
        let rest = rest.trim_start();
        if let Some(value) = rest.strip_prefix('=') {
            return Ok((
                None,
                Some(Operation::Org(parse_value(anons, zone, value, line)?)),
            ));
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
            Some(Operation::Equ(parse_value(anons, zone, value, line)?)),
        ));
    }

    // Otherwise: an optional column-0 label, then a directive or instruction.
    let (label, rest) = split_label(target, anons, code, line)?;
    // `!addr`/`!address` names a symbol and marks it an address. The mark has
    // no effect on bytes — probed at every shape, `!addr foo = $10` selects
    // zero page exactly as `foo = $10` does — so what is left is the naming,
    // and that is a binding rather than an operation.
    //
    // With `=` it is the assignment above. Without, it is a **label**: `!addr
    // foo` binds the program counter, and `!byte <foo, >foo` after three
    // bytes reads `03 10`, the same as a plain `foo` in that position.
    if let Some(args) = rest
        .strip_prefix('!')
        .map(str::trim_start)
        .and_then(|r| strip_word_ci(r, "addr").or_else(|| strip_word_ci(r, "address")))
    {
        if label.is_some() {
            return Err(AsmError::new(line, "`!addr` takes no label of its own"));
        }
        let args = args.trim();
        let (name, value) = match args.split_once('=') {
            Some((n, v)) => (n.trim(), Some(v.trim())),
            None => (args, None),
        };
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid symbol name `{name}`")));
        }
        let op = match value {
            Some(v) => Some(Operation::Equ(parse_value(anons, zone, v, line)?)),
            None => None,
        };
        return Ok((Some(name.to_string()), op));
    }
    let op = parse_op(target, anons, zone, env, conv, rest, line)?;
    Ok((label, op))
}

/// Split a column-0 label from the rest. A leading-whitespace line has no label.
/// A column-0 first word that names a known mnemonic or a `!` directive is the
/// operation, not a label; an all-`-`/all-`+` run is an anonymous label.
fn split_label<'a>(
    target: AcmeTarget,
    anons: &Anons,
    code: &'a str,
    line: usize,
) -> Result<(Option<String>, &'a str), AsmError> {
    if code.starts_with([' ', '\t']) {
        return Ok((None, code.trim()));
    }
    let trimmed = code.trim();
    let (word, remainder) = split_first_word(trimmed);
    if anon_marker(word).is_some() {
        // The walk registered this line's definition (at the current
        // evaluation position) before lowering it.
        let name = anons
            .def_here()
            .map(|d| d.name.clone())
            .ok_or_else(|| AsmError::new(line, "internal: anonymous label not registered"))?;
        return Ok((Some(name), remainder));
    }
    if let Some(name) = word.strip_suffix(':') {
        if !is_ident(name) {
            return Err(AsmError::new(line, format!("invalid label `{name}`")));
        }
        return Ok((Some(name.to_string()), remainder));
    }
    if word.starts_with('!')
        || target.set.instruction(&word.to_ascii_uppercase()).is_some()
        || target
            .ext
            .is_some_and(|set| set.instruction(&word.to_ascii_uppercase()).is_some())
    {
        return Ok((None, trimmed));
    }
    if is_ident(word) {
        return Ok((Some(word.to_string()), remainder));
    }
    Err(AsmError::new(line, format!("cannot parse `{trimmed}`")))
}

/// Parse the operation part (after any label): a `!` directive or an instruction.
fn parse_op(
    target: AcmeTarget,
    anons: &Anons,
    zone: &str,
    env: &BTreeMap<String, i64>,
    conv: ConvTable,
    rest: &str,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    if rest.is_empty() {
        return Ok(None);
    }
    if let Some(directive) = rest.strip_prefix('!') {
        return Ok(Some(parse_directive(
            anons, zone, env, conv, directive, line,
        )?));
    }
    let (mnemonic, remainder) = split_first_word(rest);
    let mnemonic = mnemonic.to_ascii_uppercase();
    if target.set.instruction(&mnemonic).is_none()
        && target
            .ext
            .and_then(|set| set.instruction(&mnemonic))
            .is_none()
    {
        return Err(AsmError::new(
            line,
            format!("unknown instruction `{mnemonic}`"),
        ));
    }
    if target
        .set
        .find_form(&mnemonic, "zeropage,relative")
        .is_some()
        || target
            .ext
            .is_some_and(|set| set.find_form(&mnemonic, "zeropage,relative").is_some())
    {
        let parts = mos6502::split_top_level(remainder, ',');
        if parts.len() != 2 {
            return Err(AsmError::new(
                line,
                format!("`{mnemonic}` needs a zero-page byte and a branch target"),
            ));
        }
        return Ok(Some(Operation::Instruction {
            mnemonic,
            mode: "zeropage,relative",
            operands: vec![
                parse_value(anons, zone, parts[0], line)?,
                parse_value(anons, zone, parts[1], line)?,
            ],
        }));
    }
    let operand = mos6502::parse_operand(remainder, line, &|s, l| parse_value(anons, zone, s, l))?;
    let force_abs = address_forces_absolute(remainder);
    let (mode, mut operand) = mos6502::resolve_mode_in_sets(
        target.set, target.ext, &mnemonic, operand, env, force_abs, line,
    )?;
    // Unlike the 65816's 16-bit relative forms, 65CE02 long branches measure
    // from opcode+2 rather than from the end of the three-byte instruction.
    // The shared encoder measures from the end, so carry that one-byte bias in
    // the lowered target expression. This is ACME/65CE02 semantics, not a
    // general property of the `relative16` ISA mode.
    if mode == "relative16" && target.ext.is_some_and(|set| set.has_mnemonic("LDZ")) {
        operand = operand.map(|e| {
            Expr::Bin(
                crate::engine::BinOp::Add,
                Box::new(e),
                Box::new(Expr::Num(1)),
            )
        });
    }
    Ok(Some(Operation::Instruction {
        mnemonic,
        mode,
        operands: operand.into_iter().collect(),
    }))
}

// ---------------------------------------------------------------------------
// Directives
// ---------------------------------------------------------------------------

/// What this dialect accepts beyond the 6502 instruction set.
///
/// The `!` is required, which is not a house style but a fact about acme: a
/// bare `byte` is a *label definition* there, and real acme answers "Label name
/// not in leftmost column" for it. `Sigilled { required: true }` is what keeps
/// that true — see `crate::directives`.
///
/// Dispatch is split across three paths and the declaration covers all of them,
/// because it describes the dialect rather than any one parser. `!src`, `!bin`
/// and `!zone` are walk-handled in [`AcmeEval::lower`] — a zone switch is
/// evaluation state, not an operation — and the conditionals are read by the
/// scanner before this point. Only the data and layout directives reach
/// [`parse_directive`].
pub const DIRECTIVES: &[Directive] = &[
    Directive {
        id: "bytes",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["byte", "by", "8"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "words",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["word", "wo", "16"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!fi` is not a conditional terminator — ACME has no such word. It is
    // the short spelling of `!fill`, and takes every form `!fill` does:
    // `!fi 3` fills with zero, `!fi 2+1, $bb` folds the count.
    Directive {
        id: "fill",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["fill", "fi"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!raw` emits its operands **without** the current conversion table,
    // where `!text` honours it. Today the two produce identical bytes,
    // because `!ct` is not implemented and ACME's default table converts
    // nothing — so `!text "ab"` and `!raw "ab"` both give `61 62`. They are
    // separate ids rather than one entry's aliases precisely because that
    // coincidence ends the day `!ct` lands: `!text` must start converting and
    // `!raw` must not.
    Directive {
        id: "raw",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["raw"],
            required: true,
        },
        category: Category::Operation,
    },
    // Six spellings, one rule: emit each value at a stated width in a stated
    // byte order. The order is the *directive's*, not the CPU's — `!be24` on
    // a 6502 is big-endian, which is the only reason to write it.
    Directive {
        id: "sized",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["be16", "be24", "be32", "le16", "le24", "le32"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!addr`/`!address` names a symbol and marks it an address. The mark is
    // invisible in the bytes — `!addr foo = $10` selects zero page exactly as
    // `foo = $10` does — so what it really is, is a binding: an assignment
    // with `=`, and a label without. Handled in `parse_statement`, because a
    // directive dispatch cannot bind the name a statement carries.
    // `!cpu <name>` selects the processor, and ACME's processors are not
    // spellings of one another: `!cpu 6510` enables the undocumented opcodes
    // (`lax $10` is `a7 10`) that `!cpu 6502` refuses, and `!cpu 65816` adds
    // `rtl` and `xba`. The walk binds those real sets lexically; processor
    // families whose executable specs have not landed remain named gaps, and
    // a name ACME does not know remains the reference's own refusal.
    Directive {
        id: "cpu",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["cpu"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!to "file"[, format]` names the output, and `!symbollist "file"` a
    // symbol dump. Both are *requests*: ACME takes the first name and warns
    // "already chosen" for any later one — including when the command line
    // chose first, which is the usual case here. So neither ever overrides a
    // flag, and a second directive never displaces the first.
    Directive {
        id: "output-file",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["to"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "symbol-list",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["symbollist"],
            required: true,
        },
        category: Category::Operation,
    },
    // The condition loops, in all five spellings ACME takes. `!while c { … }`
    // and `!do while|until c { … }` test before the body; `!do { … } while|
    // until c` tests after, so the body always runs once. Walk-handled: the
    // condition is re-read between iterations, because the body is what moves
    // it.
    Directive {
        id: "loop",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["while", "do"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!pseudopc <addr> { … }` assembles here and reports addresses there:
    // labels and `*` inside read as if the code sat at `<addr>`, while the
    // bytes stay where they are. The bare `!pseudopc` is ACME's retired
    // spelling and is declared as its refusal, above.
    Directive {
        id: "pseudopc",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["pseudopc"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "addr",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["addr", "address"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!ct`/`!convtab` choose the table `!text` converts through. `raw`,
    // `pet` and `scr` are named; a quoted operand is a table read from a file
    // and is refused by name rather than mistaken for one of the three. Like
    // `!xor` it has a block form that restores on exit, and unlike `!xor` a
    // second one **replaces** rather than combining. Walk-handled.
    Directive {
        id: "convtab",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["ct", "convtab"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!xor` masks every byte its scope *writes* — data, opcodes and an
    // included binary alike — and leaves reservations and `org` gaps alone.
    // With a block it restores the previous mask on the way out; without one
    // it runs to the end of the enclosing `!xor` block or of the file, and is
    // not scoped by `!if` or `!zone`. Masks combine rather than replace.
    // Walk-handled, like `!zone`.
    Directive {
        id: "xor",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["xor"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!as`/`!rs` select short accumulator/index registers. On a 6502 they
    // are accepted and emit nothing, which is every path this dialect has:
    // the long forms `!al`/`!rl` are refused by ACME itself here ("Chosen CPU
    // does not support long registers"), and the `!cpu 65816` that would make
    // any of them matter is not implemented.
    Directive {
        id: "register-width",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["as", "rs"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!eof` ends **this file**, not the assembly: an include stops there and
    // its parent carries on. Consumed by the line scanner rather than reaching
    // `parse_directive`, the way `!zone` is — declared here because the
    // declaration is what says the word exists.
    Directive {
        id: "eof",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["eof", "endoffile"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!scrxor` is **not** `!xor` wrapped around `!scr`, though it looks it.
    // The mask reaches the converted characters of a string and nothing else:
    // `!scrxor $80, 65` is `41`, where `!xor $80 { !scr 65 }` is `c1`. A
    // number in the list passes through unconverted and un-masked, exactly as
    // it does in `!scr` — so this belongs with the text directives, not with
    // the block that transforms whatever it encloses.
    Directive {
        id: "scrxor",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["scrxor"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!skip` reserves rather than emitting: what lands there is whatever
    // `!initmem` chose, so it is a reservation and not a run of zeros.
    Directive {
        id: "skip",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["skip"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!initmem` names the byte that fills unwritten space, for the whole
    // assembly and wherever it is written.
    Directive {
        id: "initmem",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["initmem"],
            required: true,
        },
        category: Category::Operation,
    },
    // `!hex` takes bare hex digit pairs — no `$`, no quotes, no commas, all of
    // which ACME answers with a syntax error.
    Directive {
        id: "hex",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["hex"],
            required: true,
        },
        category: Category::Operation,
    },
    // The source speaking for itself: `!error` and `!serious` stop the
    // assembly, `!warn` notes and carries on. All three render their operand
    // list the way the data directives do, so `!warn "at ", *` reports the
    // address.
    //
    // `!serious` differs from `!error` only in that ACME abandons the pass
    // immediately rather than collecting further errors first. We stop on the
    // first error either way, so the two are one behaviour here.
    Directive {
        id: "diagnose",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["error", "serious", "warn"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "align",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["align"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "text",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["text", "tx"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "scr",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["scr"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "pet",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["pet"],
            required: true,
        },
        category: Category::Operation,
    },
    // Walk-handled: these never reach `parse_directive`.
    Directive {
        id: "include",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["src", "source"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["bin", "binary"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "zone",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["zone", "zn"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "set",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["set"],
            required: true,
        },
        category: Category::Operation,
    },
    // Read by the conditional scanner before parsing.
    Directive {
        id: "conditional",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["if", "ifdef", "ifndef"],
            required: true,
        },
        category: Category::Operation,
    },
    // Scanner-handled, like the conditionals above: the macro expander reads
    // these before `parse_directive` is reached. Declared for the same reason
    // `!src` and `!zone` are — the surface describes the dialect.
    // Walk-handled: the structure parse reads `!for` into `Item::Repeat`.
    Directive {
        id: "repeat",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["for"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "macro",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["macro"],
            required: true,
        },
        category: Category::Operation,
    },
    // ACME's own retired spellings. It refuses these however they are
    // invoked — bare, with an operand, with a block — so implementing them
    // would take source the reference rejects, which is the divergence this
    // project exists to avoid. Probed against 0.97, 2026-08-25.
    Directive {
        id: "obsolete-subzone",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["subzone", "sz"],
            required: true,
        },
        category: Category::RefusedByReference(
            "retired in ACME 0.97, which answers `\"!subzone {}\" is obsolete; use \
             \"!zone {}\" instead`",
        ),
    },
    Directive {
        id: "obsolete-cbm",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["cbm"],
            required: true,
        },
        category: Category::RefusedByReference(
            "retired in ACME 0.97, which answers `\"!cbm\" is obsolete; use \
             \"!ct pet\" instead`",
        ),
    },
    // Only the bare `!realpc` is retired; the block form `!pseudopc {}` is
    // current, and is in the list below as a gap that really is ours.
    Directive {
        id: "obsolete-realpc",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["realpc"],
            required: true,
        },
        category: Category::RefusedByReference(
            "retired in ACME 0.97, which answers `\"!pseudopc/!realpc\" is obsolete; \
             use \"!pseudopc {}\" instead`",
        ),
    },
    // Nothing is left here. Every word ACME 0.97 has is now declared above:
    // taken, refused as ACME refuses it, or — for `!cpu` — taken for the one
    // processor this dialect assembles and refused by name for the rest.
];

/// The processor named by a live `!cpu` statement, including the labelled
/// form. The regular parser remains responsible for validating the name.
fn cpu_selector(code: &str) -> Option<String> {
    let trimmed = code.trim();
    let directive = if trimmed
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case("!cpu"))
    {
        trimmed
    } else {
        split_first_word(trimmed).1.trim_start()
    };
    let rest = strip_word_ci(directive, "!cpu")?;
    Some(rest.trim().to_ascii_lowercase())
}

fn parse_directive(
    anons: &Anons,
    zone: &str,
    env: &BTreeMap<String, i64>,
    conv: ConvTable,
    directive: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let (name, rest) = split_first_word(directive);
    // Dispatch through the declared surface. `directive` arrives with the `!`
    // already stripped by the caller, so the sigil is put back for the lookup —
    // the declaration is what says the sigil is mandatory, and matching the
    // bare name here would quietly make it optional.
    let sigilled = format!("!{}", name.to_ascii_lowercase());
    let Some(entry) = lookup(DIRECTIVES, &sigilled) else {
        return Err(AsmError::new(
            line,
            format!("`!{name}` is not a pseudo opcode ACME has"),
        ));
    };
    if let Category::RefusedByReference(rule) = entry.category {
        return Err(AsmError::new(
            line,
            crate::directives::refused_by_reference("acme", &format!("!{name}"), rule),
        ));
    }
    if entry.category == Category::ExpressionWord {
        return Err(AsmError::new(
            line,
            crate::directives::not_a_statement(&format!("!{name}")),
        ));
    }
    if entry.category == Category::KnownUnsupported {
        return Err(AsmError::new(
            line,
            format!(
                "`!{name}` is a real pseudo opcode here and asm198x does not \
                 implement it yet — the source is valid and the gap is ours"
            ),
        ));
    }
    match entry.id {
        "bytes" => Ok(Operation::Bytes(parse_list(anons, zone, rest, line)?)),
        "words" => Ok(Operation::Words(parse_list(anons, zone, rest, line)?)),
        "fill" => parse_fill(anons, zone, env, rest, line),
        "diagnose" => Ok(parse_diagnose(anons, zone, env, name, rest, line)),
        "align" => parse_align(anons, zone, env, rest, line),
        // The current table, which is `raw` until `!ct` says otherwise —
        // the reason `!text` and `!raw` agreed byte-for-byte before this.
        "text" => parse_text(anons, zone, rest, line, move |c| conv.convert(c)),
        // Identity today, and identity forever: see the declaration above.
        "raw" => parse_text(anons, zone, rest, line, |c| c),
        "hex" => parse_hex(rest, line),
        "cpu" => {
            let name = rest.trim().to_ascii_lowercase();
            match name.as_str() {
                // The processor this dialect already assembles.
                "6502" | "6510" | "nmos6502" | "65c02" | "r65c02" | "w65c02" | "c64dtv2"
                | "65ce02" | "4502" | "65816" => Ok(Operation::Bytes(Vec::new())),
                // ACME's other processors. Each is a different opcode set —
                // not a spelling of 6502 — so accepting one silently would
                // assemble the wrong instructions or refuse the right ones.
                "m65" => Err(AsmError::new(
                    line,
                    format!(
                        "`!cpu {name}` selects a different processor, and asm198x does \
                             not switch processors mid-assembly yet — the source is valid \
                             and the gap is ours (asm198x#302)"
                    ),
                )),
                "" => Err(AsmError::new(line, "no string given")),
                other => Err(AsmError::new(line, format!("unknown processor `{other}`"))),
            }
        }
        "output-file" => {
            let (path, rest) = quoted_operand(rest, line, "!to")?;
            let mut defaulted_format = false;
            let format = match rest.trim().strip_prefix(',') {
                None if rest.trim().is_empty() => {
                    defaulted_format = true;
                    OutputFormat::Cbm
                }
                None => {
                    return Err(AsmError::new(
                        line,
                        format!("unexpected `{}` after the file name", rest.trim()),
                    ));
                }
                Some(f) => match f.trim().to_ascii_lowercase().as_str() {
                    "plain" => OutputFormat::Plain,
                    "cbm" => OutputFormat::Cbm,
                    "apple" => OutputFormat::Apple,
                    other => {
                        return Err(AsmError::new(
                            line,
                            format!("unknown output format `{other}`"),
                        ));
                    }
                },
            };
            Ok(Operation::RequestOutput {
                path,
                format,
                defaulted_format,
            })
        }
        "symbol-list" => {
            let (path, rest) = quoted_operand(rest, line, "!symbollist")?;
            if !rest.trim().is_empty() {
                return Err(AsmError::new(
                    line,
                    format!("unexpected `{}` after the file name", rest.trim()),
                ));
            }
            Ok(Operation::RequestSymbols { path })
        }
        "skip" => {
            let n = fold_const(&parse_value(anons, zone, rest.trim(), line)?, env, line)?;
            // ACME's own words for a negative count, because it is the
            // reference's rule and not a house one.
            let n =
                usize::try_from(n).map_err(|_| AsmError::new(line, "negative size argument"))?;
            Ok(Operation::Reserve(n))
        }
        "initmem" => {
            let v = fold_const(&parse_value(anons, zone, rest.trim(), line)?, env, line)?;
            // Signed or unsigned, like every other ACME byte: `!initmem -1`
            // is `$ff`, and `!initmem 256` is "Number out of range".
            if !(-128..=0xFF).contains(&v) {
                return Err(AsmError::new(line, format!("number out of range: {v}")));
            }
            Ok(Operation::InitMem((v & 0xFF) as u8))
        }
        "sized" => {
            // `be`/`le` then the width in bits; the declaration above is what
            // guarantees the shape, so this reads it rather than re-checking.
            let big_endian = name.get(..2).is_some_and(|s| s.eq_ignore_ascii_case("be"));
            let width = match name.get(2..) {
                Some("16") => 2,
                Some("24") => 3,
                _ => 4,
            };
            Ok(Operation::Sized {
                width,
                big_endian,
                values: parse_list(anons, zone, rest, line)?,
            })
        }
        "scr" => parse_text(anons, zone, rest, line, screen_code),
        // Accepted and emits nothing. ACME rejects an operand — "Garbage data
        // at end of statement" — so this does too rather than ignoring one.
        "register-width" => {
            if !rest.trim().is_empty() {
                return Err(AsmError::new(
                    line,
                    format!("garbage data at end of statement: `{}`", rest.trim()),
                ));
            }
            Ok(Operation::Bytes(Vec::new()))
        }
        // Consumed by the scanner; reaching here means a path misrouted it.
        "eof" => Err(AsmError::new(line, "`!eof` is handled by the scanner")),
        "scrxor" => {
            let (mask_src, rest) = rest
                .split_once(',')
                .ok_or_else(|| AsmError::new(line, "`!scrxor` needs a value and a list"))?;
            let mask = fold_const(&parse_value(anons, zone, mask_src.trim(), line)?, env, line)?;
            // The low byte, whatever the value. ACME range-checks `!initmem`
            // and does **not** range-check this: `!initmem 256` is "Number
            // out of range" while `!scrxor 256, "a"` silently masks to `$00`,
            // `511` to `$ff` and `-129` to `$7f`. Reproducing that
            // inconsistency is the job; tidying it would be a divergence.
            let mask = (mask & 0xFF) as u8;
            parse_text(anons, zone, rest, line, move |c| screen_code(c) ^ mask)
        }
        "pet" => parse_text(anons, zone, rest, line, petscii),
        // `!zone`/`!zn` never reaches here: it is walk-handled in
        // [`AcmeEval::lower`] (U7 — a zone switch is evaluation state, like
        // `!src`), so the fall-through reports it loudly if a new path ever
        // misroutes it.
        // Declared, and dispatched elsewhere: `!src`, `!bin` and `!zone` are
        // walk-handled in [`AcmeEval::lower`], and the conditionals are read by
        // the scanner. Reaching here means a path misrouted one, which the
        // original fall-through reported loudly and this keeps reporting.
        _ => Err(AsmError::new(
            line,
            format!("unsupported directive `!{name}`"),
        )),
    }
}

/// `!fill amount [, value]` — `amount` bytes of `value` (default 0). Both fold
/// against the parse-time `env` (so a `= const` like `MAX_NOTES` works), because
/// the size has to be known before pass two assigns addresses.
/// `!error`/`!warn` — the source's own diagnostic. The operand is the same
/// comma list the data directives take: strings go through verbatim and values
/// are rendered, so `!warn "at ", *` names the address.
///
/// Infallible on purpose. A bad operand here must not become a *different*
/// error than the one the source asked for, and an unfoldable value in an
/// untaken branch must not fail at all — the message carries the source text
/// when it cannot be folded.
fn parse_diagnose(
    anons: &Anons,
    zone: &str,
    env: &BTreeMap<String, i64>,
    name: &str,
    rest: &str,
    line: usize,
) -> Operation {
    let mut message = String::new();
    for part in mos6502::split_top_level(rest, ',') {
        let part = part.trim();
        if let Some(text) = part.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
            message.push_str(text);
        } else if let Ok(v) =
            parse_value(anons, zone, part, line).and_then(|e| fold_const(&e, env, line))
        {
            message.push_str(&v.to_string());
        } else {
            message.push_str(part);
        }
    }
    Operation::Diagnose {
        severity: if name.eq_ignore_ascii_case("warn") {
            crate::engine::DiagSeverity::Warning
        } else {
            crate::engine::DiagSeverity::Error
        },
        message,
    }
}

fn parse_fill(
    anons: &Anons,
    zone: &str,
    env: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let amount_src = parts.next().unwrap_or("").trim();
    let amount = fold_const(&parse_value(anons, zone, amount_src, line)?, env, line)?;
    let amount = usize::try_from(amount)
        .map_err(|_| AsmError::new(line, "`!fill` byte count must be a non-negative constant"))?;
    let value = match parts.next() {
        None => 0,
        Some(v) => {
            let n = fold_const(&parse_value(anons, zone, v, line)?, env, line)?;
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
    anons: &Anons,
    zone: &str,
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
    let andmask = fold_const(&parse_value(anons, zone, parts[0], line)?, env, line)?;
    let value = fold_const(&parse_value(anons, zone, parts[1], line)?, env, line)?;
    let fill = match parts.get(2) {
        None => 0xEA, // ACME's default fill byte
        Some(v) => {
            let n = fold_const(&parse_value(anons, zone, v, line)?, env, line)?;
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

fn parse_list(anons: &Anons, zone: &str, rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    mos6502::split_top_level(rest, ',')
        .iter()
        .map(|p| parse_value(anons, zone, p, line))
        .collect()
}

/// Parse a text directive: a comma list mixing `"..."` strings (one byte per
/// character, passed through `convert`) and bare values (emitted as-is). ACME's
/// `!text` passes characters through unchanged; `!scr` maps them to screen codes.
/// `!hex 0f1e2d` — bare hex digit pairs, whitespace-separated.
///
/// Every rule here was probed against ACME 0.97 rather than read off its
/// manual, because two of them are surprising:
///
/// - **Pairing is per token, not across the operand.** `!hex 0f 1e` is fine
///   and `!hex 0 f` is not, even though both hold two digits. Each
///   whitespace-separated run must itself be even.
/// - **A non-hex character ends the scan, and which error you get depends on
///   what came before it.** `!hex 0f1g2d` answers *"Hex digits are not given
///   in pairs"* (three digits read, an odd count), while `!hex 0fgg1e`
///   answers *"Syntax error"* (two digits read, then something that is not a
///   digit). Reporting one message for both would be tidier and would not be
///   ACME.
///
/// An empty operand is accepted and emits nothing — `!hex` on its own is not
/// an error, unlike the text directives.
/// A `"…"` file name at the head of a directive's operand, and whatever
/// follows it.
fn quoted_operand(rest: &str, line: usize, what: &str) -> Result<(String, String), AsmError> {
    let rest = rest.trim_start();
    let body = rest
        .strip_prefix('"')
        .ok_or_else(|| AsmError::new(line, format!("`{what}` needs a quoted file name")))?;
    let end = body
        .find('"')
        .ok_or_else(|| AsmError::new(line, format!("`{what}`: unterminated file name")))?;
    Ok((body[..end].to_string(), body[end + 1..].to_string()))
}

fn parse_hex(rest: &str, line: usize) -> Result<Operation, AsmError> {
    let mut bytes = Vec::new();
    for token in rest.split_whitespace() {
        let digits = token
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect::<String>();
        // Odd first: ACME reports the count before it reports the character
        // that stopped it, and `!hex 0f1g2d` is the case that tells them apart.
        if digits.len() % 2 != 0 {
            return Err(AsmError::new(line, "hex digits are not given in pairs"));
        }
        if digits.len() != token.len() {
            return Err(AsmError::new(
                line,
                format!("`{token}` is not hex digits — `!hex` takes no `$`, quotes or commas"),
            ));
        }
        for pair in digits.as_bytes().chunks(2) {
            let text = std::str::from_utf8(pair).unwrap_or_default();
            let value = u8::from_str_radix(text, 16)
                .map_err(|_| AsmError::new(line, format!("`{text}` is not a hex byte")))?;
            bytes.push(Expr::Num(i64::from(value)));
        }
    }
    Ok(Operation::Bytes(bytes))
}

fn parse_text(
    anons: &Anons,
    zone: &str,
    rest: &str,
    line: usize,
    convert: impl Fn(u8) -> u8,
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
            bytes.push(parse_value(anons, zone, piece, line)?);
        }
    }
    Ok(Operation::Bytes(bytes))
}

/// A `"…"` string standing alone as a value, which ACME accepts wherever it
/// wants a number: `!byte "a"`, `!word "a"`, `lda #"a"`, `lda "a"` — and
/// parenthesised, `!byte ("a")`.
///
/// It must hold exactly one character. ACME's own message is *"There's more
/// than one character"*, because to ACME this is a **string** being coerced,
/// not a character literal being lexed — which is also why it takes no
/// operators: `"a"+1` is *"Cannot apply addition to string and number"*.
///
/// Recognising it here rather than in the tokenizer is what keeps that second
/// half true. A string reached through this path is the whole value, so an
/// expression using one as an operand never gets here and still fails to
/// tokenize — a different message from ACME's, but the same answer.
///
/// `None` means "not a lone string, carry on"; `Some(Err(_))` means it was one
/// and it was wrong.
fn lone_string_value(trimmed: &str, line: usize) -> Option<Result<Expr, AsmError>> {
    let mut inner = trimmed;
    while let Some(stripped) = inner
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .map(str::trim)
    {
        inner = stripped;
    }
    let body = inner.strip_prefix('"')?.strip_suffix('"')?;
    if body.contains('"') {
        return None;
    }
    let mut chars = body.chars();
    Some(match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(Expr::Num(c as i64)),
        _ => Err(AsmError::new(
            line,
            format!("`{inner}` is more than one character"),
        )),
    })
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

/// Parse an ACME value: a bare `-`/`+` run is an anonymous-label reference —
/// deferred to a placeholder, since a forward `+` may point into a file the
/// walk has not loaded yet (see [`Anons`]); otherwise it is an expression
/// with `<`/`>` applying loosely. Leading-`.` symbols qualify into the
/// current `zone` (U7) — this is the acme expression path's single entry
/// point, so `!fill`/`!align` counts, `!bin` windows, `!set` values, and
/// `!if` conditions all resolve zone-locals uniformly.
fn parse_value(anons: &Anons, zone: &str, raw: &str, line: usize) -> Result<Expr, AsmError> {
    let trimmed = raw.trim();
    if let Some((sign, level)) = anon_marker(trimmed) {
        return Ok(Expr::Sym(anon_ref_placeholder(sign, level, anons.vline)));
    }
    if let Some(expr) = lone_string_value(trimmed, line) {
        return expr;
    }
    let expr = mos6502::parse_expr(
        raw,
        line,
        parse_number,
        mos6502::ExprOpts {
            logical: false,
            logical_not_tight: false,
            scoped_names: false,
            fixed_point: false,
            compare: mos6502::Compare {
                eq: true,
                eq_eq: false,
                ne_angle: true,
                ne_bang: false,
                relational: true,
                ordered_eq: true,
                minus_one: false,
            },
            function: None,
            bang_is_or: false,
            prec: BytePrec::Loose,
            byte_prefix: true,
            // ACME's `^` is exponentiation and its XOR is the `XOR`/`EOR`
            // keyword; `Power` also selects ACME's precedence ladder (bitwise/
            // shift looser than arithmetic).
            caret: mos6502::Caret::Power,
            at_is_pc: false,
        },
    )?;
    Ok(if zone.is_empty() {
        expr
    } else {
        crate::ast::qualify_expr(expr, zone)
    })
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

// ---------------------------------------------------------------------------
// Macros (#93)
//
// The mechanics live in [`crate::dialects::macros`]; this is acme's grammar,
// measured against acme 0.97. acme agrees with sjasmplus on locals — a
// `.dotted` label in a body is scoped to the expansion — and is the only
// dialect measured that differs from all five on *structure*:
//
//   * a body is **brace-delimited at character level**, not closed by a
//     keyword. Braces nest inside a body (`!if .v > 3 {` is ordinary), both
//     braces share lines with code (`!macro nop2 { nop` … `nop }`), and a `}`
//     inside a string or a comment closes nothing. The opening `{` must be on
//     the header line; alone on the next one it is `No string given`.
//   * **arity is part of a macro's identity.** `ldav .v` and `ldav .v, .w` are
//     two macros that coexist, dispatched on the count at the call site, and a
//     call matching neither is `Macro not defined (or wrong signature)` — not
//     an argument-list complaint. That is a fourth arity posture and a
//     different model: acme has no wrong number of arguments, only a name it
//     has never heard of.
//   * a call is written `+ldav 5`. A bare `ldav` is not one.
// ---------------------------------------------------------------------------

/// acme's macro grammar.
struct AcmeMacros;

impl macros::MacroSyntax for AcmeMacros {
    /// Unused: [`collect`](macros::MacroSyntax::collect) is overridden, because
    /// an acme body is not delimited a line at a time.
    fn header(&self, _line: &str) -> Option<(String, Vec<String>)> {
        None
    }

    /// `!macro name [.p1[, .p2]...] {` … `}`, tracking brace depth so a nested
    /// block inside the body does not end it early.
    fn collect(&self, lines: &[&str], start: usize) -> Option<Result<macros::Definition, String>> {
        let head = macros::without_comment(lines[start]);
        let (kw, rest) = head
            .trim()
            .strip_prefix('!')?
            .split_once(char::is_whitespace)?;
        if !kw.eq_ignore_ascii_case("macro") {
            return None;
        }
        // acme wants the brace here: on the next line by itself it is an error,
        // so a header without one is a malformed definition and not a
        // non-definition.
        let Some((decl, after)) = rest.split_once('{') else {
            return Some(Err("`!macro` needs its `{` on the same line".to_string()));
        };
        let decl = decl.trim();
        let (name, params) = match decl.split_once(char::is_whitespace) {
            Some((name, tail)) => (name.trim(), parameter_list(tail)),
            None => (decl, Vec::new()),
        };
        if name.is_empty() {
            return Some(Err("`!macro` has no name".to_string()));
        }

        let mut depth = 1usize;
        let mut body = Vec::new();
        // The header line may already carry body text after its brace.
        if let Some(end) = close_brace(after, &mut depth) {
            push_code(&mut body, &after[..end]);
            return Some(Ok(definition(name, params, body, start)));
        }
        push_code(&mut body, after);

        for (offset, line) in lines.iter().enumerate().skip(start + 1) {
            let code = macros::without_comment(line);
            if let Some(end) = close_brace(code, &mut depth) {
                // Whatever follows the closing brace is dropped, as acme drops
                // it. The comment cannot hold the brace — c4 pinned that — so
                // the code prefix is the whole of this line's body text.
                push_code(&mut body, &code[..end]);
                return Some(Ok(definition(name, params, body, offset)));
            }
            body.push((*line).to_string());
        }
        Some(Err(format!("`!macro {name}` has no matching `}}`")))
    }

    /// A `.dotted` label the body defines is scoped to the expansion, which is
    /// sjasmplus's rule exactly. A plain one stays global and the second
    /// invocation gets `Symbol already defined`.
    ///
    /// Parameters are dotted too, but they are substituted before this runs, so
    /// a parameter never reaches the rename.
    fn locals(&self, body: &[String]) -> Vec<String> {
        let mut names = Vec::new();
        for line in body {
            // Unlike ordinary named source labels, ACME accepts a dotted
            // macro-local after indentation. Real macro libraries commonly
            // indent the whole body, including `.carry`/`.done`; indentation
            // must not turn those into expansion-global names.
            let text = macros::without_comment(line).trim_start();
            let name = text
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(':');
            if name.starts_with('.') && name.len() > 1 && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names
    }

    /// A call is `+name`; a bare name is not one.
    fn invocation_name<'a>(&self, head: &'a str) -> Option<&'a str> {
        head.strip_prefix('+').filter(|name| !name.is_empty())
    }

    /// The argument count picks the macro, so two definitions of one name are
    /// two macros and a count matching neither is a name acme does not know.
    fn select<'a>(
        &self,
        defs: &'a [macros::MacroDef],
        argc: usize,
    ) -> Option<&'a macros::MacroDef> {
        defs.iter().find(|def| def.params.len() == argc)
    }

    /// Nothing to reconcile: [`select`](Self::select) already matched the count
    /// exactly, or reported that no definition takes it.
    fn fit_arguments(
        &self,
        _name: &str,
        _params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        Ok(args)
    }
}

/// Assemble a [`macros::Definition`] from the parts `collect` gathered.
fn definition(
    name: &str,
    params: Vec<String>,
    body: Vec<String>,
    last_line: usize,
) -> macros::Definition {
    macros::Definition {
        name: name.to_string(),
        def: macros::MacroDef {
            params,
            body,
            defined_at: None,
        },
        last_line,
    }
}

/// Push a fragment of a brace-sharing line, unless it is only whitespace.
fn push_code(body: &mut Vec<String>, text: &str) {
    if !text.trim().is_empty() {
        body.push(text.to_string());
    }
}

/// Track brace depth across one line of code, returning the index of the `}`
/// that took the depth back to zero if it is on this line.
///
/// Braces inside a string literal are text, not structure: `!byte "}"` in a
/// body closes nothing, which acme agrees with and a naive scan would not.
fn close_brace(text: &str, depth: &mut usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (i, &c) in text.as_bytes().iter().enumerate() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'{' => *depth += 1,
                b'}' => {
                    *depth -= 1;
                    if *depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
    }
    None
}

/// A comma-separated parameter list, empties dropped.
fn parameter_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Expand acme's macros, unless this parse is the formatter's.
fn expand_acme(source: &str, mode: macros::Expand) -> Result<macros::Expansion, AsmError> {
    macros::expansion(mode, source, |s| {
        macros::expand(&AcmeMacros, s).map(|e| Some((e.text, e.origins)))
    })
}

#[cfg(test)]
mod tests {
    use crate::{AsmError, AssemblyResult, assemble_acme};

    /// `!cpu` is lexical: 65816 extends the base set, and switching back
    /// removes those instructions again.
    #[test]
    fn cpu_switches_instruction_sets_per_statement() {
        assert_eq!(asm("!cpu 6502\nnop").expect("6502").bytes, vec![0xEA]);
        assert_eq!(asm("!CPU 6502\nnop").expect("case").bytes, vec![0xEA]);
        assert_eq!(
            asm("!cpu 65816\nlda #1\nrtl\nxba\n!cpu 6502\nnop")
                .expect("switch")
                .bytes,
            vec![0xA9, 0x01, 0x6B, 0xEB, 0xEA]
        );
        let err = asm("!cpu 65816\nrtl\n!cpu 6502\nafter rtl")
            .expect_err("switched back")
            .to_string();
        assert!(err.contains("unknown instruction `RTL`"), "{err}");

        for cpu in ["6510", "nmos6502"] {
            assert_eq!(
                asm(&format!("!cpu {cpu}\nlax $10\nsre $10\n!cpu 6502\nnop"))
                    .expect(cpu)
                    .bytes,
                vec![0xA7, 0x10, 0x47, 0x10, 0xEA]
            );
        }
        let err = asm("!cpu 6510\nlax $10\n!cpu 6502\nafter lax $10")
            .expect_err("6510 switched back")
            .to_string();
        assert!(err.contains("unknown instruction `LAX`"), "{err}");

        assert_eq!(
            asm("!cpu 65c02\nlda $12\nlda ($12)\nbit $12\nbit $12,x\ninc\nbra done\ndone: nop")
                .expect("65c02 base and added modes")
                .bytes,
            vec![
                0xA5, 0x12, 0xB2, 0x12, 0x24, 0x12, 0x34, 0x12, 0x1A, 0x80, 0x00, 0xEA
            ]
        );
        let err = asm("!cpu 65c02\nphx\n!cpu 6502\nafter phx")
            .expect_err("65c02 switched back")
            .to_string();
        assert!(err.contains("unknown instruction `PHX`"), "{err}");

        assert_eq!(
            asm(
                "*=$c000\n!cpu r65c02\nrmb6 $12\nbbr4 $12,done\nsmb7 $34\nbbs5 $34,done\ndone:\nnop"
            )
            .expect("Rockwell bit operations")
            .bytes,
            vec![
                0x67, 0x12, 0x4F, 0x12, 0x05, 0xF7, 0x34, 0xDF, 0x34, 0x00, 0xEA
            ]
        );
        assert_eq!(
            asm("!cpu w65c02\nwai\nstp\nrmb0 $10")
                .expect("WDC additions plus Rockwell base")
                .bytes,
            vec![0xCB, 0xDB, 0x07, 0x10]
        );
        let err = asm("!cpu r65c02\nafter wai")
            .expect_err("WAI is WDC-only")
            .to_string();
        assert!(err.contains("unknown instruction `WAI`"), "{err}");

        assert_eq!(
            asm(
                "!cpu c64dtv2\nbra done\nsac #$12\nsir #$34\nslo $10\nlax $20\nasr #$40\ndone:\nnop"
            )
            .expect("C64DTV2 additions plus shared undocumented opcodes")
            .bytes,
            vec![
                0x12, 0x0A, 0x32, 0x12, 0x42, 0x34, 0x07, 0x10, 0xA7, 0x20, 0x4B, 0x40, 0xEA
            ]
        );
        let err = asm("!cpu c64dtv2\n anc #$12")
            .expect_err("ANC is not in ACME's C64DTV2 set")
            .to_string();
        assert!(err.contains("unknown instruction"), "{err}");
        let err = asm("!cpu c64dtv2\nsac #$12\n!cpu 6502\nafter sac #$12")
            .expect_err("C64DTV2 switched back")
            .to_string();
        assert!(err.contains("unknown instruction `SAC`"), "{err}");

        assert_eq!(
            asm("!cpu 65ce02\ncle\nldz #$12\nlda ($34),z\nsta ($56,sp),y\nphw #$1234\nlbne far\nasr $78,x\nfar:\naug\n!cpu 4502\nmap\neom")
                .expect("65CE02 and 4502 profiles")
                .bytes,
            vec![
                0x02, 0xA3, 0x12, 0xB2, 0x34, 0x82, 0x56, 0xF4, 0x34, 0x12, 0xD3, 0x03,
                0x00, 0x54, 0x78, 0x5C, 0x5C, 0xEA
            ]
        );
        let err = asm("!cpu 65ce02\n lda ($12)")
            .expect_err("65CE02 replaces plain indirect with Z-indexed indirect")
            .to_string();
        assert!(err.contains("no `LDA` form"), "{err}");
    }

    /// The processor families whose executable sets do not exist yet remain
    /// named gaps rather than aliases for the nearest implemented CPU.
    #[test]
    fn another_processor_is_refused_as_our_gap() {
        let cpu = "m65";
        let err = asm(&format!("!cpu {cpu}\nnop")).expect_err(cpu).to_string();
        assert!(err.contains("the gap is ours"), "{cpu}: {err}");
        assert!(!err.contains("unknown processor"), "{cpu}: {err}");
    }

    /// A name ACME does not know is ACME's own refusal, not ours.
    #[test]
    fn an_unknown_processor_is_the_references_refusal() {
        for cpu in ["6504", "6507", "z80", "8080", "wibble"] {
            let err = asm(&format!("!cpu {cpu}\nnop")).expect_err(cpu).to_string();
            assert!(err.contains("unknown processor"), "{cpu}: {err}");
        }
        asm("!cpu\nnop").expect_err("no string given");
    }

    /// `!to` names an output file — a *request*, surfaced on the result for
    /// the caller to honour or ignore. The command line still chooses.
    #[test]
    fn to_requests_an_output_file_and_a_format() {
        let r = crate::assemble_acme("*=$1000\n!to \"o.bin\", plain\nlda #1\n").expect("plain");
        let req = r.requested_output.expect("requested");
        assert_eq!(req.path, "o.bin");
        assert_eq!(req.format, crate::OutputFormat::Plain);
        for (spelling, want) in [
            ("cbm", crate::OutputFormat::Cbm),
            ("apple", crate::OutputFormat::Apple),
        ] {
            let src = format!("*=$1000\n!to \"o.bin\", {spelling}\nlda #1\n");
            let r = crate::assemble_acme(&src).expect(spelling);
            assert_eq!(r.requested_output.expect("requested").format, want);
        }
    }

    /// No format is `cbm`, and ACME says so rather than doing it quietly — so
    /// this warns too.
    #[test]
    fn to_without_a_format_defaults_to_cbm_and_says_so() {
        let r = crate::assemble_acme("*=$1000\n!to \"o.bin\"\nlda #1\n").expect("defaulted");
        assert_eq!(
            r.requested_output.expect("requested").format,
            crate::OutputFormat::Cbm
        );
        assert!(
            r.warnings.iter().any(|w| w.message.contains("defaulting")),
            "no warning: {:?}",
            r.warnings
        );
    }

    /// The first name stands and a second only warns — ACME's "Output file
    /// already chosen", and the same rule for the symbol list.
    #[test]
    fn a_second_name_warns_and_the_first_stands() {
        let r =
            crate::assemble_acme("*=$1000\n!to \"a.bin\", plain\n!to \"b.bin\", plain\nlda #1\n")
                .expect("twice");
        assert_eq!(r.requested_output.expect("requested").path, "a.bin");
        assert!(
            r.warnings
                .iter()
                .any(|w| w.message.contains("already chosen"))
        );

        let r =
            crate::assemble_acme("*=$1000\nlda #1\n!symbollist \"a.txt\"\n!symbollist \"b.txt\"\n")
                .expect("twice");
        assert_eq!(r.requested_symbols.as_deref(), Some("a.txt"));
        assert!(
            r.warnings
                .iter()
                .any(|w| w.message.contains("already chosen"))
        );
    }

    /// The operand shapes ACME refuses.
    #[test]
    fn a_named_file_must_be_quoted_and_its_format_known() {
        crate::assemble_acme("*=$1000\n!to o.bin, plain\n").expect_err("unquoted");
        crate::assemble_acme("*=$1000\n!to \"o.bin\", wibble\n").expect_err("unknown format");
        crate::assemble_acme("*=$1000\n!to \"o.bin\" plain\n").expect_err("missing comma");
        crate::assemble_acme("*=$1000\n!symbollist s.txt\n").expect_err("unquoted");
        crate::assemble_acme("*=$1000\n!symbollist \"s.txt\" junk\n").expect_err("trailing");
    }

    /// The five spellings ACME takes, and the one thing that separates them:
    /// whether the condition is tested before the body or after it.
    #[test]
    fn every_loop_spelling_runs_the_body_the_right_number_of_times() {
        let counted = "!set i=0\n";
        assert_eq!(
            asm(&format!("{counted}!while i<3 {{\n!byte i\n!set i=i+1\n}}"))
                .expect("while")
                .bytes,
            vec![0, 1, 2]
        );
        assert_eq!(
            asm(&format!(
                "{counted}!do while i<3 {{\n!byte i\n!set i=i+1\n}}"
            ))
            .expect("do while head")
            .bytes,
            vec![0, 1, 2]
        );
        assert_eq!(
            asm(&format!(
                "{counted}!do until i=3 {{\n!byte i\n!set i=i+1\n}}"
            ))
            .expect("do until head")
            .bytes,
            vec![0, 1, 2]
        );
        assert_eq!(
            asm(&format!(
                "{counted}!do {{\n!byte i\n!set i=i+1\n}} while i<3"
            ))
            .expect("do while tail")
            .bytes,
            vec![0, 1, 2]
        );
        assert_eq!(
            asm(&format!(
                "{counted}!do {{\n!byte i\n!set i=i+1\n}} until i=3"
            ))
            .expect("do until tail")
            .bytes,
            vec![0, 1, 2]
        );
    }

    /// Tested first, the body may run no times. Tested after, it always runs
    /// once — which is the only reason both forms exist.
    #[test]
    fn a_tail_tested_loop_always_runs_once() {
        assert_eq!(
            asm("!set i=5\n!while i<3 {\n!byte i\n}\n!byte $ff")
                .expect("head")
                .bytes,
            vec![0xFF]
        );
        assert_eq!(
            asm("!set i=5\n!do {\n!byte i\n} while i<3")
                .expect("tail")
                .bytes,
            vec![5]
        );
    }

    /// The condition is re-read between iterations — the body is what moves
    /// it — so a body that does not move it cannot be allowed to run forever.
    /// ACME stops on output size; this stops on iterations, which names the
    /// loop rather than the image.
    #[test]
    fn a_loop_whose_condition_never_moves_is_refused() {
        let err = asm("!while 1 {\n!byte 0\n}")
            .expect_err("endless")
            .to_string();
        assert!(err.contains("nothing in the body moves it"), "{err}");
    }

    /// Loops nest, and interleave with conditionals both ways round.
    #[test]
    fn loops_nest_and_mix_with_conditionals() {
        assert_eq!(
            asm("!set i=0\n!while i<2 {\n!set j=0\n!while j<2 {\n!byte i*10+j\n!set j=j+1\n}\n!set i=i+1\n}")
                .expect("nested")
                .bytes,
            vec![0x00, 0x01, 0x0A, 0x0B]
        );
        assert_eq!(
            asm("!if 1 {\n!set i=0\n!while i<2 {\n!byte i\n!set i=i+1\n}\n}")
                .expect("loop in if")
                .bytes,
            vec![0, 1]
        );
        assert_eq!(
            asm("!set i=0\n!while i<3 {\n!if i=1 {\n!byte $ee\n}\n!set i=i+1\n}")
                .expect("if in loop")
                .bytes,
            vec![0xEE]
        );
    }

    /// `!pseudopc` assembles here and reports addresses there: labels and
    /// `*` inside read as if the code sat at the given address, while the
    /// bytes stay where they are.
    #[test]
    fn pseudopc_moves_the_address_and_not_the_bytes() {
        assert_eq!(
            asm("*=$1000\n!pseudopc $2000 {\nfoo\n!byte <foo, >foo\n}")
                .expect("label")
                .bytes,
            vec![0x00, 0x20]
        );
        assert_eq!(
            asm("*=$1000\n!pseudopc $2000 {\n!byte <*, >*\n}")
                .expect("star")
                .bytes,
            vec![0x00, 0x20]
        );
        // The bytes are contiguous — the real counter never moved.
        assert_eq!(
            asm("*=$1000\n!byte $11\n!pseudopc $2000 {\n!byte $22\n}\n!byte $33")
                .expect("in place")
                .bytes,
            vec![0x11, 0x22, 0x33]
        );
    }

    /// It restores on the way out, and nests. After a block the counter is
    /// the real one again, advanced by whatever the block emitted.
    #[test]
    fn pseudopc_restores_and_nests() {
        assert_eq!(
            asm("*=$1000\n!pseudopc $2000 {\n!byte <*, >*\n}\n!byte <*, >*")
                .expect("restore")
                .bytes,
            vec![0x00, 0x20, 0x02, 0x10]
        );
        assert_eq!(
            asm("*=$1000\n!pseudopc $2000 {\n!pseudopc $3000 {\n!byte <*, >*\n}\n!byte <*, >*\n}")
                .expect("nested")
                .bytes,
            vec![0x00, 0x30, 0x02, 0x20]
        );
    }

    /// A branch inside is measured from the **claimed** address, because the
    /// label it targets is claimed too. Measuring a claimed target from a
    /// real position is off by the block's offset, which is what the first
    /// draft did — `bne` to the label one byte back reported 4093.
    #[test]
    fn a_branch_inside_is_measured_from_the_claimed_address() {
        assert_eq!(
            asm("*=$1000\n!pseudopc $2000 {\nl\tnop\n\tbne l\n}")
                .expect("in")
                .bytes,
            vec![0xEA, 0xD0, 0xFD]
        );
        // Unchanged outside a block.
        assert_eq!(
            asm("*=$1000\nl\tnop\n\tbne l").expect("out").bytes,
            vec![0xEA, 0xD0, 0xFD]
        );
    }

    /// `!addr` names a symbol and marks it an address. The mark never shows
    /// in the bytes, so what is left is the naming — and that is a binding,
    /// not an operation.
    #[test]
    fn addr_binds_a_name_exactly_as_an_assignment_does() {
        assert_eq!(
            asm("!addr foo = $c000\nlda foo").expect("addr").bytes,
            asm("foo = $c000\nlda foo").expect("plain").bytes
        );
        // Notably it does *not* force absolute addressing: zero page is
        // still chosen, exactly as for a plain `=`.
        assert_eq!(
            asm("!addr bar = $10\nlda bar").expect("zp").bytes,
            vec![0xA5, 0x10]
        );
        assert_eq!(
            asm("!address baz = $c000\nlda baz")
                .expect("spelling")
                .bytes,
            vec![0xAD, 0x00, 0xC0]
        );
        assert_eq!(
            asm("!addr foo = $c000+1\nlda foo").expect("expr").bytes,
            vec![0xAD, 0x01, 0xC0]
        );
    }

    /// Without a value it is a **label**: the program counter, bound to the
    /// name. Probed rather than assumed — it reads the same as a plain label
    /// in the same position.
    #[test]
    fn a_value_less_addr_is_a_label() {
        assert_eq!(
            asm("*=$1000\n!byte 1,2,3\n!addr foo\n!byte <foo, >foo")
                .expect("addr")
                .bytes,
            asm("*=$1000\n!byte 1,2,3\nfoo\n!byte <foo, >foo")
                .expect("label")
                .bytes
        );
    }

    /// The refusals, each ACME's own.
    #[test]
    fn addr_refuses_what_acme_refuses() {
        asm("!addr foo = 1\n!addr foo = 2\n!byte foo").expect_err("already defined");
        asm("!addr foo = 1\nfoo = 2\n!byte foo").expect_err("already defined");
        asm("lbl\t!addr foo = 1\n!byte foo").expect_err("takes no label of its own");
        asm("!addr 9foo = 1").expect_err("invalid symbol name");
    }

    /// `!ct` chooses the table `!text` converts through. The default is
    /// `raw`, which is why `!text` and `!raw` agreed byte-for-byte before
    /// this existed — and still do, until a table is named.
    #[test]
    fn ct_chooses_the_table_text_converts_through() {
        assert_eq!(
            asm("!text \"aA[]@\"").expect("default").bytes,
            vec![0x61, 0x41, 0x5B, 0x5D, 0x40]
        );
        assert_eq!(
            asm("!ct raw\n!text \"aA[]@\"").expect("raw").bytes,
            asm("!text \"aA[]@\"").expect("default").bytes
        );
        assert_eq!(
            asm("!ct pet\n!text \"aA[]@\"").expect("pet").bytes,
            asm("!pet \"aA[]@\"").expect("pet directive").bytes
        );
        assert_eq!(
            asm("!ct scr\n!text \"aA[]@\"").expect("scr").bytes,
            asm("!scr \"aA[]@\"").expect("scr directive").bytes
        );
        assert_eq!(
            asm("!convtab pet\n!text \"a\"").expect("spelling").bytes,
            vec![0x41]
        );
    }

    /// Everything the table does **not** reach: `!raw` bypasses it, `!pet`
    /// and `!scr` name their own, a number in a list is not a character, and
    /// `!byte` was never text.
    #[test]
    fn the_table_reaches_only_text() {
        assert_eq!(asm("!ct pet\n!raw \"a\"").expect("raw").bytes, vec![0x61]);
        assert_eq!(asm("!ct scr\n!pet \"a\"").expect("pet").bytes, vec![0x41]);
        assert_eq!(asm("!ct pet\n!scr \"a\"").expect("scr").bytes, vec![0x01]);
        assert_eq!(
            asm("!ct pet\n!text \"a\", 65").expect("num").bytes,
            vec![0x41, 0x41]
        );
        assert_eq!(asm("!ct pet\n!byte 65").expect("byte").bytes, vec![0x41]);
    }

    /// A second `!ct` **replaces**; `!xor`'s masks combine. Same block shape,
    /// different rule, and the block form restores either way.
    #[test]
    fn a_second_ct_replaces_where_a_second_xor_combines() {
        assert_eq!(
            asm("!ct pet\n!text \"a\"\n!ct raw\n!text \"a\"")
                .expect("replace")
                .bytes,
            vec![0x41, 0x61]
        );
        assert_eq!(
            asm("!ct pet {\n!text \"a\"\n}\n!text \"a\"")
                .expect("block")
                .bytes,
            vec![0x41, 0x61]
        );
        assert_eq!(
            asm("!ct pet {\n!ct scr {\n!text \"a\"\n}\n!text \"a\"\n}\n!text \"a\"")
                .expect("nested")
                .bytes,
            vec![0x01, 0x41, 0x61]
        );
        // A bare `!ct` is not scoped by `!if`, exactly as a bare `!xor` is not.
        assert_eq!(
            asm("!if 1 {\n!ct pet\n!text \"a\"\n}\n!text \"a\"")
                .expect("if")
                .bytes,
            vec![0x41, 0x41]
        );
    }

    /// The named encodings, and the two ACME refuses.
    #[test]
    fn only_the_named_encodings_are_taken() {
        asm("!ct wibble\n!text \"a\"").expect_err("unknown encoding");
        asm("!ct\n!text \"a\"").expect_err("no string given");
        // A table read from a file is a real ACME feature and is refused by
        // name here rather than mistaken for one of the three.
        let err = asm("!ct \"table.bin\"\n!text \"a\"")
            .expect_err("table file")
            .to_string();
        assert!(err.contains("not implemented"), "{err}");
    }

    /// `!xor` masks what its scope writes — including the opcode, which is
    /// why the mask has to reach the engine rather than the lowering.
    #[test]
    fn xor_masks_data_and_opcodes_alike() {
        assert_eq!(
            asm("!xor $ff {\n!text \"ab\"\n}").expect("text").bytes,
            vec![0x9E, 0x9D]
        );
        assert_eq!(asm("!xor $ff {\nnop\n}").expect("opcode").bytes, vec![0x15]);
        assert_eq!(
            asm("!xor $ff { !byte 1 }").expect("one line").bytes,
            vec![0xFE]
        );
    }

    /// A block restores the previous mask; a bare `!xor` does not, and runs
    /// on. Masks **combine** rather than replace, so two `$ff`s cancel.
    #[test]
    fn a_block_restores_the_mask_and_a_bare_xor_does_not() {
        assert_eq!(
            asm("!xor $ff {\n!byte 1\n}\n!byte 1")
                .expect("restore")
                .bytes,
            vec![0xFE, 0x01]
        );
        assert_eq!(
            asm("!xor $ff\n!byte 1\n!byte 2").expect("runs on").bytes,
            vec![0xFE, 0xFD]
        );
        assert_eq!(
            asm("!xor $f0\n!byte 0\n!xor $0f\n!byte 0")
                .expect("combine")
                .bytes,
            vec![0xF0, 0xFF]
        );
        assert_eq!(
            asm("!xor $ff\n!byte 1\n!xor $ff\n!byte 1")
                .expect("cancel")
                .bytes,
            vec![0xFE, 0x01]
        );
    }

    /// Only an `!xor` block scopes the mask. `!if` and `!zone` do not — a
    /// bare `!xor` inside either goes on masking after it closes.
    #[test]
    fn only_an_xor_block_scopes_the_mask() {
        assert_eq!(
            asm("!if 1 {\n!xor $ff\n!byte 0\n}\n!byte 0")
                .expect("if")
                .bytes,
            vec![0xFF, 0xFF]
        );
        assert_eq!(
            asm("!zone z {\n!xor $ff\n!byte 0\n}\n!byte 0")
                .expect("zone")
                .bytes,
            vec![0xFF, 0xFF]
        );
        // A nested block still restores, through the enclosing `!if`.
        assert_eq!(
            asm("!if 1 {\n!xor $ff {\n!byte 0\n}\n}\n!byte 0")
                .expect("nested")
                .bytes,
            vec![0xFF, 0x00]
        );
        // And a bare `!xor` inside an `!xor` block is undone with the block.
        assert_eq!(
            asm("!xor $f0 {\n!xor $0f\n!byte 0\n}\n!byte 0")
                .expect("bare in block")
                .bytes,
            vec![0xFF, 0x00]
        );
    }

    /// The mask reaches what the source *wrote*, and nothing else. `!fill`
    /// writes bytes and takes it; `!skip` and an `org` gap reserve space and
    /// do not.
    #[test]
    fn the_mask_spares_reserved_space() {
        assert_eq!(
            asm("!xor $ff {\n!fill 2\n}").expect("fill").bytes,
            vec![0xFF, 0xFF]
        );
        assert_eq!(
            asm("!xor $ff {\n!skip 2\n}").expect("skip").bytes,
            vec![0x00, 0x00]
        );
        let gap = asm("*=$1000\n!xor $ff {\nnop\n*=$1003\nnop\n}").expect("gap");
        assert_eq!(gap.bytes, vec![0x15, 0x00, 0x00, 0x15]);
    }

    /// `!xor` is range-checked where `!scrxor` is truncated. Same family,
    /// opposite answers, both ACME's.
    #[test]
    fn an_xor_value_is_range_checked_unlike_scrxor() {
        assert_eq!(
            asm("!xor 255 {\n!byte 0\n}").expect("max").bytes,
            vec![0xFF]
        );
        assert_eq!(
            asm("!xor -128 {\n!byte 0\n}").expect("min").bytes,
            vec![0x80]
        );
        asm("!xor 256 {\n!byte 0\n}").expect_err("number out of range");
        asm("!xor -129 {\n!byte 0\n}").expect_err("number out of range");
        // The contrast, which is the reason both tests exist.
        assert_eq!(
            asm("!scrxor 256, \"a\"").expect("truncates").bytes,
            vec![0x01]
        );
    }

    /// `!as`/`!rs` are accepted and emit nothing on a 6502. An operand is
    /// refused, as ACME refuses it.
    #[test]
    fn register_width_directives_emit_nothing() {
        assert_eq!(asm("!as\nlda #1").expect("as").bytes, vec![0xA9, 0x01]);
        assert_eq!(asm("!rs\nlda #1").expect("rs").bytes, vec![0xA9, 0x01]);
        asm("!as 1").expect_err("garbage at end of statement");
        asm("!rs 1").expect_err("garbage at end of statement");
    }

    /// `!eof` ends the file where it stands; nothing after it is parsed, so
    /// text that could not assemble never gets the chance to fail.
    #[test]
    fn eof_ends_the_file_without_parsing_the_rest() {
        assert_eq!(asm("nop\n!eof\n!!!garbage").expect("eof").bytes, vec![0xEA]);
        assert_eq!(
            asm("nop\n!endoffile\nlda").expect("endoffile").bytes,
            vec![0xEA]
        );
        assert!(asm("!eof\nnop").expect("at the top").bytes.is_empty());
        asm("nop\n!eof 1").expect_err("takes no operand");
    }

    /// Inside an open block it is an error, not an ending: ACME answers
    /// "Found end-of-file instead of '}'".
    #[test]
    fn eof_inside_an_open_block_is_an_error() {
        asm("nop\n!if 1 {\n!eof\n}\n!byte 2").expect_err("block still open");
    }

    /// The same check, reached the ordinary way. This was accepted before —
    /// an unclosed `!if 1 {` assembled its body and emitted bytes, where ACME
    /// refuses the file. Found while implementing `!eof`, which lands on the
    /// same path.
    #[test]
    fn an_unclosed_conditional_is_refused() {
        asm("!if 1 {\nnop").expect_err("unclosed `!if`");
        asm("!if 0 {\nnop\n} else {\n!byte 7").expect_err("unclosed `else`");
        // The closed forms still work, including nesting.
        assert_eq!(asm("!if 1 {\nnop\n}").expect("closed").bytes, vec![0xEA]);
        assert_eq!(
            asm("!if 0 {\nnop\n} else {\n!byte 7\n}")
                .expect("else")
                .bytes,
            vec![0x07]
        );
        assert_eq!(
            asm("!if 1 {\n!if 1 {\n!byte 3\n}\n}")
                .expect("nested")
                .bytes,
            vec![0x03]
        );
    }

    /// `!scrxor` converts to screen codes and masks — but only what it
    /// converted.
    ///
    /// It is not `!xor` wrapped around `!scr`, though it reads like it. The
    /// mask reaches the characters of a string and nothing else: a number in
    /// the list passes through unconverted *and* unmasked. That is the whole
    /// difference between the two directives, and it is why they are not one
    /// rule.
    #[test]
    fn scrxor_masks_what_it_converted_and_nothing_else() {
        assert_eq!(
            asm("!scrxor $80, \"ab\"").expect("str").bytes,
            vec![0x81, 0x82]
        );
        assert_eq!(asm("!scr \"ab\"").expect("scr").bytes, vec![0x01, 0x02]);
        // The number is 65 on the way in and $41 on the way out — neither
        // screen-converted nor masked. `!xor $80 { !scr 65 }` gives $c1.
        assert_eq!(asm("!scrxor $80, 65").expect("num").bytes, vec![0x41]);
        assert_eq!(
            asm("!scrxor $80, \"a\", 65, \"b\"").expect("mixed").bytes,
            vec![0x81, 0x41, 0x82]
        );
        assert_eq!(
            asm("!scrxor 0, \"ab\"").expect("zero").bytes,
            vec![0x01, 0x02]
        );
        asm("!scrxor $80 \"ab\"").expect_err("needs the comma");
    }

    /// The mask is the low byte of whatever was written, with no range check.
    ///
    /// ACME is inconsistent here and this follows it: `!initmem 256` is
    /// "Number out of range", while `!scrxor 256` masks silently. Refusing
    /// the second for symmetry would refuse source ACME accepts.
    #[test]
    fn a_scrxor_mask_is_truncated_not_range_checked() {
        // `!scr "a"` is $01 throughout; only the mask changes.
        for (written, expect) in [
            (-1i64, 0xFEu8),
            (256, 0x01),
            (257, 0x00),
            (511, 0xFE),
            (-129, 0x7E),
            (65535, 0xFE),
        ] {
            let out = asm(&format!("!scrxor {written}, \"a\"")).expect("masked");
            assert_eq!(out.bytes, vec![expect], "mask {written}");
        }
    }

    /// `!skip` reserves; `!initmem` says what lands in reserved space.
    ///
    /// The coupling is the point: `!skip` alone is zeros, and the same
    /// `!skip` after an `!initmem` is that byte instead. They cannot be
    /// implemented apart.
    #[test]
    fn skip_reserves_and_initmem_says_with_what() {
        assert_eq!(asm("!skip 4").expect("skip").bytes, vec![0, 0, 0, 0]);
        assert_eq!(
            asm("!initmem $ff\n!skip 3").expect("filled").bytes,
            vec![0xFF; 3]
        );
        assert_eq!(asm("!skip 2+1").expect("expr").bytes, vec![0, 0, 0]);
        assert!(asm("!skip 0\nnop").expect("zero is legal").bytes == vec![0xEA]);
        asm("!skip -1").expect_err("negative size argument");
        asm("!skip 3,$aa").expect_err("`!skip` takes one operand");
    }

    /// `!initmem` is not a statement that runs where it is written — it
    /// chooses what unwritten memory holds for the whole assembly. A `!skip`
    /// *earlier* in the file takes its value too, which is the behaviour that
    /// forced it out of the walk and into a pass the driver makes first.
    #[test]
    fn initmem_applies_to_the_whole_assembly_not_from_where_it_appears() {
        assert_eq!(
            asm("!skip 3\n!initmem $ff").expect("earlier skip").bytes,
            vec![0xFF; 3]
        );
    }

    /// It fills an `org` gap as well as a reservation — the same unwritten
    /// memory either way.
    #[test]
    fn initmem_fills_an_origin_gap() {
        // The origin is explicit because the helper only supplies one when
        // the source names none, and `*=$c004` below counts as naming one.
        let out = asm("*=$c000\n!initmem $ff\nnop\n*=$c004\nnop").expect("gap");
        assert_eq!(out.bytes, vec![0xEA, 0xFF, 0xFF, 0xFF, 0xEA]);
    }

    /// ACME places source-ordered regions by address. The lowest written
    /// address becomes the flat image origin, independently of which region
    /// appeared first.
    #[test]
    fn backwards_origins_are_placed_by_address() {
        let out = asm("*=$1004\nhigh !byte $44\n*=$1000\nlow !byte $11\n*=$1002\nmid !byte $22")
            .expect("address-placed regions");
        assert_eq!(out.origin, Some(0x1000));
        assert_eq!(out.bytes, vec![0x11, 0, 0x22, 0, 0x44]);
        assert_eq!(out.symbols.get("low"), Some(&0x1000));
        assert_eq!(out.symbols.get("mid"), Some(&0x1002));
        assert_eq!(out.symbols.get("high"), Some(&0x1004));
        let offsets = out
            .debug
            .lines
            .iter()
            .map(|line| line.offset)
            .collect::<Vec<_>>();
        assert_eq!(offsets, vec![4, 0, 2]);
    }

    /// A later region overwrites only bytes it writes. Its origin gap remains
    /// unwritten, so it cannot erase an earlier region that lies inside that
    /// gap; `!initmem` fills whatever remains unwritten after placement.
    #[test]
    fn later_regions_overlay_bytes_but_not_origin_gaps() {
        let out =
            asm("!initmem $aa\n*=$1004\n!byte $44\n*=$1000\n!byte $11,$22\n*=$1001\n!byte $33")
                .expect("overlay");
        assert_eq!(out.origin, Some(0x1000));
        assert_eq!(out.bytes, vec![0x11, 0x33, 0xAA, 0xAA, 0x44]);
        assert!(
            out.warnings
                .iter()
                .any(|warning| warning.message.contains("later bytes overwrite"))
        );
    }

    /// A second `!initmem` is ignored, not obeyed and not refused: ACME warns
    /// "Memory already initialised" and keeps the first value.
    #[test]
    fn a_second_initmem_does_not_displace_the_first() {
        let out = asm("!initmem $ff\n!skip 2\n!initmem $aa\n!skip 2").expect("twice");
        assert_eq!(out.bytes, vec![0xFF; 4]);
    }

    /// The fill byte spans signed and unsigned like every other ACME byte.
    #[test]
    fn an_initmem_byte_is_signed_or_unsigned() {
        assert_eq!(asm("!initmem -1\n!skip 1").expect("neg").bytes, vec![0xFF]);
        asm("!initmem 256\n!skip 1").expect_err("number out of range");
    }

    /// Six spellings of one rule. The byte order is the directive's, not the
    /// CPU's: a 6502 is little-endian and `!be24` still emits big-endian.
    #[test]
    fn the_sized_family_takes_its_byte_order_from_the_directive() {
        assert_eq!(asm("!be16 $1234").expect("be16").bytes, vec![0x12, 0x34]);
        assert_eq!(asm("!le16 $1234").expect("le16").bytes, vec![0x34, 0x12]);
        assert_eq!(
            asm("!be24 $123456").expect("be24").bytes,
            vec![0x12, 0x34, 0x56]
        );
        assert_eq!(
            asm("!le24 $123456").expect("le24").bytes,
            vec![0x56, 0x34, 0x12]
        );
        assert_eq!(
            asm("!be32 $12345678").expect("be32").bytes,
            vec![0x12, 0x34, 0x56, 0x78]
        );
        assert_eq!(
            asm("!le32 $12345678").expect("le32").bytes,
            vec![0x78, 0x56, 0x34, 0x12]
        );
    }

    /// The range is ACME's, per width: signed or unsigned, whichever the
    /// source meant, and an error either side of that.
    #[test]
    fn a_sized_value_spans_signed_and_unsigned() {
        assert_eq!(asm("!be16 -1").expect("neg").bytes, vec![0xFF, 0xFF]);
        assert_eq!(asm("!be16 -32768").expect("min").bytes, vec![0x80, 0x00]);
        assert_eq!(asm("!be16 65535").expect("max").bytes, vec![0xFF, 0xFF]);
        asm("!be16 -32769").expect_err("below the signed floor");
        asm("!be16 65536").expect_err("above the unsigned ceiling");
        assert_eq!(
            asm("!be24 $ffffff").expect("24-bit max").bytes,
            vec![0xFF, 0xFF, 0xFF]
        );
        asm("!be24 $1000000").expect_err("above the 24-bit ceiling");
    }

    /// A list, and a value that is not a literal — the directive takes the
    /// same operand shape the other data directives do.
    #[test]
    fn a_sized_directive_takes_a_list_and_an_expression() {
        assert_eq!(
            asm("!be16 $1234, $5678").expect("list").bytes,
            vec![0x12, 0x34, 0x56, 0x78]
        );
        assert_eq!(
            asm("!le32 1+2").expect("expr").bytes,
            vec![0x03, 0x00, 0x00, 0x00]
        );
        // A one-character string is a value to ACME, as it is everywhere else
        // it wants a number; two characters is "There's more than one
        // character".
        assert_eq!(asm("!be16 \"a\"").expect("char").bytes, vec![0x00, 0x61]);
        asm("!be16 \"ab\"").expect_err("more than one character");
    }

    /// `!fi` is `!fill`, not a conditional terminator. Probed: ACME 0.97
    /// answers "No value given." for a bare `!fi`, which is what `!fill`
    /// answers too — it wants a count.
    #[test]
    fn fi_is_the_short_spelling_of_fill() {
        assert_eq!(asm("!fi 3").expect("fi").bytes, vec![0, 0, 0]);
        assert_eq!(asm("!fi 2+1, $bb").expect("fi value").bytes, vec![0xBB; 3]);
        assert_eq!(
            asm("!fi 3,$ff").expect("fi").bytes,
            asm("!fill 3,$ff").expect("fill").bytes
        );
    }

    /// `!raw` emits without the conversion table. It agrees with `!text`
    /// today only because `!ct` is not implemented and ACME's default table
    /// converts nothing — so this asserts the bytes, not the equivalence,
    /// and stays true when `!text` starts converting.
    #[test]
    fn raw_emits_its_operands_unconverted() {
        assert_eq!(
            asm("!raw \"ab\", 3").expect("raw").bytes,
            vec![0x61, 0x62, 3]
        );
        assert_eq!(asm("!raw 65").expect("num").bytes, vec![0x41]);
        assert_eq!(asm("!raw 255").expect("max").bytes, vec![0xFF]);
        assert_eq!(asm("!raw -128").expect("min").bytes, vec![0x80]);
        asm("!raw -129").expect_err("out of range");
    }

    /// `!hex` takes bare digit pairs. Every case here was probed against
    /// ACME 0.97 first — the surprising ones are that pairing is counted per
    /// whitespace-separated token, and that an empty operand is allowed.
    #[test]
    fn hex_takes_bare_digit_pairs() {
        assert_eq!(
            asm("!hex 0f1e2d").expect("hex").bytes,
            vec![0x0F, 0x1E, 0x2D]
        );
        assert_eq!(
            asm("!hex 0F1E2D").expect("upper").bytes,
            vec![0x0F, 0x1E, 0x2D]
        );
        assert_eq!(
            asm("!hex 0f 1e 2d").expect("spaced").bytes,
            vec![0x0F, 0x1E, 0x2D]
        );
        assert!(asm("!hex").expect("empty is not an error").bytes.is_empty());
    }

    /// The two refusals ACME distinguishes, which a tidier implementation
    /// would merge into one. `0f1g2d` stops after three digits — an odd
    /// count — and `0fgg1e` stops after two, which is even but leaves text
    /// behind.
    #[test]
    fn hex_reports_an_odd_count_before_a_bad_character() {
        for odd in ["!hex 0f1", "!hex 0 f", "!hex 0f1g2d", "!hex 0f 1"] {
            let err = asm(odd).expect_err(odd).to_string();
            assert!(err.contains("pairs"), "{odd}: {err}");
        }
        for junk in ["!hex 0fgg1e", "!hex $0f", "!hex 0f,1e"] {
            let err = asm(junk).expect_err(junk).to_string();
            assert!(
                !err.contains("pairs"),
                "{junk} is even, not unpaired: {err}"
            );
        }
    }

    /// The four spellings ACME has **retired**. Refusing them is matching the
    /// reference, not lagging it: 0.97 answers "obsolete" for each however it
    /// is invoked, so an assembler that took them would accept source ACME
    /// rejects.
    ///
    /// Named individually rather than left to the generic surface test,
    /// because that test would still pass if these were moved back to
    /// `KnownUnsupported` — and the failure that invites is somebody reading
    /// "the gap is ours" and closing it.
    #[test]
    fn acmes_retired_spellings_are_the_references_refusal_not_our_gap() {
        for spelling in ["!cbm", "!sz", "!subzone", "!realpc"] {
            let err = asm(&format!("\t{spelling}\n")).expect_err(spelling);
            let message = err.to_string();
            assert!(
                message.contains("obsolete"),
                "{spelling} does not quote ACME's own answer: {message}"
            );
            assert!(
                !message.contains("the gap is ours"),
                "{spelling} claims a gap ACME does not leave: {message}"
            );
        }
    }

    /// Assemble ACME source, giving it a default origin when it declares none —
    /// so the byte-output tests below needn't each set `*=`. (ACME requires `*=`
    /// before code/data; a source that sets its own origin starts with `*` and
    /// passes straight through. The requirement itself is covered by
    /// `emitting_without_an_origin_is_an_error`.)
    fn asm(src: &str) -> Result<AssemblyResult, AsmError> {
        let sets_origin = src.lines().any(|l| l.trim_start().starts_with('*'));
        if sets_origin {
            assemble_acme(src)
        } else {
            assemble_acme(&format!("*= $c000\n{src}"))
        }
    }

    #[test]
    fn colon_separates_statements_after_a_label() {
        let src = "txt_hearts: !text \"HEARTS\" : !byte 0\n";
        let out = asm(src).expect("colon-separated Rachel source");
        assert_eq!(out.bytes, b"HEARTS\0");
        assert!(out.symbols.contains_key("txt_hearts"));
    }

    #[test]
    fn colon_in_a_literal_is_not_a_statement_separator() {
        assert_eq!(
            asm("!text \"a:b\" : !byte 0\n").expect("literal").bytes,
            b"a:b\0"
        );
    }

    #[test]
    fn a_label_after_a_separator_binds_at_that_point() {
        let out =
            asm("!byte 1 : second: !byte 2\n!byte <second, >second\n").expect("mid-line label");
        assert_eq!(out.bytes, vec![1, 2, 1, 0xc0]);
    }

    #[test]
    fn formatting_colon_separated_statements_preserves_bytes() {
        let src = "*= $c000\nstart: lda #1 : sta $d020 : rts\n";
        let before = assemble_acme(src).expect("assembles").bytes;
        let formatted = crate::format_acme(src).expect("formats");
        assert_eq!(assemble_acme(&formatted).expect("formatted").bytes, before);
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
        assert_eq!(a.origin, Some(0x0801));
        assert_eq!(a.bytes, vec![0x0C, 0x08, 0x0A, 0x00]);
    }

    #[test]
    fn star_equals_with_spaces() {
        assert_eq!(
            asm("* = $1000\n!byte 1\n").expect("spaced").origin,
            Some(0x1000)
        );
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

    /// ACME 0.97 gives `%` two position-dependent meanings: binary prefix and
    /// infix modulo. Modulo shares `*`/`/` precedence (probe bytes 0a 02 05 02),
    /// and the engine's safer divide-by-zero diagnostic replaces ACME's wording.
    #[test]
    fn percent_is_binary_prefix_or_modulo_by_position() {
        let a = asm("!byte %1010\n!byte 17 % 5\n!byte 2 + 9 % 4 * 3\n!byte 17%5")
            .expect("ACME percent forms");
        assert_eq!(a.bytes, vec![0x0A, 0x02, 0x05, 0x02]);

        let macro_use = asm("!macro rem .a, .b { !byte .a % .b }\n+rem 17, 5")
            .expect("modulo survives macro substitution");
        assert_eq!(macro_use.bytes, vec![2]);

        let e = asm("!byte 7 % 0").expect_err("zero divisor");
        assert!(e.to_string().contains("modulo by zero"));
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

    /// U4 (probe-pinned): an anonymous definition inside an **untaken** `!if`
    /// branch does not exist — a later `-` reference skips over it to the live
    /// definition, exactly as acme resolves it (a9 01 d0 fc). The old textual
    /// prescan collected the dead definition and failed with an undefined
    /// symbol; evaluation-order collection fixes it.
    #[test]
    fn anon_in_untaken_branch_does_not_exist() {
        let a = asm("*= $1000\n\
             FLAG = 0\n\
             -       lda #1\n\
             !if FLAG {\n\
             -       lda #2\n\
             }\n\
             \x20       bne -\n")
        .expect("the dead branch's anon is invisible");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xD0, 0xFC]);
    }

    /// U4 (probe-pinned): a forward `+` reference never matches a definition
    /// on its **own** line — acme rejects `+ jmp +` with `Value not defined`
    /// — while the backward self-reference (`- jmp -`, above) stays legal.
    #[test]
    fn forward_anon_never_matches_its_own_line() {
        let err = asm("*= $1000\n+      jmp +\n").expect_err("strictly forward");
        assert!(
            err.message.contains("no anonymous label"),
            "unexpected: {err}"
        );
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

    /// `!error` stops; `!warn` notes and carries on. Both render their operand
    /// list, and — the reason they are operations rather than parse errors —
    /// neither fires from a branch the program does not take.
    #[test]
    fn source_requested_diagnostics() {
        let err = asm("*= $1000\n!error \"stop \", 7\n").expect_err("aborts");
        assert!(err.to_string().contains("stop 7"), "got `{err}`");

        let out = asm("*= $1000\n lda #1\n!warn \"careful \", 5\n rts\n").expect("warns");
        assert_eq!(out.bytes, vec![0xA9, 0x01, 0x60]);
        assert!(
            out.warnings.iter().any(|w| w.message.contains("careful 5")),
            "got {:?}",
            out.warnings
        );

        // `!serious` is `!error` with a different word on it.
        let grave = asm("*= $1000\n!serious \"very bad\"\n").expect_err("aborts");
        assert!(grave.to_string().contains("very bad"), "got `{grave}`");

        // The whole point of lowering these rather than raising them at parse.
        for d in ["!error", "!serious"] {
            let quiet =
                asm(&format!("*= $1000\n!if 0 {{\n{d} \"never\"\n}}\n rts\n")).expect("untaken");
            assert_eq!(quiet.bytes, vec![0x60], "{d}");
        }
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
        // `!zone` (bare and titled) emits no bytes — it only scopes `.`-locals
        // (U7). Matches acme's bytes (a901 a902).
        let a = asm("*= $1000\n!zone\n        lda #1\n!zone foo\n        lda #2\n").expect("zone");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xA9, 0x02]);
    }

    // --- `!zone` local-label scoping (U7) — every byte sequence and error
    // posture below is pinned by the acme 0.97 probe runs (z1-z20, zh1-zh9,
    // za-zg in the U7 report).

    /// AE3: a `.local` reused across two zones assembles — named zones,
    /// anonymous zones, and the `!zn` alias all mint fresh scopes (probes
    /// z1, z2, zn), and the two locals are two distinct qualified symbols
    /// (C3, KTD4).
    #[test]
    fn zone_scopes_local_reuse() {
        for src in [
            "*= $1000\n!zone one\n.loop   lda #1\n        bne .loop\n!zone two\n.loop   lda #2\n        bne .loop\n",
            "*= $1000\n!zone\n.loop   lda #1\n        bne .loop\n!zone\n.loop   lda #2\n        bne .loop\n",
            "*= $1000\n!zn one\n.loop   lda #1\n        bne .loop\n!zn\n.loop   lda #2\n        bne .loop\n",
        ] {
            let a = asm(src).expect(src);
            assert_eq!(
                a.bytes,
                vec![0xA9, 0x01, 0xD0, 0xFC, 0xA9, 0x02, 0xD0, 0xFC]
            );
        }
        // The named run's two `.loop`s are distinct public symbols (AE3 + C3).
        let a = asm("*= $1000\n!zone one\n.loop   lda #1\n        bne .loop\n!zone two\n.loop   lda #2\n        bne .loop\n")
            .expect("named zones");
        assert_eq!(a.symbols.get("one@1.loop"), Some(&0x1000));
        assert_eq!(a.symbols.get("two@2.loop"), Some(&0x1004));
    }

    /// A global label does NOT delimit local scope — zones are the only
    /// delimiter (probe z3, the acme-vs-sjasmplus divergence), and a forward
    /// local reference crosses a global freely within one zone (probe z7).
    #[test]
    fn zone_globals_do_not_delimit_locals() {
        let err = asm("*= $1000\nfirst   lda #1\n.l      bne .l\nsecond  lda #2\n.l      bne .l\n")
            .expect_err("duplicate in the same zone");
        assert!(err.message.contains("duplicate"), "got {err}");
        let a = asm("*= $1000\nfirst   lda #1\n        bne .later\nsecond  lda #2\n.later  rts\n")
            .expect("forward ref across a global");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xD0, 0x02, 0xA9, 0x02, 0x60]);
    }

    /// A `.local` before any `!zone` lives in the initial zone with its bare
    /// key (probe z4) — zone-free programs keep today's public symbol keys.
    #[test]
    fn zone_initial_scope_keeps_bare_keys() {
        let a = asm("*= $1000\n.early  lda #1\n        bne .early\n").expect("initial zone");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xD0, 0xFC]);
        assert_eq!(a.symbols.get(".early"), Some(&0x1000));
    }

    /// Re-entering a zone title is a FRESH zone — the title is cosmetic
    /// (probes z12/z12b): the first zone's `.x` is not visible, and `.x`
    /// redefines cleanly.
    #[test]
    fn zone_title_reentry_is_a_fresh_zone() {
        let a = asm("*= $1000\n!zone one\n.x      lda #1\n!zone two\n        lda #2\n!zone one\n.x      lda #3\n")
            .expect("re-entry redefines");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xA9, 0x02, 0xA9, 0x03]);
        let err = asm("*= $1000\n!zone one\n.x      lda #1\n!zone two\n        lda #2\n!zone one\n        bne .x\n")
            .expect_err("the first zone's .x is gone");
        assert!(err.message.contains("undefined"), "got {err}");
    }

    /// The block form `!zone [title] { … }` resumes the enclosing zone at `}`
    /// (probes z6b, zh2, zh8 — nested blocks restore level by level).
    #[test]
    fn zone_block_restores_enclosing_zone() {
        let a = asm("*= $1000\n.out    lda #1\n!zone inner {\n.loop   lda #2\n        bne .loop\n}\n        bne .out\n")
            .expect("block restores");
        assert_eq!(
            a.bytes,
            vec![0xA9, 0x01, 0xA9, 0x02, 0xD0, 0xFC, 0xD0, 0xF8]
        );
        let nested = asm("*= $1000\n.a      lda #1\n!zone o {\n.a      lda #2\n!zone i {\n.a      lda #3\n        bne .a\n}\n        bne .a\n}\n        bne .a\n")
            .expect("nested blocks");
        assert_eq!(
            nested.bytes,
            vec![
                0xA9, 0x01, 0xA9, 0x02, 0xA9, 0x03, 0xD0, 0xFC, 0xD0, 0xF8, 0xD0, 0xF4
            ]
        );
    }

    /// `.name = expr` constants, `!set .name` variables, and `!ifdef .name`
    /// tests are all zone-scoped (probes z16, zh6, zh7).
    #[test]
    fn zone_scopes_constants_set_and_ifdef() {
        let consts = asm(
            "*= $1000\n!zone one\n.c = 3\n        lda #.c\n!zone two\n.c = 5\n        lda #.c\n",
        )
        .expect("zone-scoped constants");
        assert_eq!(consts.bytes, vec![0xA9, 0x03, 0xA9, 0x05]);
        let set =
            asm("*= $1000\n!zone one\n!set .n = 5\n        lda #.n\n!zone two\n        lda #.n\n")
                .expect_err("!set is zone-scoped");
        assert!(set.message.contains("undefined"), "got {set}");
        let ifdef = asm("*= $1000\n!zone one\n.flag = 1\n!ifdef .flag {\n        lda #1\n}\n!zone two\n!ifdef .flag {\n        lda #2\n}\n        nop\n")
            .expect("!ifdef is zone-scoped");
        assert_eq!(ifdef.bytes, vec![0xA9, 0x01, 0xEA]);
    }

    /// Zone × conditionals (probes zd, ze): an untaken branch's `!zone` never
    /// runs (still zone one — redefining `.x` errors), while a taken branch's
    /// line-form `!zone` persists past the conditional's `}`.
    #[test]
    fn zone_interacts_with_conditionals_per_the_probes() {
        let untaken = asm("FLAG = 0\n*= $1000\n!zone one\n.x      lda #1\n!if FLAG {\n!zone two\n}\n.x      lda #2\n")
            .expect_err("untaken !zone never runs");
        assert!(untaken.message.contains("duplicate"), "got {untaken}");
        let taken = asm("FLAG = 1\n*= $1000\n!zone one\n.x      lda #1\n!if FLAG {\n!zone two\n}\n.x      lda #2\n")
            .expect("taken !zone persists past the `}`");
        assert_eq!(taken.bytes, vec![0xA9, 0x01, 0xA9, 0x02]);
    }

    /// A `.local` label on the `!zone` line binds in the OLD zone, before the
    /// switch (probe zf2).
    #[test]
    fn zone_line_label_binds_in_the_old_zone() {
        let a = asm("*= $1000\n!zone one\n.x      lda #1\n        bne .mark\n.mark   !zone two\n        lda #2\n")
            .expect("label before the switch");
        assert_eq!(a.bytes, vec![0xA9, 0x01, 0xD0, 0x00, 0xA9, 0x02]);
    }

    /// Malformed zones match acme's postures: a multi-token title is rejected
    /// (probe zh4) and an unclosed block is caught at EOF (probe zh5). There
    /// is no cross-zone reference syntax (probe z5): `one.loop` is just an
    /// (undefined) plain symbol.
    #[test]
    fn zone_malformed_forms_are_rejected() {
        let junk = asm("*= $1000\n!zone one two\n        nop\n").expect_err("junk title");
        assert!(junk.message.contains("title"), "got {junk}");
        let unclosed = asm("*= $1000\n!zone one {\n        nop\n").expect_err("unclosed block");
        assert!(unclosed.message.contains("unterminated"), "got {unclosed}");
        let cross = asm("*= $1000\n!zone one\n.loop   lda #1\n!zone two\n        jmp one.loop\n")
            .expect_err("no cross-zone references");
        assert!(cross.message.contains("undefined"), "got {cross}");
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

    // ----- Macros (#93) -------------------------------------------------
    //
    // Every expectation is a byte string acme 0.97 produced for the same
    // source. acme is the dialect that made the shared collector delegate:
    // its bodies are brace-delimited at character level, and its macros are
    // identified by arity as well as name.

    /// Definition, invocation, parameters, and substitution ahead of
    /// evaluation — the parts every dialect shares, spelled acme's way.
    #[test]
    fn macros_expand() {
        assert_eq!(
            asm("!macro nop2 {\n\tnop\n\tnop\n}\n+nop2\n")
                .expect("nop2")
                .bytes,
            vec![0xEA, 0xEA]
        );
        assert_eq!(
            asm("!macro ldav .v {\n\tlda #.v\n}\n+ldav 5\n")
                .expect("one parameter")
                .bytes,
            vec![0xA9, 0x05]
        );
        assert_eq!(
            asm("!macro ldav .v, .w {\n\tlda #.v\n\tldx #.w\n}\n+ldav 5, 7\n")
                .expect("two parameters")
                .bytes,
            vec![0xA9, 0x05, 0xA2, 0x07]
        );
        assert_eq!(
            asm("!macro ldav .v {\n\tlda #.v*2\n}\n+ldav 5\n")
                .expect("substitution precedes evaluation")
                .bytes,
            vec![0xA9, 0x0A]
        );
        assert_eq!(
            asm("!MACRO nop2 {\n\tnop\n}\n+nop2\n")
                .expect("case-insensitive keyword")
                .bytes,
            vec![0xEA]
        );
    }

    /// A body is delimited by braces at character level, so the collector must
    /// count depth rather than recognise a line.
    ///
    /// Three things a line-oriented collector gets wrong, and acme accepts:
    /// braces nesting inside a body, both braces sharing a line with code, and
    /// a `}` inside a string closing nothing.
    #[test]
    fn a_body_is_delimited_by_braces_not_by_a_line() {
        assert_eq!(
            asm("!macro m {\n\t!if 1 {\n\t\tnop\n\t}\n\tlda #1\n}\n+m\n")
                .expect("nested braces")
                .bytes,
            vec![0xEA, 0xA9, 0x01]
        );
        assert_eq!(
            asm("!macro nop2 { nop\n\tnop }\n+nop2\n")
                .expect("braces share lines with code")
                .bytes,
            vec![0xEA, 0xEA]
        );
        assert_eq!(
            asm("!macro m {\n\t!text \"a}b\"\n\tnop\n}\n+m\n")
                .expect("a brace in a string is text")
                .bytes,
            vec![0x61, 0x7D, 0x62, 0xEA]
        );
        // The opening brace must be on the header line: acme rejects one alone
        // on the next, and so must we rather than swallowing the file.
        asm("!macro nop2\n{\n\tnop\n}\n+nop2\n").expect_err("`{` must be on the header line");
        asm("!macro nop2 {\n\tnop\n").expect_err("unterminated body");
    }

    /// Arity is part of a macro's identity. Two definitions of one name coexist
    /// and the call site picks by count; a count matching neither is a macro
    /// acme has never heard of, not a bad argument list.
    #[test]
    fn one_name_may_carry_several_arities() {
        assert_eq!(
            asm("!macro ldav .v {\n\tlda #.v\n}\n!macro ldav .v, .w {\n\tlda #.v\n\tldx #.w\n}\n+ldav 5\n+ldav 5, 7\n")
                .expect("two arities")
                .bytes,
            vec![0xA9, 0x05, 0xA9, 0x05, 0xA2, 0x07]
        );
        let err = asm("!macro ldav .v {\n\tlda #.v\n}\n+ldav 5, 9\n")
            .expect_err("no two-argument ldav exists");
        assert!(err.message.contains("ldav"), "{err:?}");
        asm("!macro ldav .v, .w {\n\tlda #.v\n}\n+ldav 5\n")
            .expect_err("no one-argument ldav exists");
    }

    /// A `.dotted` label in a body is scoped to the expansion — sjasmplus's
    /// rule exactly, and the one local mechanism two dialects agreed on. A
    /// plain label stays global and the second invocation collides.
    #[test]
    fn a_dotted_label_is_scoped_to_its_expansion() {
        assert_eq!(
            asm("!macro delay {\n.spin\tdex\n\tbne .spin\n}\n+delay\n+delay\n")
                .expect("two expansions")
                .bytes,
            vec![0xCA, 0xD0, 0xFD, 0xCA, 0xD0, 0xFD]
        );
        asm("!macro delay {\nspin\tdex\n}\n+delay\n+delay\n")
            .expect_err("a plain label is global and collides");
    }

    /// A call is `+name`, and it may be indented. A bare name is **not** a
    /// call: acme reads it as a label — warning `Label name not in leftmost
    /// column` and emitting nothing — so the macro must not expand there.
    #[test]
    fn a_call_needs_its_plus() {
        assert_eq!(
            asm("!macro ldav .v {\n\tlda #.v\n}\n\t+ldav 5\n")
                .expect("indented call")
                .bytes,
            vec![0xA9, 0x05]
        );
        let bare = asm("!macro nop2 {\n\tnop\n}\n\tnop2\n").expect("a label, not a call");
        assert!(bare.bytes.is_empty(), "the macro must not have expanded");
        assert!(
            bare.symbols.contains_key("nop2"),
            "acme binds it as a label"
        );
    }

    /// The formatter lays source out; it does not rewrite programs.
    ///
    /// The half that matters here is that assembling the same text still
    /// expands, so the two really are different parses of it rather than one
    /// of them being broken. See `decisions/macro-expansion-framework.md`.
    #[test]
    fn formatting_does_not_expand() {
        let src = "*= $c000\n!macro ldav .v {\n\tlda #.v\n}\n+ldav 5\n";
        let out = crate::format_acme(src).expect("the walk copies a definition");

        // The macro survives as a macro. If it were expanded, `!macro` would be
        // gone and `lda #5` would be in its place.
        assert!(out.contains("!macro ldav .v {"), "{out}");
        assert!(out.contains("+ldav 5"), "{out}");
        assert!(!out.contains("lda #5"), "expanded into the output:\n{out}");

        assert_eq!(
            crate::assemble_acme(src).expect("assembles").bytes,
            vec![0xA9, 0x05]
        );
    }

    /// Formatting a macro changes the layout and not the program.
    ///
    /// acme is the one dialect measured whose bodies are brace-delimited at
    /// *character* level, so the shapes here are the ones a line-at-a-time copy
    /// gets wrong: both braces sharing a line with code, a nested block inside
    /// a body, and a `}` inside a string or a character constant.
    #[test]
    fn a_formatted_macro_assembles_to_the_same_bytes() {
        for src in [
            "*= $c000\n!macro ldav .v {\n\tlda #.v\n}\n+ldav 5\n\trts\n",
            "*= $c000\n!macro nop2 { nop\n\tnop }\n+nop2\n\trts\n",
            "*= $c000\n!macro guard .v {\n\t!if .v >= 3 {\n\t\tlda #.v\n\t} else {\n\t\tlda #0\n\t}\n}\n+guard 5\n+guard 1\n\trts\n",
            "*= $c000\n!macro brace {\n\t!byte '}'\n\t!text \"}\"\n}\n+brace\n\trts\n",
        ] {
            let before = crate::assemble_acme(src).expect(src).bytes;
            let formatted = crate::format_acme(src).expect(src);
            let after = crate::assemble_acme(&formatted)
                .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
                .bytes;
            assert_eq!(
                before, after,
                "formatting changed the program:\n{formatted}"
            );

            // And formatting is idempotent, so a second run is a no-op.
            let again = crate::format_acme(&formatted).expect("formats");
            assert_eq!(formatted, again, "{formatted}");
        }
    }

    /// A definition that never closes is refused, rather than swallowing the
    /// rest of the file as a body.
    #[test]
    fn an_unterminated_definition_is_refused() {
        let err = crate::format_acme("*= $c000\n!macro ldav .v {\n\tlda #.v\n")
            .expect_err("no closing brace");
        assert!(err.message.contains("missing `}`"), "{err:?}");
    }

    // -----------------------------------------------------------------------
    // `!for`. Measured against acme 0.97 — the two syntaxes agree on nothing
    // but the name coming first.
    // -----------------------------------------------------------------------

    /// The old two-argument form runs `1 ..= n`.
    #[test]
    fn the_old_for_syntax_counts_from_one() {
        assert_eq!(
            crate::assemble_acme("*=$801\n!for i, 3 { !byte i }\n")
                .expect("assembles")
                .bytes,
            vec![1, 2, 3]
        );
    }

    /// The three-argument form is inclusive at both ends and **counts down**
    /// when the end is below the start. `!for i, 3, 1` is 3, 2, 1 — not empty,
    /// and not 1, 2, 3.
    #[test]
    fn the_new_for_syntax_is_inclusive_and_may_descend() {
        for (src, want) in [
            ("*=$801\n!for i, 5, 7 { !byte i }\n", vec![5, 6, 7]),
            ("*=$801\n!for i, 3, 1 { !byte i }\n", vec![3, 2, 1]),
            ("*=$801\n!for i, 4, 4 { !byte i }\n", vec![4]),
        ] {
            assert_eq!(crate::assemble_acme(src).expect(src).bytes, want, "{src}");
        }
    }

    /// Descending belongs to the three-argument form **only**. Sharing the rule
    /// made `!for i, 0` run its body twice, counting 1 down to 0.
    #[test]
    fn the_old_syntax_never_descends() {
        assert_eq!(
            crate::assemble_acme("*=$801\n!for i, 0 { !byte 9 }\n!byte $ff\n")
                .expect("assembles")
                .bytes,
            vec![0xFF],
            "the body must not run at all"
        );
        crate::assemble_acme("*=$801\n!for i, -2 { !byte 9 }\n")
            .expect_err("acme rejects a negative count in the old form");
    }

    /// The loop variable is baked into each use, so it holds a different value
    /// on each pass — a label could not, because the engine resolves those
    /// once, in a later pass, against one table.
    #[test]
    fn the_loop_variable_is_live_in_the_body() {
        assert_eq!(
            crate::assemble_acme("*=$801\n!for i, 0, 3 {\n lda #i\n}\n")
                .expect("assembles")
                .bytes,
            vec![0xA9, 0, 0xA9, 1, 0xA9, 2, 0xA9, 3]
        );
        // Bounds fold against the environment above the loop.
        assert_eq!(
            crate::assemble_acme("*=$801\nn = 2\n!for i, n, n+1 { !byte i }\n")
                .expect("assembles")
                .bytes,
            vec![2, 3]
        );
    }

    /// Nesting, in both the multi-line and the one-line form. The one-line case
    /// is the one that breaks a naive parse: the body must be read as a block,
    /// and its closing brace found by depth rather than by the first `}`.
    #[test]
    fn for_blocks_nest() {
        for src in [
            "*=$801\n!for i, 1, 2 { !for j, 1, 2 { !byte i*16+j } }\n",
            "*=$801\n!for i, 1, 2 {\n\t!for j, 1, 2 {\n\t\t!byte i*16+j\n\t}\n}\n",
        ] {
            assert_eq!(
                crate::assemble_acme(src).expect(src).bytes,
                vec![0x11, 0x12, 0x21, 0x22],
                "{src}"
            );
        }
    }

    /// Formatting a `!for` changes the layout and not the program.
    ///
    /// A one-line block comes back multi-line, which `!if` does not do — it
    /// keeps an inline body. That is a layout choice rather than a rewrite: the
    /// bytes are the same, the result is idempotent, and real acme assembles
    /// it. Preserving the inline form is worth doing when a reader asks for it.
    #[test]
    fn a_formatted_for_assembles_to_the_same_bytes() {
        let src = "*=$801\n!for i, 1, 3 {\n\tlda #i\n\tsta $400+i\n}\n!for j, 2 { !byte j }\n";
        let before = crate::assemble_acme(src).expect("assembles").bytes;
        let formatted = crate::format_acme(src).expect("formats");
        let after = crate::assemble_acme(&formatted)
            .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
            .bytes;
        assert_eq!(
            before, after,
            "formatting changed the program:\n{formatted}"
        );

        let again = crate::format_acme(&formatted).expect("formats");
        assert_eq!(formatted, again, "{formatted}");
    }

    // -----------------------------------------------------------------------
    // #128 gaps 1 and 3. Probed against acme 0.97, 2026-08-23.
    // -----------------------------------------------------------------------

    /// `!if` has every comparison the reference has. `<` and `>` were left out
    /// because they collide with the low-byte/high-byte prefixes, with a note
    /// that the curriculum only used `=` — but the curriculum was written
    /// against what this assembler accepts, so it cannot signal demand for a
    /// form the assembler rejects.
    #[test]
    fn an_if_condition_takes_every_comparison() {
        let taken = |c: &str| {
            asm(&format!("* = $0000\n!if {c} {{\n lda #5\n}}\n"))
                .unwrap_or_else(|e| panic!("`{c}`: {e}"))
                .bytes
                == vec![0xA9, 0x05]
        };
        assert!(taken("5 > 3"));
        assert!(!taken("5 < 3"));
        assert!(taken("3 < 5"));
        assert!(!taken("3 > 5"));
        assert!(taken("5 <> 3"));
        assert!(!taken("5 <> 5"));
        assert!(taken("5 >= 5"));
        assert!(taken("5 <= 5"));
        assert!(taken("5 = 5"));
        assert!(taken("5 != 3"));
        assert!(taken("1 + 2 > 2"), "the left side is a whole expression");
    }

    /// `<` and `>` are told apart from the byte prefixes by position: one with
    /// an expression to its left compares, one with nothing to its left
    /// extracts. `<$1234 > 3` does both — `$34` is 52, which is greater than 3.
    #[test]
    fn a_byte_prefix_is_not_a_comparison() {
        assert_eq!(
            asm("* = $0000\n!if <$1234 > 3 {\n lda #5\n}\n")
                .expect("assemble")
                .bytes,
            vec![0xA9, 0x05]
        );
        assert_eq!(
            asm("* = $0000\n lda #<$1234\n lda #>$1234\n")
                .expect("assemble")
                .bytes,
            vec![0xA9, 0x34, 0xA9, 0x12],
            "the prefixes still extract"
        );
    }

    /// A `"…"` string of one character is a value wherever ACME wants a
    /// number — and it is a *string* being coerced, not a character literal
    /// being lexed, which is why it holds exactly one character and takes no
    /// operators.
    #[test]
    fn a_one_character_string_is_a_value() {
        let bytes = |src: &str| asm(src).unwrap_or_else(|e| panic!("{src}: {e}")).bytes;
        assert_eq!(bytes("* = $0000\n !byte \"a\"\n"), vec![0x61]);
        assert_eq!(bytes("* = $0000\n !byte (\"a\")\n"), vec![0x61]);
        assert_eq!(bytes("* = $0000\n !byte \"a\", \"b\"\n"), vec![0x61, 0x62]);
        assert_eq!(bytes("* = $0000\n !word \"a\"\n"), vec![0x61, 0x00]);
        assert_eq!(bytes("* = $0000\n lda #\"a\"\n"), vec![0xA9, 0x61]);
        assert_eq!(
            bytes("* = $0000\n lda \"a\"\n"),
            vec![0xA5, 0x61],
            "it sizes like any other low value"
        );
    }

    /// The other half of the string rule, and the reason it is recognised as a
    /// whole value rather than lexed as a token: no operator applies to one.
    #[test]
    fn no_operator_applies_to_a_string() {
        assert!(asm("* = $0000\n !byte \"a\"*1\n").is_err(), "arithmetic");
        assert!(
            asm("* = $0000\n !if \"a\" = 97 {\n lda #5\n}\n").is_err(),
            "comparison"
        );
        assert!(
            asm("* = $0000\n !byte \"ab\"\n").is_err(),
            "more than one character"
        );
        assert_eq!(
            asm("* = $0000\n!if \"a\" {\n lda #5\n}\n")
                .expect("assemble")
                .bytes,
            vec![0xA9, 0x05],
            "but a bare string is testable"
        );
    }

    // -----------------------------------------------------------------------
    // #128 gap 3 — zero-page sizing from a tracked location counter
    // (`decisions/acme-zero-page.md`). Probed against acme 0.97, 2026-08-23.
    // -----------------------------------------------------------------------

    /// A backward label with a low address sizes to zero page, because by the
    /// time it is referenced its address is fixed and nothing later can move
    /// it. We emitted absolute — the wrong size *and* the wrong byte count.
    #[test]
    fn a_backward_label_sizes_to_zero_page() {
        assert_eq!(
            asm("* = $0000\nlbl lda #5\n lda lbl\n")
                .expect("assemble")
                .bytes,
            vec![0xA9, 0x05, 0xA5, 0x00]
        );
        assert_eq!(
            asm("* = $0000\n !byte 1,2,3\nlbl lda #5\n lda lbl\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x02, 0x03, 0xA9, 0x05, 0xA5, 0x03],
            "the counter follows data as well as code"
        );
    }

    /// The three cases that must *not* shrink: a high address, a forward
    /// reference (the reference warns and stays wide), and an operand the
    /// dialect forces absolute.
    #[test]
    fn only_a_known_low_address_shrinks() {
        assert_eq!(
            asm("* = $0200\nlbl lda #5\n lda lbl\n")
                .expect("assemble")
                .bytes,
            vec![0xA9, 0x05, 0xAD, 0x00, 0x02],
            "a high address stays absolute"
        );
        assert_eq!(
            asm("* = $0000\n lda fwd\nfwd lda #5\n")
                .expect("assemble")
                .bytes,
            vec![0xAD, 0x03, 0x00, 0xA9, 0x05],
            "a forward reference is not yet known"
        );
        assert_eq!(
            asm("* = $0000\nlbl lda #5\n lda $0000\n")
                .expect("assemble")
                .bytes,
            vec![0xA9, 0x05, 0xAD, 0x00, 0x00],
            "a 4-digit hex literal is 16-bit whatever its value"
        );
    }

    /// Indexed forms pick per addressing mode: `lda abs,x` has a zero-page
    /// form and `lda abs,y` does not, so the same label sizes differently in
    /// the two — which is the 6502's rule, not ours.
    #[test]
    fn zero_page_sizing_follows_the_addressing_mode() {
        assert_eq!(
            asm("* = $0000\nlbl lda #5\n lda lbl,x\n lda lbl,y\n")
                .expect("assemble")
                .bytes,
            vec![0xA9, 0x05, 0xB5, 0x00, 0xB9, 0x00, 0x00]
        );
    }

    /// ACME warns when an instruction sized absolute for want of a value and
    /// the value turned out to fit a byte. It warns and assembles, so the
    /// bytes are unchanged either way.
    #[test]
    fn an_oversized_addressing_mode_is_reported() {
        let warned = |src: &str| asm(src).expect("assemble").warnings.len();
        assert_eq!(warned("* = $0000\n lda fwd\nfwd lda #5\n"), 1);
        assert_eq!(
            warned("* = $0000\n lda fwd,x\nfwd lda #5\n"),
            1,
            "an indexed form with a zero-page counterpart warns too"
        );
    }

    /// The four cases that must stay silent, one for each way an absolute form
    /// can be the right one.
    #[test]
    fn a_deliberate_absolute_is_not_oversized() {
        let warned = |src: &str| asm(src).expect("assemble").warnings.len();
        assert_eq!(
            warned("* = $0000\n lda fwd\n* = $0100\nfwd lda #5\n"),
            0,
            "the value did not fit"
        );
        assert_eq!(
            warned("* = $0000\nlbl lda #5\n lda $0000\n"),
            0,
            "a 4-digit literal is 16-bit by request — and always folds, which \
             is what keeps it out of the candidate set"
        );
        assert_eq!(
            warned("* = $0000\nlbl lda #5\n lda lbl\n"),
            0,
            "a backward label shrank, so nothing was oversized"
        );
        assert_eq!(
            warned("* = $0000\n lda fwd,y\nfwd lda #5\n"),
            0,
            "`lda abs,y` has no zero-page form: the width was the CPU's call"
        );
    }
}
