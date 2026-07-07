//! Shared Z80 syntax core for the pasmo-family and sjasmplus dialects.
//!
//! The Z80's mnemonic/operand syntax is the same across assemblers — `ld a,b`,
//! `ld (ix+5),$0a`, `bit 7,(hl)` are written identically — so the bulk of a
//! Z80 front-end (operand classification, the mode-label probe against the
//! [`isa`] spec, the expression parser, the register/condition vocabulary, the
//! common directives) is shared here. A dialect supplies only the two things
//! that genuinely differ via the [`Z80Syntax`] trait: **comment style** and
//! **number formats**. Everything else is reused, so adding a dialect is a
//! handful of lines (see `pasmo.rs`, `sjasmplus.rs`).
//!
//! ## Resolving operands to spec mode labels
//!
//! The Z80 packs registers and conditions into the opcode, so a form's mode is
//! its operand signature (see [`isa::z80`]). Each parsed operand is classified
//! as a *fixed* token (register/condition/indirect), a *value* (immediate or
//! `(nn)` address), or an *indexed* `(IX+d)`. Candidate signature strings are
//! built and probed against the instruction's forms — so `ld a,c` finds form
//! `A,C` while `jr c,loop` finds `C,e`, with no need to pre-judge whether `C`
//! is a register or a flag. Operand width is settled by which form exists.

use std::collections::BTreeMap;

use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
use crate::engine::{AsmError, BinOp, Expr, Operation, Statement};
use crate::source::{MAX_INCLUDE_DEPTH, SourceLoader, SourceMap};
use crate::span::FileId;

/// The per-dialect surface: the parts of Z80 syntax that actually differ
/// between assemblers. Everything else in this module is shared.
pub(crate) trait Z80Syntax {
    /// Strip a line comment, returning the code before it.
    fn strip_comment<'a>(&self, line: &'a str) -> &'a str;

    /// Split a line into its code and its comment (with the delimiter, trailing
    /// whitespace trimmed), for carrying comments as AST trivia (U4). Defined in
    /// terms of [`strip_comment`](Self::strip_comment), which returns the code
    /// prefix, so the comment is exactly what it removed — no behaviour change.
    fn split_comment<'a>(&self, line: &'a str) -> (&'a str, Option<&'a str>) {
        let code = self.strip_comment(line);
        let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
        (code, comment)
    }

    /// Parse a numeric literal token (the dialect's hex/binary/char forms).
    fn parse_number(&self, tok: &str, line: usize) -> Result<i64, AsmError>;

    /// Whether a leading-`.` label is *local* — scoped under the most recent
    /// global (non-`.`) label, so the same `.loop` may recur in different
    /// scopes (sjasmplus). Defaults off: a leading-`.` name is then an ordinary
    /// global identifier (pasmo), and reusing it is a duplicate-label error.
    fn scopes_locals(&self) -> bool {
        false
    }

    /// Whether `word` names a directive. Defaults to the common set.
    fn is_directive(&self, word: &str) -> bool {
        is_common_directive(word)
    }

    /// Whether `^` is the bitwise-XOR operator. sjasmplus has it; pasmo does
    /// not (and rejects `^`), so it defaults off to match pasmo.
    fn has_xor_operator(&self) -> bool {
        false
    }

    /// Whether `word` is this dialect's include directive (language-surface
    /// U2). Off by default: sjasmplus overrides for `INCLUDE`; pasmo's
    /// include lands in U4. An include is walk-handled (a verbatim item in
    /// the single-source parse, a lazy load in the multi-file walk), never an
    /// [`Operation`].
    fn is_include(&self, word: &str) -> bool {
        let _ = word;
        false
    }

    /// Whether `word` is this dialect's binary-inclusion directive
    /// (language-surface U3). Off by default; sjasmplus and pasmo override for
    /// `INCBIN`. Like an include, an incbin is walk-handled: a verbatim item
    /// in the single-source parse (so `--fmt` never opens the asset), a lazy
    /// binary load in the multi-file walk.
    fn is_incbin(&self, word: &str) -> bool {
        let _ = word;
        false
    }

    /// Whether this dialect's incbin takes the `,offset[,length]` tail.
    /// sjasmplus does (probe-pinned, incl. the negative from-the-end forms);
    /// pasmo does not — its reference rejects a comma after the file name
    /// (`End line expected but ','found`), so the tail stays a parse error.
    fn incbin_offset_length(&self) -> bool {
        false
    }

    /// Whether `<file>` is a quote form for the incbin file name. sjasmplus
    /// accepts it (as its INCLUDE does); pasmo takes the token verbatim — it
    /// looks for a file literally named `<file>` (probe-pinned).
    fn incbin_angle_quotes(&self) -> bool {
        false
    }

    /// Parse a directive into an operation (`None` for ones that emit nothing,
    /// like `end`). Defaults to the common set. `consts` holds the `equ` values
    /// known so far, so a directive like `ds` can fold a constant-expression
    /// count (`ds MAX*2`) at parse time.
    fn parse_directive(
        &self,
        word: &str,
        args: &str,
        line: usize,
        consts: &BTreeMap<String, i64>,
    ) -> Result<Option<Operation>, AsmError>
    where
        Self: Sized,
    {
        common_directive(self, word, args, line, consts)
    }
}

/// Assemble Z80 source under `syntax` into the engine's statement stream, using
/// `set` (and optional Z80N `ext`) for the instruction encodings.
pub(crate) fn assemble<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    source: &str,
) -> Result<Vec<Statement>, AsmError> {
    // The Z80 front-end parses into the semantic AST (U3), then lowers it to the
    // engine's statement stream — byte-identical to the old direct parse (AE1).
    // Other CPUs stay on direct lowering behind this boundary (KTD6).
    crate::ast::lower(parse_program(syntax, set, ext, source)?)
}

/// Parse Z80 source into the semantic [`Program`](crate::ast::Program). Each line
/// becomes a node carrying its scoped label, operation, and span; trivia is
/// filled in U4. The scope resolution mirrors the old string-mangle exactly, so
/// [`lower`](crate::ast::lower) reproduces the same statements.
///
/// An include or incbin directive (a dialect that answers
/// [`Z80Syntax::is_include`] / [`Z80Syntax::is_incbin`]) becomes an
/// **unresolved** [`Item::Include`](crate::ast::Item) /
/// [`Item::Incbin`](crate::ast::Item) — the target is never opened, so
/// `--fmt` renders the directive verbatim and works with a missing target
/// (U2/U3, KTD1). Lazy resolution is [`parse_program_multi`]'s.
pub(crate) fn parse_program<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    source: &str,
) -> Result<crate::ast::Program, AsmError> {
    let mut w = Walker::new(syntax, set, ext);
    for (i, raw) in source.lines().enumerate() {
        if let Some(d) = w.walk_line(raw, i + 1, FileId(0))? {
            // Unresolved in the single-source parse: the target is never
            // opened (KTD1), so `--fmt` renders the verbatim source and works
            // with a missing file; `lower` rejects assembly with a pointer to
            // the multi-file entry points.
            let item = match d.kind {
                WalkDirective::Include { request } => crate::ast::Item::Include { request },
                WalkDirective::Incbin { request, .. } => crate::ast::Item::Incbin { request },
            };
            w.nodes.push(Node {
                operand_span: d.operand_span,
                label: d.label,
                item: Some(item),
                source: d.source,
                span: d.span,
                trivia: d.trivia,
            });
        }
    }
    w.flush_trailing(source.lines().count() as u32);
    Ok(Program { nodes: w.nodes })
}

/// Parse a multi-file Z80 program (language-surface U2, KTD1): the
/// **interleaved, environment-threaded walk**. The root (`FileId(0)` in
/// `map`) parses line by line with the environment accumulated so far; when
/// the walk reaches an include directive *live*, the target loads through
/// `loader`, its lines parse with the same environment, and everything it
/// defined — `equ` constants, the current global label — flows back out to
/// the includer's subsequent lines. That outward flow is load-bearing: z80
/// form selection (`bit`/`rst`/`ds`) consults the parse-time constants table,
/// so an include-defined constant must be visible after the include point
/// (probe-pinned against sjasmplus).
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing include target, an include cycle (the active-stack check), or the
/// [`MAX_INCLUDE_DEPTH`] backstop — all at the directive's span.
pub(crate) fn parse_program_multi<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
) -> Result<crate::ast::Program, AsmError> {
    let mut w = Walker::new(syntax, set, ext);
    let root = map.contents(FileId(0)).unwrap_or_default().to_owned();
    // The active include stack: cycle detection is membership (a file may be
    // included twice *sequentially* — the reference re-reads it — but never
    // while it is still open).
    let mut stack = vec![FileId(0)];
    walk_file(&mut w, &root, FileId(0), map, loader, &mut stack)?;
    Ok(Program { nodes: w.nodes })
}

/// One file's leg of the multi-file walk: parse each line through the shared
/// [`Walker`], and recurse into includes as they are reached.
fn walk_file<S: Z80Syntax>(
    w: &mut Walker<'_, S>,
    source: &str,
    file: FileId,
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    stack: &mut Vec<FileId>,
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
                    w.nodes.push(Node {
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
                let id = map
                    .load(loader, &request, file, line as u32)
                    .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
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
                walk_file(w, &contents, id, map, loader, stack)?;
                stack.pop();
            }
            WalkDirective::Incbin {
                request,
                offset,
                length,
            } => {
                // Resolved lazily, exactly like an include (KTD1): the asset
                // loads only when the walk reaches the directive live. The
                // binary path mints no FileId (KTD8) — the payload rides a
                // node at the *directive's* span, which is where the missing
                // asset / window diagnostics land too.
                let from = map.path(file).map(str::to_owned);
                let data = loader
                    .load_binary(&request, from.as_deref())
                    .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
                let payload = slice_incbin(&data, offset, length)
                    .map_err(|msg| AsmError::at(at.clone(), format!("`{request}`: {msg}")))?;
                w.nodes.push(Node {
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

/// Stamp `file` onto a per-line parse error: the line-oriented helpers below
/// (`split_label`, `parse_op`, the expression parser) know their line but not
/// their file, so the walk supplies it at the one per-line boundary.
fn stamp_file(mut e: AsmError, file: FileId) -> AsmError {
    match &mut e.span {
        Some(span) => span.file = file,
        None if e.line != 0 => e.span = Some(Span::in_file(file, e.line as u32, 0)),
        None => {}
    }
    e
}

/// A walk-handled directive found by [`Walker::walk_line`], handed back for
/// the driver to decide: the single-source parse keeps it as a verbatim item;
/// the multi-file walk resolves it lazily (KTD1).
struct DirectiveLine {
    kind: WalkDirective,
    /// A label on the directive line — it binds at the directive's address.
    label: Option<Symbol>,
    /// The verbatim directive text (`include "file.inc"`), for `--fmt`.
    source: String,
    span: Span,
    operand_span: Option<Span>,
    trivia: Trivia,
}

/// Which walk-handled directive a [`DirectiveLine`] carries.
enum WalkDirective {
    /// `INCLUDE "file"` — the target as the directive spelled it
    /// (quotes/brackets stripped).
    Include { request: String },
    /// `INCBIN "file"[,offset[,length]]` (U3). The offset/length are folded to
    /// parse-time constants (they set the statement's size, exactly like a
    /// `ds` count); `None` means the argument was omitted. Negative values
    /// keep sjasmplus's from-the-end meaning, applied when the asset's size is
    /// known ([`slice_incbin`]).
    Incbin {
        request: String,
        offset: Option<i64>,
        length: Option<i64>,
    },
}

/// The per-line parse walk shared by [`parse_program`] (single source) and
/// [`parse_program_multi`] (the include-capable walk). The environment — the
/// `equ` constants table, the current global label for local scoping, and
/// pending comment trivia — lives here, so in the multi-file walk it threads
/// *through* include boundaries in both directions (KTD1).
struct Walker<'a, S: Z80Syntax> {
    syntax: &'a S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    /// Constants defined with `equ`, recorded as parsed. Opcode-embedded
    /// operands (BIT n, IM n, RST n) must be known at parse time to pick the
    /// form, so they resolve against this — not the engine's pass-2 symbols.
    consts: BTreeMap<String, i64>,
    /// The most recent global (non-`.`) label, for qualifying local labels.
    current_global: Option<String>,
    /// Own-line comments seen since the last node, attached as leading trivia
    /// to the next node (U4). Comments never reach the encoder (AE1).
    pending_leading: Vec<Comment>,
    nodes: Vec<Node>,
}

impl<'a, S: Z80Syntax> Walker<'a, S> {
    fn new(
        syntax: &'a S,
        set: &'static isa::InstructionSet,
        ext: Option<&'static isa::InstructionSet>,
    ) -> Self {
        Self {
            syntax,
            set,
            ext,
            consts: BTreeMap::new(),
            current_global: None,
            pending_leading: Vec::new(),
            nodes: Vec::new(),
        }
    }

    /// Parse one line with the live environment. An ordinary line pushes its
    /// node (or nothing, for a blank/comment line) and returns `None`; a
    /// walk-handled directive (include/incbin) is returned for the driver.
    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        let (code, comment) = self.syntax.split_comment(raw);
        if code.trim().is_empty() {
            // A comment-only line becomes leading trivia for the next node; a
            // blank line carries nothing.
            if let Some(text) = comment {
                self.pending_leading.push(Comment {
                    text: text.to_string(),
                    span: Span::in_file(file, line as u32, 1),
                });
            }
            return Ok(None);
        }
        let (label, rest) = split_label(self.syntax, self.set, self.ext, code, line)?;
        // Includes and incbins are walk-handled, not Operations: the target
        // must not be opened here (KTD1 — `--fmt` succeeds with a missing
        // target, and an untaken conditional branch must never load one once
        // U8 lands).
        let (word, args) = split_first_word(rest);
        let is_include = self.syntax.is_include(word);
        let is_incbin = self.syntax.is_incbin(word);
        let mut op = if is_include || is_incbin {
            None
        } else {
            parse_op(self.syntax, self.set, self.ext, rest, line, &self.consts)?
        };

        // Resolve the label's scope into a `Symbol` (source name, scope, and the
        // qualified name lowering emits). A leading-`.` label is local to the
        // current scope; a plain label opens a new scope. Update the scope first,
        // so a local reference on the same line (`done: jr .loop`) resolves
        // against it — matching the old ordering.
        let scoped = self.syntax.scopes_locals();
        let symbol = label.map(|name| {
            if scoped && name.starts_with('.') {
                match &self.current_global {
                    Some(g) => Symbol {
                        qualified: format!("{g}{name}"),
                        scope: Scope::Local {
                            in_global: g.clone(),
                        },
                        name,
                    },
                    // A leading-`.` label with no enclosing global is left as-is
                    // (the old code qualified only when a global existed).
                    None => Symbol {
                        qualified: name.clone(),
                        scope: Scope::Global,
                        name,
                    },
                }
            } else {
                if scoped {
                    self.current_global = Some(name.clone());
                }
                Symbol {
                    qualified: name.clone(),
                    scope: Scope::Global,
                    name,
                }
            }
        });
        if scoped && let Some(g) = &self.current_global {
            op = op.map(|o| qualify_locals(o, g));
        }

        // `equ` binds its (qualified) label to a parse-time constant.
        if let (Some(sym), Some(Operation::Equ(e))) = (&symbol, &op)
            && let Some(v) = eval_const(e, &self.consts)
        {
            self.consts.insert(sym.qualified.clone(), v);
        }
        let trivia = Trivia {
            leading: std::mem::take(&mut self.pending_leading),
            trailing: comment.map(|text| Comment {
                text: text.to_string(),
                span: Span::in_file(file, line as u32, (code.len() + 1) as u32),
            }),
        };
        let operand_span = crate::ast::operand_span(raw, rest, line as u32).map(|mut s| {
            s.file = file;
            s
        });
        if is_include || is_incbin {
            let kind = if is_include {
                WalkDirective::Include {
                    request: include_request(args, line)?,
                }
            } else {
                let (request, offset, length) = incbin_args(self.syntax, args, line, &self.consts)?;
                WalkDirective::Incbin {
                    request,
                    offset,
                    length,
                }
            };
            return Ok(Some(DirectiveLine {
                kind,
                label: symbol,
                source: rest.trim().to_string(),
                span: Span::in_file(file, line as u32, 1),
                operand_span,
                trivia,
            }));
        }
        if symbol.is_none() && op.is_none() {
            return Ok(None);
        }
        self.nodes.push(Node {
            operand_span,
            label: symbol,
            item: op.map(crate::ast::item_from_operation),
            source: rest.trim().to_string(),
            span: Span::in_file(file, line as u32, 1),
            trivia,
        });
        Ok(None)
    }

    /// Flush comments after the last node (a trailing comment block, or a
    /// comment-only file) as a label-less, op-less node so the formatter keeps
    /// them (they emit no bytes, so assembly is unaffected).
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
}

/// The file name of an include directive: `"file"`, `<file>`, or a bare
/// token, matching the reference's accepted spellings (probe-pinned). Text
/// after a closing quote/bracket is ignored, as the reference does.
fn include_request(args: &str, line: usize) -> Result<String, AsmError> {
    let t = args.trim();
    let inner = if let Some(rest) = t.strip_prefix('"') {
        let end = rest
            .find('"')
            .ok_or_else(|| AsmError::new(line, "unterminated include file name"))?;
        &rest[..end]
    } else if let Some(rest) = t.strip_prefix('<') {
        let end = rest
            .find('>')
            .ok_or_else(|| AsmError::new(line, "unterminated include file name"))?;
        &rest[..end]
    } else {
        t.split_whitespace().next().unwrap_or("")
    };
    if inner.is_empty() {
        return Err(AsmError::new(line, "`include` needs a file name"));
    }
    Ok(inner.to_string())
}

/// Parse an incbin's arguments: the file name, then — where the dialect
/// supports it ([`Z80Syntax::incbin_offset_length`]) — an optional
/// `,offset[,length]` tail of parse-time constant expressions (they set the
/// statement's size, so like a `ds` count they must fold now; sjasmplus's
/// multi-pass acceptance of a *forward* constant is a known divergence).
/// Name spellings are probe-pinned: `"file"` and a bare token everywhere;
/// `<file>` only where [`Z80Syntax::incbin_angle_quotes`] says so (sjasmplus —
/// pasmo reads `<file>` as a literal file name). A bare name stops at
/// whitespace or a comma, so `incbin data.bin,2` still parses.
fn incbin_args<S: Z80Syntax>(
    syntax: &S,
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<(String, Option<i64>, Option<i64>), AsmError> {
    let t = args.trim();
    let (name, rest) = if let Some(inner) = t.strip_prefix('"') {
        let end = inner
            .find('"')
            .ok_or_else(|| AsmError::new(line, "unterminated incbin file name"))?;
        (&inner[..end], &inner[end + 1..])
    } else if syntax.incbin_angle_quotes()
        && let Some(inner) = t.strip_prefix('<')
    {
        let end = inner
            .find('>')
            .ok_or_else(|| AsmError::new(line, "unterminated incbin file name"))?;
        (&inner[..end], &inner[end + 1..])
    } else {
        let end = t
            .find(|c: char| c.is_whitespace() || c == ',')
            .unwrap_or(t.len());
        (&t[..end], &t[end..])
    };
    if name.is_empty() {
        return Err(AsmError::new(line, "`incbin` needs a file name"));
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((name.to_string(), None, None));
    }
    if !syntax.incbin_offset_length() {
        // pasmo's reference posture: nothing may follow the file name.
        return Err(AsmError::new(
            line,
            format!("`incbin` takes only a file name here (unexpected `{rest}`)"),
        ));
    }
    let Some(tail) = rest.strip_prefix(',') else {
        return Err(AsmError::new(
            line,
            format!("expected `,offset[,length]` after the incbin file name, found `{rest}`"),
        ));
    };
    let mut pieces = split_operands(tail);
    if pieces.len() > 2 {
        return Err(AsmError::new(
            line,
            "`incbin` takes at most a file name, an offset, and a length",
        ));
    }
    let fold = |what: &str, piece: &str| -> Result<i64, AsmError> {
        let expr = parse_value(syntax, piece, line)?;
        eval_const(&expr, consts).ok_or_else(|| {
            AsmError::new(
                line,
                format!(
                    "incbin {what} must be a constant here (a number, an expression of \
                     constants, or a value defined with `equ` above)"
                ),
            )
        })
    };
    let offset = fold("offset", pieces.remove(0))?;
    let length = pieces.pop().map(|p| fold("length", p)).transpose()?;
    Ok((name.to_string(), Some(offset), length))
}

/// Apply an incbin's offset/length to the loaded asset — sjasmplus semantics,
/// probe-pinned: a negative offset counts back from EOF; a negative length
/// means "all but the last |n| of the remaining bytes"; any window falling
/// outside the file is the reference's `file too short` error (offset *at*
/// EOF is legal and empty). `Err` carries the message body; the caller wraps
/// it with the request name and the directive's span.
fn slice_incbin(data: &[u8], offset: Option<i64>, length: Option<i64>) -> Result<Vec<u8>, String> {
    let len = data.len() as i64;
    let off = offset.unwrap_or(0);
    let off = if off < 0 { len + off } else { off };
    if !(0..=len).contains(&off) {
        return Err(format!(
            "file too short (offset {off} of a {len}-byte file)"
        ));
    }
    let remaining = len - off;
    let take = match length {
        None => remaining,
        Some(l) if l < 0 => remaining + l,
        Some(l) => l,
    };
    if !(0..=remaining).contains(&take) {
        return Err(format!(
            "file too short (length {take} with {remaining} byte(s) after offset {off})"
        ));
    }
    Ok(data[off as usize..(off + take) as usize].to_vec())
}

// ---------------------------------------------------------------------------
// Line structure
// ---------------------------------------------------------------------------

/// Split a (comment-stripped) line into its optional label and the remainder.
/// A `name:` token is always a label; otherwise a label sits in column 0 and
/// instructions are indented. A column-0 first word that names a known mnemonic
/// or directive is the operation, not a label.
fn split_label<'a, S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    code: &'a str,
    line: usize,
) -> Result<(Option<String>, &'a str), AsmError> {
    let trimmed = code.trim();
    if let Some(colon) = trimmed.find(':') {
        let before = &trimmed[..colon];
        if !before.contains(char::is_whitespace) {
            if !is_ident(before.trim()) {
                return Err(AsmError::new(
                    line,
                    format!("invalid label `{}`", before.trim()),
                ));
            }
            return Ok((Some(before.trim().to_string()), trimmed[colon + 1..].trim()));
        }
    }
    if code.starts_with([' ', '\t']) {
        return Ok((None, trimmed));
    }
    let (word, remainder) = split_first_word(trimmed);
    if has_mnemonic(set, ext, &word.to_ascii_uppercase()) || syntax.is_directive(word) {
        return Ok((None, trimmed));
    }
    if !is_ident(word) {
        return Err(AsmError::new(line, format!("invalid label `{word}`")));
    }
    Ok((Some(word.to_string()), remainder))
}

fn parse_op<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    rest: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<Option<Operation>, AsmError> {
    if rest.is_empty() {
        return Ok(None);
    }
    let (word, args) = split_first_word(rest);
    if syntax.is_directive(word) {
        return syntax.parse_directive(word, args, line, consts);
    }
    let mnemonic = word.to_ascii_uppercase();
    if !has_mnemonic(set, ext, &mnemonic) {
        return Err(AsmError::new(
            line,
            format!("unknown instruction `{mnemonic}`"),
        ));
    }
    let (mode, operands) = resolve(syntax, set, ext, &mnemonic, args, line, consts)?;
    Ok(Some(Operation::Instruction {
        mnemonic,
        mode,
        operands,
    }))
}

// ---------------------------------------------------------------------------
// Common directives
// ---------------------------------------------------------------------------

/// Directives both pasmo and sjasmplus share.
pub(crate) fn is_common_directive(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "org" | "equ" | "defb" | "db" | "defm" | "dm" | "defw" | "dw" | "defs" | "ds" | "end"
    )
}

/// Parse a common directive. `defs`/`ds` reserve a constant-folded number of
/// zero bytes (a literal or an expression of `equ` constants).
pub(crate) fn common_directive<S: Z80Syntax>(
    syntax: &S,
    word: &str,
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<Option<Operation>, AsmError> {
    Ok(match word.to_ascii_lowercase().as_str() {
        "org" => Some(Operation::Org(parse_value(syntax, args, line)?)),
        "equ" => Some(Operation::Equ(parse_value(syntax, args, line)?)),
        "defb" | "db" | "defm" | "dm" => Some(Operation::Bytes(parse_list(syntax, args, line)?)),
        "defw" | "dw" => Some(Operation::Words(parse_list(syntax, args, line)?)),
        "defs" | "ds" => {
            // The count must be known at parse time (it sets the statement's
            // size), but it need not be a bare literal — fold any expression of
            // `equ` constants, e.g. `ds MAX_TORCHES * 2`.
            let count = literal(&parse_value(syntax, args, line)?, consts, line)?;
            let count = usize::try_from(count).map_err(|_| {
                AsmError::new(line, "`ds`/`defs` count must be a non-negative constant")
            })?;
            Some(Operation::Bytes(vec![Expr::Num(0); count]))
        }
        // `end [addr]` marks the entry point. A flat binary ignores it, but a
        // `.sna` snapshot needs the start address — capture it when given.
        "end" if args.trim().is_empty() => None,
        "end" => Some(Operation::Entry(parse_value(syntax, args, line)?)),
        other => return Err(AsmError::new(line, format!("unknown directive `{other}`"))),
    })
}

// ---------------------------------------------------------------------------
// Instruction-set lookup (primary + optional Z80N extension)
// ---------------------------------------------------------------------------

fn find_form(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
    mode: &str,
) -> Option<&'static isa::Form> {
    set.find_form(mnemonic, mode)
        .or_else(|| ext.and_then(|e| e.find_form(mnemonic, mode)))
}

fn has_mnemonic(
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
) -> bool {
    set.has_mnemonic(mnemonic) || ext.is_some_and(|e| e.has_mnemonic(mnemonic))
}

// ---------------------------------------------------------------------------
// Operand resolution (dialect syntax -> spec mode label)
// ---------------------------------------------------------------------------

/// One classified operand.
enum Operand {
    /// A register, condition, or register-indirect — a fixed signature token.
    Fixed(String),
    /// A value: an immediate or a `(nn)` address. `paren` marks the memory form.
    Value { expr: Expr, paren: bool },
    /// An indexed operand `(IX+d)` / `(IY+d)`. `disp` is `None` for a bare
    /// `(IX)` — either register-indirect (`JP (IX)`) or `(IX+0)`, by which form
    /// exists.
    Indexed {
        reg: &'static str,
        disp: Option<Expr>,
    },
}

/// One way an operand can be written into a mode label: the token it
/// contributes, and the value(s) it emits as bytes (empty if consumed into the
/// opcode, e.g. a BIT bit-number).
type Alternative = (String, Vec<Expr>);

fn resolve<S: Z80Syntax>(
    syntax: &S,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    mnemonic: &str,
    args: &str,
    line: usize,
    consts: &BTreeMap<String, i64>,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let pieces = split_operands(args);
    let mut per_operand: Vec<Vec<Alternative>> = Vec::new();
    for (idx, piece) in pieces.iter().enumerate() {
        per_operand.push(alternatives(syntax, mnemonic, idx, piece, consts, line)?);
    }

    for combo in product(&per_operand) {
        let label = combo
            .iter()
            .map(|(token, _)| token.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if let Some(f) = find_form(set, ext, mnemonic, &label) {
            let emitted = combo.into_iter().flat_map(|(_, values)| values).collect();
            return Ok((f.mode, emitted));
        }
    }
    Err(AsmError::new(
        line,
        format!("`{mnemonic}` has no form for operands `{}`", args.trim()),
    ))
}

fn alternatives<S: Z80Syntax>(
    syntax: &S,
    mnemonic: &str,
    idx: usize,
    piece: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Vec<Alternative>, AsmError> {
    Ok(match classify(syntax, piece, line)? {
        Operand::Fixed(token) => vec![(token, vec![])],
        Operand::Indexed { reg, disp } => match disp {
            Some(d) => vec![(format!("({reg}+d)"), vec![d])],
            None => vec![
                (format!("({reg})"), vec![]),
                (format!("({reg}+d)"), vec![Expr::Num(0)]),
            ],
        },
        Operand::Value { expr, paren } => {
            if let Some(token) = embedded_token(mnemonic, paren, idx, &expr, consts, line)? {
                vec![(token, vec![])] // consumed into the opcode
            } else {
                emitted_tokens(mnemonic, paren)
                    .into_iter()
                    .map(|token| (token, vec![expr.clone()]))
                    .collect()
            }
        }
    })
}

fn classify<S: Z80Syntax>(syntax: &S, piece: &str, line: usize) -> Result<Operand, AsmError> {
    let t = piece.trim();
    if let Some(inner) = strip_parens(t) {
        let inner = inner.trim();
        if let Some((reg, rest)) = index_register(inner) {
            let disp = if rest.is_empty() {
                None
            } else if let Some(after_plus) = rest.strip_prefix('+') {
                Some(parse_value(syntax, after_plus, line)?)
            } else {
                Some(parse_value(syntax, rest, line)?) // '-': unary minus
            };
            return Ok(Operand::Indexed { reg, disp });
        }
        let up = inner.to_ascii_uppercase();
        if is_indirect_reg(&up) {
            return Ok(Operand::Fixed(format!("({up})")));
        }
        return Ok(Operand::Value {
            expr: parse_value(syntax, inner, line)?,
            paren: true,
        });
    }
    let up = t.to_ascii_uppercase();
    if is_reg_or_cond(&up) {
        return Ok(Operand::Fixed(up));
    }
    Ok(Operand::Value {
        expr: parse_value(syntax, t, line)?,
        paren: false,
    })
}

/// If `inner` names an index register with an optional displacement, return the
/// canonical register and the rest. Guards against symbols starting with
/// "ix"/"iy" by requiring the next char to be `+`, `-`, or nothing.
fn index_register(inner: &str) -> Option<(&'static str, &str)> {
    for reg in ["IX", "IY"] {
        if inner.len() >= 2 && inner[..2].eq_ignore_ascii_case(reg) {
            let rest = inner[2..].trim_start();
            if rest.is_empty() || rest.starts_with('+') || rest.starts_with('-') {
                return Some((reg, rest));
            }
        }
    }
    None
}

/// For an operand encoded *in the opcode* (RST target, IM mode, BIT/RES/SET bit
/// number), return its mode-label token. `None` for operands that become bytes.
fn embedded_token(
    mnemonic: &str,
    paren: bool,
    index: usize,
    expr: &Expr,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<String>, AsmError> {
    if paren {
        return Ok(None);
    }
    let token = match mnemonic {
        "RST" => format!("{:02X}", literal(expr, consts, line)?),
        "IM" => format!("{}", literal(expr, consts, line)?),
        "BIT" | "RES" | "SET" if index == 0 => format!("{}", literal(expr, consts, line)?),
        _ => return Ok(None),
    };
    Ok(Some(token))
}

/// Candidate tokens for a value operand that becomes bytes. Width is left
/// ambiguous (both offered) except for relative branches.
fn emitted_tokens(mnemonic: &str, paren: bool) -> Vec<String> {
    if paren {
        return vec!["(n)".to_string(), "(nn)".to_string()];
    }
    match mnemonic {
        "JR" | "DJNZ" => vec!["e".to_string()],
        _ => vec!["n".to_string(), "nn".to_string()],
    }
}

/// Resolve an opcode-embedded operand to a parse-time constant (a number, an
/// expression of constants, or an `equ` value above — but not a label).
fn literal(expr: &Expr, consts: &BTreeMap<String, i64>, line: usize) -> Result<i64, AsmError> {
    eval_const(expr, consts).ok_or_else(|| {
        AsmError::new(
            line,
            "operand must be a constant here (a number, an expression of \
             constants, or a value defined with `equ` above)",
        )
    })
}

/// Fold an expression to a constant, resolving symbols only against `equ`
/// constants. `None` if it references an unknown symbol or overflows. `$` (the
/// location counter) is unknown until the engine's emit pass, so it never folds
/// here (the parse-time-constant context passes no PC).
pub(crate) fn eval_const(expr: &Expr, consts: &BTreeMap<String, i64>) -> Option<i64> {
    expr.eval_with(&|s| consts.get(s).copied(), None, 0).ok()
}

/// Rewrite every bare local reference (a leading-`.` symbol) in an operation,
/// qualifying it with the current global scope `g` — so `jr .loop` under global
/// `start` resolves to `start.loop`. A non-local symbol, or an already-qualified
/// `global.local`, is left untouched.
fn qualify_locals(op: Operation, g: &str) -> Operation {
    match op {
        Operation::Org(e) => Operation::Org(qualify_expr(e, g)),
        Operation::Equ(e) => Operation::Equ(qualify_expr(e, g)),
        Operation::Bytes(v) => {
            Operation::Bytes(v.into_iter().map(|e| qualify_expr(e, g)).collect())
        }
        Operation::Words(v) => {
            Operation::Words(v.into_iter().map(|e| qualify_expr(e, g)).collect())
        }
        Operation::Instruction {
            mnemonic,
            mode,
            operands,
        } => Operation::Instruction {
            mnemonic,
            mode,
            operands: operands.into_iter().map(|e| qualify_expr(e, g)).collect(),
        },
        // The Z80 dialect never emits pre-encoded instructions.
        Operation::Encoded(pieces) => Operation::Encoded(pieces),
        // A binary payload carries no expressions (an incbin resolves in the
        // walk, never through parse_op — this arm keeps the match total).
        bin @ Operation::Binary(_) => bin,
        Operation::Entry(e) => Operation::Entry(qualify_expr(e, g)),
        // No sub-expressions to qualify (the acme-only align carries constants).
        align @ Operation::Align { .. } => align,
    }
}

fn qualify_expr(e: Expr, g: &str) -> Expr {
    match e {
        Expr::Sym(s) if s.starts_with('.') => Expr::Sym(format!("{g}{s}")),
        Expr::Sym(_) | Expr::Num(_) | Expr::Pc => e,
        Expr::Lo(b) => Expr::Lo(Box::new(qualify_expr(*b, g))),
        Expr::Hi(b) => Expr::Hi(Box::new(qualify_expr(*b, g))),
        Expr::Bank(b) => Expr::Bank(Box::new(qualify_expr(*b, g))),
        Expr::Neg(b) => Expr::Neg(Box::new(qualify_expr(*b, g))),
        Expr::Bin(op, l, r) => Expr::Bin(
            op,
            Box::new(qualify_expr(*l, g)),
            Box::new(qualify_expr(*r, g)),
        ),
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

// ---------------------------------------------------------------------------
// Register / condition vocabulary
// ---------------------------------------------------------------------------

fn is_indirect_reg(up: &str) -> bool {
    matches!(up, "HL" | "BC" | "DE" | "SP" | "C")
}

/// Register or condition tokens (used verbatim in a mode label). `C` is both a
/// register and the carry condition; the form lookup disambiguates by mnemonic.
fn is_reg_or_cond(up: &str) -> bool {
    matches!(
        up,
        "A" | "B"
            | "C"
            | "D"
            | "E"
            | "H"
            | "L"
            | "I"
            | "R"
            | "AF"
            | "AF'"
            | "BC"
            | "DE"
            | "HL"
            | "SP"
            | "IX"
            | "IY"
            | "NZ"
            | "Z"
            | "NC"
            | "PO"
            | "PE"
            | "P"
            | "M"
    )
}

// ---------------------------------------------------------------------------
// Tokenising and the expression parser
// ---------------------------------------------------------------------------

/// Split operand text on top-level commas (commas inside parentheses are kept).
fn split_operands(args: &str) -> Vec<&str> {
    let args = args.trim();
    if args.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(args[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(args[start..].trim());
    out
}

fn strip_parens(t: &str) -> Option<&str> {
    let t = t.trim();
    t.strip_prefix('(')?.strip_suffix(')')
}

/// Parse a `defb`/`defw` value list. A `"..."` string expands to one byte per
/// character. TODO: escape sequences in strings.
fn parse_list<S: Z80Syntax>(syntax: &S, rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.chars().map(|c| Expr::Num(c as i64)));
        } else {
            out.push(parse_value(syntax, piece, line)?);
        }
    }
    Ok(out)
}

/// Split a data list on commas not inside a `"..."` string.
fn split_data_items(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_string = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            ',' if !in_string => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

fn string_literal(piece: &str) -> Option<&str> {
    let p = piece.trim();
    (p.len() >= 2 && p.starts_with('"') && p.ends_with('"')).then(|| &p[1..p.len() - 1])
}

/// Parse an operand value: an arithmetic expression over numbers, symbols, and
/// `+`/`-`/`*`/`/` with C-style precedence and parentheses. Number literals are
/// lexed by the dialect's [`Z80Syntax::parse_number`].
fn parse_value<S: Z80Syntax>(syntax: &S, raw: &str, line: usize) -> Result<Expr, AsmError> {
    let tokens = tokenize(syntax, raw, line)?;
    if tokens.is_empty() {
        return Err(AsmError::new(line, "expected a value"));
    }
    let mut parser = ExprParser {
        tokens,
        pos: 0,
        line,
    };
    let expr = parser.expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(AsmError::new(
            line,
            format!("unexpected trailing tokens in `{}`", raw.trim()),
        ));
    }
    Ok(expr)
}

#[derive(Clone)]
enum Tok {
    Num(i64),
    Sym(String),
    /// The location counter `$` (statement-start address).
    Pc,
    Plus,
    Minus,
    Star,
    Slash,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    LParen,
    RParen,
}

/// Lex an expression. The number *extent* (a `$`/`%`/`#`/digit start then an
/// alphanumeric run) is shared; the dialect's `parse_number` interprets it,
/// which is where hex/binary format differences live.
fn tokenize<S: Z80Syntax>(syntax: &S, raw: &str, line: usize) -> Result<Vec<Tok>, AsmError> {
    let chars: Vec<char> = raw.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ws if ws.is_whitespace() => i += 1,
            '+' => {
                tokens.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Tok::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Tok::Slash);
                i += 1;
            }
            '&' => {
                tokens.push(Tok::And);
                i += 1;
            }
            '|' => {
                tokens.push(Tok::Or);
                i += 1;
            }
            // sjasmplus has `^` (XOR); pasmo does not, so it falls through to the
            // unknown-character error there.
            '^' if syntax.has_xor_operator() => {
                tokens.push(Tok::Xor);
                i += 1;
            }
            '<' if chars.get(i + 1) == Some(&'<') => {
                tokens.push(Tok::Shl);
                i += 2;
            }
            '>' if chars.get(i + 1) == Some(&'>') => {
                tokens.push(Tok::Shr);
                i += 2;
            }
            '(' => {
                tokens.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Tok::RParen);
                i += 1;
            }
            '\'' => {
                if i + 2 < chars.len() && chars[i + 2] == '\'' {
                    let s: String = chars[i..=i + 2].iter().collect();
                    tokens.push(Tok::Num(syntax.parse_number(&s, line)?));
                    i += 3;
                } else {
                    return Err(AsmError::new(line, "malformed character literal"));
                }
            }
            // A bare `$` is the location counter; `$` followed by hex digits is
            // a number. Disambiguate on the next character.
            '$' if !chars.get(i + 1).is_some_and(|c| c.is_ascii_alphanumeric()) => {
                tokens.push(Tok::Pc);
                i += 1;
            }
            // A number: a prefix sigil ($/%/#) or a digit, then an alnum run.
            '$' | '%' | '#' => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Tok::Num(syntax.parse_number(&s, line)?));
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Tok::Num(syntax.parse_number(&s, line)?));
            }
            // An identifier: letters, digits, `_`, `.` (not starting with a digit).
            l if l.is_ascii_alphabetic() || l == '_' || l == '.' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                tokens.push(Tok::Sym(chars[start..i].iter().collect()));
            }
            other => {
                return Err(AsmError::new(
                    line,
                    format!("unexpected character `{other}` in expression"),
                ));
            }
        }
    }
    Ok(tokens)
}

/// Precedence-climbing parser: `add_sub` over `mul_div` over `unary` over
/// `atom`, so `*`/`/` bind tighter than `+`/`-`.
struct ExprParser {
    tokens: Vec<Tok>,
    pos: usize,
    line: usize,
}

impl ExprParser {
    fn expr(&mut self) -> Result<Expr, AsmError> {
        self.bit_or()
    }

    // Bitwise and shift operators, C-style: `|` loosest, then `^`, `&`, then the
    // shifts, all looser than `+`/`-` (so `1+2<<1` is `(1+2)<<1`). This matches
    // sjasmplus; pasmo binds its shifts tighter than additive, a divergence that
    // only shows on unparenthesised mixed expressions.
    fn bit_or(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.bit_xor()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Or)) {
            self.pos += 1;
            let right = self.bit_xor()?;
            left = Expr::Bin(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn bit_xor(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.bit_and()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::Xor)) {
            self.pos += 1;
            let right = self.bit_and()?;
            left = Expr::Bin(BinOp::Xor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn bit_and(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.shift()?;
        while matches!(self.tokens.get(self.pos), Some(Tok::And)) {
            self.pos += 1;
            let right = self.shift()?;
            left = Expr::Bin(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn shift(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.add_sub()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Shl) => BinOp::Shl,
                Some(Tok::Shr) => BinOp::Shr,
                _ => break,
            };
            self.pos += 1;
            let right = self.add_sub()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn add_sub(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.mul_div()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.mul_div()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn mul_div(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.tokens.get(self.pos) {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let right = self.unary()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, AsmError> {
        if matches!(self.tokens.get(self.pos), Some(Tok::Minus)) {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, AsmError> {
        let tok = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or_else(|| AsmError::new(self.line, "expected a value"))?;
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Pc => Ok(Expr::Pc),
            Tok::Sym(s) => Ok(Expr::Sym(s)),
            Tok::LParen => {
                let inner = self.expr()?;
                if matches!(self.tokens.get(self.pos), Some(Tok::RParen)) {
                    self.pos += 1;
                    Ok(inner)
                } else {
                    Err(AsmError::new(self.line, "expected `)`"))
                }
            }
            _ => Err(AsmError::new(self.line, "expected a value")),
        }
    }
}

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], s[idx..].trim()),
        None => (s, ""),
    }
}

/// An identifier: letters, digits, `_`, and `.` (the last so local-style labels
/// like `.loop` read as ordinary names), not starting with a digit.
fn is_ident(s: &str) -> bool {
    let s = s.trim();
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '.' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}
