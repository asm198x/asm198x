//! Shared `.include`/`.incbin` machinery for the **ca65-syntax flat family**
//! (`ca65_816`, `ca65_huc6280`) — language-surface U4, KTD1/KTD5/KTD8 — and
//! the recursion driver the other flat walk-based dialects (rgbasm, lwasm)
//! reuse. The driver ([`walk_file`]) owns what is genuinely shared: the
//! interleaved per-line walk, lazy include/incbin resolution (KTD1), cycle
//! detection, the depth backstop, label-binds-at-the-point, and per-file error
//! stamping. What the probes proved **divergent** — the resolution anchor and
//! the incbin window arithmetic — is per-dialect, supplied through
//! [`WalkSemantics`] so a dialect states its probe-pinned semantics rather
//! than inheriting ca65's.
//!
//! Both flat ca65 dialects parse line-by-line with an accumulated environment
//! (constants; the 65816 adds the `.a8`/`.a16`/`.i8`/`.i16` width state), so
//! their multi-file walk is the z80 family's interleaved model: each live line
//! parses with the environment so far, an include's lines parse with the *same*
//! environment, and everything the include defined — constants driving zp/abs
//! selection, a width flip sizing later immediates — flows back out to the
//! includer's subsequent lines (probe-pinned against `ca65 --cpu 65816` and
//! `--cpu huc6280`, V2.18). The dialects differ only in their per-line parse,
//! supplied through [`FlatWalk`]; the directive recognition, argument grammar,
//! window semantics, and the recursion driver live here so the two skins
//! cannot drift apart.
//!
//! **Resolution order (probe-pinned, ca65 V2.18):** a relative request is
//! tried against the requesting file's own directory first, then each
//! *enclosing includer's* directory innermost → outermost (ending at the root
//! input's), and never the bare process working directory. ca65 consults its
//! `-I` dirs only after that whole chain; our `-I` rides inside the first hop
//! (the [`FsLoader`](crate::source::FsLoader) falls back to it per attempt) —
//! a deliberate CLI-surface deviation like the ones documented on the loader,
//! visible only when a name exists in both an ancestor's directory and a `-I`
//! dir.
//!
//! **`.incbin "file"[, offset[, size]]` window (probe-pinned):** offset and
//! size are parse-time constant expressions (ca65: "Constant expression
//! expected" on a forward reference). A negative offset is an error; an offset
//! in `0..=len` is honoured (at EOF → empty); past EOF is an error ("Range
//! error"). A missing **or negative** size means "the rest of the file"
//! (`.incbin "f", 2, -2` emits everything from offset 2 — ca65 treats any
//! negative size as the unspecified sentinel); size 0 is empty; a size past
//! the remaining bytes is an error.

use crate::ast::{Node, Span, Symbol, Trivia};
use crate::dialects::macros;
use crate::engine::AsmError;
use crate::source::{LoadError, MAX_INCLUDE_DEPTH, SourceLoader, SourceMap};
use crate::span::FileId;

/// A walk-handled `.include`/`.incbin` line found by a dialect's per-line
/// parse, handed back for the driver to decide: the single-source parse keeps
/// it as an unresolved verbatim item (KTD1 — `--fmt` never opens the target);
/// the multi-file walk resolves it lazily.
pub(crate) struct DirectiveLine {
    pub(crate) kind: WalkDirective,
    /// A label on the directive line — probe-pinned to bind at the include
    /// point / payload start (`here: .include …` then `.word here`).
    pub(crate) label: Option<Symbol>,
    /// The verbatim directive text (`.include "file.s"`), for `--fmt`.
    pub(crate) source: String,
    pub(crate) span: Span,
    /// The file-name operand's position, when the parse knew it — directive
    /// diagnostics (missing target, bad window) point here.
    pub(crate) operand_span: Option<Span>,
    pub(crate) trivia: Trivia,
}

/// Which walk-handled directive a [`DirectiveLine`] carries.
pub(crate) enum WalkDirective {
    /// `.include "file"` — the target as the directive spelled it.
    Include { request: String },
    /// `.incbin "file"[, offset[, size]]` — the offset/size folded to
    /// parse-time constants (probe-pinned); `None` means omitted.
    Incbin {
        request: String,
        offset: Option<i64>,
        size: Option<i64>,
    },
}

/// Where a dialect's reference anchors a relative include/incbin request —
/// probe-pinned per dialect (KTD5), because the references genuinely diverge.
/// The `-I` search dirs always apply after the anchor (the loader's fallback).
#[derive(Clone, Copy)]
pub(crate) enum Resolution {
    /// The requesting file's own directory, then each **enclosing includer's**
    /// innermost → outermost (ca65 V2.18, probe-pinned).
    AncestorChain,
    /// The requesting file's own directory only — no ancestor hops, no root
    /// fallback (lwasm 4.24, probe-pinned: a root-dir copy is *not* found from
    /// inside a subdirectory include).
    Requester,
    /// The **root input's** directory for every request, however deep the
    /// requester (rgbasm v1.0.1, probe-pinned: rgbasm anchors at the process
    /// cwd and never the including file's directory — our input's directory
    /// stands in for the cwd, the documented
    /// [`FsLoader`](crate::source::FsLoader) stance).
    Root,
}

/// A dialect's incbin window arithmetic: `(data, offset, size)` → the sliced
/// payload, or the error-message body (the driver wraps it with the request
/// name at the directive's span).
pub(crate) type IncbinWindow = fn(&[u8], Option<i64>, Option<i64>) -> Result<Vec<u8>, String>;

/// A dialect's probe-pinned multi-file semantics, handed to [`walk_file`]:
/// the resolution anchor and the incbin window arithmetic (offset/size
/// legality diverges — ca65 reads a negative size as "rest of file", rgbasm
/// rejects negatives outright, lwasm counts a negative offset from EOF).
pub(crate) struct WalkSemantics {
    pub(crate) resolution: Resolution,
    pub(crate) window: IncbinWindow,
    /// The extension appended to an **extensionless** include request before
    /// the exact spelling is tried — asl's probe-pinned `.inc` default
    /// (`include defs` finds `defs.inc` first, `defs` second); `None` for the
    /// dialects without extension defaulting. Applies to includes only —
    /// asl's `BINCLUDE` has no defaulting (probe-pinned).
    pub(crate) include_default_ext: Option<&'static str>,
}

/// ca65's own semantics: the ancestor-chain anchor and the negative-size
/// sentinel window ([`slice_incbin`]).
pub(crate) const CA65_SEMANTICS: WalkSemantics = WalkSemantics {
    resolution: Resolution::AncestorChain,
    window: slice_incbin,
    include_default_ext: None,
};

/// The per-line seam a flat ca65 dialect supplies to the shared walk: parse
/// one line with the live environment, pushing ordinary nodes internally and
/// handing a `.include`/`.incbin` back for the driver.
/// What a line does to a block, as far as the shared cursor is concerned.
///
/// Deliberately about *structure* and not about meaning: the cursor groups
/// lines and never asks what a condition says. Which spellings map to which
/// arm is each dialect's own business — ca65's `.endif` and lwasm's `endc`
/// are the same arm here and nothing else about them is shared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BlockKw {
    /// Opens a conditional (`.if`, `.ifdef`, `ifne`, …).
    CondOpen,
    /// A chained leg that both closes the previous branch and opens the next
    /// (`.elseif`).
    ElseIf,
    /// The final alternative (`.else`).
    Else,
    /// Closes a conditional (`.endif`, `endc`).
    CondClose,
    /// Opens a repetition (`.repeat`, `rept`).
    RepeatOpen,
    /// Closes a repetition (`.endrepeat`, `endr`).
    RepeatClose,
}

/// How the walk treats an include or incbin it reaches.
///
/// Two callers, one cursor. The multi-file walk resolves lazily as each
/// directive is reached (KTD1); the single-source parse never opens a target,
/// so `--fmt` renders the directive verbatim and works with a missing file.
/// Before the cursor existed these were two separate line loops, and only one
/// of them would have gained block structure.
pub(crate) enum Resolve<'a> {
    Lazily {
        map: &'a mut SourceMap,
        loader: &'a dyn SourceLoader,
        stack: &'a mut Vec<FileId>,
        sem: &'a WalkSemantics,
    },
    Never,
}

/// How a [`walk_block`] leg ended.
#[derive(PartialEq, Eq, Debug)]
enum BlockClose {
    Eof,
    /// `.else` — the next leg is the final alternative.
    Else,
    /// `.elseif <cond>`, carrying its head text and line so the chain
    /// round-trips as the flat chain the author wrote.
    ElseIf(String, usize),
    /// `.endif` / `endc`, carrying the closer as written.
    CondClose(String),
    /// `.endrepeat` / `endr`, carrying the closer exactly as written so the
    /// formatter re-emits the author's word — lwasm and vasm each have two
    /// spellings, and choosing one would be a rewrite rather than a layout.
    RepeatClose(String),
}

pub(crate) trait FlatWalk {
    /// Parse one line of `file`. Ordinary lines push their node (or nothing)
    /// and return `None`; a walk-handled directive is returned unresolved.
    ///
    /// # Errors
    /// Any per-line parse failure (the walk stamps the file onto it).
    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError>;

    /// Append a node the walk built (a label bound at the include point, an
    /// incbin's resolved payload).
    fn push_node(&mut self, node: Node);

    /// The nodes parsed so far, so the walk can put a macro expansion's spans
    /// back on the lines the author wrote.
    fn nodes_mut(&mut self) -> &mut Vec<Node>;

    /// What this line does to a **block**, in this dialect's spelling.
    ///
    /// Defaulted to "nothing": a dialect on this walk has no block structure
    /// until it declares one, and six of the seven do not yet. The default is
    /// what keeps [`walk_file`]'s cursor from grouping lines in a dialect whose
    /// reference has no conditionals — the same posture, and the same reason,
    /// as `Z80Syntax::cond_keyword`.
    ///
    /// `code` is the line with its comment already stripped.
    fn block_keyword(&self, code: &str) -> Option<BlockKw> {
        let _ = code;
        None
    }

    /// Rewrite source before parsing — macro expansion (#93).
    ///
    /// `None`, the default, means the dialect rewrites nothing and its source
    /// parses as written; every dialect on this walk but the ca65 family is in
    /// that case. Overriding it is what gives a dialect macros on the
    /// multi-file path as well as the single-source one, which is the step
    /// easiest to forget — the CLI uses only the multi-file path.
    fn expand_source(&self, source: &str) -> Result<macros::Expansion, AsmError> {
        let _ = source;
        Ok(None)
    }
}

/// One file's leg of the multi-file walk (the z80 `walk_file` model): parse
/// each line through the dialect's [`FlatWalk`], and resolve includes/incbins
/// as they are reached live (KTD1).
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing target, an include cycle (the active-stack check), a bad incbin
/// window, or the [`MAX_INCLUDE_DEPTH`] backstop — all at the directive's span.
pub(crate) fn walk_file<W: FlatWalk>(
    w: &mut W,
    source: &str,
    file: FileId,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    stack: &mut Vec<FileId>,
    sem: &WalkSemantics,
) -> Result<(), AsmError> {
    // Each file expands on its own (#93): a macro does not reach across an
    // include boundary, which is a deliberate hold rather than a measured
    // answer. This is an assembly path, so it always expands — the formatter
    // parses through `parse_program`, which asks separately.
    let expanded = w.expand_source(source)?;
    let text = macros::expanded_text(&expanded, source);
    let origins = macros::line_origins(&expanded);
    let lines: Vec<&str> = text.lines().collect();
    let mut pos = 0usize;
    let closer = walk_block(
        Cursor {
            lines: &lines,
            pos: &mut pos,
            file,
            origins,
        },
        w,
        &mut Resolve::Lazily {
            map,
            loader,
            stack,
            sem,
        },
        false,
    )?;
    debug_assert!(closer == BlockClose::Eof, "a file only ends at EOF");
    Ok(())
}

/// Parse one source with the same cursor, keeping every include unresolved —
/// the single-source entry every dialect's `parse_program` uses, and the path
/// the formatter takes.
pub(crate) fn walk_source_expanded<W: FlatWalk>(
    w: &mut W,
    text: &str,
    file: FileId,
) -> Result<(), AsmError> {
    let origins: Option<&[macros::LineOrigin]> = None;
    let lines: Vec<&str> = text.lines().collect();
    let mut pos = 0usize;
    let closer = walk_block(
        Cursor {
            lines: &lines,
            pos: &mut pos,
            file,
            origins,
        },
        w,
        &mut Resolve::Never,
        false,
    )?;
    debug_assert!(closer == BlockClose::Eof, "a file only ends at EOF");
    Ok(())
}

/// The lines a [`walk_block`] leg reads, and where it is up to.
struct Cursor<'a, 'b> {
    lines: &'a [&'a str],
    pos: &'b mut usize,
    file: FileId,
    origins: Option<&'a [macros::LineOrigin]>,
}

/// Parse lines into `w` until this leg's closer or EOF, grouping any block a
/// dialect declares into an [`Item::Conditional`](crate::ast::Item) or
/// [`Item::Repeat`](crate::ast::Item).
///
/// The cursor is **structural only**: it groups a head, a body and a closer,
/// and never folds a condition. Which branch assembles is the dialect's
/// projection or evaluator to decide, from the tree — see
/// `decisions/conditionals-in-multipass-dialects.md` and
/// `decisions/conditional-assembly-framework.md`. A dialect that declares no
/// keywords never leaves the `None` arm and walks exactly as it did before.
///
/// `in_block` is false only at the top level, where a stray closer is a line
/// like any other rather than the end of anything — lwasm accepts one and
/// ca65 rejects it, so the *diagnostic* stays with the dialect.
#[allow(clippy::too_many_arguments)]
fn walk_block<W: FlatWalk>(
    cx: Cursor<'_, '_>,
    w: &mut W,
    res: &mut Resolve<'_>,
    in_block: bool,
) -> Result<BlockClose, AsmError> {
    let Cursor {
        lines,
        pos,
        file,
        origins,
    } = cx;
    while *pos < lines.len() {
        let raw = lines[*pos];
        let line = *pos + 1;
        *pos += 1;

        // Structure first, so a block keyword never reaches the line parser —
        // which would refuse it as an unknown directive.
        if let Some(kw) = w.block_keyword(strip_block_comment(raw)) {
            let head = strip_block_comment(raw).trim().to_string();
            match kw {
                BlockKw::CondClose if in_block => return Ok(BlockClose::CondClose(head)),
                BlockKw::RepeatClose if in_block => return Ok(BlockClose::RepeatClose(head)),
                BlockKw::Else if in_block => return Ok(BlockClose::Else),
                BlockKw::ElseIf if in_block => return Ok(BlockClose::ElseIf(head, line)),
                BlockKw::CondOpen => {
                    parse_conditional(
                        Cursor {
                            lines,
                            pos,
                            file,
                            origins,
                        },
                        w,
                        res,
                        head,
                        line,
                    )?;
                    continue;
                }
                BlockKw::RepeatOpen => {
                    parse_repeat(
                        Cursor {
                            lines,
                            pos,
                            file,
                            origins,
                        },
                        w,
                        res,
                        head,
                        line,
                    )?;
                    continue;
                }
                // A closer at the top level, or one this leg does not expect.
                // Handed to the line parser, which is where each dialect's own
                // posture lives: ca65 errors, lwasm shrugs.
                _ => {}
            }
        }

        walk_one_line(w, raw, line, file, origins, res)?;
    }
    Ok(BlockClose::Eof)
}

/// Collect a conditional: the head line, each leg's body, and the closer.
#[allow(clippy::too_many_arguments)]
fn parse_conditional<W: FlatWalk>(
    cx: Cursor<'_, '_>,
    w: &mut W,
    res: &mut Resolve<'_>,
    head: String,
    line: usize,
) -> Result<(), AsmError> {
    let Cursor {
        lines,
        pos,
        file,
        origins,
    } = cx;
    let start = w.nodes_mut().len();
    let closed = walk_block(
        Cursor {
            lines,
            pos,
            file,
            origins,
        },
        w,
        res,
        true,
    )?;
    let then_body: Vec<Node> = w.nodes_mut().split_off(start);
    let mut closer = String::new();
    let else_body = match closed {
        BlockClose::CondClose(text) => {
            closer = text;
            None
        }
        BlockClose::Else => {
            let start = w.nodes_mut().len();
            let end = walk_block(
                Cursor {
                    lines,
                    pos,
                    file,
                    origins,
                },
                w,
                res,
                true,
            )?;
            match end {
                BlockClose::CondClose(text) => closer = text,
                BlockClose::Eof => {}
                _ => {
                    return Err(AsmError::at(
                        Span::in_file(file, line as u32, 1),
                        "conditional block is never closed".to_string(),
                    ));
                }
            }
            Some(w.nodes_mut().split_off(start))
        }
        // An `elseif` leg is stored as a nested conditional in the else branch,
        // which is the shape the evaluator walks and `emit` flattens back.
        BlockClose::ElseIf(leg_head, leg_line) => {
            let start = w.nodes_mut().len();
            parse_conditional(
                Cursor {
                    lines,
                    pos,
                    file,
                    origins,
                },
                w,
                res,
                leg_head,
                leg_line,
            )?;
            Some(w.nodes_mut().split_off(start))
        }
        BlockClose::Eof => {
            return Err(AsmError::at(
                Span::in_file(file, line as u32, 1),
                "conditional block is never closed".to_string(),
            ));
        }
        BlockClose::RepeatClose(_) => {
            return Err(AsmError::at(
                Span::in_file(file, line as u32, 1),
                "a repetition closer ends a conditional block".to_string(),
            ));
        }
    };
    w.push_node(Node {
        operand_span: None,
        label: None,
        item: Some(crate::ast::Item::Conditional {
            close: closer,
            head: head.clone(),
            then_body,
            else_body,
            inline: false,
            style: crate::ast::CondStyle::Keyword,
        }),
        source: head,
        span: Span::in_file(file, line as u32, 1),
        trivia: Trivia::default(),
    });
    Ok(())
}

/// Collect a repetition: the head line, the body, and the closer.
#[allow(clippy::too_many_arguments)]
fn parse_repeat<W: FlatWalk>(
    cx: Cursor<'_, '_>,
    w: &mut W,
    res: &mut Resolve<'_>,
    head: String,
    line: usize,
) -> Result<(), AsmError> {
    let Cursor {
        lines,
        pos,
        file,
        origins,
    } = cx;
    let start = w.nodes_mut().len();
    let closed = walk_block(
        Cursor {
            lines,
            pos,
            file,
            origins,
        },
        w,
        res,
        true,
    )?;
    let BlockClose::RepeatClose(close) = closed else {
        return Err(AsmError::at(
            Span::in_file(file, line as u32, 1),
            "repetition block is never closed".to_string(),
        ));
    };
    let body: Vec<Node> = w.nodes_mut().split_off(start);
    w.push_node(Node {
        operand_span: None,
        label: None,
        item: Some(crate::ast::Item::Repeat {
            head: head.clone(),
            body,
            close,
            style: crate::ast::CondStyle::Keyword,
        }),
        source: head,
        span: Span::in_file(file, line as u32, 1),
        trivia: Trivia::default(),
    });
    Ok(())
}

/// A line's comment removed for the structural test only. The dialect's own
/// comment rules still apply when the line is parsed; this needs just enough to
/// stop a trailing comment hiding a closer.
fn strip_block_comment(raw: &str) -> &str {
    let cut = raw.find(';').unwrap_or(raw.len());
    &raw[..cut]
}

/// Parse one ordinary line, resolving an include or incbin as it is reached.
#[allow(clippy::too_many_arguments)]
fn walk_one_line<W: FlatWalk>(
    w: &mut W,
    raw: &str,
    line: usize,
    file: FileId,
    origins: Option<&[macros::LineOrigin]>,
    res: &mut Resolve<'_>,
) -> Result<(), AsmError> {
    {
        // Nodes this line contributes, put back on the line the author wrote
        // before an include pushes any of its own.
        let start = w.nodes_mut().len();
        let walked = w
            .walk_line(raw, line, file)
            .map_err(|e| stamp_file(macros::remap_lines(e, origins), file));
        macros::place_nodes(&mut w.nodes_mut()[start..], origins);
        let Some(mut d) = walked? else {
            return Ok(());
        };
        if let Some(origins) = origins {
            macros::place(&mut d.span, origins);
            if let Some(span) = d.operand_span.as_mut() {
                macros::place(span, origins);
            }
        }
        if !matches!(res, Resolve::Lazily { .. }) {
            // Single-source: the target is never opened (KTD1).
            w.push_node(unresolved_node(d));
            return Ok(());
        }
        let span = d.span;
        // Diagnostics point at the directive's operand (the file name) when
        // the parse knew its column, else the line.
        let at = d.operand_span.clone().unwrap_or_else(|| span.clone());
        let Resolve::Lazily {
            map,
            loader,
            stack,
            sem,
        } = res
        else {
            unreachable!("checked above")
        };
        match d.kind {
            WalkDirective::Include { request } => {
                // A label on the include line binds at the include point's
                // address (probe-pinned), so it becomes a label-only node
                // before the target's lines.
                if d.label.is_some() {
                    w.push_node(Node {
                        operand_span: None,
                        label: d.label,
                        item: None,
                        source: String::new(),
                        span,
                        trivia: d.trivia,
                    });
                }
                if stack.len() >= MAX_INCLUDE_DEPTH {
                    return Err(AsmError::at(
                        at,
                        format!("includes nested more than {MAX_INCLUDE_DEPTH} levels deep"),
                    ));
                }
                let id = load_include_defaulted(map, *loader, &request, stack, line as u32, sem)
                    .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
                // Cycle detection is membership of the *active* stack: ca65
                // itself has none (a self-include dies on the OS's open-file
                // limit), so this diagnostic exceeds the reference — allowed,
                // diagnostics are not byte-compared (KTD5).
                if stack.contains(&id) {
                    let chain = stack
                        .iter()
                        .chain(std::iter::once(&id))
                        .map(|f| map.path(*f).unwrap_or("?"))
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    return Err(AsmError::at(at, format!("include cycle: {chain}")));
                }
                let contents = map.contents(id).unwrap_or_default().to_owned();
                stack.push(id);
                walk_file(w, &contents, id, map, *loader, stack, sem)?;
                stack.pop();
            }
            WalkDirective::Incbin {
                request,
                offset,
                size,
            } => {
                // Resolved lazily, exactly like an include (KTD1). The binary
                // path mints no FileId (KTD8) — the payload rides a node at
                // the *directive's* span, which is where the missing-asset /
                // window diagnostics land too.
                let data = load_binary(map, *loader, &request, stack, sem.resolution)
                    .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
                let payload = (sem.window)(&data, offset, size)
                    .map_err(|msg| AsmError::at(at.clone(), format!("`{request}`: {msg}")))?;
                w.push_node(Node {
                    operand_span: d.operand_span,
                    label: d.label,
                    item: Some(crate::ast::Item::Binary(payload)),
                    source: d.source,
                    span,
                    trivia: d.trivia,
                });
            }
        }
    }
    Ok(())
}

/// The unresolved node a **single-source** parse keeps for a walk-handled
/// directive: the target is never opened (KTD1), so `--fmt` renders the
/// verbatim source and works with a missing file, and `lower` rejects
/// assembly with a pointer to the multi-file entry points.
pub(crate) fn unresolved_node(d: DirectiveLine) -> Node {
    let item = match d.kind {
        WalkDirective::Include { request } => crate::ast::Item::Include { request },
        WalkDirective::Incbin { request, .. } => crate::ast::Item::Incbin { request },
    };
    Node {
        operand_span: d.operand_span,
        label: d.label,
        item: Some(item),
        source: d.source,
        span: d.span,
        trivia: d.trivia,
    }
}

/// Stamp `file` onto a per-line parse error: the line-oriented dialect helpers
/// know their line but not their file, so the walk supplies it at the one
/// per-line boundary (the z80 walk's rule). `pub(crate)` because the ca65-NES
/// assemble+link driver stamps its post-parse layout/emit errors (duplicate
/// symbol, range failures) with the owning statement's file the same way (U5).
pub(crate) fn stamp_file(mut e: AsmError, file: FileId) -> AsmError {
    match &mut e.span {
        Some(span) => span.file = file,
        None if e.line != 0 => e.span = Some(Span::in_file(file, e.line as u32, 0)),
        None => {}
    }
    e
}

/// Apply the dialect's include extension default before resolving: an
/// extensionless request tries the defaulted spelling (`request.inc`) first
/// and the exact spelling second — asl's probe-pinned order (with both
/// `bare` and `bare.inc` present, `.inc` wins; with only `bare`, it is
/// found). The fallback is gated on **resolution**, not on the load
/// succeeding: a defaulted candidate that exists but cannot be read (a
/// directory, a permission failure) is an error naming the defaulted
/// spelling, never a silent fall-through to the exact name. A
/// does-not-resolve failure reports the request **as written**. Dialects
/// without defaulting resolve the request directly.
fn load_include_defaulted(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    request: &str,
    stack: &[FileId],
    line: u32,
    sem: &WalkSemantics,
) -> Result<FileId, LoadError> {
    if let Some(ext) = sem.include_default_ext
        && std::path::Path::new(request).extension().is_none()
    {
        let defaulted = format!("{request}.{ext}");
        if include_resolves(map, loader, &defaulted, stack, sem.resolution) {
            return load_include(map, loader, &defaulted, stack, line, sem.resolution);
        }
    }
    load_include(map, loader, request, stack, line, sem.resolution)
}

/// Whether `request` resolves under the dialect's [`Resolution`] anchor —
/// the same probe order [`load_include`] loads through, minus the read.
/// Existence only: an unreadable-but-existing target still resolves, so its
/// load error surfaces instead of being swallowed.
fn include_resolves(
    map: &SourceMap,
    loader: &dyn SourceLoader,
    request: &str,
    stack: &[FileId],
    resolution: Resolution,
) -> bool {
    let requester = stack.last().copied().unwrap_or(FileId(0));
    let requester_path = map.path(requester).map(str::to_owned);
    match resolution {
        Resolution::Requester => loader
            .resolve_text(request, requester_path.as_deref())
            .is_some(),
        Resolution::Root => {
            let from = if requester == FileId(0) {
                requester_path
            } else {
                map.path(FileId(0)).map(str::to_owned)
            };
            loader.resolve_text(request, from.as_deref()).is_some()
        }
        Resolution::AncestorChain => std::iter::once(requester_path)
            .chain(
                stack
                    .iter()
                    .rev()
                    .skip(1)
                    .map(|&ancestor| map.path(ancestor).map(str::to_owned)),
            )
            .any(|from| loader.resolve_text(request, from.as_deref()).is_some()),
    }
}

/// Resolve an include per the dialect's probe-pinned [`Resolution`]. The
/// include-graph edge always names the *true* requester — a non-requester
/// anchor (an ancestor hop, the root anchor) re-requests by the canonical
/// path it resolved, so the `included from` notes stay honest — and a failure
/// is reported as the requester's own (it names the request as written and
/// the file that asked).
fn load_include(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    request: &str,
    stack: &[FileId],
    line: u32,
    resolution: Resolution,
) -> Result<FileId, LoadError> {
    let requester = stack.last().copied().unwrap_or(FileId(0));
    match resolution {
        Resolution::Requester => map.load(loader, request, requester, line),
        Resolution::Root => {
            if requester == FileId(0) {
                return map.load(loader, request, requester, line);
            }
            let root = map.path(FileId(0)).map(str::to_owned);
            // Resolve against the root anchor without paying for a read the
            // registration below would repeat; a miss falls through to the
            // full load for its error message.
            if let Some(canonical) = loader.resolve_text(request, root.as_deref()) {
                return map.load(loader, &canonical, requester, line);
            }
            match loader.load_text(request, root.as_deref()) {
                Ok((canonical, _)) => map.load(loader, &canonical, requester, line),
                Err(mut e) => {
                    // Name the file whose directive failed, not the anchor.
                    e.from = map.path(requester).map(str::to_owned);
                    Err(e)
                }
            }
        }
        Resolution::AncestorChain => {
            let first = map.load(loader, request, requester, line);
            let Err(first_err) = first else {
                return first;
            };
            for &ancestor in stack.iter().rev().skip(1) {
                let from = map.path(ancestor).map(str::to_owned);
                if let Some(canonical) = loader.resolve_text(request, from.as_deref()) {
                    return map.load(loader, &canonical, requester, line);
                }
            }
            // Every hop failed: report the requester's own failure.
            Err(first_err)
        }
    }
}

/// Resolve an incbin asset through the same [`Resolution`] as
/// [`load_include`] (KTD8: include and incbin can never fork resolution
/// behaviour — probe-confirmed for all three anchors). No `FileId` is minted
/// — binary data has no spans.
fn load_binary(
    map: &SourceMap,
    loader: &dyn SourceLoader,
    request: &str,
    stack: &[FileId],
    resolution: Resolution,
) -> Result<Vec<u8>, LoadError> {
    let requester = stack.last().copied().unwrap_or(FileId(0));
    let requester_path = map.path(requester).map(str::to_owned);
    match resolution {
        Resolution::Requester => loader.load_binary(request, requester_path.as_deref()),
        Resolution::Root => {
            let root = map.path(FileId(0)).map(str::to_owned);
            loader
                .load_binary(request, root.as_deref())
                .map_err(|mut e| {
                    e.from = requester_path;
                    e
                })
        }
        Resolution::AncestorChain => {
            let first = loader.load_binary(request, requester_path.as_deref());
            let Err(first_err) = first else {
                return first;
            };
            for &ancestor in stack.iter().rev().skip(1) {
                let from = map.path(ancestor).map(str::to_owned);
                if let Ok(bytes) = loader.load_binary(request, from.as_deref()) {
                    return Ok(bytes);
                }
            }
            Err(first_err)
        }
    }
}

/// The file name of an include directive (`directive` names it in errors):
/// a quoted string is required (ca65: "String constant expected"; rgbasm:
/// "is not a string symbol") and anything after the closing quote is
/// rejected (ca65 errors; rgbasm: "syntax error") — probe-pinned, mirrored.
pub(crate) fn include_request(
    args: &str,
    line: usize,
    directive: &str,
) -> Result<String, AsmError> {
    let (name, rest) = quoted_name(args, line, directive)?;
    if !rest.trim().is_empty() {
        return Err(AsmError::new(
            line,
            format!(
                "unexpected `{}` after the `{directive}` file name",
                rest.trim()
            ),
        ));
    }
    Ok(name)
}

/// Parse an incbin directive's arguments (`directive` names it in errors):
/// the quoted file name, then an optional `, offset[, size]` tail of
/// parse-time constant expressions. `fold` is the dialect's expression
/// parser and constant folder over its live environment (a forward reference
/// fails — ca65's "Constant expression expected"; rgbasm's "Expected
/// constant expression: undefined symbol").
pub(crate) fn incbin_args(
    args: &str,
    line: usize,
    directive: &str,
    fold: &dyn Fn(&str) -> Result<i64, AsmError>,
) -> Result<(String, Option<i64>, Option<i64>), AsmError> {
    let (name, rest) = quoted_name(args, line, directive)?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((name, None, None));
    }
    let Some(tail) = rest.strip_prefix(',') else {
        return Err(AsmError::new(
            line,
            format!("expected `,offset[,size]` after the `{directive}` file name, found `{rest}`"),
        ));
    };
    let pieces = super::mos6502::split_top_level(tail, ',');
    if pieces.len() > 2 {
        return Err(AsmError::new(
            line,
            format!("`{directive}` takes at most a file name, an offset, and a size"),
        ));
    }
    let fold_arg = |what: &str, piece: &str| -> Result<i64, AsmError> {
        fold(piece).map_err(|e| {
            AsmError::new(
                line,
                format!(
                    "`{directive}` {what} must be a constant expression: {}",
                    e.message
                ),
            )
        })
    };
    let offset = fold_arg("offset", pieces[0])?;
    let size = pieces.get(1).map(|p| fold_arg("size", p)).transpose()?;
    Ok((name, Some(offset), size))
}

/// The quoted file name a ca65 directive requires, and whatever follows the
/// closing quote (the caller decides what the tail may hold).
fn quoted_name<'a>(
    args: &'a str,
    line: usize,
    directive: &str,
) -> Result<(String, &'a str), AsmError> {
    let t = args.trim();
    let Some(inner) = t.strip_prefix('"') else {
        return Err(AsmError::new(
            line,
            format!("`{directive}` needs a quoted file name"),
        ));
    };
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
    Ok((name.to_string(), &inner[end + 1..]))
}

/// Apply ca65's `.incbin` window to the loaded asset — probe-pinned (see the
/// module docs): negative offset and any window past EOF are errors; a
/// missing **or negative** size means the rest of the file; offset at EOF or
/// size 0 are legal and empty. `Err` carries the message body; the caller
/// wraps it with the request name and the directive's span.
fn slice_incbin(data: &[u8], offset: Option<i64>, size: Option<i64>) -> Result<Vec<u8>, String> {
    let len = data.len() as i64;
    let off = offset.unwrap_or(0);
    if off < 0 {
        return Err(format!("offset {off} must not be negative"));
    }
    if off > len {
        return Err(format!(
            "offset {off} is past the end of the {len}-byte file"
        ));
    }
    let remaining = len - off;
    let take = match size {
        // ca65 reads any negative size as the "unspecified" sentinel — the
        // rest of the file (probe-pinned: `, 2, -2` on an 8-byte file emitted
        // all 6 remaining bytes).
        None => remaining,
        Some(s) if s < 0 => remaining,
        Some(s) => s,
    };
    if take > remaining {
        return Err(format!(
            "size {take} exceeds the {remaining} byte(s) after offset {off}"
        ));
    }
    Ok(data[off as usize..(off + take) as usize].to_vec())
}

/// The operand-field span of a directive line, stamped with its file — the
/// z80 walk's rule, so directive diagnostics point at the file-name operand.
/// `rest` must borrow from `raw` (see [`crate::ast::operand_span`]).
pub(crate) fn directive_operand_span(
    raw: &str,
    rest: &str,
    line: usize,
    file: FileId,
) -> Option<Span> {
    crate::ast::operand_span(raw, rest, line as u32).map(|mut s| {
        s.file = file;
        s
    })
}

// ---------------------------------------------------------------------------
// Macros (#93)
//
// The mechanics live in [`crate::dialects::macros`]; this is the ca65 family's
// grammar, measured against ca65 V2.18. One grammar serves ca65, ca65-816 and
// ca65-huc6280, because cc65 ships one assembler and the CPU is a flag.
//
// ca65 agrees with sjasmplus on the header shape — the name is followed by
// *space*, not a comma, and `.macro m1, v` is `Unexpected trailing garbage
// characters` — and with pasmo on locals, which must be declared. Its arity
// posture is a third one again:
//
//   * **too many** arguments is an error, `Too many macro parameters`;
//   * **too few** substitutes empty, and the complaint arrives from whatever
//     the emptied operand broke — or not at all, if the missing parameter is
//     not reached (`.macro m1 v, w` invoked `m1 9` assembles, as long as `w`
//     appears on no line that emits).
//
// Three dialects, three postures. That is why `fit_arguments` has no default.
// ---------------------------------------------------------------------------

/// The ca65 family's macro grammar.
pub(crate) struct Ca65Macros;

impl macros::MacroSyntax for Ca65Macros {
    /// `.macro name [p1[, p2]...]`, or the `.mac` short spelling. The leading
    /// dot is required; the keyword is matched case-insensitively and the name
    /// is kept as written, since ca65 rejects a mis-cased call.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        let text = macros::without_comment(line).trim();
        let (kw, rest) = text.split_once(char::is_whitespace)?;
        if !(kw.eq_ignore_ascii_case(".macro") || kw.eq_ignore_ascii_case(".mac")) {
            return None;
        }
        let rest = rest.trim();
        let (name, params) = match rest.split_once(char::is_whitespace) {
            Some((name, tail)) => (name.trim(), name_list(tail)),
            None => (rest, Vec::new()),
        };
        (!name.is_empty()).then(|| (name.to_string(), params))
    }

    fn is_end(&self, line: &str) -> bool {
        let text = macros::without_comment(line).trim();
        text.eq_ignore_ascii_case(".endmacro") || text.eq_ignore_ascii_case(".endmac")
    }

    fn end_keyword(&self) -> &'static str {
        ".endmacro"
    }

    /// The names `.local` declares. Nothing else is local: a plain label in a
    /// body is global, and a second expansion gets `Symbol 'spin' is already
    /// defined`.
    fn locals(&self, body: &[String]) -> Vec<String> {
        let mut names = Vec::new();
        for line in body {
            let Some(declared) = local_declaration(line) else {
                continue;
            };
            for name in declared {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// A `.local` line declares; it does not assemble, so it leaves the body.
    fn is_local_decl(&self, line: &str) -> bool {
        local_declaration(line).is_some()
    }

    /// Too many is an error and too few is not — see the note above.
    fn fit_arguments(
        &self,
        name: &str,
        params: &[String],
        mut args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        if args.len() > params.len() {
            return Err(format!("too many parameters for macro `{name}`"));
        }
        args.resize(params.len(), String::new());
        Ok(args)
    }
}

/// The names a `.local` line declares, or `None` if the line is not one.
fn local_declaration(line: &str) -> Option<Vec<String>> {
    let text = macros::without_comment(line).trim();
    let (kw, rest) = text.split_once(char::is_whitespace)?;
    kw.eq_ignore_ascii_case(".local")
        .then(|| name_list(rest))
        .filter(|names| !names.is_empty())
}

/// A comma-separated name list, empties dropped — shared by macro parameters
/// and `.local` declarations, which ca65 spells the same way.
fn name_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Expand the ca65 family's macros, unless this parse is the formatter's.
pub(crate) fn expand_ca65(
    source: &str,
    mode: macros::Expand,
) -> Result<macros::Expansion, AsmError> {
    macros::expansion(mode, source, |s| {
        macros::expand(&Ca65Macros, s).map(|e| Some((e.text, e.origins)))
    })
}

/// ca65's expression functions. Only the three byte extractions so far — they
/// are the ones the shared `Expr` already has nodes for, so they cost nothing
/// beyond the name.
///
/// An unknown name is refused here rather than left to resolve as a symbol: in
/// ca65 a `.`-prefixed word is never an ordinary identifier, so `.zzz(1)` is a
/// typo for a function and saying so beats "undefined symbol `.zzz`".
pub(crate) fn expr_function(
    name: &str,
    args: Vec<super::mos6502::ExprArg>,
    line: usize,
) -> Result<crate::engine::Expr, AsmError> {
    use crate::engine::BinOp as Op;
    use crate::engine::Expr;
    let lower = name.to_ascii_lowercase();

    // The string functions. A string is consumed here and yields a number, so
    // an expression still evaluates to an `i64` — see `ExprArg`.
    match lower.as_str() {
        ".strlen" => {
            let [t]: [_; 1] = take(name, args, 1, line)?;
            return Ok(Expr::Num(t.text(name, line)?.chars().count() as i64));
        }
        ".strat" => {
            let [t, i]: [_; 2] = take(name, args, 2, line)?;
            let text = t.text(name, line)?;
            let Expr::Num(idx) = i.value(name, line)? else {
                return Err(AsmError::new(line, "`.strat` index must be a constant"));
            };
            let ch = usize::try_from(idx)
                .ok()
                .and_then(|n| text.chars().nth(n))
                .ok_or_else(|| {
                    AsmError::new(line, format!("`.strat` index {idx} is past the string"))
                })?;
            return Ok(Expr::Num(ch as i64));
        }
        _ => {}
    }

    // The two-argument value functions, before the one-argument ones claim `args`.
    let pair = match lower.as_str() {
        ".max" => Some(Op::Max),
        ".min" => Some(Op::Min),
        _ => None,
    };
    if let Some(op) = pair {
        let [a, b]: [_; 2] = take(name, args, 2, line)?;
        return Ok(Expr::Bin(
            op,
            Box::new(a.value(name, line)?),
            Box::new(b.value(name, line)?),
        ));
    }
    let [arg]: [_; 1] = take(name, args, 1, line)?;
    let arg = arg.value(name, line)?;
    use crate::engine::BinOp;
    // The word extractions have no node of their own and need none: masking
    // and shifting say the same thing with the arithmetic already in `Expr`.
    let mask = |e: Expr| Expr::Bin(BinOp::And, Box::new(e), Box::new(Expr::Num(0xFFFF)));
    match name.to_ascii_lowercase().as_str() {
        ".loword" => return Ok(mask(arg)),
        ".hiword" => {
            return Ok(mask(Expr::Bin(
                BinOp::Shr,
                Box::new(arg),
                Box::new(Expr::Num(16)),
            )));
        }
        _ => {}
    }
    let wrap: fn(Box<Expr>) -> Expr = match name.to_ascii_lowercase().as_str() {
        ".lobyte" => Expr::Lo,
        ".hibyte" => Expr::Hi,
        ".bankbyte" => Expr::Bank,
        _ if name.starts_with('.') => {
            return Err(AsmError::new(
                line,
                format!(
                    "`{name}` is not an expression function asm198x implements yet —                      the source is valid and the gap is ours"
                ),
            ));
        }
        // Not a ca65 function name at all: a plain symbol the source then
        // parenthesised, which is its own error further up.
        _ => return Err(AsmError::new(line, format!("`{name}` is not a function"))),
    };
    Ok(wrap(Box::new(arg)))
}

/// Exactly `n` arguments, or a diagnostic naming the function.
fn take<const N: usize>(
    name: &str,
    args: Vec<super::mos6502::ExprArg>,
    n: usize,
    line: usize,
) -> Result<[super::mos6502::ExprArg; N], AsmError> {
    args.try_into().map_err(|_| {
        let plural = if n == 1 { "argument" } else { "arguments" };
        AsmError::new(line, format!("`{name}` takes {n} {plural}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_incbin_matches_the_probe_matrix() {
        let data: &[u8] = &[0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];
        // Plain, offset, offset+size — the happy windows.
        assert_eq!(
            slice_incbin(data, None, None).expect("window"),
            data.to_vec()
        );
        assert_eq!(
            slice_incbin(data, Some(2), None).expect("window"),
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17]
        );
        assert_eq!(
            slice_incbin(data, Some(2), Some(3)).expect("window"),
            vec![0x12, 0x13, 0x14]
        );
        // Offset at EOF and size 0 are legal and empty (probe-pinned).
        assert_eq!(
            slice_incbin(data, Some(8), None).expect("window"),
            Vec::<u8>::new()
        );
        assert_eq!(
            slice_incbin(data, Some(0), Some(0)).expect("window"),
            Vec::<u8>::new()
        );
        // A negative size is ca65's "rest of the file" sentinel (probe-pinned).
        assert_eq!(
            slice_incbin(data, Some(2), Some(-2)).expect("window"),
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17]
        );
        assert_eq!(
            slice_incbin(data, Some(6), Some(-9)).expect("window"),
            vec![0x16, 0x17]
        );
        // The error postures: offset past EOF, size past remaining, negative
        // offset (ca65: "Range error" / a read error; ours name the numbers).
        assert!(slice_incbin(data, Some(9), None).is_err());
        assert!(slice_incbin(data, Some(6), Some(4)).is_err());
        assert!(slice_incbin(data, Some(-2), None).is_err());
    }

    #[test]
    fn quoted_name_requires_the_string_form() {
        assert!(include_request(" \"a.s\" ", 1, ".include").is_ok());
        // Unquoted (ca65: "String constant expected") and trailing junk (ca65
        // errors) are both rejected.
        assert!(include_request(" a.s", 1, ".include").is_err());
        assert!(include_request(" \"a.s\" junk", 1, ".include").is_err());
        assert!(include_request(" \"a.s", 1, ".include").is_err());
        assert!(include_request(" \"\"", 1, ".include").is_err());
    }
}
