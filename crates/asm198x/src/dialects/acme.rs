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
//! Not yet covered (no curriculum use): macros and `!for`.
//! `!zone` is accepted but inert (no `.`-local scoping yet).

use std::collections::{BTreeMap, BTreeSet};

use super::mos6502::{
    self, BytePrec, assignment_split, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, string_literal, top_level_rfind,
};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement};
use crate::source::{MAX_INCLUDE_DEPTH, SourceLoader, SourceMap};
use crate::span::FileId;

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
        // Idea 4: assemble by **evaluating the shared conditional AST** — the same
        // source-preserving tree the formatter parses — rather than a separate
        // brace preprocessor. `evaluate` walks the tree, prunes untaken branches,
        // threads `env`, bakes `!set`, and lowers each line through
        // `parse_statement`. This retires `tokenize_braces`/`process_block`; the
        // conditional now lives in the tree, not a second parse. No loader here:
        // a `!src`/`!bin` on this single-source path is an error pointing at the
        // multi-file entry points.
        let program = parse_program(source)?;
        let mut eval = AcmeEval::new(self.instruction_set(), None);
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        eval.resolve_anon_refs(&mut out)?;
        Ok(out)
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
        let program = parse_program_in(FileId(0), &root)?;
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

    /// The formatter parses through the same source-preserving front-end
    /// (`parse_program`) the assembler now uses — conditional blocks as
    /// `Item::Conditional`, every other line's verbatim operation source. `emit`
    /// reformats this tree; `parse` evaluates it.
    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source)?))
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
#[derive(PartialEq, Eq)]
enum Closer {
    Eof,
    Brace,
    BraceElse,
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
pub(crate) fn parse_program(source: &str) -> Result<crate::ast::Program, AsmError> {
    parse_program_in(FileId(0), source)
}

/// Parse one file of a multi-file ACME program: as [`parse_program`], with
/// every span minted in `file` so diagnostics and line records name the
/// include they came from (language-surface U4).
fn parse_program_in(file: FileId, source: &str) -> Result<crate::ast::Program, AsmError> {
    let mut cx = FmtCx {
        set: &isa::mos6502::SET,
        file,
        lines: source.lines().collect(),
        pos: 0,
        pending: Vec::new(),
    };
    let (mut nodes, closer) = cx.parse_block()?;
    if closer != Closer::Eof {
        return Err(AsmError::new(cx.pos, "unbalanced `}` in conditional block"));
    }
    // Flush a trailing comment block so the formatter keeps it.
    let last = cx.lines.len();
    cx.flush_pending(&mut nodes, last);
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

            // A block close: `}`, `} else {`, `} else`.
            if let Some(rest) = trimmed.strip_prefix('}') {
                let rest = rest.trim();
                self.pos += 1;
                // Flush comments/blanks pending at the block's end into *this*
                // block, so a trailing comment stays inside the branch it closes
                // rather than leaking onto the next one (across `} else {`).
                self.flush_pending(&mut nodes, line);
                if rest.is_empty() {
                    return Ok((nodes, Closer::Brace));
                }
                if let Some(after) = rest.strip_prefix("else")
                    && (after.trim().is_empty() || after.trim() == "{")
                {
                    return Ok((nodes, Closer::BraceElse));
                }
                return Err(AsmError::new(line, format!("unexpected `{trimmed}`")));
            }

            // A conditional head opens a block (one-line or multi-line).
            if is_conditional_head(trimmed) {
                let node = self.parse_conditional(trimmed, comment, line)?;
                nodes.push(node);
                continue;
            }

            // An ordinary line.
            let leading = std::mem::take(&mut self.pending);
            let node = self.parse_line(code, comment, line, leading)?;
            nodes.push(node);
            self.pos += 1;
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
                vec![self.parse_line(body_text, None, line, Vec::new())?]
            };
            self.pos += 1;
            return Ok(self.conditional_node(head, then_body, None, true, leading, comment, line));
        }

        // Multi-line: the body starts on the following line.
        self.pos += 1;
        let (then_body, closer) = self.parse_block()?;
        let else_body = if closer == Closer::BraceElse {
            let (eb, _) = self.parse_block()?;
            Some(eb)
        } else {
            None
        };
        Ok(self.conditional_node(head, then_body, else_body, false, leading, comment, line))
    }

    /// Build one flat node from an ordinary line: its optional (column-0) label,
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
            if !word.starts_with('!')
                && self.set.instruction(&word.to_ascii_uppercase()).is_none()
                && is_ident(word)
            {
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
                head,
                then_body,
                else_body,
                inline,
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
// Assembly by evaluation of the conditional AST (idea 4) — the ACME evaluator
// ---------------------------------------------------------------------------

/// The multi-file context of an include-capable walk (language-surface U4,
/// KTD8): the source map that owns `FileId` allocation and the include graph,
/// the loader seam, and the active include stack for cycle detection.
struct MultiCx<'a> {
    map: &'a mut SourceMap,
    loader: &'a dyn SourceLoader,
    /// The files currently open, root first. Cycle detection is membership —
    /// a file may be included twice *sequentially* (acme re-reads it) but
    /// never while it is still open.
    stack: Vec<FileId>,
}

/// ACME's [`CondEval`](crate::ast::CondEval): it owns the environment (`=`/`equ`
/// constants and `!set` variables) and lowers each live line through
/// [`parse_statement`], re-parsing from the node's (label, source) with the
/// current `env` — so a direct/extended choice or an opcode-embedded operand
/// folds against exactly the bindings live at that point. The shared
/// [`evaluate`](crate::ast::evaluate) walk prunes untaken branches; this supplies
/// the ACME-specific condition test and per-line lowering.
///
/// With a [`MultiCx`] wired in, `!src`/`!bin` resolve *inside* this walk
/// (U4, KTD1): the target loads only when its directive is reached live, the
/// included tree evaluates through `self` (so the environment threads through
/// and back out), and anonymous labels register in spliced evaluation order.
/// Without one (the single-source entry points), those directives are an
/// error pointing at the multi-file entry points.
struct AcmeEval<'a> {
    set: &'static isa::InstructionSet,
    anons: Anons,
    env: BTreeMap<String, i64>,
    /// Names bound by `!set` (rebindable): each use is baked to its current value.
    set_names: BTreeSet<String>,
    multi: Option<MultiCx<'a>>,
    /// The file the walk is currently inside — stamps condition-evaluation
    /// errors, which the shared walk raises without node context.
    current_file: FileId,
}

impl<'a> AcmeEval<'a> {
    fn new(set: &'static isa::InstructionSet, multi: Option<MultiCx<'a>>) -> Self {
        Self {
            set,
            anons: Anons::default(),
            env: BTreeMap::new(),
            set_names: BTreeSet::new(),
            multi,
            current_file: FileId(0),
        }
    }

    /// Resolve every anonymous-label *reference* placeholder left in the
    /// statement stream against the definitions collected during the walk —
    /// the deferred half of the spliced-order model (see [`Anons`]). Call
    /// after the evaluation walk completes.
    fn resolve_anon_refs(&self, out: &mut [Statement]) -> Result<(), AsmError> {
        for s in out.iter_mut() {
            if let Some(op) = s.op.take() {
                s.op = Some(substitute_anon_refs(op, &self.anons, s.file, s.line)?);
            }
        }
        Ok(())
    }

    /// The label a directive line binds, as a statement-ready name: an
    /// anonymous `-`/`+` marker resolves to the definition registered for the
    /// current evaluation position; a plain name passes through.
    fn statement_label(&self, node: &crate::ast::Node) -> Result<Option<String>, AsmError> {
        let Some(sym) = &node.label else {
            return Ok(None);
        };
        if anon_marker(&sym.name).is_some() {
            let def = self.anons.def_here().ok_or_else(|| {
                AsmError::new(
                    node.span.line as usize,
                    "internal: anonymous label not registered",
                )
            })?;
            return Ok(Some(def.name.clone()));
        }
        Ok(Some(sym.name.clone()))
    }

    /// Resolve a `!src`/`!source` directive live (U4, KTD1): load the target
    /// through the loader, parse it in its own `FileId`, and evaluate its tree
    /// through `self` — the environment and anonymous-label order thread
    /// straight through. A label on the directive line binds at the include
    /// point (probe-pinned).
    fn lower_include(
        &mut self,
        node: &crate::ast::Node,
        args: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        let at = node
            .operand_span
            .clone()
            .unwrap_or_else(|| node.span.clone());
        let (request, rest) = file_request(args, line, "!src")?;
        if !rest.trim().is_empty() {
            return Err(AsmError::at(
                at,
                format!("`!src` takes one file name (unexpected `{}`)", rest.trim()),
            ));
        }
        if let Some(label) = self.statement_label(node)? {
            out.push(Statement {
                line,
                file,
                label: Some(label),
                op: None,
                operand_span: None,
            });
        }
        let Some(mcx) = self.multi.as_mut() else {
            return Err(AsmError::at(
                at,
                format!(
                    "cannot resolve `!src \"{request}\"` here — the single-source \
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
        let program = parse_program_in(id, &contents).map_err(|e| stamp_file(e, id))?;
        let saved = self.current_file;
        self.current_file = id;
        let walked = crate::ast::evaluate(self, &program.nodes, true, out);
        self.current_file = saved;
        if let Some(mcx) = self.multi.as_mut() {
            mcx.stack.pop();
        }
        walked
    }

    /// Resolve a `!bin`/`!binary` directive live (U4, KTD8): load the asset
    /// through the loader's binary path (no `FileId` — spans only ever point
    /// into source files) and window it with acme's probe-pinned size/skip
    /// semantics ([`window_bin`]). The payload rides one statement at the
    /// directive's span; a label binds at the payload's start.
    fn lower_incbin(
        &mut self,
        node: &crate::ast::Node,
        args: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        let at = node
            .operand_span
            .clone()
            .unwrap_or_else(|| node.span.clone());
        let (request, size, skip) = bin_args(&self.anons, &self.env, args, line)?;
        let label = self.statement_label(node)?;
        let Some(mcx) = self.multi.as_mut() else {
            return Err(AsmError::at(
                at,
                format!(
                    "cannot resolve `!bin \"{request}\"` here — the single-source \
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
        let payload = window_bin(&data, size, skip)
            .map_err(|msg| AsmError::at(at, format!("`{request}`: {msg}")))?;
        out.push(Statement {
            line,
            file,
            label,
            op: Some(Operation::Binary(payload)),
            operand_span: node.operand_span.clone(),
        });
        Ok(())
    }
}

impl crate::ast::CondEval for AcmeEval<'_> {
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        let taken = match classify_conditional(head) {
            Some(Conditional::IfDef(s)) => Ok(self.env.contains_key(&s)),
            Some(Conditional::IfNDef(s)) => Ok(!self.env.contains_key(&s)),
            Some(Conditional::If(e)) => eval_condition(&self.anons, &self.env, &e, line),
            None => Err(AsmError::new(line, format!("bad conditional `{head}`"))),
        };
        // The shared walk raises condition errors without node context, so a
        // failure inside an included file is stamped here (U4).
        taken.map_err(|e| stamp_file(e, self.current_file))
    }

    fn lower(&mut self, node: &crate::ast::Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        // Every live line takes the next evaluation-order position (the anon
        // "virtual line"): included files splice their lines here, so `-`/`+`
        // resolution follows the spliced order, never any single file's line
        // numbers — and a definition in an untaken branch never registers,
        // matching acme (probe-pinned, U4).
        self.anons.vline += 1;
        if let Some(sym) = &node.label
            && let Some((sign, level)) = anon_marker(&sym.name)
        {
            self.anons.define(sign, level);
        }

        // `!src`/`!bin` are walk-handled (case-insensitive, with their
        // aliases), never parsed as operations: resolution must happen inside
        // the live walk (KTD1) or not at all (the single-source pointer).
        let (word, args) = split_first_word(node.source.trim());
        match word.to_ascii_lowercase().as_str() {
            "!src" | "!source" => return self.lower_include(node, args, out),
            "!bin" | "!binary" => return self.lower_incbin(node, args, out),
            _ => {}
        }

        // Reconstruct the source line from the node's (label, operation source) —
        // canonical whitespace, which the parser treats identically to the
        // original.
        let recon = match &node.label {
            Some(sym) if node.source.is_empty() => sym.name.clone(),
            Some(sym) => format!("{} {}", sym.name, node.source),
            None => node.source.clone(),
        };

        // `!set name = expr` binds/rebinds a variable and emits nothing; later
        // uses are baked to this value.
        if split_first_word(recon.trim()).0 == "!set" {
            let (name, value) =
                parse_set(&self.anons, &self.env, &recon, line).map_err(|e| stamp_file(e, file))?;
            self.env.insert(name.clone(), value);
            self.set_names.insert(name);
            return Ok(());
        }

        let (label, op) = parse_statement(self.set, &self.anons, &self.env, &recon, line)
            .map_err(|e| stamp_file(e, file))?;
        // Bake `!set` variables to their current value; real labels stay symbolic.
        let op = op.map(|o| bake_set_vars(o, &self.env, &self.set_names));
        if let (Some(name), Some(Operation::Equ(e))) = (&label, &op)
            && let Ok(v) = fold_const(e, &self.env, line)
        {
            self.env.insert(name.clone(), v);
        }
        if !(label.is_none() && op.is_none()) {
            out.push(Statement {
                line,
                file,
                label,
                op,
                operand_span: node.operand_span.clone(),
            });
        }
        Ok(())
    }
}

/// Stamp `file` onto a per-line parse error: the line-oriented helpers
/// (`parse_statement`, the expression parser) know their line but not their
/// file, so the walk supplies it at the per-line boundary (language-surface
/// U4, the z80 walk's convention).
fn stamp_file(mut e: AsmError, file: FileId) -> AsmError {
    match &mut e.span {
        Some(span) => span.file = file,
        None if e.line != 0 => {
            e.span = Some(crate::ast::Span::in_file(file, e.line as u32, 0));
        }
        None => {}
    }
    e
}

/// The file name of a `!src`/`!bin` directive: acme requires `"file"` quotes
/// or the `<file>` library form — a bare token is rejected (probe-pinned:
/// `File name quotes not found`). Returns the name and the remaining text
/// after the closing quote/bracket for the caller's argument handling.
fn file_request<'t>(
    args: &'t str,
    line: usize,
    directive: &str,
) -> Result<(String, &'t str), AsmError> {
    let t = args.trim();
    let (inner, rest) = if let Some(body) = t.strip_prefix('"') {
        let end = body
            .find('"')
            .ok_or_else(|| AsmError::new(line, format!("unterminated `{directive}` file name")))?;
        (&body[..end], &body[end + 1..])
    } else if let Some(body) = t.strip_prefix('<') {
        let end = body
            .find('>')
            .ok_or_else(|| AsmError::new(line, format!("unterminated `{directive}` file name")))?;
        (&body[..end], &body[end + 1..])
    } else {
        return Err(AsmError::new(
            line,
            format!("`{directive}` file name must be quoted (\"file\" or <file>)"),
        ));
    };
    if inner.is_empty() {
        return Err(AsmError::new(
            line,
            format!("`{directive}` needs a file name"),
        ));
    }
    Ok((inner.to_string(), rest))
}

/// Parse `!bin`'s arguments: the file name, then acme's optional
/// `, [size] [, [skip]]` tail — **size first, then skip**, either slot
/// omittable by leaving it empty (`!bin "f", , 2` skips two and reads the
/// rest; probe-pinned). Both fold against the parse-time environment (they
/// set the statement's size, like a `!fill` count).
fn bin_args(
    anons: &Anons,
    env: &BTreeMap<String, i64>,
    args: &str,
    line: usize,
) -> Result<(String, Option<i64>, Option<i64>), AsmError> {
    let (name, rest) = file_request(args, line, "!bin")?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((name, None, None));
    }
    let Some(tail) = rest.strip_prefix(',') else {
        return Err(AsmError::new(
            line,
            format!("expected `, size [, skip]` after the `!bin` file name, found `{rest}`"),
        ));
    };
    let pieces = mos6502::split_top_level(tail, ',');
    if pieces.len() > 2 {
        return Err(AsmError::new(
            line,
            "`!bin` takes at most a file name, a size, and a skip",
        ));
    }
    let fold = |what: &str, piece: &str| -> Result<Option<i64>, AsmError> {
        if piece.trim().is_empty() {
            return Ok(None); // an empty slot: acme reads it as "not given"
        }
        let expr = parse_value(anons, piece, line)?;
        fold_const(&expr, env, line).map(Some).map_err(|_| {
            AsmError::new(
                line,
                format!(
                    "`!bin` {what} must be a constant here (a number, an expression \
                     of constants, or a symbol defined above)"
                ),
            )
        })
    };
    let size = fold("size", pieces[0])?;
    let skip = pieces
        .get(1)
        .map(|p| fold("skip", p))
        .transpose()?
        .flatten();
    Ok((name, size, skip))
}

/// Apply acme's `!bin` size/skip window to the loaded asset — probe-pinned
/// (acme 0.97): skip past EOF or a size beyond the available data **pads with
/// zeroes** rather than erroring; a negative skip reads from the start; a
/// negative size is an error; no size means "from skip to EOF" (empty when
/// skip is at or past EOF). `Err` carries the message body; the caller wraps
/// it with the request name and the directive's span.
fn window_bin(data: &[u8], size: Option<i64>, skip: Option<i64>) -> Result<Vec<u8>, String> {
    if let Some(s) = size
        && s < 0
    {
        return Err(format!("negative `!bin` size ({s})"));
    }
    // A negative skip reads from the start of the file (the reference's seek
    // fails and the read position stays at 0).
    let skip = usize::try_from(skip.unwrap_or(0).max(0)).map_err(|_| "skip overflows")?;
    let start = skip.min(data.len());
    Ok(match size {
        None => data[start..].to_vec(),
        Some(s) => {
            let s = usize::try_from(s).map_err(|_| "size overflows")?;
            let end = start.saturating_add(s).min(data.len());
            let mut v = data[start..end].to_vec();
            // acme pads a short read with zeroes to exactly `size` bytes.
            v.resize(s, 0);
            v
        }
    })
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

/// One anonymous-label definition: its **evaluation-order position** (the
/// "virtual line" — one per live lowered line, so included files splice in
/// order and untaken branches never register), its sign and level (the run
/// length, so `--` is level 2), and the unique synthetic name it binds. The
/// name carries a leading control char so it can never collide with a real
/// identifier.
struct AnonDef {
    vline: usize,
    sign: char,
    level: usize,
    name: String,
}

/// The anonymous-label state of one evaluation walk (language-surface U4).
///
/// Definitions register as the walk reaches them live, in spliced order.
/// References cannot resolve during the walk — a forward `+` may point into a
/// file not yet loaded (an include reached later) — so [`parse_value`] mints
/// a self-describing **placeholder symbol** ([`anon_ref_placeholder`])
/// encoding the sign, level, and referencing position; after the walk,
/// [`AcmeEval::resolve_anon_refs`] rewrites every placeholder to its
/// definition's name ([`substitute_anon_refs`]).
#[derive(Default)]
struct Anons {
    defs: Vec<AnonDef>,
    /// The current evaluation position; bumped once per live lowered line.
    vline: usize,
}

impl Anons {
    /// Register a definition at the current evaluation position.
    fn define(&mut self, sign: char, level: usize) {
        let name = format!("\u{1}{sign}{level}#{}", self.defs.len());
        self.defs.push(AnonDef {
            vline: self.vline,
            sign,
            level,
            name,
        });
    }

    /// The definition registered at the current evaluation position, if any —
    /// how the label side of a line finds its own synthetic name.
    fn def_here(&self) -> Option<&AnonDef> {
        self.defs.last().filter(|d| d.vline == self.vline)
    }

    /// Resolve a reference at position `vline`: the nearest preceding `-`
    /// definition (backward — the same line is allowed: `- jmp -` self-loops)
    /// or the nearest *strictly following* `+` definition (forward — acme does
    /// **not** let `+ jmp +` see its own line; probe-pinned), at the same
    /// level.
    fn resolve(&self, sign: char, level: usize, vline: usize) -> Option<&AnonDef> {
        let matching = self
            .defs
            .iter()
            .filter(|d| d.sign == sign && d.level == level);
        if sign == '-' {
            matching
                .filter(|d| d.vline <= vline)
                .max_by_key(|d| d.vline)
        } else {
            matching.filter(|d| d.vline > vline).min_by_key(|d| d.vline)
        }
    }
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

/// The self-describing placeholder a reference parses to during the walk:
/// `\u{2}{sign}{level}@{vline}`. The `\u{2}` prefix can never collide with a
/// real identifier (or with the `\u{1}` definition names), and the payload
/// carries everything post-walk resolution needs — no side table.
fn anon_ref_placeholder(sign: char, level: usize, vline: usize) -> String {
    format!("\u{2}{sign}{level}@{vline}")
}

/// Decode an [`anon_ref_placeholder`]'s `(sign, level, vline)`, or `None` for
/// an ordinary symbol.
fn decode_anon_ref(name: &str) -> Option<(char, usize, usize)> {
    let body = name.strip_prefix('\u{2}')?;
    let mut chars = body.chars();
    let sign = chars.next()?;
    let rest = chars.as_str();
    let (level, vline) = rest.split_once('@')?;
    Some((sign, level.parse().ok()?, vline.parse().ok()?))
}

/// Rewrite every anonymous-reference placeholder in `op` to its resolved
/// definition name — the post-walk half of the spliced-order model. An
/// unresolvable reference errors at the statement that made it.
fn substitute_anon_refs(
    op: Operation,
    anons: &Anons,
    file: FileId,
    line: usize,
) -> Result<Operation, AsmError> {
    let subst = |e: Expr| subst_anon_expr(e, anons, file, line);
    Ok(match op {
        Operation::Org(e) => Operation::Org(subst(e)?),
        Operation::Equ(e) => Operation::Equ(subst(e)?),
        Operation::Entry(e) => Operation::Entry(subst(e)?),
        Operation::Bytes(v) => {
            Operation::Bytes(v.into_iter().map(subst).collect::<Result<_, _>>()?)
        }
        Operation::Words(v) => {
            Operation::Words(v.into_iter().map(subst).collect::<Result<_, _>>()?)
        }
        Operation::Instruction {
            mnemonic,
            mode,
            operands,
        } => Operation::Instruction {
            mnemonic,
            mode,
            operands: operands.into_iter().map(subst).collect::<Result<_, _>>()?,
        },
        // No expressions to rewrite: pre-encoded pieces, binary payloads, and
        // the constant-argument align.
        other @ (Operation::Encoded(_) | Operation::Binary(_) | Operation::Align { .. }) => other,
    })
}

fn subst_anon_expr(e: Expr, anons: &Anons, file: FileId, line: usize) -> Result<Expr, AsmError> {
    Ok(match e {
        Expr::Sym(s) => match decode_anon_ref(&s) {
            Some((sign, level, vline)) => {
                let def = anons.resolve(sign, level, vline).ok_or_else(|| {
                    let run: String = std::iter::repeat_n(sign, level).collect();
                    AsmError::at(
                        crate::ast::Span::in_file(file, line as u32, 0),
                        format!("no anonymous label `{run}` in that direction"),
                    )
                })?;
                Expr::Sym(def.name.clone())
            }
            None => Expr::Sym(s),
        },
        Expr::Lo(b) => Expr::Lo(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Hi(b) => Expr::Hi(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Bank(b) => Expr::Bank(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Neg(b) => Expr::Neg(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Bin(op, l, r) => Expr::Bin(
            op,
            Box::new(subst_anon_expr(*l, anons, file, line)?),
            Box::new(subst_anon_expr(*r, anons, file, line)?),
        ),
        other @ (Expr::Num(_) | Expr::Pc) => other,
    })
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

/// Evaluate an `!if` condition: a comparison (`=`, `!=`, `<=`, `>=`) of two
/// constant expressions, or a bare expression tested for non-zero. Single `<`/
/// `>` comparisons are not supported (they collide with the byte prefixes); the
/// curriculum uses only `=`.
fn eval_condition(
    anons: &Anons,
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
    anons: &Anons,
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
    anons: &Anons,
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
    anons: &Anons,
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
    anons: &Anons,
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
    anons: &Anons,
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
    let andmask = fold_const(&parse_value(anons, parts[0], line)?, env, line)?;
    let value = fold_const(&parse_value(anons, parts[1], line)?, env, line)?;
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

fn parse_list(anons: &Anons, rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
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
    anons: &Anons,
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

/// Parse an ACME value: a bare `-`/`+` run is an anonymous-label reference —
/// deferred to a placeholder, since a forward `+` may point into a file the
/// walk has not loaded yet (see [`Anons`]); otherwise it is an expression
/// with `<`/`>` applying loosely.
fn parse_value(anons: &Anons, raw: &str, line: usize) -> Result<Expr, AsmError> {
    let trimmed = raw.trim();
    if let Some((sign, level)) = anon_marker(trimmed) {
        return Ok(Expr::Sym(anon_ref_placeholder(sign, level, anons.vline)));
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
            at_is_pc: false,
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
    use crate::{AsmError, AssemblyResult, assemble_acme};

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
