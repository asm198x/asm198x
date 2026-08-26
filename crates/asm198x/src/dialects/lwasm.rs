//! The lwasm 6809 dialect front-end.
//!
//! lwasm (part of LWTOOLS) is the de-facto modern 6809 assembler. There is no
//! 6809 curriculum yet, so this dialect is validated byte-for-byte against
//! `lwasm --6809 --raw` directly rather than against a curriculum corpus.
//!
//! The 6809 is the first CPU whose operands are not fixed-width slots: indexed
//! addressing carries a *computed postbyte* plus 0/1/2 extension bytes. So this
//! dialect does not hand the engine an `Operation::Instruction` to encode from a
//! form; it computes the bytes itself into [`Operation::Encoded`] pieces and
//! reuses the engine only for the two-pass driver, the symbol table, `org`, and
//! `equ`. Encoding facts come from [`isa::mos6809`]. The 6809 is big-endian.
//!
//! Covered so far: inherent, immediate, direct, extended, and relative
//! (short + long) addressing, plus `org`/`equ`/`fcb`/`fdb`/`rmb`. Indexed
//! addressing (the postbyte) and the register-list ops (`tfr`/`exg`/`pshs`/
//! `puls`) are the next increment.

use std::collections::{BTreeMap, BTreeSet};

use isa::mos6809::{self, Kind};

use super::ca65_flat::{self, DirectiveLine, FlatWalk, WalkDirective};
use super::macros;
use super::mos6502::{self, BytePrec, fold_const, split_first_word};
use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
use crate::dialect::Dialect;
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

/// The lwasm 6809 dialect.
pub(crate) struct Lwasm;

impl Dialect for Lwasm {
    /// lwtools 4.25 truncates a data directive rather than refusing it:
    /// `fcb 256` is `00` and `fdb 65536` is `00 00`, with no diagnostic
    /// (probed 2026-08-25).
    fn oversized_byte_policy(&self) -> crate::dialect::Oversize {
        crate::dialect::Oversize::Truncate
    }

    /// An **operand** is the opposite: `ldb #$1ff` is `ERROR : Byte
    /// overflow`, where `fcb $1ff` beside it is a silent `ff`. lwasm is the
    /// only reference here that splits the two.
    fn oversized_operand_policy(&self) -> crate::dialect::Oversize {
        crate::dialect::Oversize::Error
    }

    /// lwasm's flat output is contiguous: `org` names the address the code
    /// claims and the bytes keep landing where they were. `org $1000 / fcb 1 /
    /// org $2000 / fcb 2` is `01 02` with `*` then reading `$2001`, and an
    /// `org` below the current address is ordinary rather than refused
    /// (probed against lwtools 4.25 with `--raw`).
    fn org_moves_output(&self) -> bool {
        false
    }

    fn instruction_set(&self) -> &'static isa::InstructionSet {
        // The engine consults this only for byte order (the 6809 computes its own
        // encoding into `Encoded` pieces); 6809 is big-endian.
        &isa::mos6809::INSTRUCTION_SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (U6): parse into a `Program`,
        // then lower to the engine's statement stream — byte-identical to the old
        // direct parse (AE1). The 6809 is the first **computed-operand** CPU to
        // migrate: its instructions carry a precomputed `Operation::Encoded`
        // (postbyte + extension bytes), which the AST holds verbatim as
        // `Item::Encoded` and the formatter re-emits via `Node::source`.
        let program = parse_program(source, macros::Expand::Yes)?;
        let mut eval = LwasmEval {
            env: BTreeMap::new(),
            state: ParseState::default(),
        };
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        check_phases(&out)?;
        Ok(out)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        // The formatter must not expand — see `parse_program`.
        Ok(Some(parse_program(source, macros::Expand::No)?))
    }

    /// The include-capable parse (language-surface U4): the interleaved,
    /// environment-threaded walk over the source map, resolving `include`/
    /// `use`/`includebin` lazily through the loader — see
    /// [`parse_program_multi`].
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        let program = parse_program_multi(map, loader)?;
        let mut eval = LwasmEval {
            env: BTreeMap::new(),
            state: ParseState::default(),
        };
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        check_phases(&out)?;
        Ok(out)
    }
}

/// lwasm's two rules for `phase`/`dephase`, checked over the finished
/// statement stream because neither is visible from one line: a `phase` inside
/// an open one is "Nested PHASE not supported", and a `dephase` that closes
/// nothing is "DEPHASE without PHASE". The engine's own counter stack would
/// take both, so the refusal has to be made here or not at all.
fn check_phases(statements: &[Statement]) -> Result<(), AsmError> {
    let mut open = false;
    for s in statements {
        match &s.op {
            Some(Operation::PseudoPc(Some(_))) => {
                if open {
                    return Err(AsmError::new(
                        s.line,
                        "`phase` inside a `phase` — lwasm does not nest them",
                    ));
                }
                open = true;
            }
            Some(Operation::PseudoPc(None)) => {
                if !open {
                    return Err(AsmError::new(s.line, "`dephase` without a `phase`"));
                }
                open = false;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Parse 6809 source into the semantic [`Program`](crate::ast::Program). Each line
/// becomes a node with its (global) label, operation, verbatim source, span, and
/// comment trivia. The 6809 has no local-label scoping, so every label is a
/// [`Scope::Global`](crate::ast::Scope) symbol whose qualified name is the source
/// name. An instruction lowers to a computed [`Operation::Encoded`], carried as
/// [`Item::Encoded`](crate::ast::Item::Encoded) — the formatter re-emits it from
/// the node's source, so it round-trips byte-identical (the computed-operand path
/// U1 axis 2 proved, now exercised on production code).
/// An `include`/`use`/`includebin` becomes an **unresolved**
/// [`Item::Include`](crate::ast::Item) / [`Item::Incbin`](crate::ast::Item) —
/// the target is never opened, so `--fmt` renders the directive verbatim and
/// works with a missing target (U4, KTD1). Lazy resolution is
/// [`parse_program_multi`]'s.
pub(crate) fn parse_program(source: &str, mode: macros::Expand) -> Result<Program, AsmError> {
    // Macros expand before parsing (#93), but only for assembly: the formatter
    // asks with `Expand::No`, because laying source out must not replace a
    // definition with its expansions.
    let expanded = expand_lwasm(source, mode)?;
    let text = macros::expanded_text(&expanded, source);
    let origins = macros::line_origins(&expanded);
    let mut w = Walker::new();
    // The shared cursor: it groups `ifne`/`endc` blocks and keeps every include
    // unresolved (KTD1), which is what `--fmt` needs.
    ca65_flat::walk_source_expanded(&mut w, text, FileId(0))
        .map_err(|e| macros::remap_lines(e, origins))?;
    w.flush_trailing(text.lines().count() as u32);
    macros::place_nodes(&mut w.nodes, origins);
    Ok(Program { nodes: w.nodes })
}

/// Parse a multi-file lwasm program (language-surface U4, KTD1): the
/// **interleaved, environment-threaded walk**. The root (`FileId(0)` in
/// `map`) parses line by line with the environment accumulated so far; when
/// the walk reaches an `include`/`use` live, the target loads through
/// `loader` (anchored at the **requesting file's own directory**, then the
/// `-I` dirs — lwasm 4.24's probe-pinned order: no cwd, no root fallback),
/// its lines parse with the same environment, and an `equ` it defined feeds
/// the includer's later direct-vs-extended selection.
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing target, an include cycle, a bad `includebin` window, or the depth
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

/// lwasm's probe-pinned multi-file semantics: requester-directory resolution
/// (then `-I`; no ancestor hops — a root-directory copy is *not* found from
/// inside a subdirectory include) and the negative-offset-from-EOF
/// `includebin` window.
pub(crate) const SEMANTICS: ca65_flat::WalkSemantics = ca65_flat::WalkSemantics {
    resolution: ca65_flat::Resolution::Requester,
    window: slice_includebin,
    include_default_ext: None,
};

/// Apply lwasm's `includebin` window to the loaded asset — probe-pinned
/// (lwasm 4.24): a **negative offset counts back from EOF** (`-4` = the last
/// four bytes; past the start is "Start value out of range"); offset at EOF
/// or length 0 are legal and empty; an offset past EOF is an error; a
/// negative length or a length past the remaining bytes is "Length value out
/// of range". `Err` carries the message body; the driver wraps it with the
/// request name and the directive's span.
fn slice_includebin(
    data: &[u8],
    offset: Option<i64>,
    size: Option<i64>,
) -> Result<Vec<u8>, String> {
    let len = data.len() as i64;
    let off = match offset.unwrap_or(0) {
        o if o < 0 => len + o,
        o => o,
    };
    if !(0..=len).contains(&off) {
        return Err(format!(
            "start value {} is out of range for the {len}-byte file",
            offset.unwrap_or(0)
        ));
    }
    let remaining = len - off;
    let take = size.unwrap_or(remaining);
    if !(0..=remaining).contains(&take) {
        return Err(format!(
            "length value {take} is out of range ({remaining} byte(s) after the start)"
        ));
    }
    Ok(data[off as usize..(off + take) as usize].to_vec())
}

/// What one line's parse leaves for the lines after it. Three directives set
/// something a later line reads, and none of them is visible from the line that
/// reads it — so the state is carried rather than looked up.
///
/// Both walks keep one. The reading walk parses every line, live or not; the
/// lowering walk sees only the live ones, and is the copy that decides the
/// emitted bytes — which is why a `setdp` or an `org` inside a branch that is
/// not taken never counts, as it does not in lwasm.
#[derive(Default)]
struct ParseState {
    /// The direct page `setdp` last named, zero until one does.
    dp: u8,
    /// The `org` before the current one, which is where `reorg` goes back to,
    /// and the current one. `reorg` moves the current one back without moving
    /// this — so a second `reorg` repeats rather than stepping further, which
    /// is what lwtools 4.25 does.
    prev_org: Option<Expr>,
    cur_org: Option<Expr>,
    /// Whether a `phase` is open here.
    in_phase: bool,
    /// The struct types defined so far, by name.
    structs: BTreeMap<String, StructDef>,
    /// The type being defined here, if `struct` opened one and no `endstruct`
    /// has closed it yet.
    open_struct: Option<OpenStruct>,
    /// The pragmas a `pragma` or `opt` has changed, by index into [`PRAGMAS`].
    /// Anything absent stands where lwasm starts it.
    pragmas: BTreeMap<usize, bool>,
}

/// A struct type: what its members are called, how far into it each one sits,
/// and how big the whole thing is.
#[derive(Clone, Default)]
struct StructDef {
    members: Vec<(String, i64)>,
    size: i64,
}

/// A type part-way through being defined.
#[derive(Clone)]
struct OpenStruct {
    name: String,
    def: StructDef,
}

/// The per-line parse walk shared by [`parse_program`] (single source) and
/// [`parse_program_multi`] (the include-capable walk). The environment — the
/// `equ` constants driving direct-vs-extended selection, and pending comment
/// trivia — lives here, so in the multi-file walk it threads *through*
/// include boundaries in both directions (KTD1, probe-pinned).
struct Walker {
    /// `equ` bindings, consulted for the parse-time direct/extended choice
    /// and `includebin` argument folding.
    env: BTreeMap<String, i64>,
    /// Own-line comments seen since the last node, attached as leading trivia
    /// to the next one. Comments never reach the encoder, so bytes are
    /// unchanged.
    pending_leading: Vec<Comment>,
    /// Whether the walk is inside a macro definition it is copying rather than
    /// reading. Only the formatter's parse ever sets it.
    in_macro: bool,
    /// The macros defined so far, so an invocation is recognised too.
    macro_names: BTreeSet<String>,
    /// What the directives so far left for the lines after them.
    state: ParseState,
    nodes: Vec<Node>,
}

impl Walker {
    fn new() -> Self {
        Self {
            env: BTreeMap::new(),
            pending_leading: Vec::new(),
            in_macro: false,
            macro_names: BTreeSet::new(),
            state: ParseState::default(),
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

    /// Recognise a walk-handled `include`/`use`/`includebin` operation
    /// (keywords are case-insensitive) and parse its arguments with the live
    /// environment. lwasm's grammar is looser than the quoted-only dialects
    /// (probe-pinned): the file name may be quoted **or bare** (a bare name
    /// ends at whitespace — or at a comma for `includebin`), and text after a
    /// quoted name is the Motorola comment field, ignored. `includebin`'s
    /// `,offset[,length]` fold against the constants known so far (a forward
    /// reference errors — lwasm itself misfolds one to an out-of-range 0).
    fn walk_directive(&self, rest: &str, line: usize) -> Result<Option<WalkDirective>, AsmError> {
        let (word, args) = split_first_word(rest);
        let m = word.to_ascii_lowercase();
        match m.as_str() {
            "include" | "use" | "incl" | "lib" => {
                let (request, _) = file_name(args, line, &m)?;
                Ok(Some(WalkDirective::Include { request }))
            }
            "includebin" => {
                let (request, tail) = file_name(args, line, &m)?;
                let tail = tail.trim();
                let (offset, size) = if let Some(list) = tail.strip_prefix(',') {
                    let pieces = mos6502::split_top_level(list, ',');
                    if pieces.len() > 2 {
                        return Err(AsmError::new(
                            line,
                            "`includebin` takes at most a file name, an offset, and a length",
                        ));
                    }
                    let fold = |what: &str, piece: &str| -> Result<i64, AsmError> {
                        fold_const(&value(piece.trim(), line)?, &self.env, line).map_err(|e| {
                            AsmError::new(
                                line,
                                format!(
                                    "`includebin` {what} must be a constant expression: {}",
                                    e.message
                                ),
                            )
                        })
                    };
                    let offset = fold("offset", pieces[0])?;
                    let size = pieces.get(1).map(|p| fold("length", p)).transpose()?;
                    (Some(offset), size)
                } else {
                    (None, None)
                };
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

/// The file-name operand of an lwasm `include`/`use`/`includebin`: a quoted
/// string (the rest of the line after the closing quote is returned for the
/// caller — `includebin` args, else the comment field) or a bare name ending
/// at whitespace or a comma (probe-pinned: lwasm accepts both spellings).
fn file_name<'a>(
    args: &'a str,
    line: usize,
    directive: &str,
) -> Result<(String, &'a str), AsmError> {
    let t = args.trim_start();
    if let Some(inner) = t.strip_prefix('"') {
        let end = inner
            .find('"')
            .ok_or_else(|| AsmError::new(line, format!("unterminated `{directive}` file name")))?;
        let name = &inner[..end];
        if name.is_empty() {
            return Err(AsmError::new(
                line,
                format!("`{directive}` needs a file name"),
            ));
        }
        return Ok((name.to_string(), &inner[end + 1..]));
    }
    let end = t
        .find(|c: char| c.is_whitespace() || c == ',')
        .unwrap_or(t.len());
    let name = &t[..end];
    if name.is_empty() {
        return Err(AsmError::new(
            line,
            format!("`{directive}` needs a file name"),
        ));
    }
    Ok((name.to_string(), &t[end..]))
}

impl FlatWalk for Walker {
    /// lwasm's conditional vocabulary, measured against lwasm 4.24 and
    /// case-insensitive.
    ///
    /// Every numeric form compares its expression against **zero** — `ifne 1`
    /// is true, `iflt 1` is false — so each spelling is its own comparison
    /// rather than a boolean test. Both `endc` **and** `endif` close, which is
    /// one closer more than any other dialect measured here.
    ///
    /// `ifpragma` and `ifstr` are real lwasm and deliberately absent: pragma
    /// strings and string conditions are their own surfaces, demand-gated.
    fn block_keyword(&self, code: &str) -> Option<ca65_flat::BlockKw> {
        use ca65_flat::BlockKw;
        let word = code.split_whitespace().next()?.to_ascii_lowercase();
        Some(match word.as_str() {
            "if" | "ifne" | "ifeq" | "ifgt" | "ifge" | "iflt" | "ifle" | "ifdef" | "ifndef"
            | "ifpragma" | "ifopt" => BlockKw::CondOpen,
            "else" => BlockKw::Else,
            "endc" | "endif" => BlockKw::CondClose,
            _ => return None,
        })
    }

    fn nodes_mut(&mut self) -> &mut Vec<Node> {
        &mut self.nodes
    }

    /// The multi-file walk expands too, or macros would work when a file is
    /// assembled alone and vanish the moment it is included from another.
    fn expand_source(&self, source: &str) -> Result<macros::Expansion, AsmError> {
        expand_lwasm(source, macros::Expand::Yes)
    }

    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
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

        // A macro definition is copied verbatim, column included: lwasm spells
        // one `name macro` and *only* that way — its own header parser refuses
        // an indented `macro`, which is the same rule real lwasm applies. See
        // `Item::Verbatim`.
        //
        // A body is a template rather than code (`\1` is not an expression), so
        // nothing between the header and the close is parsed.
        {
            use crate::dialects::macros::MacroSyntax as _;
            let text = code.trim();
            let opened = LwasmMacros.header(code);
            let invoked = text.split_whitespace().next().unwrap_or("");
            if self.in_macro || opened.is_some() || self.macro_names.contains(invoked) {
                if let Some((name, _)) = opened {
                    self.macro_names.insert(name);
                    self.in_macro = true;
                } else if LwasmMacros.is_end(text) {
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

        let (label, rest) = split_label(code);
        // `include`/`use`/`includebin` are walk-handled, not directives: the
        // target must not be opened here (KTD1 — `--fmt` succeeds with a
        // missing target), so hand them back for the driver to resolve (or
        // keep unresolved, in the single-source parse). A label on the line
        // binds at the include point / payload start (probe-pinned).
        if let Some(kind) = self.walk_directive(rest, line)? {
            return Ok(Some(DirectiveLine {
                kind,
                label: label.map(|name| Symbol {
                    qualified: name.clone(),
                    scope: Scope::Global,
                    name,
                }),
                source: rest.trim().to_string(),
                span: Span::in_file(file, line as u32, 1),
                operand_span: ca65_flat::directive_operand_span(raw, rest, line, file),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            }));
        }
        // A struct definition describes a layout rather than stating one, and
        // an instance names a type where an opcode would go — neither reads as
        // an ordinary operation. The node is kept so the lowering walk sees the
        // line (and so `--fmt` renders it), but nothing here is parsed.
        // An error here is deferred, not raised: this walk reads every line,
        // live or not, and lwasm does not parse a branch it is not taking. The
        // lowering walk runs the same reading over the live lines only, and
        // says the same thing there — where it counts.
        let is_struct_line =
            match struct_line(label.as_deref(), rest, &mut self.state, &self.env, line) {
                Ok(effect) => effect.is_some(),
                Err(_) => true,
            };
        if is_struct_line {
            self.nodes.push(Node {
                operand_span: None,
                label: label.map(|name| Symbol {
                    qualified: name.clone(),
                    scope: Scope::Global,
                    name,
                }),
                item: Some(crate::ast::Item::Native(Box::new(StructLine))),
                source: rest.trim().to_string(),
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }
        let op = if rest.is_empty() {
            None
        } else {
            parse_op(rest, &self.env, &mut self.state, line)?
        };
        // Bind an `equ` value into the parse-time env so a later direct/extended
        // choice can fold it (mirrors the engine's pass-1 `equ`).
        if let (Some(name), Some(Operation::Equ(e) | Operation::Set(e))) = (&label, &op)
            && let Ok(v) = fold_const(e, &self.env, line)
        {
            self.env.insert(name.clone(), v);
        }
        if label.is_none() && op.is_none() {
            return Ok(None);
        }
        self.nodes.push(Node {
            operand_span: crate::ast::operand_span(raw, rest, line as u32).map(|mut s| {
                s.file = file;
                s
            }),
            label: label.map(|name| Symbol {
                qualified: name.clone(),
                scope: Scope::Global,
                name,
            }),
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

/// Split a line into its code and its comment (delimiter kept, trailing
/// whitespace trimmed) for carrying comments as AST trivia; defined via
/// [`strip_comment`] so the comment is exactly what it removes. A whole-line
/// `*` comment yields empty code and the whole line as the comment.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
}

/// Strip a comment: a `*` as the first non-blank character makes the whole line
/// a comment (lwasm convention), and a `;` outside a string starts one anywhere.
fn strip_comment(line: &str) -> &str {
    if line.trim_start().starts_with('*') {
        return "";
    }
    let bytes = line.as_bytes();
    let mut in_str = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_str = !in_str,
            b';' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Split a column-0 label from the rest of the line. A line beginning with
/// whitespace has no label; otherwise the first token is the label (an optional
/// trailing `:` is dropped), and the remainder is the opcode + operand.
fn split_label(code: &str) -> (Option<String>, &str) {
    if code.starts_with([' ', '\t']) {
        return (None, code.trim());
    }
    let (word, remainder) = split_first_word(code.trim());
    let name = word.strip_suffix(':').unwrap_or(word);
    (Some(name.to_string()), remainder)
}

/// Parse the operation part (after any label): a pseudo-op or an instruction.
/// What this dialect accepts beyond the 6809 instruction set.
///
/// lwasm spells several concepts two ways — `fcb` and `.byte` are the same
/// directive — so these are alternative spellings rather than a sigil applied
/// to a name, and `Exact` carries both.
pub const DIRECTIVES: &[Directive] = &[
    // Walk-handled: the shared cursor reads these into `Item::Conditional`
    // before `parse_op` sees a line.
    //
    // Every numeric form compares against **zero** rather than taking a
    // boolean, which is why there are six of them and not one — plus plain
    // `if`, which is `ifne` under a shorter name. Both `endc` and `endif`
    // close — the only dialect measured here with two closers.
    //
    // `ifp1` and `ifp2` ask which pass is running, and lwtools 4.25 answers
    // neither: it warns "Not supported IFP1" and takes the true branch. The
    // branch is easy here; the warning has nowhere to go, because `CondEval`
    // folds a head from `&self` and cannot report one. They stay outstanding
    // until it can.
    //
    // `ifpragma` and `ifstr` are real lwasm and deliberately absent: pragma
    // strings and string conditions are their own surfaces, demand-gated.
    Directive {
        id: "conditional",
        pattern: Pattern::Exact(&[
            "if", "ifne", "ifeq", "ifgt", "ifge", "iflt", "ifle", "ifdef", "ifndef", "ifpragma",
            "ifopt", "else", "endc", "endif",
        ]),
        category: Category::Operation,
    },
    Directive {
        id: "org",
        pattern: Pattern::Exact(&["org"]),
        category: Category::Operation,
    },
    // `reorg` sets the counter back to the value of the `org` *before* the
    // current one. Measured against lwtools 4.25: it needs two to have
    // somewhere to go ("Previous ORG not found"), takes no operand, and moves
    // the current `org` back without moving the previous one — so a second
    // `reorg` in a row repeats rather than stepping further back. It changes
    // the address only; the output stays where it was going.
    Directive {
        id: "reorg",
        pattern: Pattern::Exact(&["reorg"]),
        category: Category::Operation,
    },
    Directive {
        id: "equ",
        pattern: Pattern::Exact(&["equ"]),
        category: Category::Operation,
    },
    // `set` is `equ` for a name that moves. It binds the label again as often
    // as it likes, and a use above the statement reads the value the name is
    // left with at the end of the file where a use below reads the one bound
    // here — which is what lwtools 4.25 does, and what a two-pass assembler
    // has to do with a name that has more than one value.
    //
    // A name is one or the other: `set` over an `equ`, or an `equ` over a
    // `set`, is "Multiply defined symbol" whichever came first.
    Directive {
        id: "set",
        pattern: Pattern::Exact(&["set"]),
        category: Category::Operation,
    },
    Directive {
        id: "bytes",
        pattern: Pattern::Exact(&["fcb", ".byte"]),
        category: Category::Operation,
    },
    Directive {
        id: "words",
        pattern: Pattern::Exact(&["fdb", ".word"]),
        category: Category::Operation,
    },
    Directive {
        id: "fcc",
        pattern: Pattern::Exact(&["fcc"]),
        category: Category::Operation,
    },
    // `fcc`'s two terminated cousins. `fcn` and `fcz` are the same directive
    // under two names — both append a NUL — and `fcs` instead sets the high
    // bit of the last byte, the 6809 convention for marking a string's end
    // without spending a byte on it. All three take one delimited string, as
    // `fcc` does; a comma list is `Bad operand` for every one of them.
    Directive {
        id: "fcn",
        pattern: Pattern::Exact(&["fcn", "fcz"]),
        category: Category::Operation,
    },
    Directive {
        id: "fcs",
        pattern: Pattern::Exact(&["fcs"]),
        category: Category::Operation,
    },
    Directive {
        id: "words-swapped",
        pattern: Pattern::Exact(&["fdbs"]),
        category: Category::Operation,
    },
    Directive {
        id: "fqb",
        pattern: Pattern::Exact(&["fqb"]),
        category: Category::Operation,
    },
    // The reserve family is three entries because lwasm's spellings carry a
    // *width*, not just a count: `rmd 3` is six bytes, `rmq 3` twelve. Probed
    // against lwtools 4.25 with `--raw`, where every one of them zero-fills —
    // `rmb` and `zmb` differ only for an object target, which we never emit.
    Directive {
        id: "reserve",
        pattern: Pattern::Exact(&["rmb", ".ds", "zmb", "bsz", "fzb"]),
        category: Category::Operation,
    },
    Directive {
        id: "reserve-double",
        pattern: Pattern::Exact(&["rmd", "zmd", "rmw"]),
        category: Category::Operation,
    },
    Directive {
        id: "reserve-quad",
        pattern: Pattern::Exact(&["rmq", "zmq"]),
        category: Category::Operation,
    },
    Directive {
        id: "fill",
        pattern: Pattern::Exact(&["fill"]),
        category: Category::Operation,
    },
    Directive {
        id: "end",
        pattern: Pattern::Exact(&["end"]),
        category: Category::Ignored,
    },
    // The five words that only ever reach the listing: a title (`nam`, `ttl`),
    // a page break (`pag`, `page`) and vertical space (`spc`). asm198x writes
    // no listing of lwasm's kind, so there is nothing for them to do here.
    //
    // Ignoring them is what lwtools 4.25 does to the *bytes*, which is the
    // claim that matters: each was probed bare, with an operand and with junk
    // for an operand, and emitted nothing every time. `opt` is not among them
    // however much it looks like one — see `unsupported-lwasm`.
    Directive {
        id: "listing",
        pattern: Pattern::Exact(&["nam", "ttl", "pag", "page", "spc"]),
        category: Category::Ignored,
    },
    // Walk-handled. lwasm has four spellings of one directive, so they are
    // alternative spellings of one entry rather than four directives — the
    // same call `fcb`/`.byte` already get here. `incl` and `lib` were probed
    // against lwtools 4.25 quoted and bare, beside `include` and `use`, and
    // all four answered identically, missing-file message included.
    Directive {
        id: "include",
        pattern: Pattern::Exact(&["include", "use", "incl", "lib"]),
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Exact(&["includebin"]),
        category: Category::Operation,
    },
    // What lwasm has here and we do not.
    //
    // 10 spellings against lwtools 4.25.
    //
    // **Directives only.** The first cut of this list swept in fifteen 6809
    // *instructions* — `adca`, `bita`, `cmpd`, `cwai`, `sbca` among them —
    // because they reach this dialect the same way a directive does and
    // `lwasm` answers `Bad operand` for both when neither has one. Declaring
    // an instruction a directive is worse than saying nothing: it is wrong,
    // and it points the reader at the wrong layer.
    //
    // Telling them apart took two passes. Giving each an immediate operand
    // and keeping the ones that emitted bytes caught the instructions — and
    // also `dtb`, `dts` and `emod`, which emit bytes because they *are* data:
    // `dts` assembles to the ASCII of the current date. Reading the bytes
    // rather than counting them put those three back. The twelve that
    // remain are tracked as the ISA gap they are (#225).
    //
    // lwasm's 6309 instructions are absent for the reason vasm's 68020 ones
    // are: lwasm refuses them itself in 6809 mode.
    Directive {
        id: "align",
        pattern: Pattern::Exact(&["align"]),
        category: Category::Operation,
    },
    // `setdp page` says which 256-byte page the 6809's DP register will hold
    // when this code runs, so the assembler can reach an address on it in two
    // bytes instead of three. It emits nothing and promises something: get it
    // wrong and the code reads the wrong address at run time, which is why
    // lwasm makes it explicit rather than guessing.
    //
    // The operand is a page number, taken modulo 256 — `setdp $2000` is page
    // `$00`, not page `$20` — and it must fold on pass one ("SETDP must be
    // constant on pass 1"), so a forward symbol is refused. The last one above
    // a line is the one that line uses, and one inside a branch that is not
    // taken never happens at all.
    Directive {
        id: "setdp",
        pattern: Pattern::Exact(&["setdp"]),
        category: Category::Operation,
    },
    // `phase addr` moves the *address* without moving the output: the bytes
    // keep landing where they were going, and labels between here and
    // `dephase` read as if the code sat at `addr`. It is what a routine copied
    // to another address before it runs needs, and it is the same machinery as
    // ACME's `!pseudopc`.
    //
    // lwtools 4.25 refuses to nest one inside another ("Nested PHASE not
    // supported") and refuses a `dephase` that closes nothing ("DEPHASE
    // without PHASE"). Both are checked after the parse, over the statement
    // stream, because the parse of a line cannot see the lines around it.
    // A `phase` still open at the end of the source is fine, and stays fine.
    //
    // `reorg` is the third word of this family, declared beside `org`. The one
    // corner the two do not share is a `reorg` *inside* an open `phase`: lwasm
    // keeps the phased address counting from where it was, where this engine
    // derives it from the real counter, so the two would part. That corner is
    // refused by name rather than answered wrongly.
    Directive {
        id: "pseudo-pc",
        pattern: Pattern::Exact(&["phase", "dephase"]),
        category: Category::Operation,
    },
    // `error <text>` takes the rest of the line verbatim — no quotes, no
    // expression list. lwasm reports it as "User Specified: <text>".
    //
    // `warning` and `msg` are the same directive at warning severity: lwtools
    // 4.25 answers both identically, assembly continues, and the exit status
    // stays zero. Neither says anything from inside an untaken conditional.
    Directive {
        id: "diagnose",
        pattern: Pattern::Exact(&["error"]),
        category: Category::Operation,
    },
    Directive {
        id: "diagnose-warning",
        pattern: Pattern::Exact(&["warning", "msg"]),
        category: Category::Operation,
    },
    // The five words lwasm has and refuses for the output we produce. Probed
    // against lwtools 4.25 with `--raw`, with an operand and without: each
    // answers `Only supported for object target (EXPORT)`. asm198x emits a
    // binary and never an object file, so that is every path there is — and
    // refusing them is what matching lwasm means, not a gap.
    Directive {
        id: "object-target-only",
        pattern: Pattern::Exact(&["export", "extdep", "extern", "external", "import"]),
        category: Category::RefusedByReference(
            "only supported for an object target, and asm198x emits a binary",
        ),
    },
    // The section words go the same way, and say so in lwasm's own words:
    // "Cannot use sections unless using the object target". Probed against
    // lwtools 4.25 with `--raw`, bare and with arguments — all four answer it
    // every time, and only when the line is live: one inside a branch that is
    // not taken is never reached, here as there.
    Directive {
        id: "sections",
        pattern: Pattern::Exact(&["section", "sect", "endsection", "endsect"]),
        category: Category::RefusedByReference(
            "only usable with an object target, and asm198x emits a binary",
        ),
    },
    // `pragma` and `opt` reach the same switches — `opt 6809` really does turn
    // away a 6309 instruction — and `ifpragma`/`ifopt` ask whether one is set.
    // The difference between the two spellings is what they do with a name
    // they do not know: `pragma zzz` is "Unrecognized pragma string" and `opt
    // zzz` assembles, in silence.
    //
    // Eight of the forty-nine spellings ask for something asm198x does not do,
    // and each is refused by name where it is set rather than accepted and
    // ignored. See [`PRAGMAS`] for which, and for what the rest were measured
    // against.
    Directive {
        id: "pragma",
        pattern: Pattern::Exact(&["pragma", "opt"]),
        category: Category::Operation,
    },
    // `name struct` … `endstruct` describes a layout without laying anything
    // out. The members name offsets into the type — `pt.x` is a constant, the
    // type name itself is not a symbol at all — and `v pt` then reserves one
    // of them, binding `v` to where it sits and `v.x` to the member's address.
    //
    // Measured against lwtools 4.25: only reserving directives may sit inside
    // one (`fcb` is "Bad operand"), a member may be another struct, defining
    // the same name twice is "Duplicate structure definition", an unnamed
    // `struct` is "Structure definition with no effect - no symbol", and an
    // instance without a label is not one ("Bad opcode"). `ends` is the second
    // spelling of `endstruct`.
    Directive {
        id: "struct",
        pattern: Pattern::Exact(&["struct", "endstruct", "ends"]),
        category: Category::Operation,
    },
    // Macro definition. The walk reads a `name macro` header and its `endm`
    // before `parse_op` sees either, so these are declared for what they are
    // rather than dispatched: reaching the dispatch means the line was not
    // part of a definition, and lwasm has an answer for each of those too.
    Directive {
        id: "macro",
        pattern: Pattern::Exact(&["macro", "macr", "endm"]),
        category: Category::Operation,
    },
    Directive {
        id: "unsupported-lwasm",
        pattern: Pattern::Exact(&[
            "dtb",
            "dts",
            "emod",
            "ifp1",
            "ifp2",
            "ifstr",
            "includestr",
            "mod",
            "os9",
            "setstr",
        ]),
        category: Category::KnownUnsupported,
    },
];

/// An operation that objects where it stands rather than where it was read —
/// see [`Category::KnownUnsupported`]'s arm in [`parse_op`] for why every
/// refusal in this dialect takes this shape.
fn refused(message: impl Into<String>) -> Operation {
    Operation::Diagnose {
        severity: crate::engine::DiagSeverity::Error,
        message: message.into(),
    }
}

/// One of lwasm's pragmas: the spellings that turn it on, the spellings that
/// turn it off, whether it starts on, and — where asm198x cannot follow — what
/// changes in the direction it cannot follow.
///
/// The vocabulary and the starting state were read out of lwtools 4.25 rather
/// than guessed: every name below answers `pragma`, and `ifpragma <name>` was
/// asked for each one before anything set it. A name outside the table is
/// "Unrecognized pragma string" there and here.
struct Pragma {
    on: &'static [&'static str],
    off: &'static [&'static str],
    starts_on: bool,
    /// What turning it **on** would change, when this dialect cannot do it.
    gap_on: Option<&'static str>,
    /// The same for turning it **off**.
    gap_off: Option<&'static str>,
}

/// lwasm's pragmas, each with the spellings that reach it.
///
/// The eight refusals below are the ones measured to change something across a
/// corpus of fourteen probe programs — plain and indexed addressing, `,pc`
/// indexing, short and long branches, forward references, strings, `$` in a
/// symbol, colliding symbol case, an undefined symbol in a condition, an
/// oversized operand, a macro, a struct and a conditional. Every other pragma
/// left all fourteen byte-identical, which is evidence rather than proof: the
/// corpus is the warrant, and a construct it does not reach could still hide
/// one.
const PRAGMAS: &[Pragma] = &[
    Pragma {
        on: &["6309"],
        off: &["6809"],
        starts_on: true,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["c"],
        off: &[],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["6800compat"],
        off: &["no6800compat"],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["autobranchlength"],
        off: &["noautobranchlength"],
        starts_on: false,
        gap_on: Some("choosing a branch's length for the source"),
        gap_off: None,
    },
    Pragma {
        on: &["cescapes"],
        off: &["nocescapes"],
        starts_on: false,
        gap_on: Some("C escapes inside a string"),
        gap_off: None,
    },
    Pragma {
        on: &["cd"],
        off: &["nocd"],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["condundefzero"],
        off: &["nocondundefzero"],
        starts_on: false,
        gap_on: Some("reading an undefined symbol as zero in a condition"),
        gap_off: None,
    },
    Pragma {
        on: &["dollarlocal", "nodollarnotlocal"],
        off: &["nodollarlocal", "dollarnotlocal"],
        starts_on: true,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["expandcond"],
        off: &["noexpandcond"],
        starts_on: true,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["export"],
        off: &["noexport"],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["forwardrefmax"],
        off: &["noforwardrefmax"],
        starts_on: true,
        gap_on: None,
        gap_off: Some("sizing a forward reference by its value rather than at maximum"),
    },
    Pragma {
        on: &["importundefexport"],
        off: &["noimportundefexport"],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["index0tonone"],
        off: &["noindex0tonone"],
        starts_on: true,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["list"],
        off: &["nolist"],
        starts_on: true,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["listcode"],
        off: &["nolistcode"],
        starts_on: true,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["m80ext"],
        off: &["nom80ext"],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["oldsource", "nonewsource"],
        off: &["nooldsource", "newsource"],
        starts_on: true,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["operandsizewarning"],
        off: &["nooperandsizewarning"],
        starts_on: false,
        gap_on: Some("warning that an operand is wider than it needs to be"),
        gap_off: None,
    },
    Pragma {
        on: &["pcaspcr"],
        off: &["nopcaspcr"],
        starts_on: false,
        gap_on: Some("reading `,pc` as `,pcr`"),
        gap_off: None,
    },
    Pragma {
        on: &["shadow"],
        off: &["noshadow"],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
    Pragma {
        on: &["symbolcase", "nosymbolnocase"],
        off: &["nosymbolcase", "symbolnocase"],
        starts_on: true,
        gap_on: None,
        gap_off: Some("matching symbol names without regard to case"),
    },
    Pragma {
        on: &["undefextern"],
        off: &["noundefextern"],
        starts_on: false,
        gap_on: None,
        gap_off: None,
    },
];

/// Find the pragma one spelling reaches, and which way that spelling points.
fn pragma_named(name: &str) -> Option<(usize, bool)> {
    PRAGMAS.iter().enumerate().find_map(|(i, p)| {
        if p.on.contains(&name) {
            Some((i, true))
        } else if p.off.contains(&name) {
            Some((i, false))
        } else {
            None
        }
    })
}

/// Whether a pragma is on as the source stands. `set` holds only what a
/// `pragma` or `opt` has changed, so anything untouched answers with what
/// lwasm starts it at.
fn pragma_is_on(set: &BTreeMap<usize, bool>, index: usize) -> bool {
    set.get(&index).copied().unwrap_or(PRAGMAS[index].starts_on)
}

/// Apply one `pragma`/`opt` name. `strict` is `pragma`'s: it refuses a name it
/// does not know, where `opt` passes over one in silence (measured — `opt zzz`
/// assembles, `pragma zzz` is "Unrecognized pragma string").
fn set_pragma(
    name: &str,
    strict: bool,
    set: &mut BTreeMap<usize, bool>,
    line: usize,
) -> Result<(), AsmError> {
    let Some((index, want_on)) = pragma_named(name) else {
        if strict {
            return Err(AsmError::new(
                line,
                format!("unrecognized pragma string `{name}`"),
            ));
        }
        return Ok(());
    };
    let p = &PRAGMAS[index];
    let gap = if want_on { p.gap_on } else { p.gap_off };
    if let Some(what) = gap
        && pragma_is_on(set, index) != want_on
    {
        return Err(AsmError::new(
            line,
            format!(
                "`{name}` asks for {what}, which asm198x does not do — the source is \
                 valid and the gap is ours"
            ),
        ));
    }
    set.insert(index, want_on);
    Ok(())
}

/// Split a `pragma`/`opt` operand into its names: commas separate them, and
/// the first space ends them — everything after it is the Motorola comment
/// field, as it is elsewhere in this dialect.
///
/// That pair of rules is what makes `pragma list nolist` set only `list`,
/// while `pragma list,nolist` sets both and `pragma list, nolist` is
/// "Unrecognized pragma string": the comment field starts after the comma,
/// leaving an empty name behind it. All three were measured.
fn pragma_names(operand: &str) -> Vec<String> {
    let field = operand.trim_start();
    let field = &field[..field.find(char::is_whitespace).unwrap_or(field.len())];
    field.split(',').map(|n| n.to_ascii_lowercase()).collect()
}

/// A struct line's label belongs *on* the line: lwasm reads the type name, the
/// member name and the instance name from label position, so a formatter that
/// lifted one onto its own line above would break what it reformatted.
struct StructLine;

impl crate::ast::NativeItem for StructLine {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn inline_label(&self) -> bool {
        true
    }
}

/// What a line meant to the struct machinery.
enum StructEffect {
    /// Part of a definition: it describes the layout and emits nothing.
    Nothing,
    /// `endstruct` just closed a type — bind each `<type>.<member>` to its
    /// offset, so `pt.x` reads as a constant with no instance in sight. The
    /// type name itself is deliberately not bound: lwasm answers "Undefined
    /// symbol pt" for one.
    Closed { name: String, def: StructDef },
    /// `label <type>` — reserve one here, and name where each member landed.
    Instance { def: StructDef },
}

/// Read one line as the struct machinery sees it, updating `state`. `None`
/// means the line is nothing to do with structs and belongs to the ordinary
/// parse.
///
/// Both walks call this, each against its own state: the reading walk to keep
/// from choking on a type name where an opcode would go, the lowering walk to
/// decide the bytes.
fn struct_line(
    label: Option<&str>,
    rest: &str,
    state: &mut ParseState,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<StructEffect>, AsmError> {
    let (word, operand) = split_first_word(rest.trim());
    let w = word.to_ascii_lowercase();
    if w == "struct" {
        if state.open_struct.is_some() {
            return Err(AsmError::new(line, "`struct` inside a `struct`"));
        }
        let name = label.ok_or_else(|| {
            AsmError::new(line, "structure definition with no effect - no symbol")
        })?;
        if state.structs.contains_key(name) {
            return Err(AsmError::new(line, "duplicate structure definition"));
        }
        state.open_struct = Some(OpenStruct {
            name: name.to_string(),
            def: StructDef::default(),
        });
        return Ok(Some(StructEffect::Nothing));
    }
    if w == "endstruct" || w == "ends" {
        let open = state
            .open_struct
            .take()
            .ok_or_else(|| AsmError::new(line, "`endstruct` without a `struct`"))?;
        state.structs.insert(open.name.clone(), open.def.clone());
        return Ok(Some(StructEffect::Closed {
            name: open.name,
            def: open.def,
        }));
    }
    // Inside a definition every line describes a member: how much room it
    // takes, and — if it is labelled — what that room is called. lwasm allows
    // only the reserving directives and other struct types there; `fcb 9` is
    // "Bad operand", because a struct is a shape and not data.
    if let Some(open) = &state.open_struct {
        let width = member_width(&w, operand, &state.structs, env, line)?;
        let at = open.def.size;
        let open = state.open_struct.as_mut().expect("still open");
        if let Some(name) = label {
            open.def.members.push((name.to_string(), at));
        }
        open.def.size += width;
        return Ok(Some(StructEffect::Nothing));
    }
    // Outside one, a type name where an opcode would go reserves an instance —
    // but only with a label to bind it to. lwasm answers "Bad opcode" for a
    // bare one, which is what the ordinary parse says too, so it is left to it.
    if let (Some(_), Some(def)) = (label, state.structs.get(&w)) {
        return Ok(Some(StructEffect::Instance { def: def.clone() }));
    }
    Ok(None)
}

/// How much room one member of a struct takes: a reserving directive's count
/// times its unit, or the whole size of another struct type.
fn member_width(
    word: &str,
    operand: &str,
    structs: &BTreeMap<String, StructDef>,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<i64, AsmError> {
    if let Some(def) = structs.get(word) {
        return Ok(def.size);
    }
    let unit = match word {
        "rmb" | "zmb" | "bsz" | "fzb" | ".ds" => 1,
        "rmd" | "zmd" | "rmw" => 2,
        "rmq" | "zmq" => 4,
        _ => {
            return Err(AsmError::new(
                line,
                format!("`{word}` cannot be a struct member — a struct reserves room"),
            ));
        }
    };
    let count = fold_const(&value(operand, line)?, env, line)?;
    if count < 0 {
        return Err(AsmError::new(line, "negative block sizes make no sense"));
    }
    Ok(count * unit)
}

fn parse_op(
    rest: &str,
    env: &BTreeMap<String, i64>,
    state: &mut ParseState,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (mnem, operand) = split_first_word(rest);
    let m = mnem.to_ascii_lowercase();
    // Dispatch through the declared surface: a spelling the declaration does
    // not carry cannot be accepted here. See `crate::directives`.
    let Some(directive) = lookup(DIRECTIVES, &m) else {
        return Ok(Some(parse_instruction(&m, operand, env, state.dp, line)?));
    };
    match directive.category {
        // `end` marks the end of source; it emits nothing.
        Category::Ignored => Ok(None),
        // A refused word is refused when it is *reached*, not when it is read.
        // lwasm does not parse a branch it is not taking at all — anything at
        // all may sit inside `if 0`, an unknown opcode and an unterminated
        // string included — so a word we would turn away has to survive the
        // parse and object at assembly time. `Operation::Diagnose` is that:
        // the engine raises it where the statement stands, and a statement
        // inside a dead branch never becomes one.
        Category::KnownUnsupported => Ok(Some(Operation::Diagnose {
            severity: crate::engine::DiagSeverity::Error,
            message: format!(
                "`{m}` is a real directive here and asm198x does not implement it yet"
            ),
        })),
        // Declared for `lwasm` only where lwasm itself refuses the word for the
        // binary we emit; the refusal is the match, not a gap.
        Category::RefusedByReference(rule) => Ok(Some(Operation::Diagnose {
            severity: crate::engine::DiagSeverity::Error,
            message: crate::directives::refused_by_reference("lwasm", &m, rule),
        })),
        Category::Operation => match directive.id {
            "org" => {
                if state.in_phase {
                    return Err(AsmError::new(
                        line,
                        "`org` inside a `phase` — lwasm keeps the phased address counting \
                         from where it was, and asm198x derives it from the real counter, so \
                         the two would part here; the source is valid and the gap is ours",
                    ));
                }
                let e = value(operand, line)?;
                state.prev_org = state.cur_org.replace(e.clone());
                Ok(Some(Operation::Org(e)))
            }
            // `reorg` goes back to the `org` before the current one, so it
            // needs two to have somewhere to go — one is "Previous ORG not
            // found". It takes no operand, and it moves the current `org`
            // back without moving the previous one, so a second `reorg` in a
            // row repeats rather than stepping further.
            "reorg" => {
                if !operand.trim().is_empty() {
                    return Err(AsmError::new(
                        line,
                        format!("`reorg` takes no operand (got `{}`)", operand.trim()),
                    ));
                }
                if state.in_phase {
                    return Err(AsmError::new(
                        line,
                        "`reorg` inside a `phase` — lwasm keeps the phased address counting \
                         from where it was, and asm198x derives it from the real counter, so \
                         the two would part here; the source is valid and the gap is ours",
                    ));
                }
                let back = state
                    .prev_org
                    .clone()
                    .ok_or_else(|| AsmError::new(line, "previous `org` not found"))?;
                state.cur_org = Some(back.clone());
                Ok(Some(Operation::Org(back)))
            }
            "equ" => Ok(Some(Operation::Equ(value(operand, line)?))),
            "set" => Ok(Some(Operation::Set(value(operand, line)?))),
            // A bare `opt` is nothing to do and assembles; a bare `pragma` is a
            // name it does not know, and does not.
            "pragma" => {
                let strict = m == "pragma";
                if operand.trim().is_empty() {
                    if strict {
                        return Ok(Some(refused("unrecognized pragma string")));
                    }
                    return Ok(Some(Operation::Bytes(Vec::new())));
                }
                // A refusal here is raised where the statement stands rather
                // than where it is read, for the reason every other refusal in
                // this dialect is: lwasm does not parse a branch it is not
                // taking, so a pragma it would turn away has to reach the
                // engine before it objects.
                for name in pragma_names(operand) {
                    if let Err(e) = set_pragma(&name, strict, &mut state.pragmas, line) {
                        return Ok(Some(refused(e.message)));
                    }
                }
                Ok(Some(Operation::Bytes(Vec::new())))
            }
            // Reaching here means the walk did not take this line as part of a
            // definition — a `macro` with no name in label position, or an
            // `endm` closing nothing. lwasm names both.
            "macro" => Ok(Some(Operation::Diagnose {
                severity: crate::engine::DiagSeverity::Error,
                message: if m == "endm" {
                    "`endm` without a `macro`".to_string()
                } else {
                    "missing macro name".to_string()
                },
            })),
            "bytes" => Ok(Some(Operation::Bytes(list(operand, line)?))),
            "words" => Ok(Some(Operation::Words(list(operand, line)?))),
            "fcc" => Ok(Some(parse_fcc(operand, line, StringEnd::Bare)?)),
            "fcn" => Ok(Some(parse_fcc(operand, line, StringEnd::Nul)?)),
            "fcs" => Ok(Some(parse_fcc(operand, line, StringEnd::HighBit)?)),
            "words-swapped" => Ok(Some(parse_fdbs(operand, line)?)),
            "fqb" => Ok(Some(parse_fqb(operand, line)?)),
            "reserve" => parse_rmb(&m, operand, env, line, 1),
            "reserve-double" => parse_rmb(&m, operand, env, line, 2),
            "reserve-quad" => parse_rmb(&m, operand, env, line, 4),
            "fill" => parse_fill(operand, env, line),
            "align" => parse_align(operand, env, line),
            "setdp" => {
                if operand.trim().is_empty() {
                    return Err(AsmError::new(line, "`setdp` needs a page"));
                }
                let page = fold_const(&value(operand, line)?, env, line)
                    .map_err(|_| AsmError::new(line, "`setdp` must be constant on pass 1"))?;
                state.dp = (page & 0xFF) as u8;
                Ok(Some(Operation::Bytes(Vec::new())))
            }
            "pseudo-pc" => {
                if m == "dephase" {
                    state.in_phase = false;
                    return Ok(Some(Operation::PseudoPc(None)));
                }
                state.in_phase = true;
                if operand.trim().is_empty() {
                    return Err(AsmError::new(line, "`phase` needs an address"));
                }
                Ok(Some(Operation::PseudoPc(Some(value(operand, line)?))))
            }
            "diagnose" => Ok(Some(Operation::Diagnose {
                severity: crate::engine::DiagSeverity::Error,
                message: format!("User Specified: {}", operand.trim()),
            })),
            // The text passes through as written. lwasm prefixes its own
            // warning line with the word "Error" — it reuses the error
            // reporter's label — and that is a slip in its display rather
            // than part of what the source said, so repeating it inside our
            // own "warning:" frame would only mislead. The bytes, which are
            // what source compatibility is a claim about, are unaffected.
            "diagnose-warning" => Ok(Some(Operation::Diagnose {
                severity: crate::engine::DiagSeverity::Warning,
                message: operand.trim().to_string(),
            })),
            other => Err(AsmError::new(
                line,
                format!("`{other}` is declared but not dispatched"),
            )),
        },
    }
}

/// The reserve family — `rmb`/`zmb`/`bsz`/`fzb` (`width` 1), `rmd`/`zmd`/`rmw`
/// (2) and `rmq`/`zmq` (4): `count` units of `width` bytes, zero-filled (the
/// flat-output behaviour). `count` folds against the parse-time env so the size
/// is known in pass one.
///
/// lwasm refuses a negative count in its own words — "Negative block sizes make
/// no sense!" — rather than reading it as a huge unsigned one, so the count is
/// range-checked here and not masked.
fn parse_rmb(
    mnemonic: &str,
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
    width: usize,
) -> Result<Option<Operation>, AsmError> {
    let n = fold_const(&value(operand, line)?, env, line)?;
    let n = usize::try_from(n).map_err(|_| {
        AsmError::new(
            line,
            format!("`{mnemonic}` count must be a non-negative constant"),
        )
    })?;
    Ok(Some(Operation::Bytes(vec![Expr::Num(0); n * width])))
}

/// `align boundary[,fill]` — pad to the next multiple of `boundary`. lwasm
/// aligns to the boundary itself, not to a power of two, and `align 3` after a
/// byte really does put the next item at offset 3. The operand is required
/// (bare `align` is `Bad operand`), and the pad byte defaults to zero.
fn parse_align(
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let parts = mos6502::split_top_level(operand, ',');
    if operand.trim().is_empty() || parts.is_empty() {
        return Err(AsmError::new(line, "`align` needs a boundary"));
    }
    let modulus = fold_const(&value(parts[0].trim(), line)?, env, line)?;
    if modulus < 1 {
        return Err(AsmError::new(line, "`align` boundary must be positive"));
    }
    let fill = match parts.get(1) {
        Some(f) => {
            u8::try_from(fold_const(&value(f.trim(), line)?, env, line)? & 0xFF).expect("masked")
        }
        None => 0,
    };
    Ok(Some(Operation::AlignTo { modulus, fill }))
}

/// `fill value,count` — `count` copies of `value` (lwasm's order is value first,
/// then count; both are required). Both fold against the parse-time env so the
/// size and fill are known in pass one.
fn parse_fill(
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let parts = mos6502::split_top_level(operand, ',');
    if parts.len() != 2 {
        return Err(AsmError::new(line, "`fill` needs `value,count`"));
    }
    let fill = fold_const(&value(parts[0].trim(), line)?, env, line)?;
    let fill = u8::try_from(fill & 0xFF).expect("masked");
    let count = fold_const(&value(parts[1].trim(), line)?, env, line)?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`fill` count must be a non-negative constant"))?;
    Ok(Some(Operation::Bytes(vec![
        Expr::Num(i64::from(fill));
        count
    ])))
}

/// `fqb value[,value…]` — "form quad byte": each value as a 32-bit big-endian
/// word. Emitted through the engine's computed-operand seam so symbolic values
/// resolve in pass two.
fn parse_fqb(operand: &str, line: usize) -> Result<Operation, AsmError> {
    let pieces = list(operand, line)?
        .into_iter()
        .map(|expr| Piece::Val {
            expr,
            bytes: 4,
            rel: false,
            signed: false,
        })
        .collect();
    Ok(Operation::Encoded(pieces))
}

/// Encode one instruction into `Operation::Encoded` pieces.
fn parse_instruction(
    m: &str,
    operand: &str,
    env: &BTreeMap<String, i64>,
    dp: u8,
    line: usize,
) -> Result<Operation, AsmError> {
    if let Some(insn) = mos6809::lookup(m) {
        match &insn.kind {
            Kind::Inherent(opcode) => encode_inherent(m, opcode, operand, line),
            Kind::Branch { short, .. } => encode_branch(short, 1, operand, line),
            Kind::Mem {
                imm,
                direct,
                indexed,
                extended,
                width,
            } => encode_mem(
                m, imm, direct, indexed, extended, *width, operand, env, dp, line,
            ),
            Kind::Transfer(opcode) => encode_transfer(m, *opcode, operand, line),
            Kind::Stack { opcode, u_stack } => encode_stack(*opcode, *u_stack, operand, line),
        }
    } else if let Some(stripped) = m.strip_prefix('l')
        && let Some(insn) = mos6809::lookup(stripped)
        && let Kind::Branch { long, .. } = &insn.kind
    {
        // `lbra`/`lbeq`/… are the long forms of the branch with their `l`
        // dropped; no `Mem`/inherent mnemonic's tail is itself a branch, so this
        // is unambiguous. The long displacement is 16-bit.
        encode_branch(long, 2, operand, line)
    } else {
        Err(AsmError::new(line, format!("unknown instruction `{m}`")))
    }
}

fn encode_inherent(
    m: &str,
    opcode: &[u8],
    operand: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    if !operand.trim().is_empty() {
        return Err(AsmError::new(line, format!("`{m}` takes no operand")));
    }
    Ok(Operation::Encoded(
        opcode.iter().map(|b| Piece::Lit(*b)).collect(),
    ))
}

/// A short (`bytes == 1`) or long (`bytes == 2`) PC-relative branch. The engine
/// turns the target into an offset from the following instruction.
fn encode_branch(
    opcode: &[u8],
    bytes: u8,
    operand: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    let target = value(operand, line)?;
    let mut pieces: Vec<Piece> = opcode.iter().map(|b| Piece::Lit(*b)).collect();
    pieces.push(Piece::Val {
        expr: target,
        bytes,
        rel: true,
        signed: false,
    });
    Ok(Operation::Encoded(pieces))
}

/// Encode a register/memory instruction, choosing the addressing mode from the
/// operand syntax. Indexed addressing is a later increment.
#[allow(clippy::too_many_arguments)]
fn encode_mem(
    m: &str,
    imm: &[u8],
    direct: &[u8],
    indexed: &[u8],
    extended: &[u8],
    width: u8,
    operand: &str,
    env: &BTreeMap<String, i64>,
    dp: u8,
    line: usize,
) -> Result<Operation, AsmError> {
    let t = operand.trim();
    if t.is_empty() {
        return Err(AsmError::new(line, format!("`{m}` requires an operand")));
    }
    if let Some(rest) = t.strip_prefix('#') {
        if imm.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no immediate mode")));
        }
        return Ok(encoded(imm, value(rest, line)?, width));
    }
    // Indexed addressing (`,R` / `n,R` / `[...]`) — a computed postbyte plus
    // 0/1/2 extension bytes. Detected before the `<`/`>` direct/extended forces,
    // since inside an indexed operand `<`/`>` size the offset, not the address.
    if t.starts_with('[') || mos6502::top_level_rfind(t, ',').is_some() {
        if indexed.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no indexed mode")));
        }
        return encode_indexed(m, indexed, t, env, line);
    }
    if let Some(rest) = t.strip_prefix('<') {
        if direct.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no direct mode")));
        }
        // Direct mode carries the offset *within* the page, so it is the low
        // byte of the address that is emitted — `lda <$2010` is `96 10`, not a
        // one-byte operand asked to hold $2010. The force also fixes the size,
        // so a forward symbol is fine here where a bare one would not be.
        return Ok(encoded(direct, Expr::Lo(Box::new(value(rest, line)?)), 1));
    }
    if let Some(rest) = t.strip_prefix('>') {
        if extended.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no extended mode")));
        }
        return Ok(encoded(extended, value(rest, line)?, 2));
    }
    // Bare address: direct when it folds to a constant **on the direct page**
    // and a direct mode exists; otherwise extended. A forward symbol stays
    // extended, keeping the size stable across passes — lwasm's default.
    //
    // The page is `setdp`'s, zero until one says otherwise: an address is on
    // it when its high byte is the page number, so with `setdp $20` it is
    // `$2010` that reaches direct mode and `$0010` that no longer does.
    let e = value(t, line)?;
    let fits_direct = !direct.is_empty()
        && fold_const(&e, env, line)
            .is_ok_and(|v| (0..=0xFFFF).contains(&v) && ((v >> 8) & 0xFF) == i64::from(dp));
    if fits_direct {
        Ok(encoded(direct, Expr::Lo(Box::new(e)), 1))
    } else if !extended.is_empty() {
        Ok(encoded(extended, e, 2))
    } else {
        Err(AsmError::new(
            line,
            format!("`{m}` has no addressing mode for `{t}`"),
        ))
    }
}

/// Build an `Encoded` operation: the opcode literal bytes, then one unsigned
/// value of `width` bytes (an immediate, direct offset, or extended address).
fn encoded(opcode: &[u8], expr: Expr, width: u8) -> Operation {
    let mut pieces: Vec<Piece> = opcode.iter().map(|b| Piece::Lit(*b)).collect();
    pieces.push(Piece::Val {
        expr,
        bytes: width,
        rel: false,
        signed: false,
    });
    Operation::Encoded(pieces)
}

// ---------------------------------------------------------------------------
// Indexed addressing — the computed postbyte
// ---------------------------------------------------------------------------

/// An auto-increment / -decrement marker on the index register.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Auto {
    None,
    Inc1,
    Inc2,
    Dec1,
    Dec2,
}

/// The chosen width of an indexed offset: embedded 5-bit, an 8-bit extension, or
/// a 16-bit extension.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OffSize {
    Bits5,
    Byte,
    Word,
}

/// Encode a 6809 indexed operand into the postbyte (+ 0/1/2 extension bytes).
fn encode_indexed(
    m: &str,
    opcode: &[u8],
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    // Indirect operands are wrapped in `[ ]`.
    let (inner, indirect) = match operand.strip_prefix('[') {
        Some(rest) => (
            rest.strip_suffix(']')
                .ok_or_else(|| AsmError::new(line, format!("unclosed `[` in `{operand}`")))?
                .trim(),
            true,
        ),
        None => (operand, false),
    };

    let mut pieces: Vec<Piece> = opcode.iter().map(|b| Piece::Lit(*b)).collect();

    // No top-level comma: only the extended-indirect form `[addr]` is valid.
    let Some(c) = mos6502::top_level_rfind(inner, ',') else {
        if indirect {
            pieces.push(Piece::Lit(0x9F));
            pieces.push(Piece::Val {
                expr: value(inner, line)?,
                bytes: 2,
                rel: false,
                signed: false,
            });
            return Ok(Operation::Encoded(pieces));
        }
        return Err(AsmError::new(
            line,
            format!("`{m}`: not an indexed operand"),
        ));
    };
    let left = inner[..c].trim();
    let reg = inner[c + 1..].trim();

    let (rbits, auto, pcr) = parse_index_reg(reg, line)?;
    if auto != Auto::None && !left.is_empty() {
        return Err(AsmError::new(
            line,
            "auto-increment/decrement takes no offset",
        ));
    }
    if indirect && matches!(auto, Auto::Inc1 | Auto::Dec1) {
        return Err(AsmError::new(
            line,
            "no indirect form for single `,R+`/`,-R`",
        ));
    }

    // The postbyte, before the indirect bit is OR-ed in.
    let mut post: u8;
    let mut ext: Option<(Expr, u8, bool)> = None; // (expr, width, rel)
    if pcr {
        // `n,PCR`: the offset is relative to the following instruction. The size
        // can't be chosen from the value (it depends on the unknown PC), so it
        // defaults to 16-bit; `<` forces 8-bit. `>` also gives 16-bit.
        let (size, expr) = sized_offset(left, env, line, false, true)?;
        post = if size == OffSize::Byte { 0x8C } else { 0x8D };
        ext = Some((expr, if size == OffSize::Byte { 1 } else { 2 }, true));
    } else {
        let rr = rbits << 5;
        post = match auto {
            Auto::Inc1 => 0x80 | rr,
            Auto::Inc2 => 0x81 | rr,
            Auto::Dec1 => 0x82 | rr,
            Auto::Dec2 => 0x83 | rr,
            Auto::None if left.is_empty() => 0x84 | rr,
            Auto::None if left.eq_ignore_ascii_case("a") => 0x86 | rr,
            Auto::None if left.eq_ignore_ascii_case("b") => 0x85 | rr,
            Auto::None if left.eq_ignore_ascii_case("d") => 0x8B | rr,
            Auto::None => {
                // A numeric/symbolic offset: 5-bit embedded, or an 8-/16-bit
                // extension. Indirect has no 5-bit form (8-bit is the minimum).
                let (size, expr) = sized_offset(left, env, line, !indirect, false)?;
                match size {
                    OffSize::Bits5 => {
                        let v = fold_const(&expr, env, line)?; // constant by construction
                        rr | (v as u8 & 0x1F)
                    }
                    OffSize::Byte => {
                        ext = Some((expr, 1, false));
                        0x88 | rr
                    }
                    OffSize::Word => {
                        ext = Some((expr, 2, false));
                        0x89 | rr
                    }
                }
            }
        };
    }

    if indirect {
        post |= 0x10;
    }
    pieces.push(Piece::Lit(post));
    if let Some((expr, width, rel)) = ext {
        pieces.push(Piece::Val {
            expr,
            bytes: width,
            rel,
            // An 8-bit offset is a signed displacement; a 16-bit one is often a
            // base address, so it is range-checked across the full width.
            signed: width == 1,
        });
    }
    Ok(Operation::Encoded(pieces))
}

/// Parse the register part of an indexed operand: the index register (`x`/`y`/
/// `u`/`s`), with any auto inc/dec marker, or the PC for `pcr`/`pc`. Returns the
/// 2-bit register field, the auto marker, and whether it is PC-relative.
fn parse_index_reg(reg: &str, line: usize) -> Result<(u8, Auto, bool), AsmError> {
    let r = reg.trim();
    if r.eq_ignore_ascii_case("pcr") || r.eq_ignore_ascii_case("pc") {
        return Ok((0, Auto::None, true));
    }
    let (name, auto) = if let Some(s) = r.strip_prefix("--") {
        (s, Auto::Dec2)
    } else if let Some(s) = r.strip_prefix('-') {
        (s, Auto::Dec1)
    } else if let Some(s) = r.strip_suffix("++") {
        (s, Auto::Inc2)
    } else if let Some(s) = r.strip_suffix('+') {
        (s, Auto::Inc1)
    } else {
        (r, Auto::None)
    };
    let rbits = mos6809::index_reg(name.trim())
        .ok_or_else(|| AsmError::new(line, format!("unknown index register `{reg}`")))?;
    Ok((rbits, auto, false))
}

/// Choose the width of an indexed offset and parse its expression. `<` forces
/// 8-bit, `>` forces 16-bit. Otherwise a constant picks the smallest fit
/// (5-bit only when `allow5`); a forward/symbolic offset defaults to 16-bit. For
/// `pcr` the value can't choose the size (it depends on the PC), so it is 16-bit
/// unless `<`-forced.
fn sized_offset(
    raw: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
    allow5: bool,
    pcr: bool,
) -> Result<(OffSize, Expr), AsmError> {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix('>') {
        return Ok((OffSize::Word, value(rest, line)?));
    }
    if let Some(rest) = t.strip_prefix('<') {
        return Ok((OffSize::Byte, value(rest, line)?));
    }
    let e = value(t, line)?;
    if pcr {
        return Ok((OffSize::Word, e));
    }
    let size = match fold_const(&e, env, line) {
        Ok(v) if allow5 && (-16..=15).contains(&v) => OffSize::Bits5,
        Ok(v) if (-128..=127).contains(&v) => OffSize::Byte,
        _ => OffSize::Word,
    };
    Ok((size, e))
}

// ---------------------------------------------------------------------------
// Register-list operations
// ---------------------------------------------------------------------------

/// `tfr`/`exg src,dst` — the opcode then a postbyte of two 4-bit register codes.
fn encode_transfer(m: &str, opcode: u8, operand: &str, line: usize) -> Result<Operation, AsmError> {
    let parts = mos6502::split_top_level(operand, ',');
    if parts.len() != 2 {
        return Err(AsmError::new(line, format!("`{m}` needs two registers")));
    }
    let reg = |p: &str| {
        mos6809::transfer_reg(p.trim())
            .ok_or_else(|| AsmError::new(line, format!("unknown register `{}`", p.trim())))
    };
    let post = (reg(parts[0])? << 4) | reg(parts[1])?;
    Ok(Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Lit(post),
    ]))
}

/// `pshs`/`puls`/`pshu`/`pulu reg,…` — the opcode then a register bitmask.
fn encode_stack(
    opcode: u8,
    u_stack: bool,
    operand: &str,
    line: usize,
) -> Result<Operation, AsmError> {
    if operand.trim().is_empty() {
        return Err(AsmError::new(line, "push/pull needs at least one register"));
    }
    let mut mask = 0u8;
    for p in mos6502::split_top_level(operand, ',') {
        mask |= mos6809::stack_mask(p.trim(), u_stack)
            .ok_or_else(|| AsmError::new(line, format!("unknown register `{}`", p.trim())))?;
    }
    Ok(Operation::Encoded(vec![
        Piece::Lit(opcode),
        Piece::Lit(mask),
    ]))
}

/// How a string directive marks its end.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StringEnd {
    /// `fcc` — the characters and nothing more.
    Bare,
    /// `fcn`/`fcz` — a trailing NUL.
    Nul,
    /// `fcs` — the high bit of the last byte set. An empty string has no last
    /// byte and so emits nothing at all, where `fcn ""` still emits its NUL.
    HighBit,
}

/// `fcc`/`fcn`/`fcz`/`fcs` — a string with a self-chosen delimiter (`"text"`,
/// `/text/`, …): one byte per character, up to the closing delimiter, then
/// whatever `end` marks the end with.
fn parse_fcc(operand: &str, line: usize, end: StringEnd) -> Result<Operation, AsmError> {
    let t = operand.trim();
    let delim = t
        .chars()
        .next()
        .ok_or_else(|| AsmError::new(line, "`fcc` needs a string"))?;
    let rest = &t[delim.len_utf8()..];
    let close = rest
        .find(delim)
        .ok_or_else(|| AsmError::new(line, "unterminated `fcc` string"))?;
    let mut bytes: Vec<u8> = rest[..close].bytes().collect();
    match end {
        StringEnd::Bare => {}
        StringEnd::Nul => bytes.push(0),
        StringEnd::HighBit => {
            if let Some(last) = bytes.last_mut() {
                *last |= 0x80;
            }
        }
    }
    Ok(Operation::Bytes(
        bytes.into_iter().map(|b| Expr::Num(i64::from(b))).collect(),
    ))
}

/// `fdbs value[,value…]` — "form double byte, swapped": each value as a 16-bit
/// word with the bytes the other way round from `fdb`.
///
/// It is not a byte-swap of `fdb`, and the difference is lwasm's, not a
/// simplification here. `fdb` takes the two halves of the two's-complement
/// value, so `fdb -255` is `ff 01`; `fdbs` takes the low byte the same way but
/// derives the high byte by *dividing* by 256, and C division truncates toward
/// zero, so `fdbs -255` is `01 00` rather than the `01 ff` a swap would give.
/// The two agree everywhere the quotient is exact — `fdbs -256` is `00 ff` —
/// which is why the gap only shows on negatives that are not whole pages.
/// `Expr::Hi` shifts, so it cannot be used for the high byte here.
fn parse_fdbs(operand: &str, line: usize) -> Result<Operation, AsmError> {
    let mut bytes = Vec::new();
    for expr in list(operand, line)? {
        bytes.push(Expr::Lo(Box::new(expr.clone())));
        bytes.push(Expr::Bin(
            crate::engine::BinOp::Div,
            Box::new(expr),
            Box::new(Expr::Num(256)),
        ));
    }
    Ok(Operation::Bytes(bytes))
}

/// Parse a comma-separated list of value expressions (for `fcb`/`fdb`).
fn list(operand: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if operand.trim().is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    mos6502::split_top_level(operand, ',')
        .iter()
        .map(|p| value(p, line))
        .collect()
}

/// Parse one 6809 value expression. `$hex`/`%bin`/decimal numbers, symbols, `*`
/// for the location counter, and the bitwise/shift operators — reusing the
/// shared 6502 expression core. The `<`/`>` direct/extended forces are stripped
/// by the caller before this, so they never reach the byte-prefix paths.
fn value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    mos6502::parse_expr(
        raw,
        line,
        mos6502::parse_number,
        mos6502::ExprOpts {
            compare: mos6502::Compare {
                eq: false,
                eq_eq: false,
                ne_angle: true,
                ne_bang: false,
                relational: true,
                ordered_eq: false,
                minus_one: false,
            },
            function: None,
            bang_is_or: false,
            prec: BytePrec::Tight,
            byte_prefix: false,
            caret: mos6502::Caret::Xor,
            at_is_pc: false,
        },
    )
}

// ---------------------------------------------------------------------------
// Macros (#93)
//
// The mechanics live in [`crate::dialects::macros`]; this is lwasm's grammar,
// measured against lwasm 4.19. It is the first dialect here whose parameters
// have no names at all:
//
//   * a definition is `name macro` — **only** that way round. lwasm rejects
//     ` macro name` with `Missing macro name`, where vasm takes both.
//   * the body refers to `\1`, `\2`, so a macro's arity is decided at the call
//     site, not the definition. Extra arguments are dropped; a missing one
//     substitutes empty, and the emptied operand complains (`Bad operand`).
//   * a symbol ending in `?` or `@` is local to the expansion, and invisible
//     outside it (`Undefined symbol spin?`). It is a *suffix*, not a
//     declaration and not a prefix — a third spelling of locals across four
//     dialects.
//
// `\@`, vasm's per-expansion counter, is not lwasm's: `spin\@` is
// `Bad symbol (spin\@)`.
// ---------------------------------------------------------------------------

/// lwasm's macro grammar.
struct LwasmMacros;

impl macros::MacroSyntax for LwasmMacros {
    /// `name macro` — name first, and only name first. `macr` is lwasm's
    /// second spelling of the same word.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        let text = macros::without_comment(line);
        // A definition names its macro in label position, so an indented
        // `macro` is not one.
        if text.starts_with(char::is_whitespace) {
            return None;
        }
        let (name, rest) = text.trim_end().split_once(char::is_whitespace)?;
        let word = rest.trim();
        (word.eq_ignore_ascii_case("macro") || word.eq_ignore_ascii_case("macr"))
            .then(|| (name.trim_end_matches(':').to_string(), Vec::new()))
    }

    fn is_end(&self, line: &str) -> bool {
        macros::without_comment(line)
            .trim()
            .eq_ignore_ascii_case("endm")
    }

    fn end_keyword(&self) -> &'static str {
        "endm"
    }

    /// `\1`, `\2`, … for as many arguments as the call site passed.
    fn argument_names(&self, _declared: &[String], count: usize) -> Vec<String> {
        (1..=count).map(|n| format!("\\{n}")).collect()
    }

    /// A `?` or `@` suffix marks a symbol local to the expansion, and a
    /// backslash opens a positional parameter — neither is a symbol character
    /// anywhere else, and both must be inside the token for substitution to
    /// see them at all.
    fn is_symbol_char(&self, c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'?' || c == b'@' || c == b'\\'
    }

    /// The marker goes with the rename. lwasm strips a local's `?`/`@` itself;
    /// our parser would read it as an unexpected character mid-expression.
    fn rename_local(&self, name: &str, n: usize) -> String {
        format!("{}__{n}", name.trim_end_matches(['?', '@']))
    }

    /// The suffixed symbols the body mentions. Unlike the declared-locals
    /// dialects this scans *uses* as well as definitions, because the suffix
    /// is the whole declaration.
    fn locals(&self, body: &[String]) -> Vec<String> {
        let mut names = Vec::new();
        for line in body {
            let text = macros::without_comment(line);
            let bytes = text.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if !self.is_symbol_char(bytes[i]) || (i > 0 && self.is_symbol_char(bytes[i - 1])) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < bytes.len() && self.is_symbol_char(bytes[i]) {
                    i += 1;
                }
                let token = &text[start..i];
                if token.len() > 1
                    && token.ends_with(['?', '@'])
                    && !token.starts_with(['?', '@'])
                    && !names.iter().any(|n| n == token)
                {
                    names.push(token.to_string());
                }
            }
        }
        names
    }

    /// lwasm checks nothing: extras are dropped, and a missing argument
    /// substitutes empty so the complaint arrives from the operand it emptied.
    fn fit_arguments(
        &self,
        _name: &str,
        _params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        Ok(args)
    }
}

/// Expand lwasm's macros, unless this parse is the formatter's.
fn expand_lwasm(source: &str, mode: macros::Expand) -> Result<macros::Expansion, AsmError> {
    macros::expansion(mode, source, |s| {
        macros::expand(&LwasmMacros, s).map(|e| Some((e.text, e.origins)))
    })
}

// ---------------------------------------------------------------------------
// Conditional evaluation — lwasm's `CondEval` (the adoption recipe's steps 1
// and 3, `decisions/conditional-assembly-framework.md`).
//
// Why this dialect needs a real evaluator rather than a fold in the walk: an
// `equ` decides lwasm's **addressing mode**, and the mode decides the
// instruction's *size*. `sym equ $10` gives `96 10` (direct, two bytes) and
// `sym equ $1234` gives `b6 12 34` (extended, three). Real lwasm refuses
// `lda sym` outright when that `equ` sits in an untaken branch, so a binding
// made while parsing both branches would silently choose direct where the
// reference errors. Each live line therefore re-parses here, against the
// environment as it actually stands — which is ACME's model unchanged.
// ---------------------------------------------------------------------------

/// lwasm's conditional evaluator: the `equ` environment, threaded through the
/// walk so a later direct/extended choice sees only what a taken branch bound.
struct LwasmEval {
    env: BTreeMap<String, i64>,
    /// What the live directives so far left for the lines after them. This is
    /// the copy that decides the emitted bytes.
    state: ParseState,
}

impl crate::ast::CondEval for LwasmEval {
    /// Fold one conditional head. Every numeric form compares against zero;
    /// `ifdef`/`ifndef` test the environment for a name.
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        let (word, args) = split_first_word(head.trim());
        let word = word.to_ascii_lowercase();
        let args = args.trim();
        if word == "ifpragma" || word == "ifopt" {
            let name = args
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            // Both spellings refuse a name they do not know, where `opt`
            // itself passes over one — measured: `opt zzz` assembles and
            // `ifopt zzz` does not.
            let (index, want_on) = pragma_named(&name).ok_or_else(|| {
                AsmError::new(line, format!("unrecognized pragma string `{name}`"))
            })?;
            return Ok(pragma_is_on(&self.state.pragmas, index) == want_on);
        }
        if word == "ifdef" || word == "ifndef" {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            let defined = self.env.contains_key(name);
            return Ok(if word == "ifdef" { defined } else { !defined });
        }
        if args.is_empty() {
            return Err(AsmError::new(line, format!("`{word}` needs a condition")));
        }
        let value = fold_const(&value(args, line)?, &self.env, line).map_err(|_| {
            AsmError::new(
                line,
                format!(
                    "`{args}` must be a constant here — lwasm folds a condition against the \
                     `equ` values above it"
                ),
            )
        })?;
        Ok(match word.as_str() {
            "if" | "ifne" => value != 0,
            "ifeq" => value == 0,
            "ifgt" => value > 0,
            "ifge" => value >= 0,
            "iflt" => value < 0,
            "ifle" => value <= 0,
            _ => {
                return Err(AsmError::new(
                    line,
                    format!("internal error: `{head}` is not a conditional head"),
                ));
            }
        })
    }

    /// Lower one **live** line, re-parsing its operation against the current
    /// environment so the direct/extended choice sees the live bindings.
    fn lower(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let label = node.label.as_ref().map(|s| s.qualified.clone());
        if let Some(effect) = struct_line(
            label.as_deref(),
            &node.source,
            &mut self.state,
            &self.env,
            line,
        )? {
            let at = |op, label| Statement {
                line,
                file: node.span.file,
                label,
                op: Some(op),
                operand_span: None,
                xor_mask: 0,
            };
            match effect {
                StructEffect::Nothing => {}
                // The offsets are constants, so they are bound as constants —
                // `pt.x` reads as one with no instance anywhere.
                StructEffect::Closed { name, def } => {
                    for (member, offset) in &def.members {
                        let sym = format!("{name}.{member}");
                        self.env.insert(sym.clone(), *offset);
                        out.push(at(Operation::Equ(Expr::Num(*offset)), Some(sym)));
                    }
                }
                // An instance is room with names on it: the label lands where
                // the room starts, and each member's name lands at its offset
                // into it.
                StructEffect::Instance { def } => {
                    let base = label.clone().expect("an instance has a label");
                    out.push(at(
                        Operation::Bytes(vec![Expr::Num(0); def.size as usize]),
                        Some(base.clone()),
                    ));
                    for (member, offset) in &def.members {
                        out.push(at(
                            Operation::Equ(Expr::Bin(
                                crate::engine::BinOp::Add,
                                Box::new(Expr::Sym(base.clone())),
                                Box::new(Expr::Num(*offset)),
                            )),
                            Some(format!("{base}.{member}")),
                        ));
                    }
                }
            }
            return Ok(());
        }
        // A walk-resolved payload keeps what the walk built: an `includebin`'s
        // bytes cannot be rebuilt here, because resolving one needs the loader
        // the walk had and this does not. Everything else re-parses.
        let op = match &node.item {
            Some(crate::ast::Item::Binary(payload)) => Some(Operation::Binary(payload.clone())),
            Some(crate::ast::Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `include \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(crate::ast::Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `includebin \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            _ if node.source.is_empty() => None,
            _ => parse_op(&node.source, &self.env, &mut self.state, line)?,
        };
        if let (Some(sym), Some(Operation::Equ(e) | Operation::Set(e))) = (node.label.as_ref(), &op)
            && let Ok(v) = fold_const(e, &self.env, line)
        {
            self.env.insert(sym.qualified.clone(), v);
        }
        out.push(Statement {
            line,
            file: node.span.file,
            label: node.label.as_ref().map(|s| s.qualified.clone()),
            op,
            operand_span: node.operand_span.clone(),
            xor_mask: 0,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_lwasm as asm;

    #[test]
    fn inherent_and_immediate() {
        assert_eq!(asm("        nop\n").expect("nop").bytes, vec![0x12]);
        assert_eq!(asm("        rts\n").expect("rts").bytes, vec![0x39]);
        assert_eq!(
            asm("        lda #$42\n").expect("imm").bytes,
            vec![0x86, 0x42]
        );
        // 16-bit immediate.
        assert_eq!(
            asm("        ldx #$1234\n").expect("ldx").bytes,
            vec![0x8E, 0x12, 0x34]
        );
    }

    /// `align` needs its boundary — bare `align` is `Bad operand` in lwasm,
    /// and a boundary that cannot pad to anything is not a boundary.
    /// `error <text>` takes the rest of the line verbatim and stops, and stays
    /// silent inside an untaken conditional.
    #[test]
    fn error_stops_the_assembly_unless_the_branch_is_untaken() {
        let err = asm(" fcb 1\n error stop here\n").expect_err("aborts");
        assert!(err.to_string().contains("stop here"), "got `{err}`");
        assert_eq!(
            asm(" ifne 0\n error never\n endc\n fcb 1\n")
                .expect("untaken")
                .bytes,
            vec![1]
        );
    }

    /// `warning` and `msg` are the same directive at warning severity:
    /// assembly continues, the bytes are unaffected, and an untaken branch
    /// says nothing at all.
    #[test]
    fn warning_and_msg_say_something_and_carry_on() {
        for spelling in ["warning", "msg"] {
            let out = asm(&format!(" {spelling} careful\n fcb 1\n")).expect(spelling);
            assert_eq!(out.bytes, vec![1], "{spelling}");
            assert_eq!(out.warnings.len(), 1, "{spelling}");
            assert!(out.warnings[0].message.contains("careful"), "{spelling}");
            let quiet =
                asm(&format!(" ifne 0\n {spelling} never\n endc\n fcb 1\n")).expect("untaken");
            assert_eq!(quiet.bytes, vec![1], "{spelling}");
            assert!(quiet.warnings.is_empty(), "{spelling} inside a dead branch");
        }
    }

    /// The listing words reach the listing and nothing else. lwasm takes any
    /// operand for them, or none, and emits nothing either way — so a source
    /// that titles and paginates itself assembles to the same bytes here.
    #[test]
    fn the_listing_words_emit_nothing_whatever_they_are_given() {
        for spelling in ["nam", "ttl", "pag", "page", "spc"] {
            for operand in ["", " a title here", " 3,4 $x"] {
                let out = asm(&format!(" fcb 1\n {spelling}{operand}\n fcb 2\n"))
                    .unwrap_or_else(|e| panic!("{spelling}{operand}: {e}"));
                assert_eq!(out.bytes, vec![1, 2], "{spelling}{operand}");
            }
        }
    }

    /// `phase` moves the address without moving the output, and `dephase`
    /// puts it back. A label between the two reads as the claimed address; the
    /// bytes land where they were always going to.
    #[test]
    fn phase_claims_an_address_without_moving_the_output() {
        assert_eq!(
            asm(" org $1000\n phase $2000\nl fcb 1\n fdb l\n dephase\n fdb *\n")
                .expect("phase")
                .bytes,
            vec![0x01, 0x20, 0x00, 0x10, 0x03]
        );
        // The claimed address counts on with the output, and a `phase` left
        // open at the end of the source is lwasm's business, not an error.
        assert_eq!(
            asm(" org $1000\n phase $5000\n fcb 1\n fdb *\n")
                .expect("open")
                .bytes,
            vec![0x01, 0x50, 0x01]
        );
    }

    /// A branch inside a relocated block measures claimed-against-claimed. The
    /// 6809 computes its own operands, and the computed path took the *real*
    /// address as the branch base while the target label was already claimed —
    /// so `bra` over one byte read as a jump four kilobytes away.
    #[test]
    fn a_branch_inside_a_phase_measures_from_the_claimed_address() {
        assert_eq!(
            asm(" org $1000\n phase $2000\n bra t\nt fcb 9\n dephase\n")
                .expect("bra")
                .bytes,
            vec![0x20, 0x00, 9]
        );
        assert_eq!(
            asm(" org $1000\n phase $2000\n lbra t\nt fcb 9\n dephase\n")
                .expect("lbra")
                .bytes,
            vec![0x16, 0x00, 0x00, 9]
        );
        // …and `,pcr` indexing, which is the same measurement by another name.
        assert_eq!(
            asm(" org $1000\n phase $2000\n leax t,pcr\nt fcb 9\n dephase\n")
                .expect("pcr")
                .bytes,
            vec![0x30, 0x8D, 0x00, 0x00, 9]
        );
    }

    /// lwasm's two refusals, which the engine's own counter stack would
    /// otherwise take: it does not nest `phase`, and `dephase` must close one.
    #[test]
    fn lwasm_does_not_nest_a_phase() {
        assert!(
            asm(" org $1000\n phase $2000\n phase $3000\n dephase\n dephase\n").is_err(),
            "nested `phase`"
        );
        assert!(asm(" org $1000\n dephase\n").is_err(), "`dephase` alone");
        assert!(asm(" org $1000\n phase\n").is_err(), "bare `phase`");
        // Two in sequence are not nested, and are fine.
        assert!(
            asm(" org $1000\n phase $2000\n dephase\n phase $3000\n dephase\n").is_ok(),
            "sequential phases"
        );
    }

    /// `set` is `equ` for a name that moves — and the direction of a
    /// reference decides which value it reads. A use *below* the binding gets
    /// the one bound there; a use *above* it gets the value the name is left
    /// with at the end of the file, because that is what pass one leaves for
    /// pass two to start from. lwtools 4.25 was measured doing both.
    #[test]
    fn set_binds_a_name_that_moves() {
        assert_eq!(
            asm(" org $1000\nn set 1\n fcb n\nn set 2\n fcb n\nn set n+5\n fcb n\n")
                .expect("set")
                .bytes,
            vec![1, 2, 7]
        );
        // The leading `fcb n` reads 2 — the last binding — and the two after
        // each read the binding above them.
        assert_eq!(
            asm(" org $1000\n fcb n\nn set 1\n fcb n\nn set 2\n fcb n\n")
                .expect("forward")
                .bytes,
            vec![2, 1, 2]
        );
    }

    /// A name is redefinable or it is fixed, never both — lwasm answers
    /// "Multiply defined symbol" whichever kind came first.
    #[test]
    fn a_name_is_redefinable_or_fixed_but_not_both() {
        assert!(
            asm(" org $1000\nn set 1\nn equ 2\n fcb n\n").is_err(),
            "set then equ"
        );
        assert!(
            asm(" org $1000\nn equ 1\nn set 2\n fcb n\n").is_err(),
            "equ then set"
        );
        assert!(
            asm(" org $1000\nn set 1\nn fcb 2\n").is_err(),
            "set then label"
        );
        assert!(
            asm(" org $1000\nn fcb 2\nn set 1\n").is_err(),
            "label then set"
        );
        assert!(asm(" org $1000\n set 1\n").is_err(), "`set` with no label");
        assert!(
            asm(" org $1000\nn set\n fcb 1\n").is_err(),
            "`set` with no value"
        );
    }

    /// A `set` value reaches the parse-time direct/extended choice the same
    /// way an `equ` does, and the last binding above the use is the one that
    /// decides it.
    #[test]
    fn a_set_value_chooses_direct_or_extended() {
        assert_eq!(
            asm(" org $1000\nn set $50\n lda n\n")
                .expect("direct")
                .bytes,
            vec![0x96, 0x50]
        );
        assert_eq!(
            asm(" org $1000\nn set $50\nn set $2050\n lda n\n")
                .expect("extended")
                .bytes,
            vec![0xB6, 0x20, 0x50]
        );
    }

    /// Plain `if` is `ifne` under a shorter name: true when the condition is
    /// anything but zero, negatives included.
    #[test]
    fn plain_if_tests_against_zero() {
        assert_eq!(
            asm(" org $1000\n if 1\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("true")
                .bytes,
            vec![1]
        );
        assert_eq!(
            asm(" org $1000\n if 0\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("false")
                .bytes,
            vec![2]
        );
        assert_eq!(
            asm(" org $1000\n if -1\n fcb 1\n endc\n")
                .expect("neg")
                .bytes,
            vec![1]
        );
        // It nests, and closes with either closer.
        assert_eq!(
            asm(" org $1000\n if 1\n if 0\n fcb 1\n endc\n fcb 2\n endif\n")
                .expect("nested")
                .bytes,
            vec![2]
        );
        assert!(
            asm(" org $1000\n if\n fcb 1\n endc\n").is_err(),
            "bare `if`"
        );
    }

    /// The `<` force means direct mode, and direct mode carries the offset
    /// within the page — so what is emitted is the *low byte* of the address.
    /// It also fixes the size, which is why a forward symbol is allowed here
    /// where a bare one would stay extended.
    #[test]
    fn the_direct_force_emits_the_offset_within_the_page() {
        assert_eq!(
            asm(" lda <$2010\n").expect("high page").bytes,
            vec![0x96, 0x10]
        );
        assert_eq!(
            asm(" lda <$10\n").expect("page zero").bytes,
            vec![0x96, 0x10]
        );
        assert_eq!(
            asm(" lda <l\nl equ $2010\n").expect("forward").bytes,
            vec![0x96, 0x10]
        );
    }

    /// `setdp` says which page the DP register will hold at run time, so an
    /// address *on that page* reaches direct mode and one off it no longer
    /// does — including page zero, which stops being special the moment a
    /// `setdp` names another.
    #[test]
    fn setdp_moves_which_page_is_direct() {
        assert_eq!(
            asm(" setdp $20\n lda $2010\n lda $2110\n lda $10\n")
                .expect("setdp")
                .bytes,
            vec![0x96, 0x10, 0xB6, 0x21, 0x10, 0xB6, 0x00, 0x10]
        );
        // The last one above the line wins, and it reaches every addressing
        // mode that has a direct form — not just the accumulator loads.
        assert_eq!(
            asm(" setdp $20\n setdp $30\n lda $3010\n lda $2010\n")
                .expect("twice")
                .bytes,
            vec![0x96, 0x10, 0xB6, 0x20, 0x10]
        );
        assert_eq!(
            asm(" setdp $20\n jmp $2010\n cmpx $2010\n")
                .expect("others")
                .bytes,
            vec![0x0E, 0x10, 0x9C, 0x10]
        );
        // Indexed and immediate operands never had a page to be on.
        assert_eq!(
            asm(" setdp $20\n lda $2010,x\n").expect("indexed").bytes,
            vec![0xA6, 0x89, 0x20, 0x10]
        );
    }

    /// The operand is a page number taken modulo 256 — `setdp $2000` is page
    /// zero, not page $20 — it must fold on pass one, and one inside a branch
    /// that is not taken never happens.
    #[test]
    fn setdp_takes_a_constant_page_number() {
        assert_eq!(
            asm(" setdp $2000\n lda $0010\n lda $2010\n")
                .expect("masked")
                .bytes,
            vec![0x96, 0x10, 0xB6, 0x20, 0x10]
        );
        assert_eq!(
            asm(" setdp -1\n lda $ff10\n").expect("negative").bytes,
            vec![0x96, 0x10]
        );
        assert!(
            asm(" setdp later\nlater equ $20\n lda $2010\n").is_err(),
            "a forward `setdp` cannot fold on pass one"
        );
        assert!(asm(" setdp\n fcb 1\n").is_err(), "bare `setdp`");
        assert_eq!(
            asm(" ifne 0\n setdp $20\n endc\n lda $2010\n")
                .expect("untaken")
                .bytes,
            vec![0xB6, 0x20, 0x10]
        );
    }

    /// lwasm's flat output is contiguous, so a second `org` names an address
    /// and leaves the bytes where they were: `org $1000 / fcb 1 / org $2000 /
    /// fcb 2` is two bytes and `*` then reads $2001. An `org` below the
    /// current address is ordinary here, where a padding dialect refuses it.
    #[test]
    fn a_second_org_moves_the_address_and_not_the_output() {
        assert_eq!(
            asm(" org $1000\n fcb 1\n org $2000\n fcb 2\n fdb *\n")
                .expect("forward")
                .bytes,
            vec![1, 2, 0x20, 0x01]
        );
        assert_eq!(
            asm(" org $2000\n fcb 1\n org $1000\n fcb 2\n fdb *\n")
                .expect("backwards")
                .bytes,
            vec![1, 2, 0x10, 0x01]
        );
        // Labels either side read their own address, and a branch across the
        // move measures between them.
        assert_eq!(
            asm(" org $1000\nl fcb 1\n org $2000\nm fcb 2\n fdb l\n fdb m\n")
                .expect("labels")
                .bytes,
            vec![1, 2, 0x10, 0x00, 0x20, 0x00]
        );
    }

    /// `reorg` goes back to the `org` before the current one — not one step
    /// further each time. A second in a row repeats, because it moves the
    /// current `org` back without moving the previous one.
    #[test]
    fn reorg_goes_back_to_the_org_before_this_one() {
        assert_eq!(
            asm(
                " org $1000\n org $2000\n org $3000\n fdb *\n reorg\n fdb *\n \
                 reorg\n fdb *\n"
            )
            .expect("three")
            .bytes,
            vec![0x30, 0x00, 0x20, 0x00, 0x20, 0x00]
        );
        assert_eq!(
            asm(" org $1000\n fcb 1\n org $2000\n fcb 2\n reorg\n fdb *\n fcb 3\n")
                .expect("then more")
                .bytes,
            vec![1, 2, 0x10, 0x00, 3]
        );
        // One `org` leaves nothing to go back to, and it takes no operand.
        assert!(asm(" org $1000\n reorg\n").is_err(), "one `org` only");
        assert!(asm(" reorg\n").is_err(), "no `org` at all");
        assert!(
            asm(" org $1000\n org $2000\n reorg $3000\n").is_err(),
            "an operand"
        );
    }

    /// The one corner `org` and `reorg` do not share with lwasm: inside an
    /// open `phase` it keeps the phased address counting from where it was,
    /// where this engine derives it from the real counter. Refused by name
    /// rather than answered wrongly.
    #[test]
    fn moving_the_counter_inside_a_phase_is_refused_as_our_gap() {
        for word in ["org $2000", "reorg"] {
            let err = asm(&format!(
                " org $1000\n org $2000\n phase $5000\n {word}\n dephase\n"
            ))
            .expect_err(word)
            .to_string();
            assert!(err.contains("the gap is ours"), "{word}: {err}");
        }
    }

    /// lwasm's section words are lwasm's own refusal for the output we make,
    /// not a gap here: "Cannot use sections unless using the object target",
    /// bare or with arguments, every time.
    #[test]
    fn the_section_words_are_the_references_refusal() {
        for spelling in ["section", "sect", "endsection", "endsect"] {
            for operand in ["", " name,bss"] {
                let err = asm(&format!(" {spelling}{operand}\n fcb 1\n"))
                    .expect_err(spelling)
                    .to_string();
                assert!(err.contains("object target"), "{spelling}: {err}");
                assert!(!err.contains("does not implement"), "{spelling}: {err}");
            }
        }
    }

    /// A word we turn away is turned away when it is *reached*, not when it is
    /// read. lwasm does not parse a branch it is not taking at all, so a
    /// `section` or an `export` guarded behind `if 0` has to assemble here —
    /// and the same word on a live line still has to be refused.
    #[test]
    fn a_refused_word_inside_a_dead_branch_is_never_reached() {
        for spelling in ["section", "export", "import", "os9", "struct", "endm"] {
            assert_eq!(
                asm(&format!(" ifne 0\n {spelling}\n endc\n fcb 1\n"))
                    .unwrap_or_else(|e| panic!("{spelling} behind `if 0`: {e}"))
                    .bytes,
                vec![1],
                "{spelling}"
            );
            assert!(
                asm(&format!(" {spelling}\n fcb 1\n")).is_err(),
                "{spelling} live"
            );
        }
    }

    /// `macr` is lwasm's second spelling of `macro`, and both are read by the
    /// walk before the directive table sees them — so reaching the table means
    /// the line was not part of a definition, and each of those has an answer
    /// too.
    #[test]
    fn macr_defines_a_macro_and_a_stray_one_says_why_not() {
        for spelling in ["macro", "macr", "MACRO"] {
            assert_eq!(
                asm(&format!(
                    "twice {spelling}\n fcb \\1\n endm\n org $1000\n twice 7\n"
                ))
                .unwrap_or_else(|e| panic!("{spelling}: {e}"))
                .bytes,
                vec![7],
                "{spelling}"
            );
            // Indented, it names nothing, and lwasm says so.
            let err = asm(&format!(" {spelling}\n fcb 1\n"))
                .expect_err(spelling)
                .to_string();
            assert!(err.contains("macro name"), "{spelling}: {err}");
        }
        let err = asm(" endm\n fcb 1\n").expect_err("stray endm").to_string();
        assert!(err.contains("without a `macro`"), "{err}");
    }

    /// A struct describes a layout without laying anything out: the
    /// definition emits nothing, the members name offsets into the type, and
    /// `pt.x` reads as a constant with no instance in sight. The type name
    /// itself is not a symbol — lwasm answers "Undefined symbol pt" for one.
    #[test]
    fn a_struct_names_offsets_and_emits_nothing() {
        assert_eq!(
            asm(" org $1000\npt struct\nx rmb 1\ny rmb 2\n endstruct\n fdb pt.x\n fdb pt.y\n")
                .expect("offsets")
                .bytes,
            vec![0, 0, 0, 1]
        );
        assert!(
            asm(" org $1000\npt struct\nx rmb 1\n endstruct\n fcb pt\n").is_err(),
            "the type name is not a symbol"
        );
        assert!(
            asm(" org $1000\npt struct\nx rmb 1\n endstruct\n fdb pt.zz\n").is_err(),
            "an unknown member"
        );
    }

    /// An instance is room with names on it. The label lands where the room
    /// starts and each member's name at its offset into it, so two instances
    /// sit one after the other and read their own addresses.
    #[test]
    fn an_instance_reserves_the_room_and_names_it() {
        assert_eq!(
            asm(
                " org $1000\npt struct\nx rmb 1\ny rmb 2\n endstruct\nv pt\nw pt\n \
                 fdb v.y\n fdb w.x\n"
            )
            .expect("two")
            .bytes,
            vec![0, 0, 0, 0, 0, 0, 0x10, 0x01, 0x10, 0x03]
        );
        // A member may be another struct, and the sizes add up through it.
        assert_eq!(
            asm(
                " org $1000\npt struct\nx rmb 1\n endstruct\nq struct\na pt\nb rmb 1\n \
                 endstruct\nv q\n fdb v.a\n fdb v.b\n"
            )
            .expect("nested")
            .bytes,
            vec![0, 0, 0x10, 0x00, 0x10, 0x01]
        );
        // `ends` is the second spelling of the close, and an empty struct
        // reserves nothing at all.
        assert_eq!(
            asm(" org $1000\npt struct\nx rmb 1\n ends\n fdb pt.x\n")
                .expect("ends")
                .bytes,
            vec![0, 0]
        );
        assert_eq!(
            asm(" org $1000\npt struct\n endstruct\nv pt\n fdb *\n")
                .expect("empty")
                .bytes,
            vec![0x10, 0x00]
        );
    }

    /// A struct behind a branch that is not taken is never read, the way
    /// every other refused word is not — and one in a branch that *is* taken
    /// works as it would anywhere.
    #[test]
    fn a_struct_inside_a_dead_branch_is_never_read() {
        for word in ["struct", "endstruct", "ends"] {
            assert_eq!(
                asm(&format!(" ifne 0\n {word}\n endc\n fcb 1\n"))
                    .unwrap_or_else(|e| panic!("{word} behind `if 0`: {e}"))
                    .bytes,
                vec![1],
                "{word}"
            );
        }
        assert_eq!(
            asm(
                " ifne 1\npt struct\nx rmb 1\n endstruct\n endc\n org $1000\nv pt\n \
                 fdb v.x\n"
            )
            .expect("taken")
            .bytes,
            vec![0, 0x10, 0x00]
        );
    }

    /// The four ways to get one wrong, each of which lwasm names.
    #[test]
    fn a_struct_that_is_not_one_says_so() {
        assert!(
            asm(" org $1000\n struct\nx rmb 1\n endstruct\n").is_err(),
            "a definition with no name has no effect"
        );
        assert!(
            asm(" org $1000\n endstruct\n fcb 1\n").is_err(),
            "a close with nothing open"
        );
        assert!(
            asm(
                " org $1000\npt struct\nx rmb 1\n endstruct\npt struct\ny rmb 1\n \
                 endstruct\n"
            )
            .is_err(),
            "the same name twice"
        );
        assert!(
            asm(" org $1000\npt struct\nx fcb 9\n endstruct\n").is_err(),
            "a struct reserves room; it does not hold data"
        );
    }

    /// `pragma` and `opt` reach the same switches, and `ifpragma`/`ifopt` ask
    /// whether one is set. A pragma untouched by the source answers with what
    /// lwasm starts it at, which is not "off" for all of them.
    #[test]
    fn a_pragma_is_set_and_asked_after() {
        // `6309` starts on, `cd` starts off — both measured against lwtools.
        assert_eq!(
            asm(" ifpragma 6309\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("6309")
                .bytes,
            vec![1]
        );
        assert_eq!(
            asm(" ifopt cd\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("cd")
                .bytes,
            vec![2]
        );
        // Setting one shows through either spelling of the question.
        for ask in ["ifpragma", "ifopt"] {
            assert_eq!(
                asm(&format!(
                    " opt cd\n {ask} cd\n fcb 1\n else\n fcb 2\n endc\n"
                ))
                .expect(ask)
                .bytes,
                vec![1],
                "{ask}"
            );
        }
        // The off-spelling is the same switch seen the other way round, and
        // `6809` is `6309`'s.
        assert_eq!(
            asm(" pragma cd\n ifpragma nocd\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("negative")
                .bytes,
            vec![2]
        );
        assert_eq!(
            asm(" pragma 6809\n ifpragma 6309\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("6809")
                .bytes,
            vec![2]
        );
    }

    /// Commas separate the names and the first space ends them — everything
    /// after it is the comment field. So `list nolist` sets only `list`, where
    /// `list,nolist` sets both and `list, nolist` leaves an empty name behind
    /// the comma and is refused.
    #[test]
    fn a_pragma_takes_a_comma_list_and_then_a_comment() {
        assert_eq!(
            asm(" pragma list nolist\n ifpragma list\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("space")
                .bytes,
            vec![1]
        );
        assert_eq!(
            asm(" pragma list,nolist\n ifpragma list\n fcb 1\n else\n fcb 2\n endc\n")
                .expect("comma")
                .bytes,
            vec![2]
        );
        assert!(
            asm(" pragma list, nolist\n fcb 1\n").is_err(),
            "comma then space"
        );
    }

    /// The two spellings part company over a name neither knows: `pragma`
    /// refuses it, `opt` passes over it in silence. A bare `opt` is nothing to
    /// do; a bare `pragma` is a name it does not know.
    #[test]
    fn opt_forgives_a_name_pragma_refuses() {
        assert_eq!(asm(" opt zzz\n fcb 1\n").expect("opt").bytes, vec![1]);
        assert!(
            asm(" pragma zzz\n fcb 1\n").is_err(),
            "`pragma` with an unknown name"
        );
        assert_eq!(asm(" opt\n fcb 1\n").expect("bare opt").bytes, vec![1]);
        assert!(asm(" pragma\n fcb 1\n").is_err(), "bare `pragma`");
        // Both refuse a name they do not know when *asking* about it.
        assert!(
            asm(" ifopt zzz\n fcb 1\n endc\n").is_err(),
            "`ifopt` with an unknown name"
        );
    }

    /// Eight of the forty-nine spellings ask for something this dialect does
    /// not do. Each is refused where it is set, by name, rather than accepted
    /// and quietly ignored — and, like every refusal here, only when the line
    /// is reached.
    #[test]
    fn a_pragma_asking_for_what_we_cannot_do_says_so() {
        for spelling in [
            "autobranchlength",
            "cescapes",
            "condundefzero",
            "noforwardrefmax",
            "operandsizewarning",
            "pcaspcr",
            "nosymbolcase",
            "symbolnocase",
        ] {
            let err = asm(&format!(" pragma {spelling}\n fcb 1\n"))
                .expect_err(spelling)
                .to_string();
            assert!(err.contains("the gap is ours"), "{spelling}: {err}");
            assert_eq!(
                asm(&format!(" ifne 0\n pragma {spelling}\n endc\n fcb 1\n"))
                    .unwrap_or_else(|e| panic!("{spelling} behind `if 0`: {e}"))
                    .bytes,
                vec![1],
                "{spelling}"
            );
        }
        // Asking for the direction it already stands in is not asking for
        // anything, and is taken.
        for spelling in [
            "noautobranchlength",
            "nopcaspcr",
            "symbolcase",
            "forwardrefmax",
        ] {
            assert_eq!(
                asm(&format!(" pragma {spelling}\n fcb 1\n"))
                    .unwrap_or_else(|e| panic!("{spelling}: {e}"))
                    .bytes,
                vec![1],
                "{spelling}"
            );
        }
    }

    #[test]
    fn align_needs_a_positive_boundary() {
        assert!(asm(" fcb 1\n align\n").is_err(), "bare `align`");
        assert!(asm(" fcb 1\n align 0\n").is_err(), "`align 0`");
        assert!(asm(" fcb 1\n align -4\n").is_err(), "a negative boundary");
    }

    /// The boundary is the boundary, not an exponent, and it need not be a
    /// power of two — the two things a mask-based align would get wrong.
    #[test]
    fn align_pads_to_the_stated_boundary() {
        assert_eq!(
            asm(" fcb 1\n align 4\n fcb 2\n").expect("4").bytes,
            vec![1, 0, 0, 0, 2]
        );
        assert_eq!(
            asm(" fcb 1\n align 3\n fcb 2\n").expect("3").bytes,
            vec![1, 0, 0, 2]
        );
        assert_eq!(
            asm(" fcb 1\n align 4,$ff\n fcb 2\n").expect("fill").bytes,
            vec![1, 0xFF, 0xFF, 0xFF, 2]
        );
        assert_eq!(
            asm(" fcb 1,2,3,4\n align 4\n fcb 9\n")
                .expect("aligned")
                .bytes,
            vec![1, 2, 3, 4, 9]
        );
    }

    #[test]
    fn direct_and_extended_selection() {
        // Low constant -> direct; high constant -> extended.
        assert_eq!(
            asm("        lda $20\n").expect("dir").bytes,
            vec![0x96, 0x20]
        );
        assert_eq!(
            asm("        lda $1234\n").expect("ext").bytes,
            vec![0xB6, 0x12, 0x34]
        );
        // Forces: `<` direct, `>` extended.
        assert_eq!(
            asm("        lda <$20\n").expect("force dir").bytes,
            vec![0x96, 0x20]
        );
        assert_eq!(
            asm("        lda >$20\n").expect("force ext").bytes,
            vec![0xB6, 0x00, 0x20]
        );
    }

    #[test]
    fn big_endian_data() {
        assert_eq!(
            asm("        fcb $01,$02\n").expect("fcb").bytes,
            vec![0x01, 0x02]
        );
        // fdb is big-endian.
        assert_eq!(
            asm("        fdb $1234\n").expect("fdb").bytes,
            vec![0x12, 0x34]
        );
    }

    #[test]
    fn fill_zmb_fqb_match_reference_bytes() {
        // Byte-for-byte against `lwasm --6809 --raw`:
        //   fill $ff,3 -> ff ff ff   (lwasm order is value,count)
        //   zmb 2      -> 00 00
        //   fqb $12345678 -> 12 34 56 78  (32-bit big-endian)
        let a = asm("        fcb $aa\n        fill $ff,3\n        zmb 2\n        fqb $12345678\n        fcb $bb\n")
            .expect("fill/zmb/fqb");
        assert_eq!(
            a.bytes,
            vec![
                0xAA, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x12, 0x34, 0x56, 0x78, 0xBB
            ]
        );
    }

    #[test]
    fn short_and_long_branches() {
        // bra to self+2 with a backward loop: org so the target resolves.
        let a = asm("        org $1000\nloop    bra loop\n").expect("bra");
        // bra opcode 0x20, offset = loop - (pc+2) = -2 = 0xFE.
        assert_eq!(a.bytes, vec![0x20, 0xFE]);
        let a = asm("        org $1000\nloop    lbra loop\n").expect("lbra");
        // lbra opcode 0x16, 16-bit offset = -3 = 0xFFFD.
        assert_eq!(a.bytes, vec![0x16, 0xFF, 0xFD]);
        // A conditional long branch is 0x10-prefixed.
        let a = asm("        org $1000\nloop    lbeq loop\n").expect("lbeq");
        assert_eq!(a.bytes, vec![0x10, 0x27, 0xFF, 0xFC]);
    }

    #[test]
    fn labels_and_org() {
        let a = asm("        org $2000\nstart   lda #$00\n        sta $1000\n        rts\n")
            .expect("prog");
        assert_eq!(a.origin, Some(0x2000));
        assert_eq!(a.bytes, vec![0x86, 0x00, 0xB7, 0x10, 0x00, 0x39]);
        assert_eq!(a.symbols.get("start"), Some(&0x2000));
    }

    #[test]
    fn indexed_offsets_pick_smallest() {
        // No offset, 5-bit, 8-bit, 16-bit — register X (opcode 0xA6).
        assert_eq!(
            asm("        lda ,x\n").expect("noff").bytes,
            vec![0xA6, 0x84]
        );
        assert_eq!(
            asm("        lda 5,x\n").expect("5bit").bytes,
            vec![0xA6, 0x05]
        );
        assert_eq!(
            asm("        lda -16,x\n").expect("5neg").bytes,
            vec![0xA6, 0x10]
        );
        assert_eq!(
            asm("        lda 16,x\n").expect("8bit").bytes,
            vec![0xA6, 0x88, 0x10]
        );
        assert_eq!(
            asm("        lda $1234,x\n").expect("16bit").bytes,
            vec![0xA6, 0x89, 0x12, 0x34]
        );
        // Other registers shift the postbyte; Y=+0x20, U=+0x40, S=+0x60.
        assert_eq!(asm("        lda ,y\n").expect("y").bytes, vec![0xA6, 0xA4]);
        assert_eq!(asm("        ldx 2,s\n").expect("s").bytes, vec![0xAE, 0x62]);
    }

    #[test]
    fn indexed_auto_and_accumulator() {
        assert_eq!(
            asm("        lda ,x+\n").expect("inc1").bytes,
            vec![0xA6, 0x80]
        );
        assert_eq!(
            asm("        lda ,x++\n").expect("inc2").bytes,
            vec![0xA6, 0x81]
        );
        assert_eq!(
            asm("        lda ,-x\n").expect("dec1").bytes,
            vec![0xA6, 0x82]
        );
        assert_eq!(
            asm("        lda ,--x\n").expect("dec2").bytes,
            vec![0xA6, 0x83]
        );
        assert_eq!(asm("        lda a,x\n").expect("a").bytes, vec![0xA6, 0x86]);
        assert_eq!(asm("        lda b,x\n").expect("b").bytes, vec![0xA6, 0x85]);
        assert_eq!(asm("        lda d,x\n").expect("d").bytes, vec![0xA6, 0x8B]);
    }

    #[test]
    fn indexed_indirect_and_pcr() {
        assert_eq!(
            asm("        lda [,x]\n").expect("ind").bytes,
            vec![0xA6, 0x94]
        );
        // Indirect has no 5-bit form: a small offset still uses the 8-bit form.
        assert_eq!(
            asm("        lda [5,x]\n").expect("ind8").bytes,
            vec![0xA6, 0x98, 0x05]
        );
        assert_eq!(
            asm("        lda [$2000]\n").expect("extind").bytes,
            vec![0xA6, 0x9F, 0x20, 0x00]
        );
        // PCR to a label: 16-bit offset relative to the next instruction.
        let a =
            asm("        org $1000\n        leax msg,pcr\n        nop\nmsg fcb 1\n").expect("pcr");
        // leax=0x30, postbyte 0x8D, offset = msg($1005) - next($1004) = 1.
        assert_eq!(a.bytes[..4], [0x30, 0x8D, 0x00, 0x01]);
    }

    #[test]
    fn transfer_and_stack() {
        assert_eq!(
            asm("        tfr a,b\n").expect("tfr").bytes,
            vec![0x1F, 0x89]
        );
        assert_eq!(
            asm("        exg x,d\n").expect("exg").bytes,
            vec![0x1E, 0x10]
        );
        assert_eq!(
            asm("        pshs a,b,x\n").expect("pshs").bytes,
            vec![0x34, 0x16]
        );
        // `d` sets both the A and B bits.
        assert_eq!(
            asm("        puls x,y,d\n").expect("puls").bytes,
            vec![0x35, 0x36]
        );
        // pshu's bit 6 is S, not U.
        assert_eq!(
            asm("        pshu a,b,s\n").expect("pshu").bytes,
            vec![0x36, 0x46]
        );
    }

    /// `fcn` and `fcz` are the same directive under two names, and `fcs`
    /// marks the end in the byte it already has rather than spending another.
    /// An empty string is where the two conventions part: `fcn ""` still has
    /// a NUL to emit, `fcs ""` has no last byte to set a bit in.
    #[test]
    fn a_terminated_string_marks_its_own_end() {
        assert_eq!(
            asm("        fcn \"AB\"\n").expect("fcn").bytes,
            vec![0x41, 0x42, 0]
        );
        assert_eq!(
            asm("        fcz \"AB\"\n").expect("fcz").bytes,
            vec![0x41, 0x42, 0]
        );
        assert_eq!(
            asm("        fcs \"AB\"\n").expect("fcs").bytes,
            vec![0x41, 0xC2]
        );
        // The delimiter is whatever the string opens with, as for `fcc`.
        assert_eq!(
            asm("        fcs /Hi/\n").expect("slash").bytes,
            vec![0x48, 0xE9]
        );
        assert_eq!(asm("        fcn \"\"\n").expect("empty fcn").bytes, vec![0]);
        assert!(
            asm("        fcs \"\"\n")
                .expect("empty fcs")
                .bytes
                .is_empty(),
            "an empty `fcs` has no byte to mark"
        );
    }

    /// `fdbs` is not a byte-swapped `fdb`, and the gap is lwasm's: its high
    /// byte comes from a truncating division, so a negative that is not a
    /// whole page loses the sign extension `fdb` keeps.
    #[test]
    fn fdbs_swaps_the_bytes_the_way_lwasm_does() {
        assert_eq!(
            asm("        fdbs $1234\n").expect("swap").bytes,
            vec![0x34, 0x12]
        );
        assert_eq!(
            asm("        fdbs 1,2,3\n").expect("list").bytes,
            vec![1, 0, 2, 0, 3, 0]
        );
        // `fdb -255` is `ff 01`; a swap of it would be `01 ff`.
        assert_eq!(
            asm("        fdb -255\n").expect("fdb").bytes,
            vec![0xFF, 0x01]
        );
        assert_eq!(
            asm("        fdbs -255\n").expect("fdbs").bytes,
            vec![0x01, 0x00]
        );
        // Where the quotient is exact the two really are a swap.
        assert_eq!(
            asm("        fdbs -256\n").expect("page").bytes,
            vec![0x00, 0xFF]
        );
        assert_eq!(
            asm("        fdbs -32768\n").expect("min").bytes,
            vec![0x00, 0x80]
        );
        // Symbols resolve in pass two, forward references included.
        assert_eq!(
            asm("        org $1000\n        fdbs later\nlater   fcb 9\n")
                .expect("forward")
                .bytes,
            vec![0x02, 0x10, 9]
        );
    }

    /// The reserve spellings carry a width, so the count is units and not
    /// bytes. A negative count is lwasm's own refusal ("Negative block sizes
    /// make no sense!"), not a huge unsigned one.
    #[test]
    fn the_reserve_family_reserves_in_units_of_its_width() {
        for byte_wide in ["rmb", "zmb", "bsz", "fzb"] {
            let bytes = asm(&format!("        {byte_wide} 3\n"))
                .expect(byte_wide)
                .bytes;
            assert_eq!(bytes, vec![0; 3], "{byte_wide}");
        }
        for two_wide in ["rmd", "zmd", "rmw"] {
            let bytes = asm(&format!("        {two_wide} 3\n"))
                .expect(two_wide)
                .bytes;
            assert_eq!(bytes, vec![0; 6], "{two_wide}");
        }
        for four_wide in ["rmq", "zmq"] {
            let bytes = asm(&format!("        {four_wide} 3\n"))
                .expect(four_wide)
                .bytes;
            assert_eq!(bytes, vec![0; 12], "{four_wide}");
        }
        for any in ["rmb", "rmd", "rmq"] {
            assert!(
                asm(&format!("        {any} 0\n"))
                    .expect("zero")
                    .bytes
                    .is_empty()
            );
            assert!(asm(&format!("        {any} -1\n")).is_err(), "{any} -1");
        }
    }

    #[test]
    fn fcc_string() {
        assert_eq!(
            asm("        fcc \"AB\"\n").expect("dq").bytes,
            vec![0x41, 0x42]
        );
        assert_eq!(
            asm("        fcc /CD/\n").expect("slash").bytes,
            vec![0x43, 0x44]
        );
    }

    /// U6 — the 6809 front-end routes through the AST. Its computed-operand
    /// instructions carry `Item::Encoded`, and comments are carried as trivia
    /// (both `*` whole-line and `;` inline) without changing the bytes (AE1).
    #[test]
    fn comments_are_carried_as_trivia() {
        let src = "* header\nstart   lda #$05   ; load\n        leax 5,x\n";
        let prog = super::parse_program(src, crate::dialects::macros::Expand::Yes).expect("parses");
        assert!(
            prog.nodes[0]
                .trivia
                .leading
                .iter()
                .any(|c| c.text == "* header"),
            "whole-line `*` comment attaches as leading trivia"
        );
        assert!(
            prog.nodes.iter().any(|n| n
                .trivia
                .trailing
                .as_ref()
                .is_some_and(|c| c.text == "; load")),
            "same-line `;` comment attaches as trailing trivia"
        );
        // The indexed `leax 5,x` is a computed-operand instruction: its item is
        // `Item::Encoded`, proving the wrap path.
        assert!(
            prog.nodes
                .iter()
                .any(|n| matches!(n.item, Some(crate::ast::Item::Encoded(_)))),
            "a computed-operand instruction carries Item::Encoded"
        );
        assert_eq!(
            asm(src).expect("with comments").bytes,
            asm("start   lda #$05\n        leax 5,x\n")
                .expect("without")
                .bytes,
            "comments do not change bytes"
        );
    }

    // ----- Macros (#93) -------------------------------------------------
    //
    // Every expectation is a byte string lwasm 4.19 produced for the same
    // source, with `--6809 --raw`.

    /// A definition is `name macro` and only that way round — lwasm rejects
    /// ` macro name` with `Missing macro name`, where vasm takes both.
    #[test]
    fn macros_expand() {
        assert_eq!(
            asm("nop2\tmacro\n nop\n nop\n endm\n nop2\n")
                .expect("nop2")
                .bytes,
            vec![0x12, 0x12]
        );
        asm(" macro nop2\n nop\n endm\n nop2\n").expect_err("lwasm has no keyword-first form");
    }

    /// Parameters have no names: a body refers to `\1`, `\2`, so a macro's
    /// arity is decided at the call site. Extras are dropped; a missing one
    /// substitutes empty and the emptied operand complains.
    #[test]
    fn parameters_are_positional_and_unchecked() {
        assert_eq!(
            asm("ldav\tmacro\n lda #\\1\n endm\n ldav 5\n")
                .expect("one")
                .bytes,
            vec![0x86, 0x05]
        );
        assert_eq!(
            asm("ldav\tmacro\n lda #\\1\n ldb #\\2\n endm\n ldav 5,7\n")
                .expect("two")
                .bytes,
            vec![0x86, 0x05, 0xC6, 0x07]
        );
        assert_eq!(
            asm("ldav\tmacro\n lda #\\1\n endm\n ldav 5,9\n")
                .expect("extras dropped")
                .bytes,
            vec![0x86, 0x05]
        );
        asm("ldav\tmacro\n lda #\\1\n endm\n ldav\n").expect_err("`lda #` has no value");
    }

    /// A `?` or `@` **suffix** marks a symbol local to the expansion — a third
    /// spelling of locals across four dialects, and the only one that is not a
    /// prefix or a declaration. lwasm strips the marker itself, so we must too:
    /// leaving it in would put a `?` in the middle of a name our parser reads.
    #[test]
    fn a_suffix_scopes_a_label_to_its_expansion() {
        for marker in ['?', '@'] {
            let src = format!(
                "delay\tmacro\nspin{marker} deca\n bne spin{marker}\n endm\n delay\n delay\n"
            );
            assert_eq!(
                asm(&src).expect("two expansions").bytes,
                vec![0x4A, 0x26, 0xFD, 0x4A, 0x26, 0xFD],
                "{src}"
            );
        }
        // Without the marker the label is global and the second use collides.
        asm("delay\tmacro\nspin deca\n endm\n delay\n delay\n")
            .expect_err("a plain label is global");
    }

    /// A local does not escape its expansion: lwasm answers `Undefined symbol
    /// spin?` outside, and so must we.
    #[test]
    fn a_local_does_not_escape_its_expansion() {
        asm("delay\tmacro\nspin? deca\n endm\n delay\n jmp spin?\n")
            .expect_err("spin? is local to the expansion");
    }

    /// The formatter lays source out; it does not rewrite programs.
    ///
    /// This used to assert the walk **refused** a macro, which pinned a
    /// limitation rather than a property. The definition is copied verbatim
    /// now — column included, because lwasm names its macro in label position
    /// and *only* there.
    #[test]
    fn formatting_does_not_expand() {
        let src = "ldav\tmacro\n lda #\\1\n endm\n ldav 5\n";
        let out = crate::format_lwasm(src).expect("the walk copies a definition");

        assert!(out.contains("macro"), "{out}");
        assert!(out.contains("endm"), "{out}");
        assert!(out.contains("ldav 5"), "{out}");
        assert!(!out.contains("lda #5"), "expanded into the output:\n{out}");
        assert!(
            out.lines().any(|l| l.starts_with("ldav")),
            "the macro name left column 0:\n{out}"
        );
    }

    /// Formatting a macro changes the layout and not the program.
    #[test]
    fn a_formatted_macro_assembles_to_the_same_bytes() {
        let src = "ldav\tmacro\n lda #\\1\n endm\n ldav 5\n";
        let before = asm(src).expect("assembles").bytes;
        let formatted = crate::format_lwasm(src).expect("formats");
        let after = asm(&formatted)
            .expect("the formatted source assembles")
            .bytes;
        assert_eq!(
            before, after,
            "formatting changed the program:\n{formatted}"
        );
        assert_eq!(formatted, crate::format_lwasm(&formatted).expect("formats"));
    }

    // -----------------------------------------------------------------------
    // Conditionals. Measured against lwasm 4.25.
    // -----------------------------------------------------------------------

    /// Every numeric form compares its expression against **zero**, so each
    /// spelling is its own comparison rather than a boolean test.
    #[test]
    fn each_comparison_tests_against_zero() {
        // nop = $12, rts = $39.
        for (src, want) in [
            (" ifne 1\n nop\n endc\n rts\n", vec![0x12, 0x39]),
            (" ifne 0\n nop\n endc\n rts\n", vec![0x39]),
            (" ifeq 0\n nop\n endc\n rts\n", vec![0x12, 0x39]),
            (" ifgt 1\n nop\n endc\n rts\n", vec![0x12, 0x39]),
            (" ifge 0\n nop\n endc\n rts\n", vec![0x12, 0x39]),
            (" iflt 1\n nop\n endc\n rts\n", vec![0x39]),
            (" ifle 0\n nop\n endc\n rts\n", vec![0x12, 0x39]),
        ] {
            assert_eq!(crate::assemble_lwasm(src).expect(src).bytes, want, "{src}");
        }
    }

    /// `endc` **and** `endif` both close — lwasm is the only dialect measured
    /// here with two closers — and the keywords are case-insensitive.
    #[test]
    fn either_closer_ends_a_conditional() {
        for src in [
            " ifne 1\n nop\n endc\n rts\n",
            " ifne 1\n nop\n endif\n rts\n",
            " IFNE 1\n NOP\n ENDC\n RTS\n",
        ] {
            assert_eq!(
                crate::assemble_lwasm(src).expect(src).bytes,
                vec![0x12, 0x39],
                "{src}"
            );
        }
    }

    /// **The reason this dialect needed a real evaluator.** An `equ` decides
    /// lwasm's addressing mode, and the mode decides the instruction's *size*:
    /// direct is two bytes, extended is three. A binding made while parsing
    /// both branches would pick direct where the reference errors, so each live
    /// line re-parses against the environment as it actually stands.
    #[test]
    fn an_untaken_branch_binds_nothing_that_could_change_a_size() {
        // The untaken `equ $10` must not make this direct.
        assert_eq!(
            crate::assemble_lwasm(" ifne 0\nsym equ $10\n endc\nsym equ $1234\n lda sym\n")
                .expect("assembles")
                .bytes,
            vec![0xB6, 0x12, 0x34],
            "extended: three bytes"
        );
        // A taken one does.
        assert_eq!(
            crate::assemble_lwasm(" ifne 1\nsym equ $10\n endc\n lda sym\n")
                .expect("assembles")
                .bytes,
            vec![0x96, 0x10],
            "direct: two bytes"
        );
    }

    #[test]
    fn conditionals_nest_and_take_an_else() {
        assert_eq!(
            crate::assemble_lwasm(" ifne 0\n nop\n else\n clra\n endc\n rts\n")
                .expect("assembles")
                .bytes,
            vec![0x4F, 0x39]
        );
        assert_eq!(
            crate::assemble_lwasm(" ifne 1\n ifne 1\n nop\n endc\n rts\n endc\n")
                .expect("assembles")
                .bytes,
            vec![0x12, 0x39]
        );
    }

    /// Formatting a conditional changes the layout and not the program.
    ///
    /// The closer keeps the author's word. lwasm takes `endc` and `endif`
    /// alike, and the formatter used to render both as `endif` (#195) — which
    /// looked cosmetic until rgbasm, whose *only* closer is `ENDC`, turned the
    /// same bug into source the assembler would not take.
    #[test]
    fn a_formatted_conditional_assembles_to_the_same_bytes() {
        let src = "n equ 1\n ifne n\n lda #5\n else\n clra\n endc\n rts\n";
        let before = crate::assemble_lwasm(src).expect("assembles").bytes;
        let formatted = crate::format_lwasm(src).expect("formats");
        let after = crate::assemble_lwasm(&formatted)
            .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
            .bytes;
        assert_eq!(
            before, after,
            "formatting changed the program:\n{formatted}"
        );

        let again = crate::format_lwasm(&formatted).expect("formats");
        assert_eq!(formatted, again, "{formatted}");

        assert!(
            formatted.contains("endc"),
            "the author wrote `endc`: {formatted}"
        );
    }
}
