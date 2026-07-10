//! The ca65 65816 dialect front-end (native 16-bit mode).
//!
//! The 65816 is a target extension of the 6502, so this dialect assembles
//! against the 6502 [`isa`] spec **plus** the [`isa::mos65816`] extension set —
//! the same primary-plus-extension mechanism the Z80/Z80N pair uses. Encoding
//! and the two-pass driver are the engine's; only ca65's surface (directives,
//! the `.a8`/`.a16`/`.i8`/`.i16` width state, the new operand syntax) lives here.
//!
//! The 6502 expression and lexer helpers are shared from [`super::mos6502`]; the
//! operand-structure parsing and mode resolution are 65816-specific because the
//! chip adds long (24-bit) addressing, `[dp]` indirect-long, stack-relative, and
//! the width-variable immediate. Mode resolution is **spec-driven**: an
//! ambiguous syntax (`(expr)` is `jmp`-indirect or `(dp)`; `[expr]` is
//! `jmp`-long or `[dp]`) is settled by asking which form the mnemonic actually
//! has, not by hardcoding mnemonic lists.
//!
//! Output is a flat binary, validated byte-identical against `ca65 --cpu 65816`
//! linked flat. There is no SNES curriculum yet, so (as with 6809/lwasm) the
//! reference is the tool directly. Deferred: `.smart` rep/sep width tracking,
//! the `^` bank-byte operator, `@cheap` locals, and `mvn`/`mvp`/`cop`/`wdm`.
//!
//! `.include`/`.incbin` (language-surface U4) resolve through the shared
//! ca65-flat walk in [`super::ca65_flat`]: the environment — constants *and*
//! the `.a8`/`.a16`/`.i8`/`.i16` width state — threads through an include and
//! back out, so a width flip inside an include sizes the includer's later
//! immediates exactly as `ca65` does (probe-pinned).

use std::collections::BTreeMap;

use super::ca65_flat::{self, DirectiveLine, FlatWalk, WalkDirective};
use super::mos6502::{
    self, BytePrec, fold_const, is_ident, parse_number, split_data_items, split_first_word,
    split_top_level, string_literal, top_level_rfind,
};
use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

/// The ca65 65816 dialect.
pub(crate) struct Ca65_816;

impl Dialect for Ca65_816 {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::mos6502::SET
    }

    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        Some(&isa::mos65816::SET)
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        // Route assembly through the semantic AST (0b straggler migration): parse
        // into a `Program`, then lower to the engine's statement stream —
        // byte-identical to the old direct parse (AE1).
        crate::ast::lower(parse_program(source)?)
    }

    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(parse_program(source)?))
    }

    /// The include-capable parse (language-surface U4): the interleaved,
    /// environment-threaded walk over the source map, resolving `.include`/
    /// `.incbin` lazily through the loader — see [`parse_program_multi`].
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        crate::ast::lower(parse_program_multi(map, loader)?)
    }

    /// ca65 binds constants with `name = expr` — no `equ` keyword and no colon on
    /// the label, so the formatter emits `name = expr`.
    fn equ_label_colon(&self) -> bool {
        false
    }
}

/// Parse ca65-syntax 65816 source into the semantic [`Program`](crate::ast::Program).
/// Each line becomes a node with its (global) label, operation, verbatim source,
/// span, and comment trivia; the flat 65816 dialect has no local-label scoping,
/// so every label is a [`Scope::Global`](crate::ast::Scope) symbol and
/// [`lower`](crate::ast::lower) reproduces the old statements exactly.
///
/// A `.include`/`.incbin` becomes an **unresolved**
/// [`Item::Include`](crate::ast::Item) / [`Item::Incbin`](crate::ast::Item) —
/// the target is never opened, so `--fmt` renders the directive verbatim and
/// works with a missing target (U4, KTD1). Lazy resolution is
/// [`parse_program_multi`]'s.
pub(crate) fn parse_program(source: &str) -> Result<Program, AsmError> {
    let mut w = Walker::new();
    for (i, raw) in source.lines().enumerate() {
        if let Some(d) = w.walk_line(raw, i + 1, FileId(0))? {
            w.nodes.push(ca65_flat::unresolved_node(d));
        }
    }
    w.flush_trailing(source.lines().count() as u32);
    Ok(Program { nodes: w.nodes })
}

/// Parse a multi-file ca65-65816 program (language-surface U4, KTD1): the
/// **interleaved, environment-threaded walk**. The root (`FileId(0)` in `map`)
/// parses line by line with the environment accumulated so far; when the walk
/// reaches a `.include` live, the target loads through `loader`, its lines
/// parse with the same environment, and everything it defined — constants
/// driving zp/abs selection, a `.a16`/`.i16` width flip sizing later
/// immediates — flows back out to the includer's subsequent lines
/// (probe-pinned against `ca65 --cpu 65816`).
///
/// # Errors
/// Any per-line parse failure (stamped with the file it occurred in), a
/// missing target, an include cycle, a bad `.incbin` window, or the depth
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
        &ca65_flat::CA65_SEMANTICS,
    )?;
    Ok(Program { nodes: w.nodes })
}

/// The per-line parse walk shared by [`parse_program`] (single source) and
/// [`parse_program_multi`] (the include-capable walk). The environment — the
/// `name = expr` constants, the `.a8`/`.a16`/`.i8`/`.i16` width state, and
/// pending comment trivia — lives here, so in the multi-file walk it threads
/// *through* include boundaries in both directions (KTD1, probe-pinned).
struct Walker {
    /// Constants bound with `name = expr`, consulted for zp/abs/long sizing
    /// and `.res`/`.incbin` argument folding at parse time.
    env: BTreeMap<String, i64>,
    /// Accumulator/index immediate widths in bytes (1 or 2), driven by the
    /// `.a8`/`.a16`/`.i8`/`.i16` directives. Native reset state is 8-bit.
    a_width: u8,
    i_width: u8,
    /// Own-line comments seen since the last node, attached as leading trivia
    /// to the next one. Comments never reach the encoder, so bytes are
    /// unchanged.
    pending_leading: Vec<Comment>,
    nodes: Vec<Node>,
}

impl Walker {
    fn new() -> Self {
        Self {
            env: BTreeMap::new(),
            a_width: 1,
            i_width: 1,
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
}

impl FlatWalk for Walker {
    fn walk_line(
        &mut self,
        raw: &str,
        line: usize,
        file: FileId,
    ) -> Result<Option<DirectiveLine>, AsmError> {
        let prim = &isa::mos6502::SET;
        let ext = &isa::mos65816::SET;
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

        // A width directive mutates parse state and emits no bytes, but is kept
        // as a source-only node so the formatter reproduces it; dropping one
        // would reassemble the following immediates at the default width, a
        // round-trip byte mismatch.
        if let Some(w) = width_directive(code.trim()) {
            match w {
                Width::A(n) => self.a_width = n,
                Width::I(n) => self.i_width = n,
            }
            self.nodes.push(Node {
                operand_span: None,
                label: None,
                item: None,
                source: code.trim().to_string(),
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        // `name = expr` binds a named constant (a lone `=`, not `==`/`!=`/…).
        if let Some(eq) = mos6502::assignment_split(code.trim()) {
            let trimmed = code.trim();
            let name = trimmed[..eq].trim();
            if !is_ident(name) {
                return Err(AsmError::new(line, format!("invalid symbol `{name}`")));
            }
            let e = value(trimmed[eq + 1..].trim(), line)?;
            if let Ok(v) = fold_const(&e, &self.env, line) {
                self.env.insert(name.to_string(), v);
            }
            self.nodes.push(Node {
                operand_span: None,
                label: Some(Symbol {
                    qualified: name.to_string(),
                    scope: Scope::Global,
                    name: name.to_string(),
                }),
                item: Some(crate::ast::item_from_operation(Operation::Equ(e))),
                source: trimmed[eq..].trim().to_string(),
                span: Span::in_file(file, line as u32, 1),
                trivia: Trivia {
                    leading: std::mem::take(&mut self.pending_leading),
                    trailing,
                },
            });
            return Ok(None);
        }

        let (label, rest) = split_label(code);
        // `.include`/`.incbin` are walk-handled, not directives: the target
        // must not be opened here (KTD1 — `--fmt` succeeds with a missing
        // target), so hand them back for the driver to resolve (or keep
        // unresolved, in the single-source parse).
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
            parse_op(prim, ext, rest, self.a_width, self.i_width, &self.env, line)?
        };
        // Keep source-bearing no-op directives (`.segment`/`setcpu`/…) as
        // source-only nodes so the formatter reproduces them; skip only a truly
        // empty line (nothing to lower or format).
        if label.is_none() && op.is_none() && rest.trim().is_empty() {
            return Ok(None);
        }
        self.nodes.push(Node {
            operand_span: None,
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

impl Walker {
    /// Recognise a walk-handled `.include`/`.incbin` operation and parse its
    /// arguments with the live environment (the `.incbin` offset/size fold
    /// against the constants known so far — a forward reference is ca65's
    /// "Constant expression expected" error, probe-pinned).
    fn walk_directive(&self, rest: &str, line: usize) -> Result<Option<WalkDirective>, AsmError> {
        let (word, args) = split_first_word(rest);
        match word.to_ascii_lowercase().as_str() {
            ".include" => Ok(Some(WalkDirective::Include {
                request: ca65_flat::include_request(args, line, ".include")?,
            })),
            ".incbin" => {
                let fold = |piece: &str| fold_const(&value(piece.trim(), line)?, &self.env, line);
                let (request, offset, size) = ca65_flat::incbin_args(args, line, ".incbin", &fold)?;
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

/// Split a line into its code and its `;` comment (leading `;` and whitespace
/// trimmed) for carrying comments as AST trivia. Defined via [`strip_comment`] so
/// the comment is exactly what it removes — no behaviour change to assembly.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let code = strip_comment(line);
    let comment = (code.len() < line.len()).then(|| line[code.len()..].trim_end());
    (code, comment)
}

/// Strip a `;` comment, ignoring `;` inside a `'c'` char or `"..."` string.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let (mut in_char, mut in_str) = (false, false);
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

/// Split a `label:` from the rest of the line. ca65 labels require a trailing
/// colon by default, so a colon-less column-0 word is the instruction or
/// directive, not a label. (`name = expr` is handled before this is reached.)
fn split_label(code: &str) -> (Option<String>, &str) {
    if code.starts_with([' ', '\t']) {
        return (None, code.trim());
    }
    let trimmed = code.trim();
    let (word, remainder) = split_first_word(trimmed);
    match word.strip_suffix(':') {
        Some(name) if is_ident(name) => (Some(name.to_string()), remainder),
        _ => (None, trimmed),
    }
}

enum Width {
    A(u8),
    I(u8),
}

/// Recognise an `.a8`/`.a16`/`.i8`/`.i16` width directive.
fn width_directive(rest: &str) -> Option<Width> {
    match rest.trim() {
        ".a8" => Some(Width::A(1)),
        ".a16" => Some(Width::A(2)),
        ".i8" => Some(Width::I(1)),
        ".i16" => Some(Width::I(2)),
        _ => None,
    }
}

/// Parse the operation part of a line: a `name = expr` constant, a `.directive`,
/// or an instruction.
fn parse_op(
    prim: &'static isa::InstructionSet,
    ext: &'static isa::InstructionSet,
    rest: &str,
    a_width: u8,
    i_width: u8,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    if let Some(dir) = rest.strip_prefix('.') {
        return parse_directive(dir, env, line);
    }
    let (mnemonic, operand) = split_first_word(rest);
    let mn = mnemonic.to_ascii_uppercase();
    let (mode, exprs) = resolve(prim, ext, &mn, operand, a_width, i_width, env, line)?;
    Ok(Some(Operation::Instruction {
        mnemonic: mn,
        mode,
        operands: exprs,
    }))
}

/// Parse a `.directive`.
fn parse_directive(
    dir: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let (name, rest) = split_first_word(dir);
    match name.to_ascii_lowercase().as_str() {
        // Placement/CPU directives are no-ops in the flat model (one origin).
        "setcpu" | "segment" | "smart" | "p816" | "i16" | "a16" | "i8" | "a8" => Ok(None),
        "org" => Ok(Some(Operation::Org(value(rest, line)?))),
        "byte" | "byt" => Ok(Some(Operation::Bytes(byte_list(rest, line)?))),
        "word" | "addr" => Ok(Some(Operation::Words(value_list(rest, line)?))),
        // `.dword` — 32-bit little-endian. Emitted as width-4 computed pieces so
        // symbolic values resolve in pass two (the 65816 is little-endian).
        "dword" => Ok(Some(Operation::Encoded(
            value_list(rest, line)?
                .into_iter()
                .map(|expr| Piece::Val {
                    expr,
                    bytes: 4,
                    rel: false,
                    signed: false,
                })
                .collect(),
        ))),
        // `.dbyt` — 16-bit big-endian (high byte first), independent of the CPU's
        // little-endianness, so lay each value down as `[Hi, Lo]` bytes.
        "dbyt" => Ok(Some(Operation::Bytes(
            value_list(rest, line)?
                .into_iter()
                .flat_map(|e| [Expr::Hi(Box::new(e.clone())), Expr::Lo(Box::new(e))])
                .collect(),
        ))),
        // `.asciiz` — a `.byte` string list with one terminating $00.
        "asciiz" => {
            let mut out = byte_list(rest, line)?;
            out.push(Expr::Num(0));
            Ok(Some(Operation::Bytes(out)))
        }
        "res" => parse_res(rest, env, line),
        other => Err(AsmError::new(
            line,
            format!("unsupported directive `.{other}`"),
        )),
    }
}

/// `.res count [, fill]` — reserve `count` bytes (zero or `fill`).
fn parse_res(
    rest: &str,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Option<Operation>, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let count_src = parts.next().unwrap_or("").trim();
    let count = fold_const(&value(count_src, line)?, env, line)?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`.res` count must be a non-negative constant"))?;
    let fill = match parts.next() {
        None => 0,
        Some(v) => {
            let n = fold_const(&value(v.trim(), line)?, env, line)?;
            u8::try_from(n).map_err(|_| AsmError::new(line, "`.res` fill must be a byte"))?
        }
    };
    Ok(Some(Operation::Bytes(vec![
        Expr::Num(i64::from(fill));
        count
    ])))
}

fn byte_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "`.byte` needs a value"));
    }
    let mut out = Vec::new();
    for piece in split_data_items(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.bytes().map(|b| Expr::Num(i64::from(b))));
        } else {
            out.push(value(piece, line)?);
        }
    }
    Ok(out)
}

fn value_list(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    if rest.trim().is_empty() {
        return Err(AsmError::new(line, "`.word` needs a value"));
    }
    split_top_level(rest, ',')
        .iter()
        .map(|p| value(p, line))
        .collect()
}

fn value(raw: &str, line: usize) -> Result<Expr, AsmError> {
    mos6502::parse_expr(
        raw,
        line,
        parse_number,
        mos6502::ExprOpts {
            bang_is_or: false,
            prec: BytePrec::Tight,
            byte_prefix: true,
            caret: mos6502::Caret::BankOrXor,
            at_is_pc: false,
        },
    )
}

// ---------------------------------------------------------------------------
// Operand mode resolution
// ---------------------------------------------------------------------------

/// A size force from a `z:`/`a:`/`f:` prefix.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Force {
    None,
    Dp,
    Abs,
    Long,
}

/// Resolve a 65816 operand to its spec mode label and value expressions.
#[allow(clippy::too_many_arguments)]
fn resolve(
    prim: &'static isa::InstructionSet,
    ext: &'static isa::InstructionSet,
    mn: &str,
    operand: &str,
    a_width: u8,
    i_width: u8,
    env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let has = |mode: &str| prim.find_form(mn, mode).is_some() || ext.find_form(mn, mode).is_some();
    let t = operand.trim();

    // No operand, or an explicit accumulator target.
    if t.is_empty() {
        return Ok(("implied", vec![]));
    }
    if t.eq_ignore_ascii_case("a") {
        return Ok(("accumulator", vec![]));
    }

    // `cop`/`wdm` take a bare signature byte (no `#`).
    if matches!(mn, "COP" | "WDM") {
        return Ok(("signature", vec![value(t, line)?]));
    }
    // `mvn`/`mvp src,dest` — emitted as opcode, dest-bank, src-bank.
    if matches!(mn, "MVN" | "MVP") {
        return resolve_block_move(t, operand, line);
    }

    // Branches: relative (8-bit) or relative-long (16-bit). No other instruction
    // carries a `relative`/`relative16` form, so the presence of one settles it.
    if has("relative16") {
        return Ok(("relative16", vec![value(t, line)?]));
    }
    if has("relative") {
        return Ok(("relative", vec![value(t, line)?]));
    }

    // Immediate. Width comes from the .aXX/.iXX state: index ops use the X-width,
    // rep/sep are always 8-bit, the rest use the accumulator width.
    if let Some(imm) = t.strip_prefix('#') {
        let width = if matches!(mn, "REP" | "SEP") {
            1
        } else if matches!(mn, "LDX" | "LDY" | "CPX" | "CPY") {
            i_width
        } else {
            a_width
        };
        let mode = if width == 2 && has("immediate16") {
            "immediate16"
        } else {
            "immediate"
        };
        return Ok((mode, vec![value(imm, line)?]));
    }

    // `[dp]` / `[dp],y` / `[abs]` (jmp long-indirect).
    if t.starts_with('[') {
        if let Some(body) = t
            .strip_suffix("],y")
            .or_else(|| t.strip_suffix("],Y"))
            .and_then(|s| s.strip_prefix('['))
        {
            let (_f, e) = addr_expr(body, line)?;
            return Ok(("[indirect],y", vec![e]));
        }
        if let Some(body) = t.strip_suffix(']').and_then(|s| s.strip_prefix('[')) {
            // jmp/jml use [abs]; the ALU ops use [dp].
            let (_f, e) = addr_expr(body, line)?;
            let mode = if has("[absolute]") {
                "[absolute]"
            } else {
                "[indirect]"
            };
            return Ok((mode, vec![e]));
        }
        return Err(AsmError::new(
            line,
            format!("malformed `[...]` operand `{operand}`"),
        ));
    }

    // Parenthesised indirect forms.
    if t.starts_with('(') {
        return resolve_indirect(&has, t, operand, line);
    }

    // Stack relative: `n,s` and (handled above for the indirect) `(n,s),y`.
    if let Some(c) = top_level_rfind(t, ',')
        && t[c + 1..].trim().eq_ignore_ascii_case("s")
    {
        return Ok(("stack,s", vec![value(t[..c].trim(), line)?]));
    }

    // Indexed and plain memory: size by value or a `z:`/`a:`/`f:` force.
    let (base, index) = match top_level_rfind(t, ',') {
        Some(c) => (t[..c].trim(), Some(t[c + 1..].trim())),
        None => (t, None),
    };
    let (force, expr) = addr_expr(base, line)?;
    let size = size_of(force, &expr, env, line);
    let mode = pick_mode(&has, size, index, line)?;
    Ok((mode, vec![expr]))
}

/// Resolve a `mvn`/`mvp src,dest` block move. Each operand is either `#bank`
/// (an explicit bank byte) or a 24-bit address whose bank (`^`, bits 16–23) is
/// taken. The encoding order is dest-bank then src-bank.
fn resolve_block_move(
    t: &str,
    operand: &str,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    let parts = split_top_level(t, ',');
    if parts.len() != 2 {
        return Err(AsmError::new(
            line,
            format!("block move needs two banks: `{operand}`"),
        ));
    }
    let bank = |p: &str| -> Result<Expr, AsmError> {
        let p = p.trim();
        match p.strip_prefix('#') {
            Some(r) => value(r, line),
            None => Ok(Expr::Bank(Box::new(value(p, line)?))),
        }
    };
    let src = bank(parts[0])?;
    let dest = bank(parts[1])?;
    Ok(("block-move", vec![dest, src]))
}

/// Resolve a `(...)`-shaped operand.
fn resolve_indirect(
    has: &dyn Fn(&str) -> bool,
    t: &str,
    operand: &str,
    line: usize,
) -> Result<(&'static str, Vec<Expr>), AsmError> {
    // `(expr,s),y` stack-relative indirect indexed.
    if let Some(body) = t
        .strip_suffix("),y")
        .or_else(|| t.strip_suffix("),Y"))
        .and_then(|s| s.strip_prefix('('))
        && let Some(c) = top_level_rfind(body, ',')
        && body[c + 1..].trim().eq_ignore_ascii_case("s")
    {
        return Ok(("(stack,s),y", vec![value(body[..c].trim(), line)?]));
    }
    // `(expr),y` indirect indexed.
    if let Some(body) = t
        .strip_suffix("),y")
        .or_else(|| t.strip_suffix("),Y"))
        .and_then(|s| s.strip_prefix('('))
    {
        return Ok(("(indirect),y", vec![value(body, line)?]));
    }
    // `(expr,x)` — indexed indirect (dp) or `jmp (abs,x)`.
    if let Some(body) = t
        .strip_suffix(",x)")
        .or_else(|| t.strip_suffix(",X)"))
        .and_then(|s| s.strip_prefix('('))
    {
        let (_f, e) = addr_expr(body, line)?;
        let mode = if has("(absolute,x)") {
            "(absolute,x)"
        } else {
            "(indirect,x)"
        };
        return Ok((mode, vec![e]));
    }
    // `(expr)` — `jmp (abs)` indirect, or the `(dp)` indirect.
    if let Some(body) = t.strip_suffix(')').and_then(|s| s.strip_prefix('(')) {
        let (_f, e) = addr_expr(body, line)?;
        let mode = if has("indirect") {
            "indirect"
        } else {
            "(indirect)"
        };
        return Ok((mode, vec![e]));
    }
    Err(AsmError::new(
        line,
        format!("malformed indirect operand `{operand}`"),
    ))
}

/// Map a resolved (size, index) to a spec mode label. ca65 picks the smallest
/// mode that fits the value, then **falls up** the size ladder (DP → absolute →
/// long) to the first form the instruction actually has — so a small operand on
/// a long-only op (`jsl sub`) widens to long, and `lda $12,y` (no DP,Y form)
/// widens to absolute,Y.
fn pick_mode(
    has: &dyn Fn(&str) -> bool,
    size: Size,
    index: Option<&str>,
    line: usize,
) -> Result<&'static str, AsmError> {
    let ladder: &[&'static str] = match index {
        None => &["zeropage", "absolute", "long"],
        Some(i) if i.eq_ignore_ascii_case("x") => &["zeropage,x", "absolute,x", "long,x"],
        Some(i) if i.eq_ignore_ascii_case("y") => &["zeropage,y", "absolute,y"],
        Some(i) => return Err(AsmError::new(line, format!("bad index `,{i}`"))),
    };
    let start = match size {
        Size::Dp => 0,
        Size::Abs => 1,
        Size::Long => ladder
            .len()
            .checked_sub(1)
            .filter(|&n| n >= 2)
            .ok_or_else(|| {
                AsmError::new(line, "long addressing is not available with this index")
            })?,
    };
    for &label in &ladder[start..] {
        if has(label) {
            return Ok(label);
        }
    }
    Err(AsmError::new(
        line,
        "no suitable addressing mode for this operand",
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Size {
    Dp,
    Abs,
    Long,
}

/// Strip a `z:`/`a:`/`f:` size-force prefix and parse the address expression.
fn addr_expr(raw: &str, line: usize) -> Result<(Force, Expr), AsmError> {
    let t = raw.trim();
    let (force, body) = if let Some(r) = t.strip_prefix("z:").or_else(|| t.strip_prefix("Z:")) {
        (Force::Dp, r)
    } else if let Some(r) = t.strip_prefix("a:").or_else(|| t.strip_prefix("A:")) {
        (Force::Abs, r)
    } else if let Some(r) = t.strip_prefix("f:").or_else(|| t.strip_prefix("F:")) {
        (Force::Long, r)
    } else {
        (Force::None, t)
    };
    Ok((force, value(body, line)?))
}

/// Decide DP vs absolute vs long: an explicit force wins; otherwise a constant
/// picks the smallest that fits (DP < $100, absolute < $10000, else long); a
/// forward/symbolic address defaults to absolute, matching ca65.
fn size_of(force: Force, expr: &Expr, env: &BTreeMap<String, i64>, line: usize) -> Size {
    match force {
        Force::Dp => return Size::Dp,
        Force::Abs => return Size::Abs,
        Force::Long => return Size::Long,
        Force::None => {}
    }
    match fold_const(expr, env, line) {
        Ok(v) if (0..=0xFF).contains(&v) => Size::Dp,
        Ok(v) if (0..=0xFFFF).contains(&v) => Size::Abs,
        Ok(_) => Size::Long,
        Err(_) => Size::Abs,
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_ca65_816 as asm;

    fn bytes(src: &str) -> Vec<u8> {
        asm(src).expect("assemble").bytes
    }

    #[test]
    fn mx_width_follows_directives() {
        // .a16 makes the accumulator immediate 16-bit regardless of value.
        assert_eq!(bytes(".a8\n lda #$12\n"), vec![0xA9, 0x12]);
        assert_eq!(bytes(".a16\n lda #$12\n"), vec![0xA9, 0x12, 0x00]);
        // .i16 widens the index immediate independently.
        assert_eq!(bytes(".i16\n ldx #1\n"), vec![0xA2, 0x01, 0x00]);
        // rep/sep immediates are always 8-bit.
        assert_eq!(bytes(".a16\n .i16\n rep #$30\n"), vec![0xC2, 0x30]);
    }

    #[test]
    fn dword_dbyt_asciiz_match_reference_bytes() {
        // Byte-for-byte against `ca65 --cpu 65816` + ld65:
        //   .dword $12345678 -> 78 56 34 12 (32-bit little-endian)
        //   .dbyt  $1234     -> 12 34       (16-bit big-endian)
        //   .asciiz "hi"     -> 68 69 00
        assert_eq!(
            bytes(" .dword $12345678\n .dbyt $1234\n .asciiz \"hi\"\n"),
            vec![0x78, 0x56, 0x34, 0x12, 0x12, 0x34, 0x68, 0x69, 0x00]
        );
    }

    #[test]
    fn size_picks_smallest_with_forces() {
        assert_eq!(bytes(" lda $12\n"), vec![0xA5, 0x12]); // dp
        assert_eq!(bytes(" lda $1234\n"), vec![0xAD, 0x34, 0x12]); // abs
        assert_eq!(bytes(" lda $123456\n"), vec![0xAF, 0x56, 0x34, 0x12]); // long
        assert_eq!(bytes("v = $34\n lda z:v\n"), vec![0xA5, 0x34]); // forced dp (fits a byte)
        assert_eq!(bytes(" lda a:$12\n"), vec![0xAD, 0x12, 0x00]); // forced abs
        assert_eq!(bytes(" lda f:$12\n"), vec![0xAF, 0x12, 0x00, 0x00]); // forced long
    }

    #[test]
    fn new_addressing_modes() {
        assert_eq!(bytes(" lda [$12]\n"), vec![0xA7, 0x12]); // [dp]
        assert_eq!(bytes(" lda [$12],y\n"), vec![0xB7, 0x12]); // [dp],y
        assert_eq!(bytes(" lda 3,s\n"), vec![0xA3, 0x03]); // stack rel
        assert_eq!(bytes(" lda (3,s),y\n"), vec![0xB3, 0x03]); // (sr),y
        assert_eq!(bytes(" lda ($12)\n"), vec![0xB2, 0x12]); // (dp)
        assert_eq!(bytes(" lda $123456,x\n"), vec![0xBF, 0x56, 0x34, 0x12]); // long,x
    }

    #[test]
    fn jumps_and_long_calls() {
        assert_eq!(bytes(" jml $123456\n"), vec![0x5C, 0x56, 0x34, 0x12]);
        assert_eq!(bytes(" jsl $123456\n"), vec![0x22, 0x56, 0x34, 0x12]);
        assert_eq!(bytes(" jmp [$1234]\n"), vec![0xDC, 0x34, 0x12]);
        assert_eq!(bytes(" jmp ($1234,x)\n"), vec![0x7C, 0x34, 0x12]);
        // A 16-bit (or forward) operand on the long-only jsl widens to long.
        assert_eq!(
            bytes("sub = $1234\n jsl sub\n"),
            vec![0x22, 0x34, 0x12, 0x00]
        );
    }

    #[test]
    fn brl_relative_is_16_bit() {
        // brl to self: offset = -3 (the instruction is 3 bytes).
        assert_eq!(bytes(".org $1000\nl: brl l\n"), vec![0x82, 0xFD, 0xFF]);
    }

    #[test]
    fn block_moves_cop_wdm_and_bank_byte() {
        // mvn src,dest -> opcode, dest-bank, src-bank (order swapped).
        assert_eq!(bytes(" mvn #$7e,#$7f\n"), vec![0x54, 0x7F, 0x7E]);
        assert_eq!(bytes(" mvp #$00,#$01\n"), vec![0x44, 0x01, 0x00]);
        // Bare addresses contribute their bank byte (bits 16-23).
        assert_eq!(
            bytes("s = $7e0000\nd = $7f0000\n mvn s,d\n"),
            vec![0x54, 0x7F, 0x7E]
        );
        // cop/wdm take a bare signature byte (no #).
        assert_eq!(bytes(" cop $12\n"), vec![0x02, 0x12]);
        assert_eq!(bytes(" wdm $34\n"), vec![0x42, 0x34]);
        // The `^` bank-byte operator (and 24-bit constants).
        assert_eq!(bytes("p = $7e1234\n lda #^p\n"), vec![0xA9, 0x7E]);
        assert_eq!(bytes("p = $7e1234\n lda #>p\n"), vec![0xA9, 0x12]);
        assert_eq!(bytes("p = $7e1234\n lda #<p\n"), vec![0xA9, 0x34]);
    }

    #[test]
    fn standalone_and_stz() {
        assert_eq!(bytes(" xce\n"), vec![0xFB]);
        assert_eq!(bytes(" xba\n"), vec![0xEB]);
        assert_eq!(bytes(" tcd\n"), vec![0x5B]);
        assert_eq!(bytes(" phb\n"), vec![0x8B]);
        assert_eq!(bytes(" inc a\n"), vec![0x1A]);
        assert_eq!(bytes(" stz $12\n"), vec![0x64, 0x12]);
        assert_eq!(bytes(" stz $1234,x\n"), vec![0x9E, 0x34, 0x12]);
        assert_eq!(bytes(" bra l\nl: rts\n"), vec![0x80, 0x00, 0x60]);
    }
}
