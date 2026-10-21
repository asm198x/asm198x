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

use std::borrow::Cow;
use std::collections::BTreeMap;

use super::ca65::string_tokens::StringTokens;
use crate::ast::{Node, Span, Symbol, Trivia};
use crate::dialects::macros;
use crate::dialects::text;
use crate::engine::AsmError;
use crate::source::{LoadError, MAX_INCLUDE_DEPTH, SourceLoader, SourceMap};
use crate::span::FileId;

/// ca65 also admits digit-starting hexadecimal tokens ending in h/H.
/// Native scanner.c bounds those tokens independently of expression arithmetic.
pub(crate) fn parse_number(token: &str, line: usize) -> Result<i64, AsmError> {
    let token = token.trim();
    let (digits, radix) = if let Some(digits) = token.strip_prefix('$') {
        (digits, 16)
    } else if let Some(digits) = token.strip_prefix('%') {
        (digits, 2)
    } else if token.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        if let Some(digits) = token.strip_suffix(['h', 'H']) {
            (digits, 16)
        } else {
            (token, 10)
        }
    } else {
        // Character constants retain the shared parser's existing semantics.
        return super::mos6502::parse_number(token, line);
    };
    let invalid = || AsmError::new(line, format!("invalid number `{token}`"));
    if digits.is_empty() {
        return Err(invalid());
    }
    // Native scanner.c checks the previous partial value, before the next
    // multiply/add. Decimal consequently admits 4294967296 through 4294967299;
    // a final u32 conversion would incorrectly refuse those native tokens.
    let limit = i64::from(u32::MAX) / i64::from(radix);
    let mut value = 0i64;
    for digit in digits.chars() {
        if value > limit {
            return Err(AsmError::new(
                line,
                format!("number `{token}` is outside the native lexical range"),
            ));
        }
        let digit = digit.to_digit(radix).ok_or_else(invalid)?;
        value = value * i64::from(radix) + i64::from(digit);
    }
    Ok(value)
}

/// Check native numeric and quoted tokens without interpreting source meaning.
pub(crate) fn validate_lexical_tokens(source: &str, at: &Span) -> Result<(), AsmError> {
    StringTokens::reject_authored(source, at.line as usize)
        .map_err(|error| AsmError::at(at.clone(), error.message))?;
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        match bytes[i] {
            b';' => break,
            b'"' => {
                i += 1;
                while i < bytes.len() && !matches!(bytes[i], b'"' | b'\n') {
                    i += 1;
                }
                if bytes.get(i) != Some(&b'"') {
                    let mut span = at.clone();
                    span.col = start as u32 + 1;
                    return Err(AsmError::at(span, "unterminated string literal"));
                }
                i += 1;
                continue;
            }
            b'\'' => {
                // Native characters contain exactly one printable ASCII byte, including
                // a raw backslash or apostrophe, followed by the closing quote.
                if !matches!(bytes.get(i + 1), Some(b' '..=b'~'))
                    || bytes.get(i + 2) != Some(&b'\'')
                {
                    let mut span = at.clone();
                    span.col = start as u32 + 1;
                    return Err(AsmError::at(span, "malformed character literal"));
                }
                i += 3;
                continue;
            }
            b'$' | b'%' => {
                let binary = bytes[i] == b'%';
                i += 1;
                while i < bytes.len()
                    && if binary {
                        matches!(bytes[i], b'0' | b'1')
                    } else {
                        bytes[i].is_ascii_hexdigit()
                    }
                {
                    i += 1;
                }
            }
            b'0'..=b'9' => {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if matches!(bytes.get(i), Some(b'h' | b'H')) {
                    i += 1;
                }
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'@' | b'.' => {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        parse_number(&source[start..i], at.line as usize).map_err(|error| {
            let mut span = at.clone();
            span.col = start as u32 + 1;
            AsmError::at(span, error.message)
        })?;
    }
    Ok(())
}

/// Split native fields while protecting quoted tokens and nested expressions.
/// Assertions split only their two header separators; data lists split all.
pub(crate) fn split_fields(rest: &str, max_splits: usize) -> Vec<&str> {
    let bytes = rest.as_bytes();
    let mut fields = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                i += usize::from(bytes.get(i) == Some(&b'"'));
                continue;
            }
            b'\'' => {
                i = (i + 2).min(bytes.len());
                i += usize::from(bytes.get(i) == Some(&b'\''));
                continue;
            }
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                fields.push(rest[start..i].trim());
                start = i + 1;
                if fields.len() == max_splits {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    fields.push(rest[start..].trim());
    fields
}

/// Bind `*` to this data element while the engine keeps statement-start PCs.
/// The authored source stays on the AST node for canonical formatting.
pub(crate) fn data_element(
    mut value: crate::engine::Expr,
    index: usize,
    width: usize,
    line: usize,
) -> Result<crate::engine::Expr, AsmError> {
    use crate::engine::{BinOp, Expr};

    fn bind(value: &mut Expr, offset: i64) {
        match value {
            Expr::Pc => {
                *value = Expr::Bin(BinOp::Add, Box::new(Expr::Pc), Box::new(Expr::Num(offset)));
            }
            Expr::Num(_) | Expr::Sym(_) => {}
            Expr::Lo(inner)
            | Expr::Hi(inner)
            | Expr::Bank(inner)
            | Expr::Neg(inner)
            | Expr::BitNot(inner)
            | Expr::FixRound(inner)
            | Expr::TrailingZeros(inner)
            | Expr::LogNot(inner) => bind(inner, offset),
            Expr::Bin(_, left, right) => {
                bind(left, offset);
                bind(right, offset);
            }
        }
    }

    let offset = index
        .checked_mul(width)
        .and_then(|offset| i64::try_from(offset).ok())
        .ok_or_else(|| AsmError::new(line, "data list is too large"))?;
    if offset != 0 {
        bind(&mut value, offset);
    }
    Ok(value)
}

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
    /// Apply a dialect's live, line-oriented source state before structural
    /// block recognition. Returning `None` consumes a source-only directive.
    /// The same walker instance crosses include boundaries, so state stored by
    /// an implementation naturally follows textual-include semantics.
    fn preprocess_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<String>, AsmError> {
        let _ = (line, file);
        Ok(Some(raw.to_string()))
    }

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

    /// Split an optional label from a block-opening line after
    /// [`block_keyword`](Self::block_keyword) has recognised it.
    ///
    /// Most keyword-block dialects do not permit a label here. ca65 does, so
    /// its walker returns the symbol to attach to the block node and the head
    /// text that begins with the directive itself.
    fn block_open<'a>(
        &mut self,
        code: &'a str,
        line: usize,
    ) -> Result<(Option<Symbol>, &'a str), AsmError> {
        let _ = line;
        Ok((None, code.trim()))
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
    let expanded = w
        .expand_source(source)
        .map_err(|error| stamp_file(error, file))?;
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

/// Block heads need the same authored origin as ordinary lines. In the
/// multi-file walk, ordinary lines are placed as they are read, while block
/// nodes are constructed only after their bodies have been collected.
fn block_span(file: FileId, line: usize, origins: Option<&[macros::LineOrigin]>) -> Span {
    let mut span = Span::in_file(file, line as u32, 1);
    if let Some(origins) = origins {
        macros::place(&mut span, origins);
    }
    span
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
        let original = lines[*pos];
        let line = *pos + 1;
        *pos += 1;

        let Some(processed) = w
            .preprocess_line(original, line, file)
            .map_err(|e| stamp_file(e, file))?
        else {
            continue;
        };
        let raw = processed.as_str();

        // Structure first, so a block keyword never reaches the line parser —
        // which would refuse it as an unknown directive.
        let code = strip_block_comment(raw);
        if let Some(kw) = w.block_keyword(code) {
            let head = code.trim().to_string();
            match kw {
                BlockKw::CondClose if in_block => return Ok(BlockClose::CondClose(head)),
                BlockKw::RepeatClose if in_block => return Ok(BlockClose::RepeatClose(head)),
                BlockKw::Else if in_block => return Ok(BlockClose::Else),
                BlockKw::ElseIf if in_block => return Ok(BlockClose::ElseIf(head, line)),
                BlockKw::CondOpen => {
                    let (label, head) = w.block_open(code, line)?;
                    parse_conditional(
                        Cursor {
                            lines,
                            pos,
                            file,
                            origins,
                        },
                        w,
                        res,
                        head.to_string(),
                        label,
                        line,
                    )?;
                    continue;
                }
                BlockKw::RepeatOpen => {
                    let (label, head) = w.block_open(code, line)?;
                    parse_repeat(
                        Cursor {
                            lines,
                            pos,
                            file,
                            origins,
                        },
                        w,
                        res,
                        head.to_string(),
                        label,
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
    label: Option<Symbol>,
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
                        block_span(file, line, origins),
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
                None,
                leg_line,
            )?;
            Some(w.nodes_mut().split_off(start))
        }
        BlockClose::Eof => {
            return Err(AsmError::at(
                block_span(file, line, origins),
                "conditional block is never closed".to_string(),
            ));
        }
        BlockClose::RepeatClose(_) => {
            return Err(AsmError::at(
                block_span(file, line, origins),
                "a repetition closer ends a conditional block".to_string(),
            ));
        }
    };
    w.push_node(Node {
        operand_span: None,
        label,
        item: Some(crate::ast::Item::Conditional {
            delimiters: crate::ast::BlockTrivia::default(),
            close: closer,
            head: head.clone(),
            then_body,
            else_body,
            inline: false,
            style: crate::ast::CondStyle::Keyword,
        }),
        source: head,
        span: block_span(file, line, origins),
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
    label: Option<Symbol>,
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
            block_span(file, line, origins),
            "repetition block is never closed".to_string(),
        ));
    };
    let body: Vec<Node> = w.nodes_mut().split_off(start);
    w.push_node(Node {
        operand_span: None,
        label,
        item: Some(crate::ast::Item::Repeat {
            delimiters: crate::ast::BlockTrivia::default(),
            head: head.clone(),
            body,
            close,
            style: crate::ast::CondStyle::Keyword,
        }),
        source: head,
        span: block_span(file, line, origins),
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
                let (id, contents) =
                    open_include(&request, &at, map, *loader, stack, sem, line as u32)?;
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
                let payload =
                    incbin_payload(&request, offset, size, &at, map, *loader, stack, sem)?;
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

/// Open an `.include` target: the depth backstop, resolution through the
/// dialect's [`WalkSemantics`], and cycle detection — everything but the walk
/// of the target itself, which differs by caller: the eager walk recurses
/// [`walk_file`] here, and the ca65 projection sweep (#481) reads the target
/// lazily, under the text environment in force at the include point.
///
/// # Errors
/// The depth backstop, a request that does not resolve, or an include cycle
/// — all at `at`, the directive's operand span.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_include(
    request: &str,
    at: &Span,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    stack: &[FileId],
    sem: &WalkSemantics,
    line: u32,
) -> Result<(FileId, String), AsmError> {
    open_include_in_stream(request, at, map, loader, stack, stack, sem, line)
}

/// Open a file whose search path may include suspended sibling files. Only
/// actual recursive ancestry is a cycle: opening the same pending file twice
/// from one repeated caller is valid native ca65 input.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_include_in_stream(
    request: &str,
    at: &Span,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    search_stack: &[FileId],
    ancestry: &[FileId],
    sem: &WalkSemantics,
    line: u32,
) -> Result<(FileId, String), AsmError> {
    if ancestry.len() >= MAX_INCLUDE_DEPTH {
        return Err(AsmError::at(
            at.clone(),
            format!("includes nested more than {MAX_INCLUDE_DEPTH} levels deep"),
        ));
    }
    let id = load_include_defaulted(map, loader, request, search_stack, line, sem)
        .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
    // Cycle detection is membership of the *active* stack: ca65
    // itself has none (a self-include dies on the OS's open-file
    // limit), so this diagnostic exceeds the reference — allowed,
    // diagnostics are not byte-compared (KTD5).
    if ancestry.contains(&id) {
        let chain = ancestry
            .iter()
            .chain(std::iter::once(&id))
            .map(|f| map.path(*f).unwrap_or("?"))
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(AsmError::at(at.clone(), format!("include cycle: {chain}")));
    }
    let contents = map.contents(id).unwrap_or_default().to_owned();
    Ok((id, contents))
}

/// Resolve an `.incbin` payload: load the binary and cut the requested
/// window. The binary path mints no FileId (KTD8) — the payload rides a node
/// at the *directive's* span, which is where the missing-asset / window
/// diagnostics land too.
///
/// # Errors
/// A request that does not resolve, or a window outside the data.
#[allow(clippy::too_many_arguments)]
pub(crate) fn incbin_payload(
    request: &str,
    offset: Option<i64>,
    size: Option<i64>,
    at: &Span,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    stack: &[FileId],
    sem: &WalkSemantics,
) -> Result<Vec<u8>, AsmError> {
    let data = load_binary(map, loader, request, stack, sem.resolution)
        .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
    (sem.window)(&data, offset, size)
        .map_err(|msg| AsmError::at(at.clone(), format!("`{request}`: {msg}")))
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

// These two flat ca65 readers expand each physical file independently. Their
// expansion's FileId(0) is local to that file, rather than the root input.
fn local_macro_span(span: &mut Span, file: FileId) {
    if span.file.is_root() && span.file != file {
        span.file = file;
        span.path = None;
    }
    local_macro_frames(span, file);
}

fn local_macro_frames(span: &mut Span, file: FileId) {
    for frame in &mut span.expansion_frames {
        if let Some(definition) = &mut frame.defined_at {
            local_macro_span(definition, file);
        }
        local_macro_span(&mut frame.invoked_at, file);
    }
}

pub(crate) fn resolve_local_macro_error(mut error: AsmError) -> AsmError {
    if let Some(span) = &mut error.span {
        local_macro_frames(span, span.file);
    }
    error
}

pub(crate) fn resolve_local_macro_files(nodes: &mut [Node]) {
    for node in nodes {
        let file = node.span.file;
        local_macro_frames(&mut node.span, file);
        if let Some(span) = &mut node.operand_span {
            local_macro_frames(span, span.file);
        }
        match node.item.as_mut() {
            Some(crate::ast::Item::Repeat { body, .. } | crate::ast::Item::Loop { body, .. }) => {
                resolve_local_macro_files(body);
            }
            Some(crate::ast::Item::Conditional {
                then_body,
                else_body,
                ..
            }) => {
                resolve_local_macro_files(then_body);
                if let Some(body) = else_body {
                    resolve_local_macro_files(body);
                }
            }
            _ => {}
        }
    }
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
    include_value_request(args, line, directive, &StringTokens::default())
}

pub(crate) fn include_value_request(
    args: &str,
    line: usize,
    directive: &str,
    strings: &StringTokens,
) -> Result<String, AsmError> {
    let (name, rest) = quoted_name(args, line, directive, strings)?;
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

/// Classify file requests without evaluating source in the unexpanded formatter.
/// Known literal requests keep their existing AST kind; computed or otherwise
/// opaque spellings remain verbatim instead of inventing a resolved filename.
pub(crate) fn formatter_file_item(rest: &str, line: usize) -> Option<crate::ast::Item> {
    use crate::ast::Item;
    let (word, args) = super::mos6502::split_first_word(rest.trim());
    if word.eq_ignore_ascii_case(".include") {
        Some(
            include_request(args, line, word)
                .map(|request| Item::Include { request })
                .unwrap_or(Item::Verbatim),
        )
    } else if word.eq_ignore_ascii_case(".incbin") {
        // The window is source text, not a formatter-time constant query.
        Some(
            quoted_name(args, line, word, &StringTokens::default())
                .map(|(request, _)| Item::Incbin { request })
                .unwrap_or(Item::Verbatim),
        )
    } else {
        None
    }
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
    incbin_value_args(args, line, directive, fold, &StringTokens::default())
}

pub(crate) fn incbin_value_args(
    args: &str,
    line: usize,
    directive: &str,
    fold: &dyn Fn(&str) -> Result<i64, AsmError>,
    strings: &StringTokens,
) -> Result<(String, Option<i64>, Option<i64>), AsmError> {
    let (name, rest) = quoted_name(args, line, directive, strings)?;
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
    strings: &StringTokens,
) -> Result<(String, &'a str), AsmError> {
    let t = args.trim();
    if StringTokens::is_handle(t) {
        let (name, tail) = strings.text_prefix(t, line)?;
        if name.is_empty() {
            return Err(AsmError::new(
                line,
                format!("`{directive}` needs a file name"),
            ));
        }
        return Ok((name, tail));
    }
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
    fn admit_line(&self, source: &str, at: &Span) -> Result<(), AsmError> {
        validate_lexical_tokens(source, at)
    }

    fn is_symbol_char(&self, c: u8) -> bool {
        // ca65 tokenises @value independently of a parameter named value.
        c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'@')
    }

    fn argument_count_word(&self) -> Option<&'static str> {
        Some(".paramcount")
    }

    fn defined_macro_word(&self) -> Option<&'static str> {
        Some(".definedmacro")
    }

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

/// The text layer's function names, lower-cased. One list, read by the pass
/// that folds them and by the stand-in the formatter parses.
///
/// `.definedmacro` is not here — it is answered a layer earlier, during macro
/// expansion — but it needs the same stand-in for the same reason, so the
/// caller adds it.
pub(crate) fn is_text_function(lower: &str) -> bool {
    matches!(
        lower,
        ".concat"
            | ".strlen"
            | ".strat"
            | ".string"
            | ".ident"
            | ".sprintf"
            | ".tcount"
            | ".blank"
            | ".match"
            | ".xmatch"
            | ".left"
            | ".mid"
            | ".right"
            | ".const"
            | ".ismnem"
            | ".ismnemonic"
    )
}

/// The prefix on the stand-in a text-layer call parses to when the text pass
/// has not run — the formatter's path. `\u{1}` cannot appear in ca65 source, so
/// the name can never collide with a real symbol.
const TEXT_MARK: &str = "\u{1}text\u{1}";

/// A comma-separated name list, empties dropped — shared by macro parameters
/// and `.local` declarations, which ca65 spells the same way.
fn name_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Expand the ca65 family's macros and resolve its text layer, unless this
/// parse is the formatter's.
///
/// The text pass runs **after** macro expansion — ca65's string functions are
/// written for macro bodies, so most of what they fold only exists once the
/// macro has been placed — and it emits one line per line, so the origins the
/// expansion recorded still line up.
pub(crate) struct Ca65Expansion {
    pub(crate) expansion: macros::Expansion,
    pub(crate) strings: StringTokens,
}

pub(crate) fn expand_ca65(
    source: &str,
    mode: macros::Expand,
    target: Target,
) -> Result<Ca65Expansion, AsmError> {
    let strings = StringTokens::default();
    if mode == macros::Expand::Yes {
        for (line, source) in source.lines().enumerate() {
            StringTokens::reject_authored(source, line + 1)?;
        }
    }
    let expansion = macros::expansion(mode, source, |s| {
        let expanded = macros::expand(&Ca65Macros, s)?;
        let text = Ca65Text {
            target,
            defined: Cow::Owned(defined_names(&expanded.text)),
            strings: Cow::Borrowed(&strings),
            ..Ca65Text::default()
        };
        let resolved = text::expand(&text, &expanded.text).map_err(|e| strings.error(e))?;
        Ok(Some((resolved, expanded.origins)))
    })?;
    Ok(Ca65Expansion { expansion, strings })
}

/// The linked ca65 reader resolves text under its live CPU in the sweep.
/// Keep macro origin mapping while leaving those calls intact for that reader.
pub(crate) fn expand_ca65_macros(
    source: &str,
    mode: macros::Expand,
) -> Result<macros::Expansion, AsmError> {
    macros::expansion(mode, source, |s| {
        let expanded = macros::expand(&Ca65Macros, s)?;
        Ok(Some((expanded.text, expanded.origins)))
    })
}

/// Borrowed source-time numeric semantics for linked ca65 text consumers.
pub(crate) trait LiveNumbers {
    fn literal(&self, text: &str, line: usize) -> Result<Option<i64>, AsmError>;
    fn number(&self, text: &str, line: usize) -> Result<Option<i64>, AsmError>;
    fn constancy(&self, text: &str, line: usize) -> Result<i64, AsmError>;
}

pub(crate) struct LiveText<'a> {
    pub target: Target,
    pub token_target: Option<Target>,
    pub numbers: &'a BTreeMap<String, i64>,
    pub strings: &'a StringTokens,
    pub defined: &'a std::collections::BTreeSet<String>,
    pub context: &'a dyn LiveNumbers,
}

pub(crate) fn expand_ca65_live(
    source: &str,
    line: usize,
    live: LiveText<'_>,
) -> Result<String, AsmError> {
    let syntax = Ca65Text {
        source_line: Some(line),
        target: live.target,
        token_target: live.token_target,
        depth: std::cell::Cell::new(0),
        numbers: Cow::Borrowed(live.numbers),
        defined: Cow::Borrowed(live.defined),
        strings: Cow::Borrowed(live.strings),
        live: Some(live.context),
        numeric_admission: None,
    };
    text::expand(&syntax, source)
        .map(|text| text.trim_end_matches('\n').to_string())
        .map_err(|mut error| {
            if error.span.is_none() {
                error.line = line;
            }
            live.strings.error(error)
        })
}

type NumericAdmission<'a> = dyn Fn(&str, usize) -> Result<(), AsmError> + 'a;

pub(crate) struct LineText<'a> {
    pub target: Target,
    pub token_target: Option<Target>,
    pub numbers: &'a BTreeMap<String, i64>,
    pub defined: &'a std::collections::BTreeSet<String>,
    pub strings: &'a StringTokens,
    pub depth: usize,
    pub admission: Option<&'a NumericAdmission<'a>>,
}

pub(crate) fn expand_ca65_line(
    source: &str,
    line: usize,
    text: LineText<'_>,
) -> Result<String, AsmError> {
    let syntax = Ca65Text {
        live: None,
        numeric_admission: text.admission,
        source_line: Some(line),
        target: text.target,
        token_target: text.token_target,
        depth: std::cell::Cell::new(text.depth),
        numbers: Cow::Borrowed(text.numbers),
        defined: Cow::Borrowed(text.defined),
        strings: Cow::Borrowed(text.strings),
    };
    text::expand(&syntax, source)
        .map(|text| text.trim_end_matches('\n').to_string())
        .map_err(|mut error| {
            if error.span.is_none() {
                error.line = line;
            }
            text.strings.error(error)
        })
}

/// Which CPU a ca65 parse is for.
///
/// The text layer reads two things off it, and both are observable rather than
/// cosmetic:
///
/// - **the register names a token list holds.** `.match({s},{q})` is 1 for a
///   6502, where `s` is an ordinary identifier, and 0 for a 65816, where it is
///   the stack register its stack-relative modes need.
/// - **which mnemonics exist.** `.ismnem(bra)` is 0 for a 6502 and 1 for a
///   65816.
#[derive(Clone, Copy, Default)]
pub(crate) enum Target {
    #[default]
    Mos6502,
    Csg4510,
    Wdc65816,
    HuC6280,
}

impl Target {
    fn holds_register(self, name: &str) -> bool {
        matches!(
            (self, name),
            (_, "a" | "x" | "y") | (Target::Wdc65816, "s") | (Target::Csg4510, "s" | "z")
        )
    }

    /// Whether the CPU has this mnemonic. ca65 reads the name
    /// case-insensitively — `.ismnem(LDA)` is 1 — and the spec stores it
    /// upper-case; every one of these targets is a 6502 with additions.
    fn has_mnemonic(self, name: &str) -> bool {
        let upper = name.to_ascii_uppercase();
        isa::mos6502::SET.has_mnemonic(&upper)
            || match self {
                Target::Mos6502 => false,
                Target::Csg4510 => isa::csg65ce02::CSG4502_SET.has_mnemonic(if upper == "PHD" {
                    "PHW"
                } else {
                    &upper
                }),
                Target::Wdc65816 => isa::mos65816::SET.has_mnemonic(&upper),
                Target::HuC6280 => isa::huc6280::SET.has_mnemonic(&upper),
            }
    }
}

/// ca65's string grammar. It has no string *symbol* — `.define` is a macro
/// there — so this is the function half only.
///
/// Each rule was read off ca65 V2.18 before it was written. The one a reader
/// would most likely guess wrong is `.string`, which stringifies its argument's
/// **token** rather than its value: with `N = 7`, `.string(N)` is `"N"`.
#[derive(Default)]
struct Ca65Text<'a> {
    live: Option<&'a dyn LiveNumbers>,
    numeric_admission: Option<&'a NumericAdmission<'a>>,
    source_line: Option<usize>,
    strings: Cow<'a, StringTokens>,
    /// How many `.proc`/`.scope` blocks are open around the line being read.
    ///
    /// The pass has no scope stack, so a constant defined inside one is not
    /// recorded at all. That is deliberate: recording it flat would let an
    /// inner `v` answer a `v` written outside the scope, where ca65 resolves
    /// two different symbols. Refusing to fold is a gap; folding the wrong
    /// symbol is a wrong answer.
    depth: std::cell::Cell<usize>,
    /// The CPU, which decides both the register names a token list holds and
    /// which mnemonics `.ismnem` knows.
    target: Target,
    token_target: Option<Target>,
    /// Constants visible to a line read by the linked ca65 sweep.
    numbers: Cow<'a, BTreeMap<String, i64>>,
    /// Every name defined anywhere in the source.
    ///
    /// `.const` needs it: ca65 answers 0 for a symbol that is defined but not
    /// constant *here* — a label, or a constant defined below the line — and
    /// **errors** for one that is not defined at all. Telling those apart needs
    /// the whole file, which is why this is collected before the walk rather
    /// than accumulated during it.
    defined: Cow<'a, std::collections::BTreeSet<String>>,
}

/// Every name the source defines, wherever it defines it.
///
/// Deliberately generous about what counts and deliberately blind to scope: a
/// name it misses makes `.const` refuse rather than answer, which is the safe
/// direction.
pub(crate) fn defined_names(source: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for raw in source.lines() {
        let code = macros::without_comment(raw);
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        // `name:` and `@cheap:` labels, which may be followed by a statement.
        if let Some(colon) = code.find(':')
            && !code[..colon].contains(char::is_whitespace)
        {
            let name = code[..colon].trim_start_matches('@');
            if super::mos6502::is_ident(name) {
                out.insert(name.to_string());
            }
        }
        let (word, rest) = super::mos6502::split_first_word(code);
        match word.to_ascii_lowercase().as_str() {
            // A declaration names a symbol without defining a value.
            ".import" | ".importzp" | ".global" | ".globalzp" | ".export" | ".exportzp"
            | ".forceimport" => {
                for name in rest.split(',') {
                    let name = name.split('=').next().unwrap_or_default().trim();
                    if super::mos6502::is_ident(name) {
                        out.insert(name.to_string());
                    }
                }
                continue;
            }
            ".proc" | ".scope" | ".enum" | ".struct" | ".union" => {
                let name = super::mos6502::split_first_word(rest).0;
                if super::mos6502::is_ident(name) {
                    out.insert(name.to_string());
                }
                continue;
            }
            _ => {}
        }
        // `NAME = expr` and `NAME .set expr`.
        if let Some(eq) = super::mos6502::assignment_split(code) {
            let name = code[..eq].trim();
            if super::mos6502::is_ident(name) {
                out.insert(name.to_string());
            }
        }
    }
    out
}

impl Ca65Text<'_> {
    fn bytes(&self, arg: &text::Arg, name: &str, line: usize) -> Result<Vec<u8>, AsmError> {
        if self.live.is_some()
            && let text::Arg::Bare(raw) = arg
            && !StringTokens::is_handle(raw.trim())
        {
            let expanded = text::expand(self, raw)?;
            return self
                .strings
                .bytes(&text::Arg::Bare(expanded.trim().to_string()), name, line);
        }
        self.strings.bytes(arg, name, line)
    }

    /// Answer `.const`: 1 if the expression folds here, 0 if every name in it
    /// is defined somewhere but not to a value this pass can reach.
    ///
    /// # Errors
    ///
    /// A name defined nowhere — which ca65 answers with "Symbol is undefined"
    /// rather than with 0 — and the two cases this pass cannot see: a scoped
    /// name, and any expression inside an open `.proc`/`.scope`, where a
    /// constant is in scope for ca65 and invisible here.
    fn constancy(
        &self,
        arg: &text::Arg,
        scope: &text::Scope,
        name: &str,
        line: usize,
    ) -> Result<i64, AsmError> {
        let text::Arg::Bare(raw) = arg else {
            return Err(AsmError::new(
                line,
                format!("`{name}` takes an expression, not a string"),
            ));
        };
        let raw = raw.trim();
        if StringTokens::is_handle(raw) {
            return Err(AsmError::new(
                line,
                format!("`{name}` takes an expression, not a string"),
            ));
        }
        if let Some(live) = self.live {
            return live.constancy(raw, line);
        }
        if self.depth.get() > 0 || raw.contains("::") {
            return Err(AsmError::new(
                line,
                format!(
                    "`{name}` cannot answer for `{raw}`: this pass keeps no scope stack, so a \
                     constant inside a `.proc` or `.scope` is invisible to it"
                ),
            ));
        }
        if scope.number(arg, name, line).is_ok() {
            return Ok(1);
        }
        let expr = super::ca65::constant_value(raw, line)?;
        // Not constant — but ca65 tells "defined and not constant" from "not
        // defined", and so must this.
        let mut unknown = None;
        crate::ast::map_sym_expr(expr, &mut |sym| {
            if !self.defined.contains(&sym) {
                unknown.get_or_insert_with(|| sym.clone());
            }
            crate::engine::Expr::Sym(sym)
        });
        match unknown {
            Some(sym) => Err(AsmError::new(
                line,
                format!("`{sym}` is not defined anywhere in this source"),
            )),
            None => Ok(0),
        }
    }
}

impl text::TextSyntax for Ca65Text<'_> {
    fn reads_arguments(&self, name: &str) -> bool {
        self.live.is_some()
            && matches!(
                name.to_ascii_lowercase().as_str(),
                ".const" | ".sprintf" | ".concat" | ".strlen" | ".strat" | ".ident"
            )
    }

    fn folds_in_braces(&self, name: &str) -> bool {
        self.live.is_none() && !matches!(name.to_ascii_lowercase().as_str(), ".strlen" | ".strat")
    }

    fn permits_function_space(&self) -> bool {
        true
    }

    fn definition(&self, _line: &str) -> Option<(String, String)> {
        None
    }

    /// `NAME = expr` at the top level, read through the same expression parser
    /// the statement itself uses.
    fn constant(
        &self,
        line: &str,
        numbers: &std::collections::BTreeMap<String, i64>,
    ) -> Option<(String, i64)> {
        let code = macros::without_comment(line);
        let code = code.trim();
        match code
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            ".proc" | ".scope" => {
                self.depth.set(self.depth.get() + 1);
                return None;
            }
            ".endproc" | ".endscope" => {
                self.depth.set(self.depth.get().saturating_sub(1));
                return None;
            }
            _ => {}
        }
        if self.depth.get() > 0 {
            return None;
        }
        let eq = super::mos6502::assignment_split(code)?;
        let name = code[..eq].trim();
        if !super::mos6502::is_ident(name) {
            return None;
        }
        let expr = super::ca65::constant_value(&code[eq + 1..], 0).ok()?;
        let value = super::mos6502::fold_const(&expr, numbers, 0).ok()?;
        Some((name.to_string(), value))
    }

    /// A ca65 expression over the constants above the line. A label's address
    /// is unreachable here, and ca65 refuses that case itself — `.sprintf("%d",
    /// L)` on a label is "Constant expression expected" there too.
    fn evaluate_checked(
        &self,
        text: &str,
        numbers: &BTreeMap<String, i64>,
        line: usize,
    ) -> Result<Option<i64>, AsmError> {
        match self.live {
            Some(live) => live.number(text, line),
            None => {
                if let Some(admit) = self.numeric_admission {
                    admit(text, line)?;
                }
                Ok(self.evaluate(text, numbers, line))
            }
        }
    }

    fn evaluate(
        &self,
        text: &str,
        numbers: &std::collections::BTreeMap<String, i64>,
        line: usize,
    ) -> Option<i64> {
        let expr = super::ca65::constant_value(text, line).ok()?;
        if self.numbers.is_empty() {
            return super::mos6502::fold_const(&expr, numbers, line).ok();
        }
        if numbers.is_empty() {
            return super::mos6502::fold_const(&expr, &self.numbers, line).ok();
        }
        let mut visible = self.numbers.as_ref().clone();
        visible.extend(numbers.iter().map(|(name, value)| (name.clone(), *value)));
        super::mos6502::fold_const(&expr, &visible, line).ok()
    }

    fn function(
        &self,
        name: &str,
        args: &[text::Arg],
        scope: &text::Scope,
        line: usize,
    ) -> Result<Option<text::Folded>, AsmError> {
        use text::Folded;
        let lower = name.to_ascii_lowercase();
        if !is_text_function(&lower) {
            return Ok(None);
        }
        // Recognition probes use line zero. A real numeric string query must
        // keep its native arity, even when the argument list is empty.
        if args.is_empty() {
            if line != 0 {
                match lower.as_str() {
                    ".strlen" => arity(args, 1, name, line)?,
                    ".strat" => arity(args, 2, name, line)?,
                    _ => {}
                }
            }
            return Ok(Some(Folded::Text(String::new())));
        }
        let source = format!(
            "{name}({})",
            args.iter()
                .map(|arg| match arg {
                    text::Arg::Text { spelling, .. } => spelling.as_str(),
                    text::Arg::Bare(source) => source.as_str(),
                })
                .collect::<Vec<_>>()
                .join(",")
        );
        let source = self.strings.display(&source);
        let string = |bytes| {
            self.strings
                .intern(bytes, source.clone(), self.source_line.unwrap_or(line))
                .map(Folded::Bare)
        };
        let one = |args: &[text::Arg]| arity(args, 1, name, line);
        Ok(Some(match lower.as_str() {
            ".concat" => {
                let mut out = Vec::new();
                for a in args {
                    out.extend(self.bytes(a, name, line)?);
                }
                string(out)?
            }
            // The *token*, not its value: ca65 takes one identifier or one
            // number here and refuses an expression or a string.
            ".string" => {
                one(args)?;
                let token = match &args[0] {
                    text::Arg::Bare(b) => b.trim(),
                    text::Arg::Text { .. } => {
                        let spelling = args[0].literal_spelling().expect("matched quoted argument");
                        return Err(AsmError::new(
                            line,
                            format!(
                                "`{name}` takes a name or a number, and `{spelling}` is a string"
                            ),
                        ));
                    }
                };
                if !is_one_token(token) {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "`{name}` takes a single name or number, and `{token}` is an \
                             expression"
                        ),
                    ));
                }
                string(token.as_bytes().to_vec())?
            }
            ".sprintf" => string(sprintf(
                &self.bytes(&args[0], name, line)?,
                &args[1..],
                &|arg, name, line| self.bytes(arg, name, line),
                scope,
                line,
            )?)?,
            ".strlen" => {
                one(args)?;
                Folded::Number(
                    self.bytes(&args[0], name, line)
                        .map_err(|error| {
                            if self.live.is_some() && error.span.is_some() {
                                error
                            } else {
                                AsmError::new(line, format!("`{name}` takes a string, not a value"))
                            }
                        })?
                        .len() as i64,
                )
            }
            ".strat" => {
                arity(args, 2, name, line)?;
                let bytes = self.bytes(&args[0], name, line)?;
                let index = if let Some(live) = self.live {
                    if StringTokens::is_string(&args[1]) {
                        return Err(AsmError::new(
                            line,
                            "`.strat` needs a numeric bound, not a string",
                        ));
                    }
                    let text::Arg::Bare(source) = &args[1] else {
                        unreachable!("quoted arguments rejected")
                    };
                    live.literal(source, line)?.ok_or_else(|| {
                        AsmError::new(line, "`.strat` index must be a literal constant expression")
                    })?
                } else {
                    if let text::Arg::Bare(source) = &args[1]
                        && !StringTokens::is_string(&args[1])
                    {
                        let expression = super::ca65::constant_value(source, line)?;
                        let mut named = false;
                        let _ = crate::ast::map_sym_expr(expression, &mut |symbol| {
                            named = true;
                            crate::engine::Expr::Sym(symbol)
                        });
                        if named {
                            return Err(AsmError::new(
                                line,
                                "`.strat` index must be a literal constant expression",
                            ));
                        }
                    }
                    selector_bound(&args[1], scope, name, line)?
                };
                let byte = usize::try_from(index)
                    .ok()
                    .and_then(|i| bytes.get(i))
                    .ok_or_else(|| {
                        AsmError::new(line, format!("`.strat` index {index} is past the string"))
                    })?;
                Folded::Number(i64::from(*byte))
            }
            // The token-list half. A token list is unevaluated source, so these
            // answer over what is *written* rather than what it is worth — and
            // the sub-list ones splice the author's own text back, rather than
            // re-rendering tokens that would have to be spaced back together.
            ".tcount" => {
                one(args)?;
                Folded::Number(
                    tokens(
                        token_list(&args[0], name, line)?,
                        self.token_target.unwrap_or(self.target),
                    )
                    .len() as i64,
                )
            }
            ".blank" => {
                one(args)?;
                Folded::Number(i64::from(
                    tokens(
                        token_list(&args[0], name, line)?,
                        self.token_target.unwrap_or(self.target),
                    )
                    .is_empty(),
                ))
            }
            // `.match` compares what each token *is*; `.xmatch` compares what it
            // says as well. So `.match({1},{2})` is 1 and `.xmatch({1},{2})` is
            // 0, while `.match({a},{b})` is 0 — `a` is the accumulator and `b`
            // is an identifier.
            ".match" | ".xmatch" => {
                arity(args, 2, name, line)?;
                let exact = lower == ".xmatch";
                let (a, b) = (
                    token_list(&args[0], name, line)?,
                    token_list(&args[1], name, line)?,
                );
                let (a, b) = (
                    tokens(a, self.token_target.unwrap_or(self.target)),
                    tokens(b, self.token_target.unwrap_or(self.target)),
                );
                let mut same = a.len() == b.len();
                for (x, y) in a.iter().zip(&b) {
                    let value = |token: &Token| -> Result<Vec<u8>, AsmError> {
                        if StringTokens::is_handle(token.text) {
                            self.strings.resolve(token.text, line)
                        } else {
                            Ok(token_value(token).into_bytes())
                        }
                    };
                    if x.kind != y.kind || (exact && value(x)? != value(y)?) {
                        same = false;
                        break;
                    }
                }
                Folded::Number(i64::from(same))
            }
            ".left" | ".right" => {
                arity(args, 2, name, line)?;
                let count = selector_bound(&args[0], scope, name, line)?.max(0) as usize;
                let list = token_list(&args[1], name, line)?;
                let all = tokens(list, self.token_target.unwrap_or(self.target));
                let (from, to) = if lower == ".left" {
                    (0, count)
                } else {
                    (all.len().saturating_sub(count), all.len())
                };
                Folded::Bare(token_slice(list, &all, from, to).to_string())
            }
            // Whether the CPU has this mnemonic. ca65 takes a bare name here
            // and nothing else — `.ismnem("lda")` is "Identifier expected" —
            // and reads it case-insensitively.
            ".ismnem" | ".ismnemonic" => {
                one(args)?;
                let text::Arg::Bare(word) = &args[0] else {
                    return Err(AsmError::new(
                        line,
                        format!("`{name}` takes a bare name, not a string"),
                    ));
                };
                let word = word.trim();
                if !super::mos6502::is_ident(word) {
                    return Err(AsmError::new(
                        line,
                        format!("`{name}` takes a name, and `{word}` is not one"),
                    ));
                }
                Folded::Number(i64::from(self.target.has_mnemonic(word)))
            }
            // Whether the expression is constant *here*. A symbol defined below
            // the line is not, and neither is a label or the location counter;
            // a symbol defined nowhere is an error in ca65, so it is one here.
            ".const" => {
                one(args)?;
                Folded::Number(self.constancy(&args[0], scope, name, line)?)
            }
            ".mid" => {
                arity(args, 3, name, line)?;
                let start = selector_bound(&args[0], scope, name, line)?.max(0) as usize;
                let count = selector_bound(&args[1], scope, name, line)?.max(0) as usize;
                let list = token_list(&args[2], name, line)?;
                let all = tokens(list, self.token_target.unwrap_or(self.target));
                Folded::Bare(
                    token_slice(list, &all, start, start.saturating_add(count)).to_string(),
                )
            }
            // A name built from text, spliced back in unquoted for the parse to
            // resolve — forward references included, as in ca65.
            ".ident" => {
                one(args)?;
                let bytes = self.bytes(&args[0], name, line)?;
                let value = std::str::from_utf8(&bytes).map_err(|_| {
                    AsmError::new(line, "`.ident` argument is not a valid identifier")
                })?;
                let mut bytes = value.strip_prefix('@').unwrap_or(value).bytes();
                if !bytes
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                    || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                {
                    return Err(AsmError::new(
                        line,
                        "`.ident` argument is not a valid identifier",
                    ));
                }
                Folded::Bare(value.to_string())
            }
            other => unreachable!("`{other}` was matched as known and then not folded"),
        }))
    }
}

/// Whether a line holds a `{…}` token list, and so is the formatter's to hand
/// back rather than lay out.
///
/// A token list is unevaluated source — that is the whole point of one — so
/// there is no expression to render it from. The text pass folds every line
/// that reaches assembly, which is why this only ever answers `true` on the
/// formatter's parse. Braces mean nothing else in ca65.
pub(crate) fn holds_a_token_list(code: &str) -> bool {
    let mut quote = None;
    for c in code.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, ';') => return false,
            (None, '{') => return true,
            _ => {}
        }
    }
    false
}

/// Exactly `want` arguments, or a diagnostic naming the function.
fn arity(args: &[text::Arg], want: usize, name: &str, line: usize) -> Result<(), AsmError> {
    if args.len() == want {
        Ok(())
    } else {
        Err(AsmError::new(
            line,
            format!(
                "`{name}` takes {want} argument(s), and was given {}",
                args.len()
            ),
        ))
    }
}

/// What a token in a ca65 token list *is*, which is what `.match` compares.
///
/// Probed against ca65 V2.18, and the surprises are all here: `a`, `x` and `y`
/// are register tokens and nothing else is (`s` is an ordinary identifier), a
/// character literal is not a number, and each dot-keyword and each punctuation
/// mark is its own kind — `.match({.byte},{.word})` is 0 where
/// `.match({abc},{abd})` is 1.
#[derive(PartialEq, Eq, Debug)]
enum TokenKind {
    Number,
    Char,
    Str,
    Ident,
    /// `a`, `x` or `y`, lower-cased.
    Register(char),
    /// `.byte`, `.word`, … lower-cased, since each is its own kind.
    DotKeyword(String),
    /// `+`, `<<`, `::`, … verbatim, since each is its own kind.
    Punct(String),
}

/// One token, with the slice of source it came from so a sub-list can be
/// spliced back as the author wrote it rather than re-rendered.
struct Token<'a> {
    kind: TokenKind,
    text: &'a str,
    start: usize,
    end: usize,
}

/// ca65's two-character operators, longest-match-first.
const OPERATORS: &[&str] = &["::", "<<", ">>", "<=", ">=", "<>", "&&", "||"];

/// Split a token list into tokens. Whitespace separates and is not a token, so
/// `{+ +}` and `{++}` are the same two tokens.
fn tokens(text: &str, target: Target) -> Vec<Token<'_>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let kind = match c {
            1 if StringTokens::is_handle(&text[i..]) => {
                i += StringTokens::handle_len(&text[i..]).unwrap_or(text.len() - i);
                TokenKind::Str
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                TokenKind::Str
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                TokenKind::Char
            }
            // `$ff` and `%1010` are numbers; a lone `$` is the location counter
            // and a lone `%` is the modulo operator.
            b'$' | b'%' if bytes.get(i + 1).is_some_and(|d| d.is_ascii_alphanumeric()) => {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                TokenKind::Number
            }
            _ if c.is_ascii_digit() => {
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                TokenKind::Number
            }
            // A dot-keyword, and not an identifier that happens to start with a
            // dot: ca65 has no such identifier.
            b'.' if bytes.get(i + 1).is_some_and(|d| d.is_ascii_alphabetic()) => {
                i += 1;
                while i < bytes.len() && word(bytes[i]) {
                    i += 1;
                }
                TokenKind::DotKeyword(text[start..i].to_ascii_lowercase())
            }
            _ if c.is_ascii_alphabetic() || c == b'_' || c == b'@' => {
                i += 1;
                while i < bytes.len() && word(bytes[i]) {
                    i += 1;
                }
                let name = text[start..i].to_ascii_lowercase();
                if target.holds_register(&name) {
                    TokenKind::Register(name.chars().next().unwrap_or('a'))
                } else {
                    TokenKind::Ident
                }
            }
            // `:+` and `:---` are one anonymous-label reference, not a colon
            // followed by operators.
            b':' if matches!(bytes.get(i + 1), Some(b'+' | b'-')) => {
                let sign = bytes[i + 1];
                i += 1;
                while bytes.get(i) == Some(&sign) {
                    i += 1;
                }
                TokenKind::Punct(text[start..i].to_string())
            }
            _ => {
                let rest = &text[i..];
                let op = OPERATORS.iter().find(|o| rest.starts_with(**o));
                i += op.map_or_else(
                    || text[i..].chars().next().map_or(1, char::len_utf8),
                    |o| o.len(),
                );
                TokenKind::Punct(text[start..i].to_string())
            }
        };
        out.push(Token {
            kind,
            text: &text[start..i],
            start,
            end: i,
        });
    }
    out
}

/// The value `.xmatch` compares once the kinds agree: a number by what it is
/// worth (`1` and `$1` are the same token), a string or character by its
/// contents, an identifier by its name **case-sensitively**, and everything
/// else by nothing, because its kind already said everything.
fn token_value(token: &Token) -> String {
    match token.kind {
        TokenKind::Number => match parse_number(token.text, 0) {
            Ok(n) => n.to_string(),
            Err(_) => token.text.to_string(),
        },
        TokenKind::Str | TokenKind::Char => token
            .text
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string(),
        TokenKind::Ident => token.text.to_string(),
        _ => String::new(),
    }
}

/// Selector positions need numeric expressions, not decoded string contents.
fn selector_bound(
    arg: &text::Arg,
    scope: &text::Scope<'_>,
    name: &str,
    line: usize,
) -> Result<i64, AsmError> {
    if StringTokens::is_string(arg) {
        return Err(AsmError::new(
            line,
            format!("`{name}` needs a numeric bound, not a string"),
        ));
    }
    scope.number(arg, name, line)
}

/// The contents of a `{…}` token list, or a refusal naming what was wanted.
fn token_list<'a>(arg: &'a text::Arg, name: &str, line: usize) -> Result<&'a str, AsmError> {
    let text::Arg::Bare(raw) = arg else {
        return Err(AsmError::new(
            line,
            format!("`{name}` takes a token list in braces, and was given a string"),
        ));
    };
    raw.trim()
        .strip_prefix('{')
        .and_then(|t| t.strip_suffix('}'))
        .ok_or_else(|| {
            AsmError::new(
                line,
                format!("`{name}` takes a token list in braces, and `{raw}` has none"),
            )
        })
}

/// The source slice covering `tokens[from..to]`, clamped to what is there —
/// ca65 stops at the end of the list rather than refusing.
fn token_slice<'a>(list: &'a str, tokens: &[Token<'a>], from: usize, to: usize) -> &'a str {
    let to = to.min(tokens.len());
    if from >= to {
        return "";
    }
    &list[tokens[from].start..tokens[to - 1].end]
}

/// One `%` conversion in a ca65 format string.
///
/// C's shape, and mostly C's rules — but not all of them, and the differences
/// were read off ca65 V2.18 rather than assumed:
///
/// - **`%x` is signed and `%X` is not.** `%x` of `-255` is `-ff`; `%X` of the
///   same value is `FFFFFFFFFFFFFF01`, the 64-bit pattern.
/// - **`%s` and `%c` align the other way round.** `%6s` pads on the *right*
///   and `%-6s` on the left, which is the reverse of every other type here.
/// - **`#` on `%x` shows even for zero.** `%#x` of `0` is `0x0`, where C
///   suppresses the prefix.
/// - Every flag is accepted on every type, repeats included, and ignored where
///   it means nothing. Only an unknown *type* is "Invalid format string".
#[derive(Default)]
struct FormatSpec {
    sign: Option<char>,
    prefix: bool,
    left: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
    kind: char,
}

/// Parse one spec, starting just after the `%`, and answer it with the index of
/// the first byte past it.
fn format_spec(fmt: &[u8], mut i: usize) -> Option<(FormatSpec, usize)> {
    let mut spec = FormatSpec::default();
    // Flags in any order, and a repeat is not an error.
    while let Some(c) = fmt.get(i) {
        match c {
            b'-' => spec.left = true,
            b'+' => spec.sign = Some('+'),
            b' ' => {
                if spec.sign.is_none() {
                    spec.sign = Some(' ');
                }
            }
            b'#' => spec.prefix = true,
            b'0' => spec.zero = true,
            _ => break,
        }
        i += 1;
    }
    while let Some(d) = fmt.get(i).filter(|c| c.is_ascii_digit()) {
        spec.width = spec.width * 10 + usize::from(d - b'0');
        i += 1;
    }
    if fmt.get(i) == Some(&b'.') {
        i += 1;
        let mut precision = 0usize;
        while let Some(d) = fmt.get(i).filter(|c| c.is_ascii_digit()) {
            precision = precision * 10 + usize::from(d - b'0');
            i += 1;
        }
        spec.precision = Some(precision);
    }
    spec.kind = *fmt.get(i)? as char;
    if !matches!(spec.kind, 'd' | 'i' | 'u' | 'x' | 'X' | 'o' | 'c' | 's') {
        return None;
    }
    Some((spec, i + 1))
}

/// Fold one `.sprintf` call.
type StringBytes<'a> = dyn Fn(&text::Arg, &str, usize) -> Result<Vec<u8>, AsmError> + 'a;

fn sprintf(
    bytes: &[u8],
    args: &[text::Arg],
    string_bytes: &StringBytes<'_>,
    scope: &text::Scope,
    line: usize,
) -> Result<Vec<u8>, AsmError> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut used = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'%' {
                i += 1;
            }
            out.extend_from_slice(&bytes[start..i]);
            continue;
        }
        if bytes.get(i + 1) == Some(&b'%') {
            out.push(b'%');
            i += 2;
            continue;
        }
        let (spec, next) = format_spec(bytes, i + 1).ok_or_else(|| {
            AsmError::new(
                line,
                format!(
                    "`.sprintf` cannot read the format spec for argument {}: ca65 takes \
                     `d`, `i`, `u`, `x`, `X`, `o`, `c` and `s`, and no others",
                    used + 1
                ),
            )
        })?;
        let arg = args.get(used).ok_or_else(|| {
            AsmError::new(
                line,
                format!(
                    "`.sprintf` has {} format spec(s) and was given {} argument(s) to fill them",
                    used + 1,
                    args.len()
                ),
            )
        })?;
        out.extend(format_one(&spec, arg, string_bytes, scope, line)?);
        used += 1;
        i = next;
    }
    if used < args.len() {
        return Err(AsmError::new(
            line,
            format!(
                "`.sprintf` was given {} argument(s) and has {used} format spec(s) to put \
                 them in",
                args.len()
            ),
        ));
    }
    Ok(out)
}

/// Render one argument through one spec.
fn format_one(
    spec: &FormatSpec,
    arg: &text::Arg,
    string_bytes: &StringBytes<'_>,
    scope: &text::Scope,
    line: usize,
) -> Result<Vec<u8>, AsmError> {
    let kind = spec.kind;
    let refuse = |what: &str| AsmError::new(line, format!("`.sprintf` cannot format {what}"));

    // `%s` and `%c` share a padding rule that is the reverse of the numeric
    // types': no flag pads on the right, and `-` pads on the left.
    if matches!(kind, 's' | 'c') {
        let mut body = if kind == 's' {
            let mut bytes = string_bytes(arg, ".sprintf", line)?;
            if let Some(n) = spec.precision {
                bytes.truncate(n);
            }
            bytes
        } else {
            if StringTokens::is_string(arg) {
                return Err(refuse("a string as `%c`"));
            }
            let value = scope.number(arg, ".sprintf", line)?;
            let byte = u8::try_from(value)
                .ok()
                .filter(|b| *b != 0)
                .ok_or_else(|| {
                    refuse(&format!("{value} as `%c`: a character runs from 1 to 255"))
                })?;
            vec![byte]
        };
        let pad = spec.width.saturating_sub(body.len());
        if spec.left {
            let mut padded = vec![b' '; pad];
            padded.extend(body);
            body = padded;
        } else {
            body.resize(body.len() + pad, b' ');
        }
        return Ok(body);
    }

    if StringTokens::is_string(arg) {
        return Err(refuse(&format!("a string as `%{kind}`")));
    }
    let value = scope.number(arg, ".sprintf", line)?;
    // `%d`, `%i` and `%x` read the value signed; `%u`, `%X` and `%o` read the
    // same bits as a 64-bit unsigned, which is why `%x` and `%X` disagree about
    // a negative one.
    let (negative, magnitude) = match kind {
        'd' | 'i' | 'x' => (value < 0, value.unsigned_abs()),
        _ => (false, value as u64),
    };
    let digits = match kind {
        'x' => format!("{magnitude:x}"),
        'X' => format!("{magnitude:X}"),
        'o' => format!("{magnitude:o}"),
        _ => magnitude.to_string(),
    };
    // A precision is a minimum digit count, and a precision of zero renders a
    // zero value as nothing at all.
    let digits = match spec.precision {
        Some(0) if magnitude == 0 => String::new(),
        Some(n) => format!("{:0>n$}", digits, n = n),
        None => digits,
    };
    let sign = if negative {
        "-".to_string()
    } else {
        spec.sign.map(String::from).unwrap_or_default()
    };
    let prefix = match (spec.prefix, kind) {
        (true, 'x') => "0x",
        (true, 'X') => "0X",
        // The octal prefix is a leading zero, so it is not written twice when
        // the digits already start with one — `%#.4o` of 9 is `0011`.
        (true, 'o') if !digits.starts_with('0') => "0",
        _ => "",
    };
    let width = sign.len() + prefix.len() + digits.chars().count();
    let pad = spec.width.saturating_sub(width);
    Ok((match () {
        _ if spec.left => format!("{sign}{prefix}{digits}{:pad$}", "", pad = pad),
        // A precision has already said how many digits there are, so the zero
        // flag has nothing left to say.
        _ if spec.zero && spec.precision.is_none() => {
            format!("{sign}{prefix}{:0>pad$}{digits}", "", pad = pad)
        }
        _ => format!("{:pad$}{sign}{prefix}{digits}", "", pad = pad),
    })
    .into_bytes())
}

/// Whether the text is one identifier or one number literal — what `.string`
/// accepts.
fn is_one_token(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '$' || c == '%' || c == '.'
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

    // The text layer's functions (`decisions/string-and-text-layer.md`).
    // Assembly never arrives here: the pass folds them to text before the parse
    // runs. This arm is the **formatter's**, which parses without expanding and
    // then re-emits the call from its source — so all it needs is a value that
    // parses. The mark cannot be a ca65 identifier, so a fold that somehow did
    // not happen fails as an unresolved symbol rather than answering a number.
    if is_text_function(&lower) || lower == ".definedmacro" {
        return Ok(Expr::Sym(format!("{TEXT_MARK}{lower}")));
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
    #[test]
    fn nested_live_string_queries_preserve_definition_diagnostics() {
        use crate::span::{ExpansionFrame, FileId, Span};
        struct Reject(Span);
        impl super::LiveNumbers for Reject {
            fn literal(
                &self,
                _text: &str,
                _line: usize,
            ) -> Result<Option<i64>, crate::engine::AsmError> {
                unreachable!("sprintf numeric argument")
            }
            fn number(
                &self,
                _text: &str,
                _line: usize,
            ) -> Result<Option<i64>, crate::engine::AsmError> {
                let mut error = crate::engine::AsmError::unsupported(
                    self.0.line as usize,
                    "definition cannot be evaluated",
                );
                error.span = Some(self.0.clone());
                Err(error)
            }
            fn constancy(&self, _text: &str, _line: usize) -> Result<i64, crate::engine::AsmError> {
                unreachable!("numeric sprintf argument")
            }
        }
        let mut span = Span::in_file(FileId(3), 7, 4);
        span.expansion_frames.push(ExpansionFrame {
            macro_name: "definition".to_string(),
            defined_at: Some(Box::new(Span::in_file(FileId(3), 2, 1))),
            invoked_at: Box::new(Span::in_file(FileId(0), 11, 1)),
        });
        let strings = super::StringTokens::default();
        let numbers = std::collections::BTreeMap::new();
        let defined = std::collections::BTreeSet::new();
        let error = super::expand_ca65_live(
            ".strlen(.sprintf(\"%d\",VALUE))",
            17,
            super::LiveText {
                target: super::Target::Mos6502,
                token_target: None,
                numbers: &numbers,
                strings: &strings,
                defined: &defined,
                context: &Reject(span.clone()),
            },
        )
        .expect_err("nested read must refuse");
        assert_eq!(error.span, Some(span));
        assert_eq!(error.code, crate::contract::Code::UnsupportedFeature);
        assert_eq!(error.message, "definition cannot be evaluated");
    }

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
