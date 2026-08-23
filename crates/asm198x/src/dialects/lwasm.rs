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
        };
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
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
        };
        let mut out = Vec::new();
        crate::ast::evaluate(&mut eval, &program.nodes, true, &mut out)?;
        Ok(out)
    }
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
    nodes: Vec<Node>,
}

impl Walker {
    fn new() -> Self {
        Self {
            env: BTreeMap::new(),
            pending_leading: Vec::new(),
            in_macro: false,
            macro_names: BTreeSet::new(),
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
            "include" | "use" => {
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
            "ifne" | "ifeq" | "ifgt" | "ifge" | "iflt" | "ifle" | "ifdef" | "ifndef" => {
                BlockKw::CondOpen
            }
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
        let op = if rest.is_empty() {
            None
        } else {
            parse_op(rest, &self.env, line)?
        };
        // Bind an `equ` value into the parse-time env so a later direct/extended
        // choice can fold it (mirrors the engine's pass-1 `equ`).
        if let (Some(name), Some(Operation::Equ(e))) = (&label, &op)
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
    // boolean, which is why there are six of them and not one. Both `endc` and
    // `endif` close — the only dialect measured here with two closers.
    //
    // `ifpragma` and `ifstr` are real lwasm and deliberately absent: pragma
    // strings and string conditions are their own surfaces, demand-gated.
    Directive {
        id: "conditional",
        pattern: Pattern::Exact(&[
            "ifne", "ifeq", "ifgt", "ifge", "iflt", "ifle", "ifdef", "ifndef", "else", "endc",
            "endif",
        ]),
        category: Category::Operation,
    },
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
    Directive {
        id: "fqb",
        pattern: Pattern::Exact(&["fqb"]),
        category: Category::Operation,
    },
    Directive {
        id: "reserve",
        pattern: Pattern::Exact(&["rmb", ".ds", "zmb"]),
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
    // Walk-handled. `use` is lwasm's own second spelling of `include`, so it
    // is an alternative spelling of one entry rather than a directive of its
    // own — the same call `fcb`/`.byte` already get here.
    Directive {
        id: "include",
        pattern: Pattern::Exact(&["include", "use"]),
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Exact(&["includebin"]),
        category: Category::Operation,
    },
];

fn parse_op(
    rest: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (mnem, operand) = split_first_word(rest);
    let m = mnem.to_ascii_lowercase();
    // Dispatch through the declared surface: a spelling the declaration does
    // not carry cannot be accepted here. See `crate::directives`.
    let Some(directive) = lookup(DIRECTIVES, &m) else {
        return Ok(Some(parse_instruction(&m, operand, env, line)?));
    };
    match directive.category {
        // `end` marks the end of source; it emits nothing.
        Category::Ignored => Ok(None),
        Category::KnownUnsupported => Err(AsmError::new(
            line,
            format!("`{m}` is a real directive here and asm198x does not implement it yet"),
        )),
        Category::Operation => match directive.id {
            "org" => Ok(Some(Operation::Org(value(operand, line)?))),
            "equ" => Ok(Some(Operation::Equ(value(operand, line)?))),
            "bytes" => Ok(Some(Operation::Bytes(list(operand, line)?))),
            "words" => Ok(Some(Operation::Words(list(operand, line)?))),
            "fcc" => Ok(Some(parse_fcc(operand, line)?)),
            "fqb" => Ok(Some(parse_fqb(operand, line)?)),
            "reserve" => parse_rmb(operand, env, line),
            "fill" => parse_fill(operand, env, line),
            other => Err(AsmError::new(
                line,
                format!("`{other}` is declared but not dispatched"),
            )),
        },
    }
}

/// `rmb count` / `zmb count` — reserve/zero `count` bytes, zero-filled (the
/// flat-output behaviour). `count` folds against the parse-time env so the size
/// is known in pass one.
fn parse_rmb(
    operand: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let n = fold_const(&value(operand, line)?, env, line)?;
    let n = usize::try_from(n)
        .map_err(|_| AsmError::new(line, "`rmb` count must be a non-negative constant"))?;
    Ok(Some(Operation::Bytes(vec![Expr::Num(0); n])))
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
                m, imm, direct, indexed, extended, *width, operand, env, line,
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
        return Ok(encoded(direct, value(rest, line)?, 1));
    }
    if let Some(rest) = t.strip_prefix('>') {
        if extended.is_empty() {
            return Err(AsmError::new(line, format!("`{m}` has no extended mode")));
        }
        return Ok(encoded(extended, value(rest, line)?, 2));
    }
    // Bare address: direct when it folds to a constant that fits in a byte and a
    // direct mode exists; otherwise extended. A forward symbol stays extended,
    // keeping the size stable across passes — lwasm's default.
    let e = value(t, line)?;
    let fits_direct =
        !direct.is_empty() && fold_const(&e, env, line).is_ok_and(|v| (0..=0xFF).contains(&v));
    if fits_direct {
        Ok(encoded(direct, e, 1))
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

/// `fcc` — a string with a self-chosen delimiter (`"text"`, `/text/`, …): one
/// byte per character, up to the closing delimiter.
fn parse_fcc(operand: &str, line: usize) -> Result<Operation, AsmError> {
    let t = operand.trim();
    let delim = t
        .chars()
        .next()
        .ok_or_else(|| AsmError::new(line, "`fcc` needs a string"))?;
    let rest = &t[delim.len_utf8()..];
    let end = rest
        .find(delim)
        .ok_or_else(|| AsmError::new(line, "unterminated `fcc` string"))?;
    Ok(Operation::Bytes(
        rest[..end]
            .bytes()
            .map(|b| Expr::Num(i64::from(b)))
            .collect(),
    ))
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
    /// `name macro` — name first, and only name first.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        let text = macros::without_comment(line);
        // A definition names its macro in label position, so an indented
        // `macro` is not one.
        if text.starts_with(char::is_whitespace) {
            return None;
        }
        let (name, rest) = text.trim_end().split_once(char::is_whitespace)?;
        rest.trim()
            .eq_ignore_ascii_case("macro")
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
}

impl crate::ast::CondEval for LwasmEval {
    /// Fold one conditional head. Every numeric form compares against zero;
    /// `ifdef`/`ifndef` test the environment for a name.
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        let (word, args) = split_first_word(head.trim());
        let word = word.to_ascii_lowercase();
        let args = args.trim();
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
            "ifne" => value != 0,
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
            _ => parse_op(&node.source, &self.env, line)?,
        };
        if let (Some(sym), Some(Operation::Equ(e))) = (node.label.as_ref(), &op)
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
    /// It does change one word: `endc` comes back as `endif` (#195). Both are
    /// lwasm's and both assemble to the same bytes, so this pins the current
    /// behaviour rather than blessing it — a fix will show up here as a failure.
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

        assert!(formatted.contains("endif"), "#195: {formatted}");
    }
}
