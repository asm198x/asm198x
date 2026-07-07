//! Shared `INCLUDE`/`BINCLUDE` machinery for the **asl-syntax chips** — the
//! twelve dialects whose reference arbiter is `asl` (Macroassembler AS):
//! 8080, 6800, 1802, 8048, SC/MP, F8, 2650, TMS7000, PDP-11, TMS9900, CP1610,
//! Z8000. Language-surface U4, KTD1/KTD5/KTD8.
//!
//! asl's multi-file surface is **uniform across chips** (probe-pinned on the
//! 8080, spot-checked on the TMS9900 and CP1610 — asl 1.42), so one skin
//! serves the family. The chips also share one parse-loop shape — comment
//! split, `NAME EQU expr` constants, a column-0 `label:`, then the operation
//! parse with the live constants — so the whole per-line walk lives here as a
//! generic [`Walker`] over the small per-dialect [`AslChip`] seam; a dialect
//! supplies its own helpers (its comment scanner, its number lexer for the
//! `BINCLUDE` window arguments, its operation parse) and inherits the
//! multi-file walk. The recursion driver itself is
//! [`ca65_flat::walk_file`] with asl's probe-pinned [`WalkSemantics`].
//!
//! **The probe-pinned asl semantics (KTD5):**
//!
//! - `INCLUDE file` / `INCLUDE "file"` — the name may be **quoted or bare**
//!   (a bare name ends at whitespace or a comma); the keyword is
//!   case-insensitive. An **extensionless** request tries `name.inc` first
//!   and the exact spelling second (probe: with both `bare` and `bare.inc`
//!   present, `.inc` wins; with only `bare`, it is found) — the
//!   [`WalkSemantics::include_default_ext`] hook.
//! - **Resolution** anchors at the requesting file's **own directory only**
//!   ([`Resolution::Requester`]): the process cwd is *not* searched and a
//!   root-directory copy is *not* found from inside a subdirectory include
//!   (probe p3). asl's `-i` search paths apply after that — our `-I` dirs
//!   ride the loader's per-attempt fallback, the same surface.
//! - A label on the directive line binds at the include point / payload
//!   start (`here: INCLUDE …` then `lxi h,here` → the include's address).
//! - State threads through the boundary in both directions: an `EQU` defined
//!   inside an include is consumed by the includer's later lines (KTD1's
//!   interleaved walk).
//! - `BINCLUDE "file"[, offset[, length]]` — quoted or bare, **no** extension
//!   defaulting. The window is **strict** ([`slice_binclude`]): a negative
//!   offset or length is an error (asl: "unexpected end of file" / "address
//!   overflow" — no ca65-style rest-of-file sentinel, no lwasm-style
//!   from-EOF counting), any window past EOF is an error, and offset-at-EOF /
//!   length-0 are legal and empty.
//! - **CP1610** (`cpu CP-1600`): `BINCLUDE`'s offset/length count **bytes**,
//!   and an N-byte window advances the location counter by **N decles** with
//!   the image carrying the N raw file bytes followed by N zero bytes (probed
//!   with 3-, 4- and 8-byte assets; an odd count is *not* an error and is
//!   *not* packed two-per-decle). [`slice_binclude_cp1610`] reproduces that
//!   byte-identically: window ++ zeros(window.len()) is 2N bytes = exactly N
//!   address units under the CP1610's `addr_unit = 2`.

use std::collections::BTreeMap;

use super::ca65_flat::{self, DirectiveLine, FlatWalk, Resolution, WalkDirective, WalkSemantics};
use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
use crate::engine::{AsmError, Expr, Operation};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

use super::mos6502::{fold_const, split_first_word, split_top_level};

/// asl's probe-pinned multi-file semantics, shared by eleven of the twelve
/// chips: requester-directory resolution, the strict `BINCLUDE` window, and
/// the `.inc` extension default on `INCLUDE`.
pub(crate) const SEMANTICS: WalkSemantics = WalkSemantics {
    resolution: Resolution::Requester,
    window: slice_binclude,
    include_default_ext: Some("inc"),
};

/// The CP1610's semantics: identical to [`SEMANTICS`] except the window,
/// which appends the probe-pinned zero tail so an N-byte window occupies N
/// decles ([`slice_binclude_cp1610`]).
pub(crate) const CP1610_SEMANTICS: WalkSemantics = WalkSemantics {
    resolution: Resolution::Requester,
    window: slice_binclude_cp1610,
    include_default_ext: Some("inc"),
};

/// The per-dialect seam the shared walk drives: each asl chip supplies its
/// own helpers — the functions its `parse_program` loop already used — and
/// the generic [`Walker`] owns the loop itself, so the twelve dialects cannot
/// drift apart on the multi-file surface.
pub(crate) trait AslChip {
    /// Split a line into its code and its comment (the module's own scanner —
    /// they differ in string/char quoting details).
    fn split_comment<'a>(&self, line: &'a str) -> (&'a str, Option<&'a str>);

    /// `NAME EQU expr` / `NAME = expr` recognition: the name, the value
    /// expression, and the operation's verbatim source for the formatter.
    ///
    /// # Errors
    /// A malformed constant line (a bad value expression).
    fn constant(&self, code: &str, line: usize)
    -> Result<Option<(String, Expr, String)>, AsmError>;

    /// Split a leading column-0 `label:` from the line.
    fn split_label<'a>(&self, code: &'a str) -> (Option<String>, &'a str);

    /// Parse the operation part with the live constants. `&mut self` so a
    /// stateful dialect (the CP1610's `SDBD` prefix flag) can thread its
    /// state through the walk.
    ///
    /// # Errors
    /// Any tokenising or mode-resolution failure.
    fn parse_op(
        &mut self,
        rest: &str,
        consts: &BTreeMap<String, i64>,
        line: usize,
    ) -> Result<Option<Operation>, AsmError>;

    /// The dialect's expression parser (its own number lexer), for folding
    /// `BINCLUDE`'s offset/length arguments against the live constants.
    ///
    /// # Errors
    /// An unparseable expression.
    fn value(&self, raw: &str, line: usize) -> Result<Expr, AsmError>;

    /// The operand-field span for an operation line — `Some` only on the
    /// dialects that carry column-accurate spans (core-contract U3: 8080,
    /// 6800, 1802, SC/MP); everything else stays line-granular.
    fn operand_span(&self, _raw: &str, _rest: &str, _line: usize) -> Option<Span> {
        None
    }
}

/// The shared per-line walk (the family's one parse-loop shape), generic over
/// the [`AslChip`] seam. The environment — the `EQU` constants and pending
/// comment trivia — lives here, so in the multi-file walk it threads
/// *through* include boundaries in both directions (KTD1, probe-pinned).
pub(crate) struct Walker<C> {
    chip: C,
    /// `EQU`/`=` bindings, consulted by the chip's operation parse (form
    /// selection: `rst` vectors, `ds` counts) and `BINCLUDE` argument folding.
    consts: BTreeMap<String, i64>,
    /// Own-line comments seen since the last node, attached as leading trivia
    /// to the next one. Comments never reach the encoder, so bytes are
    /// unchanged.
    pending_leading: Vec<Comment>,
    nodes: Vec<Node>,
}

/// Parse a single-source asl-chip program: the same walk as the multi-file
/// entry, but an `INCLUDE`/`BINCLUDE` stays an **unresolved**
/// [`Item::Include`](crate::ast::Item) / [`Item::Incbin`](crate::ast::Item) —
/// the target is never opened (KTD1), so `--fmt` renders the directive
/// verbatim and works with a missing target, and `lower` rejects assembly
/// with a pointer to the multi-file entry points.
///
/// # Errors
/// Any per-line parse failure.
pub(crate) fn parse_single<C: AslChip>(chip: C, source: &str) -> Result<Program, AsmError> {
    let mut w = Walker::new(chip);
    for (i, raw) in source.lines().enumerate() {
        if let Some(d) = w.walk_line(raw, i + 1, FileId(0))? {
            w.push_node(ca65_flat::unresolved_node(d));
        }
    }
    w.flush_trailing(source.lines().count() as u32);
    Ok(Program { nodes: w.nodes })
}

/// Parse a multi-file asl-chip program (language-surface U4, KTD1): the
/// **interleaved, environment-threaded walk**. The root (`FileId(0)` in
/// `map`) parses line by line with the environment accumulated so far; when
/// the walk reaches an `INCLUDE` live, the target loads through `loader`
/// (anchored at the requesting file's own directory — asl's probe-pinned
/// order, with the `.inc` extension default), its lines parse with the same
/// environment, and an `EQU` it defined feeds the includer's later lines.
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing target, an include cycle, a bad `BINCLUDE` window, or the depth
/// backstop — all at the directive's span.
pub(crate) fn parse_multi_files<C: AslChip>(
    chip: C,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    sem: &WalkSemantics,
) -> Result<Program, AsmError> {
    let mut w = Walker::new(chip);
    let root = map.contents(FileId(0)).unwrap_or_default().to_owned();
    let mut stack = vec![FileId(0)];
    ca65_flat::walk_file(&mut w, &root, FileId(0), map, loader, &mut stack, sem)?;
    Ok(Program { nodes: w.nodes })
}

impl<C: AslChip> Walker<C> {
    fn new(chip: C) -> Self {
        Self {
            chip,
            consts: BTreeMap::new(),
            pending_leading: Vec::new(),
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

    /// Recognise a walk-handled `INCLUDE`/`BINCLUDE` operation (keywords are
    /// case-insensitive) and parse its arguments with the live environment:
    /// the file name may be quoted **or bare** (probe-pinned), junk after a
    /// name is rejected, and `BINCLUDE`'s offset/length fold against the
    /// constants known so far through the chip's own expression parser.
    fn walk_directive(&self, rest: &str, line: usize) -> Result<Option<WalkDirective>, AsmError> {
        let (word, args) = split_first_word(rest);
        match word.to_ascii_lowercase().as_str() {
            "include" => {
                let (request, tail) = file_name(args, line, "include")?;
                if !tail.trim().is_empty() {
                    return Err(AsmError::new(
                        line,
                        format!("unexpected `{}` after the `include` file name", tail.trim()),
                    ));
                }
                Ok(Some(WalkDirective::Include { request }))
            }
            "binclude" => {
                let (request, tail) = file_name(args, line, "binclude")?;
                let tail = tail.trim();
                let (offset, size) = if tail.is_empty() {
                    (None, None)
                } else if let Some(list) = tail.strip_prefix(',') {
                    let pieces = split_top_level(list, ',');
                    if pieces.len() > 2 {
                        return Err(AsmError::new(
                            line,
                            "`binclude` takes at most a file name, an offset, and a length",
                        ));
                    }
                    let fold = |what: &str, piece: &str| -> Result<i64, AsmError> {
                        fold_const(&self.chip.value(piece.trim(), line)?, &self.consts, line)
                            .map_err(|e| {
                                AsmError::new(
                                    line,
                                    format!(
                                        "`binclude` {what} must be a constant expression: {}",
                                        e.message
                                    ),
                                )
                            })
                    };
                    let offset = fold("offset", pieces[0])?;
                    let size = pieces.get(1).map(|p| fold("length", p)).transpose()?;
                    (Some(offset), size)
                } else {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "expected `,offset[,length]` after the `binclude` file name, \
                             found `{tail}`"
                        ),
                    ));
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

impl<C: AslChip> FlatWalk for Walker<C> {
    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        let (code, comment) = self.chip.split_comment(raw);
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

        // `NAME EQU expr` / `NAME = expr` — a constant binds its label on the
        // same line, so the label cannot split off (the formatter keeps it
        // there).
        if let Some((name, expr, op_source)) = self.chip.constant(code.trim(), line)? {
            if let Ok(v) = fold_const(&expr, &self.consts, line) {
                self.consts.insert(name.clone(), v);
            }
            self.nodes.push(Node {
                operand_span: None,
                label: Some(global_symbol(name)),
                item: Some(crate::ast::item_from_operation(Operation::Equ(expr))),
                source: op_source,
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        let (label, rest) = self.chip.split_label(code);
        // `INCLUDE`/`BINCLUDE` are walk-handled, not directives: the target
        // must not be opened here (KTD1 — `--fmt` succeeds with a missing
        // target), so hand them back for the driver to resolve (or keep
        // unresolved, in the single-source parse). A label on the line binds
        // at the include point / payload start (probe-pinned).
        if let Some(kind) = self.walk_directive(rest, line)? {
            return Ok(Some(DirectiveLine {
                kind,
                label: label.map(global_symbol),
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
            self.chip.parse_op(rest, &self.consts, line)?
        };
        if label.is_none() && op.is_none() {
            return Ok(None);
        }
        self.nodes.push(Node {
            operand_span: self.chip.operand_span(raw, rest, line).map(|mut s| {
                s.file = file;
                s
            }),
            label: label.map(global_symbol),
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

/// The asl chips have no local-label scoping: every label is a global symbol
/// whose qualified name is the source name.
fn global_symbol(name: String) -> Symbol {
    Symbol {
        qualified: name.clone(),
        scope: Scope::Global,
        name,
    }
}

/// The file-name operand of an asl `include`/`binclude`: a quoted string
/// (the rest of the line after the closing quote is returned for the caller)
/// or a bare name ending at whitespace or a comma — probe-pinned: asl accepts
/// both spellings on both directives.
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

/// Apply asl's `BINCLUDE` window to the loaded asset — probe-pinned (see the
/// module docs): a negative offset or length is an error (asl: "unexpected
/// end of file" / "address overflow"), offset at EOF or length 0 are legal
/// and empty, any window past EOF is an error. `Err` carries the message
/// body; the driver wraps it with the request name and the directive's span.
fn slice_binclude(data: &[u8], offset: Option<i64>, size: Option<i64>) -> Result<Vec<u8>, String> {
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
        None => remaining,
        Some(s) if s < 0 => {
            return Err(format!("length must not be negative, got {s}"));
        }
        Some(s) => s,
    };
    if take > remaining {
        return Err(format!(
            "length {take} exceeds the {remaining} byte(s) after offset {off}"
        ));
    }
    Ok(data[off as usize..(off + take) as usize].to_vec())
}

/// The CP1610's `BINCLUDE` window: asl's strict byte window, then the
/// probe-pinned zero tail — an N-byte window occupies N **decles**, the image
/// carrying the N raw bytes followed by N zeros (`cpu CP-1600` + p2bin,
/// probed with 3-, 4- and 8-byte assets and windows). 2N bytes is exactly N
/// address units under `addr_unit = 2`, so pass-1/pass-2 accounting and the
/// emitted bytes both match asl with no engine special-casing.
fn slice_binclude_cp1610(
    data: &[u8],
    offset: Option<i64>,
    size: Option<i64>,
) -> Result<Vec<u8>, String> {
    let mut window = slice_binclude(data, offset, size)?;
    let tail = window.len();
    window.extend(std::iter::repeat_n(0u8, tail));
    Ok(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_binclude_matches_the_probe_matrix() {
        let data: &[u8] = &[0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];
        // Plain, offset, offset+length — the happy windows.
        assert_eq!(
            slice_binclude(data, None, None).expect("window"),
            data.to_vec()
        );
        assert_eq!(
            slice_binclude(data, Some(2), None).expect("window"),
            vec![0x12, 0x13, 0x14, 0x15, 0x16, 0x17]
        );
        assert_eq!(
            slice_binclude(data, Some(2), Some(3)).expect("window"),
            vec![0x12, 0x13, 0x14]
        );
        // Offset at EOF and length 0 are legal and empty (probe-pinned).
        assert_eq!(
            slice_binclude(data, Some(8), None).expect("window"),
            Vec::<u8>::new()
        );
        assert_eq!(
            slice_binclude(data, Some(0), Some(0)).expect("window"),
            Vec::<u8>::new()
        );
        // The error postures, all probe-pinned: negative offset, negative
        // length (asl has no sentinels), offset past EOF, length past the
        // remaining bytes.
        assert!(slice_binclude(data, Some(-2), None).is_err());
        assert!(slice_binclude(data, Some(2), Some(-2)).is_err());
        assert!(slice_binclude(data, Some(9), None).is_err());
        assert!(slice_binclude(data, Some(2), Some(7)).is_err());
    }

    #[test]
    fn cp1610_window_appends_the_decle_zero_tail() {
        let data: &[u8] = &[0x10, 0x11, 0x12];
        // 3 bytes -> 3 decles: the raw bytes then 3 zeros (probe-pinned).
        assert_eq!(
            slice_binclude_cp1610(data, None, None).expect("window"),
            vec![0x10, 0x11, 0x12, 0x00, 0x00, 0x00]
        );
        // A window slices in bytes, then tails: `,1,2` -> 11 12 00 00.
        assert_eq!(
            slice_binclude_cp1610(data, Some(1), Some(2)).expect("window"),
            vec![0x11, 0x12, 0x00, 0x00]
        );
        // Empty stays empty; the strict errors pass through.
        assert_eq!(
            slice_binclude_cp1610(data, Some(3), None).expect("window"),
            Vec::<u8>::new()
        );
        assert!(slice_binclude_cp1610(data, Some(4), None).is_err());
    }

    #[test]
    fn file_name_accepts_quoted_and_bare_spellings() {
        assert_eq!(
            file_name(" \"a.inc\" ", 1, "include").expect("quoted"),
            ("a.inc".to_string(), " ")
        );
        assert_eq!(
            file_name(" defs.inc", 1, "include").expect("bare"),
            ("defs.inc".to_string(), "")
        );
        // A bare binclude name ends at the window's comma.
        assert_eq!(
            file_name("data.bin,2", 1, "binclude").expect("bare + args"),
            ("data.bin".to_string(), ",2")
        );
        assert!(file_name(" \"a.inc", 1, "include").is_err());
        assert!(file_name(" \"\"", 1, "include").is_err());
        assert!(file_name("", 1, "include").is_err());
    }
}
