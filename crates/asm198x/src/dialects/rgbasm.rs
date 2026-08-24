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
//!
//! `INCLUDE`/`INCBIN` (language-surface U4) resolve through the shared
//! [`super::ca65_flat`] walk driver with rgbasm's probe-pinned semantics
//! (rgbasm v1.0.1): every relative request — however deeply nested — anchors
//! at the **root input's directory** (rgbasm searches the process cwd, never
//! the including file's directory; our input's directory stands in for the
//! cwd, the documented [`FsLoader`](crate::source::FsLoader) stance), then
//! the `-I` dirs first-listed-wins. State threads through the boundary in
//! both directions: a `DEF … EQU` inside an include feeds the includer's
//! later opcode-embedded operands (`bit`/`rst`/`ds`), and `.local` labels
//! scope under the most recent global across files — a global defined inside
//! the include rescopes the includer's subsequent locals.
//!
//! **`INCBIN "file"[, offset[, length]]` window (probe-pinned):** offset and
//! length are parse-time constant expressions (a forward reference is
//! rgbasm's "Expected constant expression"); a negative offset or length is
//! an error ("Constant must not be negative"); offset in `0..=len` is
//! honoured (at EOF → empty, past → "Specified start position is greater
//! than length"); a length past the remaining bytes is an error ("out of
//! bounds"); length 0 is empty.

use std::collections::BTreeMap;

use super::ca65_flat::{self, DirectiveLine, FlatWalk, WalkDirective};
use super::macros;
use super::mos6502::{
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, parse_number, split_data_items,
    split_first_word, split_top_level, string_literal,
};
use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
use crate::dialect::Dialect;
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, Expr, Operation, Statement};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

/// The rgbasm (SM83) dialect.
pub(crate) struct Rgbasm;

impl Dialect for Rgbasm {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::sm83::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (U6): parse into a `Program`,
        // then lower to the engine's statement stream — byte-identical to the old
        // direct parse (AE1). Other CPUs stay on direct lowering (KTD6).
        let program = parse_program(source, macros::Expand::Yes)?;
        let mut eval = RgbasmEval {
            set: self.instruction_set(),
            consts: BTreeMap::new(),
            global: String::new(),
        };
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        Ok(out)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source, macros::Expand::No)?))
    }

    /// The include-capable parse (language-surface U4): the interleaved,
    /// environment-threaded walk over the source map, resolving `INCLUDE`/
    /// `INCBIN` lazily through the loader — see [`parse_program_multi`].
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        let program = parse_program_multi(map, loader)?;
        let mut eval = RgbasmEval {
            set: self.instruction_set(),
            consts: BTreeMap::new(),
            global: String::new(),
        };
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        Ok(out)
    }

    /// rgbasm `equ` takes no colon on its label (`NAME equ …`); a colon would
    /// fail to reassemble, since the label is disambiguated by the keyword.
    /// (Normal `name:` labels still keep their colon — this governs `equ` only.)
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse rgbasm (SM83) source into the semantic [`Program`](crate::ast::Program).
/// Each line becomes a node with its scoped label, operation, verbatim source,
/// span, and comment trivia. rgbasm scopes `.local` labels under the most recent
/// non-`.` global, so a `.local` becomes a [`Scope::Local`](crate::ast::Scope)
/// symbol qualified as `global.local` (the string-mangle the old parser did);
/// [`lower`](crate::ast::lower) reproduces the old statements exactly. A
/// `SECTION` directive keeps its verbatim source for the formatter and lowers to
/// an `Org` only when it pins an address.
///
/// An `INCLUDE`/`INCBIN` becomes an **unresolved**
/// [`Item::Include`](crate::ast::Item) / [`Item::Incbin`](crate::ast::Item) —
/// the target is never opened, so `--fmt` renders the directive verbatim and
/// works with a missing target (U4, KTD1). Lazy resolution is
/// [`parse_program_multi`]'s.
pub(crate) fn parse_program(source: &str, mode: macros::Expand) -> Result<Program, AsmError> {
    let mut w = Walker::new();
    // Macros expand before parsing (#93), but only for assembly: the
    // formatter asks with `Expand::No`, because laying source out must not
    // replace a definition with its expansions.
    let expanded = expand_rgbasm(source, mode)?;
    let text = macros::expanded_text(&expanded, source);
    let origins = macros::line_origins(&expanded);
    // The shared cursor: it groups `IF`/`REPT` blocks and keeps every include
    // unresolved (KTD1), which is what `--fmt` needs.
    ca65_flat::walk_source_expanded(&mut w, text, FileId(0))
        .map_err(|e| macros::remap_lines(e, origins))?;
    w.flush_trailing(text.lines().count() as u32);
    macros::place_nodes(&mut w.nodes, origins);
    Ok(Program { nodes: w.nodes })
}

/// Parse a multi-file rgbasm program (language-surface U4, KTD1): the
/// **interleaved, environment-threaded walk**. The root (`FileId(0)` in `map`)
/// parses line by line with the environment accumulated so far; when the walk
/// reaches an `INCLUDE` live, the target loads through `loader` (anchored at
/// the root input's directory — rgbasm's probe-pinned cwd anchor), its lines
/// parse with the same environment, and everything it defined — `DEF`
/// constants feeding `bit`/`rst`/`ds`, the current global scoping later
/// `.local`s — flows back out to the includer's subsequent lines.
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing target, an include cycle, a bad `INCBIN` window, or the depth
/// backstop — all at the directive's span.
pub(crate) fn parse_program_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<Program, AsmError> {
    let mut w = Walker::new();
    let root = map.contents(FileId(0)).unwrap_or_default().to_owned();
    let mut stack = vec![FileId(0)];
    ca65_flat::walk_file(
        &mut w,
        &root,
        FileId(0),
        map,
        loader,
        &mut stack,
        &SEMANTICS,
    )?;
    Ok(Program { nodes: w.nodes })
}

/// rgbasm's probe-pinned multi-file semantics: root-anchored resolution (the
/// cwd stance mapped to our input directory) and the no-negatives incbin
/// window.
pub(crate) const SEMANTICS: ca65_flat::WalkSemantics = ca65_flat::WalkSemantics {
    resolution: ca65_flat::Resolution::Root,
    window: slice_incbin,
    include_default_ext: None,
};

/// Apply rgbasm's `INCBIN` window to the loaded asset — probe-pinned (see the
/// module docs): negative offset or length are errors, offset at EOF or
/// length 0 are legal and empty, any window past EOF is an error. `Err`
/// carries the message body; the driver wraps it with the request name and
/// the directive's span.
fn slice_incbin(data: &[u8], offset: Option<i64>, size: Option<i64>) -> Result<Vec<u8>, String> {
    let len = data.len() as i64;
    let off = offset.unwrap_or(0);
    if off < 0 {
        return Err(format!("offset must not be negative (rgbasm), got {off}"));
    }
    if off > len {
        return Err(format!(
            "start position {off} is greater than the length of the {len}-byte file"
        ));
    }
    let remaining = len - off;
    let take = match size {
        None => remaining,
        Some(s) if s < 0 => {
            return Err(format!("length must not be negative (rgbasm), got {s}"));
        }
        Some(s) => s,
    };
    if take > remaining {
        return Err(format!("range is out of bounds ({off} + {take} > {len})"));
    }
    Ok(data[off as usize..(off + take) as usize].to_vec())
}

/// The per-line parse walk shared by [`parse_program`] (single source) and
/// [`parse_program_multi`] (the include-capable walk). The environment — the
/// `EQU`/`DEF` constants, the current global label scoping `.local`s, and
/// pending comment trivia — lives here, so in the multi-file walk it threads
/// *through* include boundaries in both directions (KTD1, probe-pinned).
struct Walker {
    /// Constants bound with `[DEF] NAME EQU/= expr`, consulted for
    /// opcode-embedded operands (`bit`/`rst`), `ds` counts, and `INCBIN`
    /// argument folding at parse time.
    consts: BTreeMap<String, i64>,
    /// The most recent non-`.` global label, for qualifying `.local`
    /// labels/refs — a global defined inside an include rescopes the
    /// includer's later locals (probe-pinned).
    global: String,
    /// Own-line comments seen since the last node, attached as leading trivia
    /// to the next one. Comments never reach the encoder, so bytes are
    /// unchanged.
    pending_leading: Vec<Comment>,
    /// Inside a macro definition, whose lines are copied and never read.
    in_macro: bool,
    /// The macros defined so far, so an invocation is copied too.
    macro_names: std::collections::BTreeSet<String>,
    nodes: Vec<Node>,
}

impl Walker {
    fn new() -> Self {
        Self {
            consts: BTreeMap::new(),
            global: String::new(),
            pending_leading: Vec::new(),
            in_macro: false,
            macro_names: std::collections::BTreeSet::new(),
            nodes: Vec::new(),
        }
    }

    /// Flush comments after the last node (a trailing block or comment-only
    /// file) as a label-less, op-less node so the formatter keeps them.
    fn flush_trailing(&mut self, last_line: u32) {
        if !self.pending_leading.is_empty() {
            self.nodes.push(Node {
                operand_span: None,
                label: None,
                item: None,
                source: String::new(),
                span: Span::at(last_line, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing: None,
                },
            });
        }
    }

    /// The scoped symbol for a label: a leading-`.` local qualifies under the
    /// current global (rgbasm's scoping rule), anything else is global.
    fn symbol(&self, name: String) -> Symbol {
        if name.starts_with('.') && !self.global.is_empty() {
            Symbol {
                qualified: format!("{}{name}", self.global),
                scope: Scope::Local {
                    in_global: self.global.clone(),
                },
                name,
            }
        } else {
            Symbol {
                qualified: name.clone(),
                scope: Scope::Global,
                name,
            }
        }
    }

    /// Recognise a walk-handled `INCLUDE`/`INCBIN` operation (keywords are
    /// case-insensitive) and parse its arguments with the live environment:
    /// a quoted file name is required and trailing junk is rejected (both
    /// probe-pinned — rgbasm: "is not a string symbol" / a syntax error);
    /// `INCBIN` offset/length fold against the constants known so far (a
    /// forward reference is rgbasm's "Expected constant expression").
    fn walk_directive(&self, rest: &str, line: usize) -> Result<Option<WalkDirective>, AsmError> {
        let (word, args) = split_first_word(rest);
        match word.to_ascii_uppercase().as_str() {
            "INCLUDE" => Ok(Some(WalkDirective::Include {
                request: ca65_flat::include_request(args, line, "INCLUDE")?,
            })),
            "INCBIN" => {
                let fold =
                    |piece: &str| fold_const(&value(piece.trim(), line)?, &self.consts, line);
                let (request, offset, size) = ca65_flat::incbin_args(args, line, "INCBIN", &fold)?;
                Ok(Some(WalkDirective::Incbin {
                    request,
                    offset,
                    size,
                }))
            }
            _ => Ok(None),
        }
    }
}

impl FlatWalk for Walker {
    /// rgbasm's block vocabulary, measured against rgbds 1.0.3 and
    /// case-insensitive.
    ///
    /// `ELIF` and not `ELSEIF`, and **`ENDC` is the only closer** — rgbds
    /// answers `ENDIF` with `Undefined macro`, so accepting it would take
    /// source the reference refuses. Unlike pasmo no word here means two
    /// things: `ENDC` closes a conditional, `ENDR` a repetition, `ENDM` a
    /// macro.
    /// Macros expand on the multi-file path too — the step easiest to forget,
    /// because the CLI uses only that path and every library test uses the
    /// other.
    fn expand_source(&self, source: &str) -> Result<macros::Expansion, AsmError> {
        expand_rgbasm(source, macros::Expand::Yes)
    }

    fn block_keyword(&self, code: &str) -> Option<ca65_flat::BlockKw> {
        use ca65_flat::BlockKw;
        let word = code.split_whitespace().next()?.to_ascii_uppercase();
        Some(match word.as_str() {
            "IF" => BlockKw::CondOpen,
            "ELIF" => BlockKw::ElseIf,
            "ELSE" => BlockKw::Else,
            "ENDC" => BlockKw::CondClose,
            "REPT" => BlockKw::RepeatOpen,
            "ENDR" => BlockKw::RepeatClose,
            _ => return None,
        })
    }

    fn nodes_mut(&mut self) -> &mut Vec<Node> {
        &mut self.nodes
    }

    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        let set = &isa::sm83::SET;
        let (code, comment) = split_comment(raw);
        if code.trim().is_empty() {
            if let Some(text) = comment {
                self.pending_leading.push(Comment {
                    text: text.to_string(),
                    span: Span::in_file(file, line as u32, 1),
                });
            }
            return Ok(None);
        }
        let trailing = comment.map(|text| Comment {
            text: text.to_string(),
            span: Span::in_file(file, line as u32, (code.len() + 1) as u32),
        });

        // `SECTION "name", TYPE[$addr]` — a directive preserved verbatim; it
        // lowers to an `Org` only when it pins an address (a flat binary ignores
        // an address-less section, but the formatter still keeps the line).
        if code
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("SECTION")
        {
            let item = section_origin(code.trim(), line)?
                .map(|org| crate::ast::item_from_operation(Operation::Org(org)));
            self.nodes.push(Node {
                operand_span: None,
                label: None,
                item,
                source: code.trim().to_string(),
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        // `[DEF] NAME EQU expr` / `[DEF] NAME = expr` — a (global) constant.
        if let Some(c) = constant(code.trim(), line)? {
            if let Ok(v) = fold_const(&c.expr, &self.consts, line) {
                self.consts.insert(c.name.clone(), v);
            }
            self.nodes.push(Node {
                operand_span: None,
                label: Some(Symbol {
                    qualified: c.name.clone(),
                    scope: Scope::Global,
                    name: c.render_name,
                }),
                item: Some(crate::ast::item_from_operation(Operation::Equ(c.expr))),
                source: c.op_source,
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        let (label, rest) = split_label(code, line)?;
        // A non-`.` label opens a new scope; resolve it before qualifying the op
        // (also on an `INCLUDE`/`INCBIN` line — the label is a label like any
        // other).
        if let Some(name) = &label
            && !name.starts_with('.')
        {
            self.global = name.clone();
        }
        // `INCLUDE`/`INCBIN` are walk-handled, not directives: the target must
        // not be opened here (KTD1 — `--fmt` succeeds with a missing target),
        // so hand them back for the driver to resolve (or keep unresolved, in
        // the single-source parse).
        if let Some(kind) = self.walk_directive(rest, line)? {
            return Ok(Some(DirectiveLine {
                kind,
                label: label.map(|name| self.symbol(name)),
                source: rest.trim().to_string(),
                span: Span::in_file(file, line as u32, 1),
                operand_span: ca65_flat::directive_operand_span(raw, rest, line, file),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            }));
        }
        // A macro definition is copied, not read: a body is a template rather
        // than code, and only the formatter's parse ever reaches here with one
        // intact — the assembling path expands it away first. See
        // `Item::Verbatim`.
        {
            use crate::dialects::macros::MacroSyntax as _;
            let text = code.trim();
            let opened = RgbasmMacros.header(code);
            let invoked = text.split_whitespace().next().unwrap_or("");
            if self.in_macro || opened.is_some() || self.macro_names.contains(invoked) {
                if let Some((name, _)) = opened {
                    self.macro_names.insert(name);
                    self.in_macro = true;
                } else if RgbasmMacros.is_end(text) {
                    self.in_macro = false;
                }
                self.nodes.push(Node {
                    operand_span: None,
                    label: None,
                    item: Some(crate::ast::Item::Verbatim),
                    source: code.trim_end().to_string(),
                    span: Span::in_file(file, line as u32, 1),
                    trivia: Trivia {
                        leading: std::mem::take(&mut self.pending_leading),
                        trailing,
                    },
                });
                return Ok(None);
            }
        }

        let symbol = label.map(|name| self.symbol(name));
        let op = if rest.is_empty() {
            None
        } else {
            parse_op(set, rest, &self.consts, &self.global, line)?
        };
        if symbol.is_none() && op.is_none() {
            return Ok(None);
        }
        self.nodes.push(Node {
            operand_span: crate::ast::operand_span(raw, rest, line as u32).map(|mut s| {
                s.file = file;
                s
            }),
            label: symbol,
            item: op.map(crate::ast::item_from_operation),
            source: rest.trim().to_string(),
            span: Span::in_file(file, line as u32, 1),
            trivia: Trivia {
                leading: std::mem::take(&mut self.pending_leading),
                trailing,
            },
        });
        Ok(None)
    }

    fn push_node(&mut self, node: Node) {
        self.nodes.push(node);
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

/// A parsed constant definition: `name` is the symbol the engine binds;
/// `render_name` is what the formatter prints in the label position —
/// `DEF NAME` for the `DEF`-keyword spelling (rgbasm v1.0's required form,
/// preserved verbatim so formatted output stays reference-valid), plain
/// `NAME` for the legacy bare spelling; `op_source` is the operation's
/// source text (`EQU expr` / `= expr`), re-emitted after the render name.
struct Constant {
    name: String,
    render_name: String,
    expr: Expr,
    op_source: String,
}

/// `[DEF] NAME EQU expr` or `[DEF] NAME = expr` (redefinable). rgbasm v1.0
/// requires the `DEF` keyword (a bare `NAME EQU` is "Undefined macro" there);
/// the bare spelling is kept accepted for older sources.
fn constant(code: &str, line: usize) -> Result<Option<Constant>, AsmError> {
    // An optional leading `DEF` keyword; remember it for verbatim re-emit.
    let (def, code) = match split_first_word(code) {
        (kw, rest) if kw.eq_ignore_ascii_case("def") && !rest.is_empty() => (true, rest),
        _ => (false, code),
    };
    let render = |name: &str| {
        if def {
            format!("DEF {name}")
        } else {
            name.to_string()
        }
    };
    // `NAME EQU expr` — the keyword form.
    let (first, rest) = split_first_word(code);
    if !rest.is_empty() {
        let (kw, tail) = split_first_word(rest);
        if kw.eq_ignore_ascii_case("equ") && is_ident(first) {
            return Ok(Some(Constant {
                name: first.to_string(),
                render_name: render(first),
                expr: value(tail, line)?,
                op_source: rest.trim().to_string(),
            }));
        }
    }
    // `NAME = expr` — a lone `=`.
    if let Some(eq) = mos6502::assignment_split(code) {
        let name = code[..eq].trim();
        if is_ident(name) {
            return Ok(Some(Constant {
                name: name.to_string(),
                render_name: render(name),
                expr: value(code[eq + 1..].trim(), line)?,
                op_source: code[eq..].trim().to_string(),
            }));
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

/// Parse the operation part of a line: a directive or an instruction.
/// What this dialect accepts beyond its instruction set.
///
/// asl is the reference for this chip. The ignored spellings emit no bytes
/// and change no encoding, so source carrying them assembles unchanged.
pub const DIRECTIVES: &[Directive] = &[
    // Walk-handled: the shared cursor reads these into `Item::Conditional` /
    // `Item::Repeat` before `parse_op` sees a line.
    //
    // `ELIF` and not `ELSEIF`, and `ENDC` is the **only** conditional closer —
    // rgbds answers `ENDIF` with `Undefined macro`, so declaring it would
    // accept a spelling the reference refuses.
    //
    // `FOR` is real rgbasm and absent: it binds a loop variable with an
    // exclusive stop and an optional step, and adopting it means the
    // `Iteration::Over` path rather than a count.
    // Expander-handled: the macro scanner consumes a definition before the
    // walk sees a line. `ENDM` is not declared — it is part of the block
    // rather than vocabulary of its own.
    Directive {
        id: "macro",
        pattern: Pattern::Exact(&["macro"]),
        category: Category::Operation,
    },
    Directive {
        id: "conditional",
        pattern: Pattern::Exact(&["if", "elif", "else", "endc"]),
        category: Category::Operation,
    },
    Directive {
        id: "repeat",
        pattern: Pattern::Exact(&["rept", "endr"]),
        category: Category::Operation,
    },
    Directive {
        id: "bytes",
        pattern: Pattern::Exact(&["db"]),
        category: Category::Operation,
    },
    Directive {
        id: "words",
        pattern: Pattern::Exact(&["dw"]),
        category: Category::Operation,
    },
    Directive {
        id: "reserve",
        pattern: Pattern::Exact(&["ds"]),
        category: Category::Operation,
    },
    // Walk-handled, not seen by `parse_op`.
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
    // -----------------------------------------------------------------------
    // What rgbasm has here and we do not.
    //
    // Declared so the diagnostic stops saying `unknown instruction` for a word
    // the reference has — the same call as ca65's and the asl family's. It
    // changes no bytes; it changes whether a reader with valid source goes
    // looking for a typo.
    //
    // Thirty-three spellings, each confirmed in **statement position** against
    // rgbasm 1.0.3. The rest of what rgbasm knows and we do not is not
    // directive vocabulary and is deliberately absent:
    //
    // - **Registers** — `af`, `bc`, `hli`. Operand vocabulary, and ours.
    // - **Built-in functions** — `strlen`, `sizeof`, `bank`, `cos`, `strfmt`,
    //   and the fixed-point pair rgbasm's parser names `FDIV`/`FMUL`. They
    //   live inside expressions.
    // - **Predefined symbols** — `__DATE__`, `_NARG`, `_RS`. Case-sensitively
    //   uppercase: rgbasm answers `Undefined macro` for `__date__` and warns
    //   that `__DATE__` is deprecated, which is how the two were told apart.
    // - **Section attributes** — `ROM0`, `WRAMX`, `FRAGMENT`. Operands of
    //   `SECTION`, not statements.
    //
    // Naming any of those here would describe them in a position they never
    // appear in.
    // assertions and diagnostics
    // `FAIL "msg"` aborts, `WARN "msg"` notes and carries on.
    Directive {
        id: "diagnose",
        pattern: Pattern::Exact(&["fail", "warn"]),
        category: Category::Operation,
    },
    Directive {
        id: "unsupported-assertions",
        pattern: Pattern::Exact(&["assert", "static_assert", "print", "println"]),
        category: Category::KnownUnsupported,
    },
    // option and character-map state
    Directive {
        id: "unsupported-option",
        pattern: Pattern::Exact(&[
            "opt",
            "popo",
            "pusho",
            "popc",
            "pushc",
            "pops",
            "pushs",
            "charmap",
            "newcharmap",
            "setcharmap",
        ]),
        category: Category::KnownUnsupported,
    },
    // symbol management
    Directive {
        id: "unsupported-symbol",
        pattern: Pattern::Exact(&["export", "purge", "redef", "def", "shift"]),
        category: Category::KnownUnsupported,
    },
    // the RS counter
    Directive {
        id: "unsupported-the",
        pattern: Pattern::Exact(&["rsset", "rsreset"]),
        category: Category::KnownUnsupported,
    },
    // blocks and layout
    Directive {
        id: "unsupported-blocks",
        pattern: Pattern::Exact(&[
            "union",
            "nextu",
            "endu",
            "load",
            "endl",
            "align",
            "for",
            "break",
            "endsection",
            "dl",
        ]),
        category: Category::KnownUnsupported,
    },
];

fn parse_op(
    set: &'static isa::InstructionSet,
    rest: &str,
    consts: &BTreeMap<String, i64>,
    global: &str,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (word, args) = split_first_word(rest);
    // Dispatch through the declared surface: a spelling the declaration
    // does not carry cannot be accepted here. See `crate::directives`.
    let op = match lookup(DIRECTIVES, word) {
        Some(directive) => match directive.category {
            Category::Ignored => return Ok(None),
            Category::KnownUnsupported => {
                return Err(AsmError::new(
                    line,
                    format!(
                        "`{word}` is a real directive here and asm198x does not implement it yet"
                    ),
                ));
            }
            Category::Operation => match directive.id {
                "bytes" => Operation::Bytes(byte_list(args, line)?),
                "words" => Operation::Words(value_list(args, line)?),
                "reserve" => parse_ds(args, consts, line)?,
                "diagnose" => Operation::Diagnose {
                    fatal: word.eq_ignore_ascii_case("fail"),
                    message: args.trim().trim_matches('"').to_string(),
                },
                other => {
                    return Err(AsmError::new(
                        line,
                        format!("`{other}` is declared but not dispatched"),
                    ));
                }
            },
        },
        None => {
            let mn = word.to_ascii_uppercase();
            let (mode, operands) = resolve(set, &mn, args, consts, line)?;
            Operation::Instruction {
                mnemonic: mn,
                mode,
                operands,
            }
        }
    };
    Ok(Some(crate::ast::qualify_locals(op, global)))
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
            function: None,
            bang_is_or: false,
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
    // A word this instruction set does not have is refused as one, before any
    // operand handling. Reaching the mode resolution first makes an unknown
    // word report as a bad operand — "has no form for operands", or worse a
    // message naming nothing — which sends the reader to check the wrong half
    // of their line.
    if set.instruction(mn).is_none() {
        return Err(AsmError::new(line, format!("unknown instruction `{mn}`")));
    }
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

// Local qualification — `jr .loop` under `start` → `start.loop` — is the
// shared [`crate::ast::qualify_locals`] (language-surface U7): rgbasm's copy
// was character-identical to z80's over the same engine types (its
// `other => other` arm differed only for `Operation::Entry`, which rgbasm
// never constructs — no `end`-style directive), so the mangle lives in one
// place; rgbasm's *scope rule* (the last non-`.` global) stays in [`Walker`].

// ---------------------------------------------------------------------------
// Conditional evaluation — rgbasm's `CondEval`.
//
// `ast::lower` rejects an `Item::Conditional`, so assembly runs through
// `ast::evaluate`. Each live line re-parses against the constants as they
// actually stand, because rgbasm threads them into `parse_op` and SM83's
// `ldh [$FF40],a` selection reads them — a constant bound inside an untaken
// branch could otherwise change an instruction.
//
// What does **not** re-parse is a line the walk handled as a directive.
// `SECTION` has no form `parse_op` could rebuild, so a blanket re-parse answers
// `unknown instruction \`SECTION\`` on line 1 of every file. Those keep the
// walk's item through `lower_item_ref`.
// ---------------------------------------------------------------------------

/// rgbasm's conditional evaluator: the constant environment, threaded through
/// the walk so a condition folds against what a taken branch bound.
struct RgbasmEval {
    set: &'static isa::InstructionSet,
    consts: BTreeMap<String, i64>,
    global: String,
}

impl crate::ast::CondEval for RgbasmEval {
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        let (word, args) = split_first_word(head.trim());
        let args = args.trim();
        if args.is_empty() {
            return Err(AsmError::new(line, format!("`{word}` needs a condition")));
        }
        let v = fold_const(&value(args, line)?, &self.consts, line).map_err(|_| {
            AsmError::new(
                line,
                format!(
                    "`{args}` must be a constant here — rgbasm folds a condition against the \
                     values above it, and refuses a forward reference"
                ),
            )
        })?;
        Ok(v != 0)
    }

    /// `REPT n` names no loop variable; `FOR` does and is not adopted yet.
    fn iteration(&self, head: &str, line: u32) -> Result<crate::ast::Iteration, AsmError> {
        let line = line as usize;
        let (_, args) = split_first_word(head.trim());
        let n = fold_const(&value(args.trim(), line)?, &self.consts, line)?;
        Ok(crate::ast::Iteration::Times(n))
    }

    fn lower(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        if let Some(sym) = node.label.as_ref()
            && !sym.name.starts_with('.')
        {
            self.global = sym.qualified.clone();
        }
        let op = match &node.item {
            // Walk-handled: keep what it built rather than rebuilding it.
            Some(crate::ast::Item::Binary(_)) => Some(crate::ast::lower_item_ref(
                node.item.as_ref().expect("matched"),
            )?),
            Some(crate::ast::Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `INCLUDE \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(crate::ast::Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `INCBIN \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            // The walk made nothing of this line, so neither does assembly —
            // a bare `SECTION "a", ROM0` is the case, and re-parsing it would
            // answer `unknown instruction` for a line the reference accepts.
            None => None,
            Some(it) => match parse_op(self.set, &node.source, &self.consts, &self.global, line) {
                Ok(op) => op,
                // A directive the walk handled and the line parser cannot
                // rebuild — `SECTION "a", ROM0[0]`, which carries an origin.
                Err(e) => Some(crate::ast::lower_item_ref(it).map_err(|_| e)?),
            },
        };
        if let (Some(sym), Some(Operation::Equ(e))) = (node.label.as_ref(), &op)
            && let Ok(v) = fold_const(e, &self.consts, line)
        {
            self.consts.insert(sym.qualified.clone(), v);
        }
        out.push(Statement {
            line,
            file: node.span.file,
            label: node.label.as_ref().map(|s| s.qualified.clone()),
            op,
            operand_span: node.operand_span.clone(),
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Macros (#93). The mechanics live in `crate::dialects::macros`; this is
// rgbasm's grammar, measured against rgbds 1.0.3.
//
//   * the header is **keyword-first**: `MACRO name` … `ENDM`. The old
//     `name: MACRO` form that rgbds once took is a `syntax error, unexpected
//     MACRO` in 1.0, so accepting it would take source the reference refuses.
//   * parameters are **positional** — a body says `\1`, `\2` — so, as in lwasm
//     and vasm, the names depend on how many arguments arrived rather than on
//     anything the definition declared.
//   * a call is the macro's bare name, and an unknown one is `Undefined macro`.
//
// Not adopted, and demand-gated: `\#` (all arguments verbatim), `_NARG` (the
// argument count) and `\@` (a per-expansion unique suffix).
// ---------------------------------------------------------------------------

/// rgbasm's macro grammar.
struct RgbasmMacros;

impl macros::MacroSyntax for RgbasmMacros {
    /// `MACRO name` — keyword first, and only keyword first.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        let text = macros::without_comment(line);
        let (kw, rest) = text.trim().split_once(char::is_whitespace)?;
        if !kw.eq_ignore_ascii_case("macro") {
            return None;
        }
        let name = rest.trim().trim_end_matches(':');
        (!name.is_empty() && !name.contains(char::is_whitespace))
            .then(|| (name.to_string(), Vec::new()))
    }

    fn is_end(&self, line: &str) -> bool {
        macros::without_comment(line)
            .trim()
            .eq_ignore_ascii_case("endm")
    }

    fn end_keyword(&self) -> &'static str {
        "ENDM"
    }

    /// `\1`, `\2`, … for as many arguments as the call site passed.
    fn argument_names(&self, _declared: &[String], count: usize) -> Vec<String> {
        (1..=count).map(|n| format!("\\{n}")).collect()
    }

    /// A backslash opens a positional parameter, so it has to be inside the
    /// token for substitution to see it at all.
    fn is_symbol_char(&self, c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'\\'
    }

    /// rgbasm scopes nothing per expansion on its own: a plain label in a body
    /// is global, and a second invocation is `already defined` — the reference
    /// agrees, so there is nothing to rename.
    fn locals(&self, _body: &[String]) -> Vec<String> {
        Vec::new()
    }

    /// A fifth arity posture. rgbasm does not check the call at all: extra
    /// arguments are **dropped silently**, and a missing one is an error where
    /// the body *refers* to it — `Macro argument \2 not defined` — not where
    /// it is called.
    ///
    /// So the call site never rejects anything, and a body that reads past the
    /// arguments it got leaves `\2` unsubstituted, which fails at parse. The
    /// reference errors on the same source; only the wording differs, and
    /// diagnostics are not byte-compared.
    fn fit_arguments(
        &self,
        _name: &str,
        _params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        Ok(args)
    }
}

/// Expand rgbasm's macros, unless this parse is the formatter's.
fn expand_rgbasm(source: &str, mode: macros::Expand) -> Result<macros::Expansion, AsmError> {
    macros::expansion(mode, source, |s| {
        macros::expand(&RgbasmMacros, s).map(|e| Some((e.text, e.origins)))
    })
}

#[cfg(test)]
mod tests {

    /// `FAIL` stops, `WARN` notes and carries on, and neither fires from an
    /// untaken branch.
    #[test]
    fn source_requested_diagnostics() {
        let src = "SECTION \"s\",ROM0[0]\n db 1\n";
        let err = crate::assemble_rgbasm(&format!("{src} FAIL \"stop\"\n")).expect_err("aborts");
        assert!(err.to_string().contains("stop"), "got `{err}`");

        let out = crate::assemble_rgbasm(&format!("{src} WARN \"careful\"\n")).expect("warns");
        assert_eq!(out.bytes, vec![1]);
        assert!(
            out.warnings.iter().any(|w| w.message.contains("careful")),
            "got {:?}",
            out.warnings
        );

        let quiet = crate::assemble_rgbasm(&format!("{src}IF 0\n FAIL \"never\"\nENDC\n"))
            .expect("untaken");
        assert_eq!(quiet.bytes, vec![1]);
    }
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

    /// U6 — the rgbasm front-end routes through the AST, carrying comments as
    /// trivia without changing the emitted bytes (AE1), and preserving the
    /// scoped `.local` resolution.
    #[test]
    fn comments_are_carried_as_trivia() {
        let src =
            "; header\nSECTION \"c\", ROM0[$0]\nstart:\n ld a, $05   ; load\n.loop:\n jr .loop\n";
        let prog = super::parse_program(src, super::macros::Expand::Yes).expect("parses");
        assert!(
            prog.nodes[0]
                .trivia
                .leading
                .iter()
                .any(|c| c.text == "; header"),
            "own-line comment attaches as leading trivia"
        );
        assert!(
            prog.nodes.iter().any(|n| n
                .trivia
                .trailing
                .as_ref()
                .is_some_and(|c| c.text == "; load")),
            "same-line comment attaches as trailing trivia"
        );
        // The reused `.loop` resolves under its global (`start.loop`).
        assert!(
            prog.nodes.iter().any(|n| n
                .label
                .as_ref()
                .is_some_and(|s| s.qualified == "start.loop")),
            "scoped local qualifies under its global"
        );
        assert_eq!(
            bytes(src),
            bytes("SECTION \"c\", ROM0[$0]\nstart:\n ld a, $05\n.loop:\n jr .loop\n"),
            "comments do not change bytes"
        );
    }

    // -----------------------------------------------------------------------
    // Conditionals and repetition. Measured against rgbds 1.0.3.
    // -----------------------------------------------------------------------

    fn out(src: &str) -> Vec<u8> {
        crate::assemble_rgbasm(src).expect(src).bytes
    }

    /// nop = $00, ret = $C9, inc a = $3C.
    #[test]
    fn a_conditional_picks_one_branch() {
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nIF 1\n nop\nENDC\n ret\n"),
            vec![0x00, 0xC9]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nIF 0\n nop\nELSE\n ret\nENDC\n"),
            vec![0xC9]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nIF 0\n nop\nELIF 1\n ret\nENDC\n"),
            vec![0xC9]
        );
    }

    /// **`ENDC` is the only closer.** rgbds answers `ENDIF` with `Undefined
    /// macro`, and the chain keyword is `ELIF` rather than `ELSEIF` — two
    /// spellings that every other dialect here would have led us to expect.
    #[test]
    fn the_spellings_rgbasm_lacks_are_refused() {
        crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\nIF 1\n nop\nENDIF\n")
            .expect_err("rgbds: Undefined macro `ENDIF`");
        crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\nIF 0\n nop\nELSEIF 1\n ret\nENDC\n")
            .expect_err("rgbds has ELIF, not ELSEIF");
    }

    #[test]
    fn a_repetition_assembles_its_body_n_times() {
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nREPT 3\n nop\nENDR\n"),
            vec![0x00, 0x00, 0x00]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nREPT 0\n nop\nENDR\n ret\n"),
            vec![0xC9]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nREPT 2\nREPT 2\n nop\nENDR\n inc a\nENDR\n"),
            vec![0x00, 0x00, 0x3C, 0x00, 0x00, 0x3C]
        );
    }

    /// A constant defined with `DEF … EQU` above the block folds into it, and
    /// one inside an untaken branch never binds.
    #[test]
    fn a_condition_folds_against_the_constants_above_it() {
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nDEF N EQU 1\nIF N\n nop\nENDC\n ret\n"),
            vec![0x00, 0xC9]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nDEF N EQU 3\nREPT N\n nop\nENDR\n"),
            vec![0x00, 0x00, 0x00]
        );
    }

    /// `SECTION` is the reason the evaluator keeps what the walk built. The
    /// line parser has no form for it, so a blanket re-parse answered
    /// `unknown instruction \`SECTION\`` on line 1 of every file.
    #[test]
    fn a_walk_handled_directive_survives_the_re_parse() {
        assert_eq!(out("SECTION \"s\",ROM0[0]\n nop\n"), vec![0x00]);
    }

    /// Formatting a block changes the layout and not the program.
    #[test]
    fn a_formatted_block_assembles_to_the_same_bytes() {
        for src in [
            "SECTION \"s\",ROM0[0]\nDEF N EQU 1\nIF N\n nop\nELSE\n ret\nENDC\n",
            "SECTION \"s\",ROM0[0]\nREPT 3\n nop\nENDR\n",
        ] {
            let before = out(src);
            let formatted = crate::format_rgbasm(src).expect(src);
            let after = crate::assemble_rgbasm(&formatted)
                .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
                .bytes;
            assert_eq!(
                before, after,
                "formatting changed the program:\n{formatted}"
            );
            let again = crate::format_rgbasm(&formatted).expect("formats");
            assert_eq!(formatted, again, "{formatted}");
        }
    }

    /// `MACRO name` … `ENDM`, with positional `\\1` parameters.
    #[test]
    fn a_macro_expands_at_its_call_site() {
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nMACRO m\n nop\nENDM\n m\n ret\n"),
            vec![0x00, 0xC9]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nMACRO ldav\n ld a,\\1\nENDM\n ldav 5\n ret\n"),
            vec![0x3E, 0x05, 0xC9]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nMACRO two\n ld a,\\1\n ld b,\\2\nENDM\n two 1,2\n"),
            vec![0x3E, 0x01, 0x06, 0x02]
        );
    }

    /// **Keyword first, and only keyword first.** rgbds 1.0 answers the old
    /// `name: MACRO` header with `syntax error, unexpected MACRO`, so taking it
    /// would accept source the reference refuses.
    #[test]
    fn the_old_header_form_is_refused() {
        crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\nm: MACRO\n nop\nENDM\n m\n")
            .expect_err("rgbds 1.0: syntax error, unexpected MACRO");
    }

    /// Extra arguments are dropped silently; a missing one fails where the body
    /// refers to it, not at the call. Measured — it is a fifth arity posture
    /// across the dialects here.
    #[test]
    fn extra_arguments_are_dropped_and_a_missing_one_fails_at_use() {
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nMACRO m\n ld a,\\1\nENDM\n m 5,9\n"),
            vec![0x3E, 0x05]
        );
        crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\nMACRO m\n ld a,\\2\nENDM\n m 5\n")
            .expect_err("rgbds: Macro argument `\\2` not defined");
    }

    /// Macros compose with the blocks: a definition may be invoked from inside
    /// a conditional or a repetition, and from another macro's body.
    #[test]
    fn macros_compose_with_conditionals_and_repetitions() {
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nMACRO m\n nop\nENDM\nIF 1\n m\nENDC\n"),
            vec![0x00]
        );
        assert_eq!(
            out("SECTION \"s\",ROM0[0]\nMACRO m\n nop\nENDM\nREPT 3\n m\nENDR\n"),
            vec![0x00, 0x00, 0x00]
        );
        assert_eq!(
            out(
                "SECTION \"s\",ROM0[0]\nMACRO inner\n nop\nENDM\nMACRO outer\n inner\nENDM\n outer\n"
            ),
            vec![0x00]
        );
    }

    /// The formatter lays source out; it does not expand.
    #[test]
    fn formatting_does_not_expand() {
        let src = "SECTION \"s\",ROM0[0]\nMACRO ldav\n ld a,\\1\nENDM\n ldav 5\n";
        let formatted = crate::format_rgbasm(src).expect("formats");
        assert!(formatted.contains("MACRO ldav"), "{formatted}");
        assert!(formatted.contains("ENDM"), "{formatted}");
        assert!(!formatted.contains("ld a,5"), "expanded:\n{formatted}");

        let before = out(src);
        let after = crate::assemble_rgbasm(&formatted)
            .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
            .bytes;
        assert_eq!(
            before, after,
            "formatting changed the program:\n{formatted}"
        );
    }

    /// A directive rgbasm has and we do not names itself as one; a word it does
    /// not have stays an unknown instruction. The same sentence for both told a
    /// reader with valid source to go looking for a typo.
    #[test]
    fn a_real_directive_is_told_apart_from_a_typo() {
        let err = |body: &str| {
            crate::assemble_rgbasm(&format!("\tSECTION \"s\",ROM0\n\t{body}\n"))
                .expect_err(body)
                .to_string()
        };
        for d in ["ASSERT 1", "PRINT \"x\"", "UNION", "OPT b.X", "PUSHS"] {
            let e = err(d);
            assert!(
                e.contains("is a real directive here"),
                "`{d}` should name itself a real rgbasm directive, got: {e}"
            );
        }
        assert!(err("ZZQQ").contains("unknown instruction"));
    }

    /// Only *statement* vocabulary is declared. rgbasm's registers, built-in
    /// functions, predefined symbols and section attributes are all words it
    /// knows and we may or may not have, in positions that are not this one —
    /// naming them here would describe them where they never appear.
    #[test]
    fn only_statement_vocabulary_is_declared() {
        let declared: Vec<String> = crate::directives::surfaces()
            .into_iter()
            .filter(|s| s.dialect == "rgbasm")
            .flat_map(|s| s.directives)
            .flat_map(|d| d.spellings())
            .map(|s| s.to_ascii_lowercase())
            .collect();
        for absent in [
            "af", "hli", "strlen", "sizeof", "cos", "__date__", "_narg", "rom0",
        ] {
            assert!(
                !declared.iter().any(|s| s == absent),
                "`{absent}` is not statement vocabulary"
            );
        }
        for present in ["assert", "print", "union", "align", "db"] {
            assert!(
                declared.iter().any(|s| s == present),
                "`{present}` should be declared"
            );
        }
    }
}
