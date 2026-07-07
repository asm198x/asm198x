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
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let Some(d) = w
            .walk_line(raw, line, file)
            .map_err(|e| stamp_file(e, file))?
        else {
            continue;
        };
        let span = d.span;
        // Diagnostics point at the directive's operand (the file name) when
        // the parse knew its column, else the line.
        let at = d.operand_span.clone().unwrap_or_else(|| span.clone());
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
                let id = load_include_defaulted(map, loader, &request, stack, line as u32, sem)
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
                walk_file(w, &contents, id, map, loader, stack, sem)?;
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
                let data = load_binary(map, loader, &request, stack, sem.resolution)
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
/// found). A failure reports the request **as written**, not the defaulted
/// spelling. Dialects without defaulting resolve the request directly.
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
        && let Ok(id) = load_include(
            map,
            loader,
            &format!("{request}.{ext}"),
            stack,
            line,
            sem.resolution,
        )
    {
        return Ok(id);
    }
    load_include(map, loader, request, stack, line, sem.resolution)
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
