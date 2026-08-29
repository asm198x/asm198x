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
    self, BytePrec, Caret, ExprOpts, fold_const, is_ident, split_data_items, split_first_word,
    split_top_level, string_literal,
};
use super::text;
use crate::ast::{Comment, Node, Program, Scope, Span, Symbol, Trivia};
use crate::dialect::Dialect;
use crate::directives::{Category, Directive, Pattern, lookup};
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

/// The rgbasm (SM83) dialect.
pub(crate) struct Rgbasm;

impl Dialect for Rgbasm {
    /// rgbasm 1.0.3 truncates **and says so**: `db 256` and `ld a,300` both
    /// give `warning: Expression must be 8-bit; use LOW() to force 8-bit
    /// [-Wtruncation]` and assemble to `00` / `3e 2c`. It warns for operands
    /// and data alike (probed 2026-08-25).
    fn oversized_byte_policy(&self) -> crate::dialect::Oversize {
        crate::dialect::Oversize::TruncateWarn
    }

    /// A Game Boy ROM starts at file offset 0, whatever the program's lowest
    /// section is: a lone `SECTION "p", ROMX, BANK[2]` still emits the 32K
    /// before it (probe-pinned — `rgblink` writes 49152 bytes for that).
    fn image_base(&self) -> Option<i64> {
        Some(0)
    }

    /// A Game Boy ROM is a whole number of $4000 banks: `rgblink` writes
    /// `(highest bank + 1) * $4000` bytes, with no rounding to a power of two
    /// (three banks give 49152, six give 98304 — both probe-pinned).
    ///
    /// Only when a bank was actually used. A ROM0-only program keeps the exact
    /// length it was written to, which is what `rgblink -x` emits and what
    /// every existing probe compares.
    fn image_size(&self, image: &[u8]) -> Option<usize> {
        let banks = image.len().div_ceil(0x4000);
        (banks > 1).then(|| banks * 0x4000)
    }

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
        let banks = declared_banks(&out);
        resolve_banks(&mut out, &banks)?;
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
        let banks = declared_banks(&out);
        resolve_banks(&mut out, &banks)?;
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

        // `SECTION "name", TYPE[$addr]` — a directive preserved verbatim, and
        // an actual section rather than an origin in disguise. Lowering it to
        // `Org` could only move the counter forward, so two pinned sections in
        // descending address order — which `rgblink` places without complaint
        // — failed with `cannot move origin backwards`.
        if code
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("SECTION")
        {
            let code = code.trim();
            let bank = section_bank(code, line)?;
            let pinned = section_origin(code, line)?
                .map(|e| fold_const(&e, &self.consts, line))
                .transpose()?;
            // A banked section is addressed at $4000 whichever bank holds it,
            // and lands at `bank * $4000` in the ROM. Bank 0 is `ROM0`, which
            // is addressed and placed at the same $0000.
            let (base, at) = match bank {
                Some(n) if n > 0 => (
                    Some(pinned.unwrap_or(ROMX_BASE)),
                    crate::engine::Place::At(n * BANK_SIZE),
                ),
                _ => (pinned, crate::engine::Place::ByAddress),
            };
            let item = Some(crate::ast::item_from_operation(Operation::Section {
                name: section_name(code),
                base,
                at,
            }));
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

        // `[DEF] NAME EQUS "text"` — a string symbol. What it holds is text
        // spliced into the source rather than a value bound to a name, so
        // there is nothing here for the engine to lower, and the text pass has
        // already taken the line by the time assembly walks. This is the
        // formatter's path, and a line it must hand back exactly as written:
        // the value may hold quotes, escapes and calls that any re-rendering
        // would have to reproduce byte for byte. See `Item::Verbatim`.
        // A `{name}` interpolation is the same kind of line: text spliced into
        // the middle of a token, which the pass resolved before assembly and
        // which cannot be laid out as an expression because it is not one yet.
        // An unresolved interpolation is refused by the pass, so reaching here
        // with one means the formatter's parse.
        if string_symbol(code.trim()).is_some() || code.contains('{') {
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
    // Only the bracket on the *type*. `SECTION "p", ROMX, BANK[2]` has one
    // bracketed number and it is not an address — reading the whole line put
    // the bank number in the origin and every label in a banked section came
    // out two bytes into the ROM.
    let code = match code.to_ascii_uppercase().find("BANK") {
        Some(i) => &code[..i],
        None => code,
    };
    match (code.find('['), code.rfind(']')) {
        (Some(a), Some(b)) if a < b => Ok(Some(value(code[a + 1..b].trim(), line)?)),
        _ => Ok(None),
    }
}

/// Marks a `BANK("name")` whose section may not have been seen yet. `\u{1}`
/// cannot appear in source.
const BANK_MARK: &str = "\u{1}bank\u{1}";

/// The prefix on the stand-in a text-layer call parses to when the text pass
/// has not run — the formatter's path. `\u{1}` cannot appear in rgbasm source,
/// so the name can never collide with a real symbol.
const TEXT_MARK: &str = "\u{1}text\u{1}";

/// The text layer's function names, lower-cased. One list, read by the pass
/// that folds them and by the stand-in the formatter parses.
fn is_text_function(lower: &str) -> bool {
    matches!(
        lower,
        "strcat"
            | "strfmt"
            | "strupr"
            | "strlwr"
            | "strsub"
            | "strslice"
            | "strlen"
            | "strcmp"
            | "strfind"
            | "strin"
            | "strrin"
            | "strrpl"
    )
}

/// rgbasm's expression functions. `BANK("name")` is the bank its section was
/// given, and it reaches **forward** — `db BANK("paged")` above the section it
/// names assembles — so it cannot fold while walking. It becomes a marker that
/// [`resolve_banks`] answers once every `SECTION` in the program is known.
/// rgbasm's numbers, which include fixed-point literals.
///
/// `1.0` is `$10000`: the value is scaled by `1 << 16` and the fraction
/// truncated toward zero, so `3.7` is `$3B333` rather than a rounded
/// `$3B334`. A `qN` suffix names another precision — `3.7q8` is `$3B3` —
/// which is how a program that has changed the default writes a literal.
/// Anything without a point is an ordinary integer.
fn number(text: &str, line: usize) -> Result<i64, AsmError> {
    let (digits, precision) = match text.split_once(['q', 'Q']) {
        Some((head, bits)) if head.contains('.') => {
            let bits: u32 = bits.parse().map_err(|_| {
                AsmError::new(line, format!("`{text}`: `q` needs a bit count after it"))
            })?;
            if bits == 0 || bits > 31 {
                return Err(AsmError::new(
                    line,
                    format!("`{text}`: a fixed-point precision runs from 1 to 31 bits"),
                ));
            }
            (head, bits)
        }
        _ => (text, crate::engine::FIX_BITS as u32),
    };
    let Some((whole, fraction)) = digits.split_once('.') else {
        return mos6502::parse_number(text, line);
    };
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole
            .parse()
            .map_err(|_| AsmError::new(line, format!("`{text}` is not a number")))?
    };
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(AsmError::new(line, format!("`{text}` is not a number")));
    }
    // The fraction as a rational, scaled and rounded to nearest with a half
    // going away from zero (the sign is a separate unary minus, so the value
    // here is never negative). `.1` at 16 bits is `65536 / 10`, which is
    // `6553.6` and lands on `6554` — truncation would answer `6553`, and
    // rgbasm v1.0.3 emits `$199A`.
    let scale = 1i64 << precision;
    let denominator = 10i64
        .checked_pow(fraction.len() as u32)
        .ok_or_else(|| AsmError::new(line, format!("`{text}` has too many decimal places")))?;
    let numerator: i64 = fraction
        .parse()
        .map_err(|_| AsmError::new(line, format!("`{text}` is not a number")))?;
    Ok(whole * scale + (numerator * scale + denominator / 2) / denominator)
}

fn expr_function(name: &str, args: Vec<mos6502::ExprArg>, line: usize) -> Result<Expr, AsmError> {
    use crate::engine::BinOp as Op;
    let lower = name.to_ascii_lowercase();

    // The text layer's functions (`decisions/string-and-text-layer.md`).
    // Assembly never arrives here: the pass folds them to text before the parse
    // runs. This arm is the **formatter's**, which parses without expanding and
    // then re-emits the call from its source — so all it needs is a value that
    // parses. The mark cannot be an rgbasm identifier, so a fold that somehow
    // did not happen fails as an unresolved symbol rather than answering a
    // number.
    if is_text_function(&lower) {
        return Ok(Expr::Sym(format!("{TEXT_MARK}{lower}")));
    }

    // The byte extractions, and the one bit-counting function whose answer is a
    // plain integer rather than a fixed-point value.
    let unary: Option<fn(Box<Expr>) -> Expr> = match lower.as_str() {
        "high" => Some(Expr::Hi),
        "low" => Some(Expr::Lo),
        "round" => Some(Expr::FixRound),
        "tzcount" => Some(Expr::TrailingZeros),
        _ => None,
    };
    if let Some(build) = unary {
        let [arg]: [_; 1] = args
            .try_into()
            .map_err(|_| AsmError::new(line, format!("`{name}` takes one argument")))?;
        return Ok(build(Box::new(arg.value(name, line)?)));
    }

    // `FLOOR` and `CEIL` need no node of their own: masking the fraction away
    // rounds toward minus infinity on a two's-complement value, which is what
    // flooring is, and ceiling is flooring one step up.
    let fraction = (1i64 << crate::engine::FIX_BITS) - 1;
    if matches!(lower.as_str(), "floor" | "ceil") {
        let [arg]: [_; 1] = args
            .try_into()
            .map_err(|_| AsmError::new(line, format!("`{name}` takes one argument")))?;
        let value = arg.value(name, line)?;
        let value = if lower == "ceil" {
            Expr::Bin(Op::Add, Box::new(value), Box::new(Expr::Num(fraction)))
        } else {
            value
        };
        return Ok(Expr::Bin(
            Op::And,
            Box::new(value),
            Box::new(Expr::Num(!fraction)),
        ));
    }

    // The two-argument fixed-point arithmetic. `FMOD` is the ordinary
    // remainder: both operands carry the same scale, so it survives it.
    let pair = match lower.as_str() {
        "mul" => Some(Op::FixMul),
        "div" => Some(Op::FixDiv),
        "fmod" => Some(Op::Mod),
        _ => None,
    };
    if let Some(op) = pair {
        let [a, b]: [_; 2] = args
            .try_into()
            .map_err(|_| AsmError::new(line, format!("`{name}` takes two arguments")))?;
        return Ok(Expr::Bin(
            op,
            Box::new(a.value(name, line)?),
            Box::new(b.value(name, line)?),
        ));
    }

    if !name.eq_ignore_ascii_case("bank") {
        return Err(AsmError::new(
            line,
            format!(
                "`{name}` is not an expression function asm198x implements yet — \
                 the source is valid and the gap is ours"
            ),
        ));
    }
    let [arg]: [_; 1] = args
        .try_into()
        .map_err(|_| AsmError::new(line, "`BANK` takes one argument"))?;
    Ok(Expr::Sym(format!("{BANK_MARK}{}", arg.text(name, line)?)))
}

/// Every section the program declared, and the bank it went in. A section with
/// no `BANK[...]` is bank 0 — `ROM0` and the unbanked types all live there.
fn declared_banks(ops: &[Statement]) -> BTreeMap<String, i64> {
    ops.iter()
        .filter_map(|st| match &st.op {
            Some(Operation::Section { name, at, .. }) => Some((
                name.clone(),
                match at {
                    crate::engine::Place::At(n) => n / BANK_SIZE,
                    _ => 0,
                },
            )),
            _ => None,
        })
        .collect()
}

/// Answer every `BANK` marker against the sections the program declared.
fn resolve_banks(ops: &mut [Statement], banks: &BTreeMap<String, i64>) -> Result<(), AsmError> {
    let mut missing = None;
    for st in ops.iter_mut() {
        if let Some(op) = st.op.take() {
            st.op = Some(crate::ast::map_syms(
                op,
                &mut |sym| match sym.strip_prefix(BANK_MARK) {
                    Some(name) => match banks.get(name) {
                        Some(bank) => Expr::Num(*bank),
                        None => {
                            missing.get_or_insert_with(|| name.to_string());
                            Expr::Num(0)
                        }
                    },
                    None => Expr::Sym(sym),
                },
            ));
        }
    }
    match missing {
        Some(name) => Err(AsmError::new(0, format!("no section named `{name}`"))),
        None => Ok(()),
    }
}

/// A Game Boy ROM bank, and where the CPU sees a banked one.
const BANK_SIZE: i64 = 0x4000;
const ROMX_BASE: i64 = 0x4000;

/// The bank in `SECTION "n", ROMX, BANK[k]`, if the source named one.
fn section_bank(code: &str, line: usize) -> Result<Option<i64>, AsmError> {
    let upper = code.to_ascii_uppercase();
    let Some(kw) = upper.find("BANK") else {
        return Ok(None);
    };
    let tail = &code[kw + 4..];
    match (tail.find('['), tail.find(']')) {
        (Some(a), Some(b)) if a < b => match value(tail[a + 1..b].trim(), line)? {
            Expr::Num(n) => Ok(Some(n)),
            _ => Err(AsmError::new(line, "`BANK[...]` needs a constant")),
        },
        _ => Ok(None),
    }
}

/// Render a `"text", value, ...` list into one message. rgbasm prints numbers
/// as `$5`; each reference has its own radix and nothing compares console
/// output.
fn render_message(args: &str, line: usize) -> Result<String, AsmError> {
    let mut out = String::new();
    for part in mos6502::split_top_level(args, ',') {
        let part = part.trim();
        if let Some(text) = part.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
            out.push_str(text);
        } else if let Ok(Expr::Num(v)) = value(part, line) {
            out.push_str(&format!("${v:X}"));
        } else {
            out.push_str(part);
        }
    }
    Ok(out)
}

/// The quoted name in `SECTION "name", TYPE[...]`, for the debug section table.
/// Empty when the source did not quote one — rgbasm requires it, so an empty
/// name means the line is malformed and the reference will say so.
fn section_name(code: &str) -> String {
    let mut parts = code.split('"');
    parts.next();
    parts.next().unwrap_or("").to_string()
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

/// `[DEF] NAME EQUS "text"` — a string symbol: the name the text pass binds,
/// and the text it holds.
///
/// Two readers, one grammar. The text pass stores the value and leaves the line
/// blank, so assembly never parses one; the formatter runs with the pass
/// switched off, so it does.
fn string_symbol(code: &str) -> Option<(String, String)> {
    let code = macros::without_comment(code);
    let mut words = code.split_whitespace();
    let first = words.next()?;
    let (name, keyword) = if first.eq_ignore_ascii_case("def") {
        (words.next()?, words.next()?)
    } else {
        (first, words.next()?)
    };
    if !keyword.eq_ignore_ascii_case("equs") {
        return None;
    }
    let value = code.split_once(keyword)?.1.trim();
    Some((name.trim_end_matches(':').to_string(), value.to_string()))
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
    // `dl` is 32-bit, little-endian like the rest — the width a fixed-point
    // value needs to survive being written down.
    Directive {
        id: "longs",
        pattern: Pattern::Exact(&["dl"]),
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
    // `ASSERT` checks at link time and `STATIC_ASSERT` while assembling; with
    // assembly and linking fused there is no moment between them, so both are
    // the same check here.
    Directive {
        id: "assert",
        pattern: Pattern::Exact(&["assert", "static_assert"]),
        category: Category::Operation,
    },
    // `PRINT` and `PRINTLN` differ only by a trailing newline, which a
    // diagnostic carries no more than a line does.
    Directive {
        id: "print",
        pattern: Pattern::Exact(&["print", "println"]),
        category: Category::Operation,
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
    // `EXPORT` is the one visibility word in any of the six references that
    // asks nothing at all: `EXPORT nope` for a name defined nowhere links to a
    // ROM without complaint, and only a *reference* to an undefined name fails
    // — at rgblink, which is the ordinary undefined-symbol refusal here.
    // Probed against rgbasm 1.0.3 + rgblink;
    // `decisions/symbol-visibility-in-a-fused-assembler.md`.
    Directive {
        id: "export",
        pattern: Pattern::Exact(&["export"]),
        category: Category::Ignored,
    },
    // symbol management
    Directive {
        id: "unsupported-symbol",
        pattern: Pattern::Exact(&["purge", "redef", "def", "shift"]),
        category: Category::KnownUnsupported,
    },
    // the RS counter
    Directive {
        id: "unsupported-the",
        pattern: Pattern::Exact(&["rsset", "rsreset"]),
        category: Category::KnownUnsupported,
    },
    // blocks and layout
    // The expression functions this dialect implements. Declared as what they
    // are: they never begin a statement, so naming them as operations would
    // claim a line they cannot start (`Category::ExpressionWord`).
    // The text layer's vocabulary (`decisions/string-and-text-layer.md`).
    // `EQUS` defines a string symbol and the rest are string functions, all
    // resolved by a source pre-pass before the parse sees them.
    Directive {
        id: "string-symbol",
        pattern: Pattern::Exact(&["equs"]),
        category: Category::Operation,
    },
    Directive {
        id: "string-functions",
        pattern: Pattern::Exact(&[
            "strcat", "strfmt", "strupr", "strlwr", "strsub", "strslice", "strlen", "strcmp",
            "strfind", "strin", "strrin", "strrpl",
        ]),
        category: Category::ExpressionWord,
    },
    Directive {
        id: "fixed-point-functions",
        pattern: Pattern::Exact(&[
            "high", "low", "mul", "div", "fmod", "floor", "ceil", "round", "tzcount",
        ]),
        category: Category::ExpressionWord,
    },
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
            Category::ExpressionWord => {
                return Err(AsmError::new(
                    line,
                    crate::directives::not_a_statement(word),
                ));
            }
            Category::KnownUnsupported => {
                return Err(AsmError::new(
                    line,
                    format!(
                        "`{word}` is a real directive here and asm198x does not implement it yet"
                    ),
                ));
            }
            // Declared for `rgbasm` only where rgbasm itself refuses the word for the
            // binary we emit; the refusal is the match, not a gap.
            Category::RefusedByReference(rule) => {
                return Err(AsmError::new(
                    line,
                    crate::directives::refused_by_reference("rgbasm", word, rule),
                ));
            }
            Category::Operation => match directive.id {
                "bytes" => Operation::Bytes(byte_list(args, line)?),
                "words" => Operation::Words(value_list(args, line)?),
                "longs" => Operation::Encoded(
                    value_list(args, line)?
                        .into_iter()
                        .map(|expr| Piece::Val {
                            expr,
                            bytes: 4,
                            rel: false,
                            signed: false,
                        })
                        .collect(),
                ),
                "reserve" => parse_ds(args, consts, line)?,
                "assert" => {
                    let parts = mos6502::split_top_level(args, ',');
                    let cond = value(parts.first().copied().unwrap_or("").trim(), line)?;
                    let message = parts
                        .get(1)
                        .map(|m| format!("Assertion failed: {}", m.trim().trim_matches('"')))
                        .unwrap_or_else(|| "Assertion failed".to_string());
                    Operation::Assert {
                        cond,
                        fatal: true,
                        message,
                    }
                }
                "print" => Operation::Diagnose {
                    severity: crate::engine::DiagSeverity::Note,
                    message: render_message(args, line)?,
                },
                "diagnose" => Operation::Diagnose {
                    severity: if word.eq_ignore_ascii_case("fail") {
                        crate::engine::DiagSeverity::Error
                    } else {
                        crate::engine::DiagSeverity::Warning
                    },
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
        number,
        ExprOpts {
            logical: false,
            scoped_names: false,
            fixed_point: true,
            compare: mos6502::Compare {
                eq: false,
                eq_eq: true,
                ne_angle: false,
                ne_bang: true,
                relational: true,
                ordered_eq: true,
                minus_one: false,
            },
            function: Some(expr_function),
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
            xor_mask: 0,
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
// Not adopted, and registered in `decisions/reference-parity-goal.md`: `\#`
// (all arguments verbatim), `_NARG` (the argument count) and `\@` (a
// per-expansion unique suffix).
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

/// Expand rgbasm's macros and resolve its text layer, unless this parse is the
/// formatter's.
///
/// The text pass runs **after** macro expansion, so a string function a macro
/// produced is folded like any other, and it emits one line per line, so the
/// origins the expansion recorded still line up.
fn expand_rgbasm(source: &str, mode: macros::Expand) -> Result<macros::Expansion, AsmError> {
    macros::expansion(mode, source, |s| {
        let expanded = macros::expand(&RgbasmMacros, s)?;
        let resolved = text::expand(&RgbasmText, &expanded.text)?;
        Ok(Some((resolved, expanded.origins)))
    })
}

// ---------------------------------------------------------------------------
// The text layer (`decisions/string-and-text-layer.md`)
// ---------------------------------------------------------------------------

/// rgbasm's string grammar: `EQUS` symbols, `{name}` interpolation, and the
/// string functions whose answers do not need the layout.
///
/// Every rule here was read off v1.0.3 before it was written, and the two that
/// a reader would most likely guess wrong are the index conventions:
/// `STRSUB` is **1-based with a length**, `STRSLICE` is **0-based with an end**,
/// `STRFIND` answers a 0-based index or `-1`, and `STRIN` answers a 1-based one
/// or `0`.
struct RgbasmText;

impl text::TextSyntax for RgbasmText {
    /// `DEF name EQUS "text"`, and the bare `name EQUS "text"` older form.
    fn definition(&self, line: &str) -> Option<(String, String)> {
        string_symbol(line)
    }

    fn interpolation(&self) -> Option<(&'static str, char)> {
        Some(("{", '}'))
    }

    /// `[DEF] NAME EQU expr` and `[DEF] NAME = expr`, read through the same
    /// parser the statement itself uses, so the environment the text pass
    /// folds against and the one the engine binds cannot drift apart.
    fn constant(&self, line: &str, numbers: &BTreeMap<String, i64>) -> Option<(String, i64)> {
        let found = constant(macros::without_comment(line).trim(), 0)
            .ok()
            .flatten()?;
        let value = fold_const(&found.expr, numbers, 0).ok()?;
        Some((found.name, value))
    }

    /// An rgbasm expression over the constants above the line. A label's
    /// address is deliberately unreachable here: the layout has not run.
    fn evaluate(&self, text: &str, numbers: &BTreeMap<String, i64>, line: usize) -> Option<i64> {
        let expr = value(text, line).ok()?;
        fold_const(&expr, numbers, line).ok()
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
        // An empty argument list is how the pass asks "is this a call?", so it
        // is answered without complaint.
        if args.is_empty() {
            return Ok(Some(Folded::Text(String::new())));
        }
        let arity = |want: usize| -> Result<(), AsmError> {
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
        };
        let chars = |a: &text::Arg| -> Result<Vec<char>, AsmError> {
            Ok(a.text(name, line)?.chars().collect())
        };
        Ok(Some(match lower.as_str() {
            "strcat" => {
                let mut out = String::new();
                for a in args {
                    out.push_str(a.text(name, line)?);
                }
                Folded::Text(out)
            }
            "strupr" => {
                arity(1)?;
                Folded::Text(args[0].text(name, line)?.to_ascii_uppercase())
            }
            "strlwr" => {
                arity(1)?;
                Folded::Text(args[0].text(name, line)?.to_ascii_lowercase())
            }
            // 1-based, and a length: `STRSUB("abcd", 2, 2)` is `bc`. Past the
            // end it stops at the end rather than refusing.
            "strsub" => {
                arity(3)?;
                let text = chars(&args[0])?;
                let start = scope.number(&args[1], name, line)?.max(1) as usize - 1;
                let len = scope.number(&args[2], name, line)?.max(0) as usize;
                Folded::Text(text.into_iter().skip(start).take(len).collect())
            }
            // 0-based, and an end: `STRSLICE("abcd", 1, 3)` is `bc`.
            "strslice" => {
                arity(3)?;
                let text = chars(&args[0])?;
                let start = scope.number(&args[1], name, line)?.max(0) as usize;
                let end = scope.number(&args[2], name, line)?.max(0) as usize;
                Folded::Text(text.into_iter().take(end).skip(start).collect())
            }
            "strlen" => {
                arity(1)?;
                Folded::Number(args[0].text(name, line)?.chars().count() as i64)
            }
            // `-1`, `0` or `1`, the way C's `strcmp` reports it.
            "strcmp" => {
                arity(2)?;
                let (a, b) = (args[0].text(name, line)?, args[1].text(name, line)?);
                Folded::Number(match a.cmp(b) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                })
            }
            // The two searches differ in *both* base and miss value, which is
            // the pair most likely to be implemented as one function.
            "strfind" => {
                arity(2)?;
                let (h, n) = (args[0].text(name, line)?, args[1].text(name, line)?);
                Folded::Number(h.find(n).map_or(-1, |i| h[..i].chars().count() as i64))
            }
            "strin" => {
                arity(2)?;
                let (h, n) = (args[0].text(name, line)?, args[1].text(name, line)?);
                Folded::Number(h.find(n).map_or(0, |i| h[..i].chars().count() as i64 + 1))
            }
            "strrin" => {
                arity(2)?;
                let (h, n) = (args[0].text(name, line)?, args[1].text(name, line)?);
                Folded::Number(h.rfind(n).map_or(0, |i| h[..i].chars().count() as i64 + 1))
            }
            // printf's shape, with rgbasm's own rules: the flags come in a
            // fixed order, `#` is a base prefix rather than C's alternate
            // form, and `%f` reads its argument as Q16.16.
            "strfmt" => {
                if args.is_empty() {
                    return Err(AsmError::new(line, "`STRFMT` needs a format string"));
                }
                Folded::Text(strfmt(args[0].text(name, line)?, &args[1..], scope, line)?)
            }
            "strrpl" => {
                arity(3)?;
                let (h, from, to) = (
                    args[0].text(name, line)?,
                    args[1].text(name, line)?,
                    args[2].text(name, line)?,
                );
                Folded::Text(h.replace(from, to))
            }
            other => unreachable!("`{other}` was matched as known and then not folded"),
        }))
    }
}

/// One `%` conversion in an rgbasm format string.
///
/// The flags are parsed in a **fixed order** — sign, then `#`, then `-`, then
/// `0` — because that is what v1.0.3 accepts: `%+#x` assembles and `%#+x` is
/// "Invalid format spec". Anything a C programmer would expect to be
/// order-free is therefore not.
#[derive(Default)]
struct FormatSpec {
    /// `+` or a space: what stands in for the sign of a value that has none.
    sign: Option<char>,
    /// `#`: `$`/`%`/`&` before a hex/binary/octal body, and rgbasm's `q16`
    /// suffix after a fixed-point one.
    prefix: bool,
    left: bool,
    zero: bool,
    width: usize,
    /// Digits after the point, which only `%f` takes.
    precision: Option<usize>,
    kind: char,
}

/// Parse one spec, starting just after the `%`, and answer it with the index of
/// the first byte past it.
fn format_spec(fmt: &[u8], mut i: usize) -> Option<(FormatSpec, usize)> {
    let mut spec = FormatSpec::default();
    if matches!(fmt.get(i), Some(b'+' | b' ')) {
        spec.sign = Some(fmt[i] as char);
        i += 1;
    }
    if fmt.get(i) == Some(&b'#') {
        spec.prefix = true;
        i += 1;
    }
    if fmt.get(i) == Some(&b'-') {
        spec.left = true;
        i += 1;
    }
    if fmt.get(i) == Some(&b'0') {
        spec.zero = true;
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
    if !matches!(spec.kind, 'd' | 'u' | 'x' | 'X' | 'b' | 'o' | 'f' | 's') {
        return None;
    }
    Some((spec, i + 1))
}

/// Fold one `STRFMT` call.
fn strfmt(
    fmt: &str,
    args: &[text::Arg],
    scope: &text::Scope,
    line: usize,
) -> Result<String, AsmError> {
    let bytes = fmt.as_bytes();
    let mut out = String::with_capacity(fmt.len());
    let mut used = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'%' {
                i += 1;
            }
            out.push_str(&fmt[start..i]);
            continue;
        }
        // `%%` is a literal per cent and takes no argument.
        if bytes.get(i + 1) == Some(&b'%') {
            out.push('%');
            i += 2;
            continue;
        }
        if i + 1 >= bytes.len() {
            return Err(AsmError::new(
                line,
                "`STRFMT` was given a format string ending in a lone `%`",
            ));
        }
        let (spec, next) = format_spec(bytes, i + 1).ok_or_else(|| {
            AsmError::new(
                line,
                format!(
                    "`STRFMT` cannot read the format spec for argument {}: rgbasm takes the \
                     flags in the order `+`/space, `#`, `-`, `0`, and only `%f` takes a \
                     precision",
                    used + 1
                ),
            )
        })?;
        let arg = args.get(used).ok_or_else(|| {
            AsmError::new(
                line,
                format!(
                    "`STRFMT` has {} format spec(s) and was given {} argument(s) to fill them",
                    used + 1,
                    args.len()
                ),
            )
        })?;
        out.push_str(&format_one(&spec, arg, scope, line)?);
        used += 1;
        i = next;
    }
    if used < args.len() {
        return Err(AsmError::new(
            line,
            format!(
                "`STRFMT` was given {} argument(s) and has {used} format spec(s) to put them in",
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
    scope: &text::Scope,
    line: usize,
) -> Result<String, AsmError> {
    let refuse = |what: &str| AsmError::new(line, format!("`STRFMT` cannot format {what}"));
    let kind = spec.kind;
    if spec.precision.is_some() && kind != 'f' {
        return Err(refuse(&format!(
            "`%{kind}` with a precision: only `%f` takes one"
        )));
    }
    if spec.prefix && matches!(kind, 'd' | 'u') {
        return Err(refuse(&format!(
            "`%{kind}` with `#`: it has no base to mark"
        )));
    }
    // The sign of a number stands in front of its zero padding; a prefix stands
    // between the two. `%+#08x` of 5 is `+$000005`.
    let (sign, prefix, body) = if kind == 's' {
        if let Some(flag) = spec.sign {
            return Err(refuse(&format!("a string with the sign flag `{flag}`")));
        }
        if spec.zero {
            return Err(refuse("a string with the padding flag `0`"));
        }
        let text::Arg::Text(text) = arg else {
            return Err(refuse("a number as `%s`"));
        };
        (String::new(), String::new(), text.clone())
    } else {
        if let text::Arg::Text(text) = arg {
            return Err(refuse(&format!("the string \"{text}\" as `%{kind}`")));
        }
        // rgbasm's values are 32-bit, and every type but `%d` and `%f` reads
        // them unsigned: `%x` of -1 is `ffffffff`.
        let value = scope.number(arg, "STRFMT", line)? as i32;
        let (negative, body) = match kind {
            'd' => (value < 0, value.unsigned_abs().to_string()),
            'u' => (false, (value as u32).to_string()),
            'x' => (false, format!("{:x}", value as u32)),
            'X' => (false, format!("{:X}", value as u32)),
            'b' => (false, format!("{:b}", value as u32)),
            'o' => (false, format!("{:o}", value as u32)),
            _ => (value < 0, fixed_point(value, spec.precision.unwrap_or(5))),
        };
        let sign = if negative {
            "-".to_string()
        } else {
            spec.sign.map(String::from).unwrap_or_default()
        };
        let prefix = match (spec.prefix, kind) {
            (true, 'x' | 'X') => "$",
            (true, 'b') => "%",
            (true, 'o') => "&",
            _ => "",
        };
        // `%#f` marks the fixed-point precision after the digits rather than
        // the base before them.
        let body = if spec.prefix && kind == 'f' {
            format!("{body}q{}", crate::engine::FIX_BITS)
        } else {
            body
        };
        (sign, prefix.to_string(), body)
    };
    let width = sign.len() + prefix.len() + body.chars().count();
    let pad = spec.width.saturating_sub(width);
    Ok(match () {
        // Left alignment wins over zero padding, as it does in C.
        _ if spec.left => format!("{sign}{prefix}{body}{:pad$}", "", pad = pad),
        _ if spec.zero => format!("{sign}{prefix}{:0>pad$}{body}", "", pad = pad),
        _ => format!("{:pad$}{sign}{prefix}{body}", "", pad = pad),
    })
}

/// The digits of a Q16.16 value's magnitude, to `precision` decimal places.
///
/// The value is a dyadic rational, so its decimal expansion terminates and a
/// `f64` holds it exactly; the rounding at the cut is to nearest, ties to even,
/// which is what v1.0.3 does — `%.0f` of `0.5` is `0` and of `1.5` is `2`.
fn fixed_point(value: i32, precision: usize) -> String {
    let magnitude = f64::from(value.unsigned_abs()) / f64::from(1u32 << crate::engine::FIX_BITS);
    format!("{magnitude:.precision$}")
}

#[cfg(test)]
mod tests {

    /// The text layer: string symbols and string functions, folded before the
    /// parse. Every value here was read off rgbasm v1.0.3 first.
    #[test]
    fn the_string_functions_fold_the_way_rgbasm_folds_them() {
        let b = |src: &str| {
            asm(&format!("SECTION \"a\", ROM0[0]\n{src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .bytes
        };
        assert_eq!(b("db STRCAT(\"ab\",\"cd\")"), b"abcd");
        assert_eq!(b("db STRUPR(\"ab\"), STRLWR(\"CD\")"), b"ABcd");
        assert_eq!(b("db STRRPL(\"abab\",\"b\",\"X\")"), b"aXaX");

        // The two slicing functions disagree about both ends: `STRSUB` is
        // 1-based and takes a *length*, `STRSLICE` is 0-based and takes an
        // *end*. They pick out the same two characters here by different routes.
        assert_eq!(b("db STRSUB(\"abcd\", 2, 2)"), b"bc");
        assert_eq!(b("db STRSLICE(\"abcd\", 1, 3)"), b"bc");
        // Past the end, `STRSUB` stops rather than refusing.
        assert_eq!(b("db STRSUB(\"ab\", 2, 9)"), b"b");

        assert_eq!(b("db STRLEN(\"abc\")"), vec![3]);
        // The searches differ in base *and* in what a miss answers.
        assert_eq!(b("db STRFIND(\"abc\",\"b\")"), vec![1]);
        assert_eq!(b("db STRIN(\"abc\",\"b\")"), vec![2]);
        assert_eq!(b("db STRRIN(\"abab\",\"b\")"), vec![4]);
        assert_eq!(b("db STRFIND(\"abc\",\"z\")"), vec![0xFF]);
        assert_eq!(b("db STRIN(\"abc\",\"z\")"), vec![0]);
        assert_eq!(
            b("db STRCMP(\"a\",\"b\"), STRCMP(\"b\",\"a\"), STRCMP(\"a\",\"a\")"),
            vec![0xFF, 1, 0]
        );

        // Nesting: the innermost call folds first, so the outer one sees a
        // literal.
        assert_eq!(b("db STRLEN(STRCAT(\"ab\",\"cd\"))"), vec![4]);
        assert_eq!(b("db STRUPR(STRSUB(\"abcd\", 2, 2))"), b"BC");
    }

    /// `STRFMT` is printf's shape with rgbasm's rules. Every value here was
    /// read off rgbasm v1.0.3 first, and several are not what a C programmer
    /// would predict.
    #[test]
    fn strfmt_formats_the_way_rgbasm_formats() {
        let b = |src: &str| {
            asm(&format!("SECTION \"a\", ROM0[0]\n{src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .bytes
        };
        assert_eq!(b("db STRFMT(\"%d\", 42)"), b"42");
        assert_eq!(b("db STRFMT(\"%X|%x\", 255, 255)"), b"FF|ff");
        assert_eq!(b("db STRFMT(\"%b|%o\", 5, 9)"), b"101|11");
        assert_eq!(b("db STRFMT(\"[%s]\", \"hi\")"), b"[hi]");
        assert_eq!(b("db STRFMT(\"100%%\")"), b"100%");

        // Width, then the padding rules: zero padding stands *inside* the sign
        // and the base prefix, and left alignment overrides it.
        assert_eq!(b("db STRFMT(\"%5d|%05d\", 42, 42)"), b"   42|00042");
        assert_eq!(b("db STRFMT(\"%-5d|\", 42)"), b"42   |");
        assert_eq!(b("db STRFMT(\"%-06d|\", 42)"), b"42    |");
        assert_eq!(b("db STRFMT(\"%+06d|%06d\", 42, -42)"), b"+00042|-00042");
        assert_eq!(b("db STRFMT(\"%+#08x\", 5)"), b"+$000005");
        assert_eq!(b("db STRFMT(\"% -6d|\", 42)"), b" 42   |");

        // `#` marks the base rather than C's alternate form, and the spellings
        // are rgbasm's own.
        assert_eq!(b("db STRFMT(\"%#x|%#b|%#o\", 255, 5, 9)"), b"$ff|%101|&11");

        // A value is 32 bits wide, and every type but `%d` reads it unsigned.
        assert_eq!(b("db STRFMT(\"%x\", -1)"), b"ffffffff");
        assert_eq!(b("db STRFMT(\"%u\", -1)"), b"4294967295");
        assert_eq!(b("db STRFMT(\"%d\", $FFFFFFFF)"), b"-1");
        // The sign flag stands in only where the value supplies none.
        assert_eq!(b("db STRFMT(\"%+d|%+u\", -1, -1)"), b"-1|+4294967295");

        // `%f` reads its argument as Q16.16 — so a plain `1` is a very small
        // fraction — and `#` marks the precision after the digits.
        assert_eq!(b("db STRFMT(\"%f\", 1.5)"), b"1.50000");
        assert_eq!(b("db STRFMT(\"%f\", 1)"), b"0.00002");
        assert_eq!(b("db STRFMT(\"%.2f|%f\", 1.5, -1.5)"), b"1.50|-1.50000");
        assert_eq!(b("db STRFMT(\"%#f\", 1.5)"), b"1.50000q16");
        assert_eq!(b("db STRFMT(\"%#014f\", 1.5)"), b"00001.50000q16");
        // The cut rounds to nearest with ties to *even*, and the sign survives
        // a magnitude that rounds away.
        assert_eq!(b("db STRFMT(\"%.0f %.0f %.0f\", 0.5, 1.5, 2.5)"), b"0 2 2");
        assert_eq!(b("db STRFMT(\"%.0f|%.0f\", -1.5, -0.5)"), b"-2|-0");
        // The value is a dyadic rational, so the expansion is exact and
        // terminates rather than drifting.
        assert_eq!(b("db STRFMT(\"%.17f\", 0.1)"), b"0.10000610351562500");
    }

    /// What `STRFMT` refuses, and why each one is a rule rather than an
    /// oversight: rgbasm v1.0.3 refuses every line here.
    #[test]
    fn strfmt_refuses_what_rgbasm_refuses() {
        let refused = |src: &str| {
            asm(&format!("SECTION \"a\", ROM0[0]\n{src}\n"))
                .expect_err(&format!("rgbasm refuses `{src}`"));
        };
        // The flags come in a fixed order — sign, `#`, `-`, `0` — so every
        // other arrangement is a spec the reference will not read.
        refused("db STRFMT(\"%#+x\", 5)");
        refused("db STRFMT(\"%0-6d\", 5)");
        refused("db STRFMT(\"%+ d\", 5)");
        refused("db STRFMT(\"%6-d\", 5)");
        // A flag may appear once.
        refused("db STRFMT(\"%--6d\", 5)");
        refused("db STRFMT(\"%q\", 5)");
        refused("db STRFMT(\"abc%\")");

        // Only `%f` has a fraction to cut, and only a based type has a base to
        // mark.
        refused("db STRFMT(\"%.2d\", 5)");
        refused("db STRFMT(\"%.2x\", 5)");
        refused("db STRFMT(\"%#d\", 5)");
        // A string takes neither a sign nor zero padding.
        refused("db STRFMT(\"%+s\", \"ab\")");
        refused("db STRFMT(\"%06s\", \"ab\")");

        // The two halves cannot be swapped.
        refused("db STRFMT(\"%s\", 5)");
        refused("db STRFMT(\"%d\", \"hi\")");

        // The argument count is exact in both directions.
        refused("db STRFMT(\"%d %d\", 1)");
        refused("db STRFMT(\"%d\", 1, 2)");
    }

    /// The pass folds a constants environment as it walks, so a number
    /// argument may be a constant defined above the line or an expression over
    /// one — and a label's address, which the layout has not assigned yet, is
    /// refused by name.
    #[test]
    fn a_number_argument_reads_the_constants_above_it() {
        let b = |src: &str| {
            asm(&format!("SECTION \"a\", ROM0[0]\n{src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .bytes
        };
        assert_eq!(b("DEF N EQU 7\ndb STRFMT(\"n=%d\", N)"), b"n=7");
        assert_eq!(b("DEF N EQU 7\ndb STRFMT(\"%d\", N*2+1)"), b"15");
        // The same environment answers an index, which was a literal before.
        assert_eq!(b("DEF N EQU 2\ndb STRSUB(\"abcd\", N, 2)"), b"bc");
        // A constant is in scope from the line below its definition, exactly as
        // the pass walks.
        assert_eq!(b("DEF N EQU 3\nDEF M EQU N+1\ndb STRFMT(\"%d\", M)"), b"4");

        let refused = asm("SECTION \"a\", ROM0[0]\nlbl:\ndb STRFMT(\"%d\", lbl)\n")
            .expect_err("a label's address is not reachable from the text pass");
        assert!(
            refused.to_string().contains("label's address"),
            "the refusal should name the case: {refused}"
        );
    }

    /// `EQUS` is a *text* symbol: what it holds is spliced into the source and
    /// then read as source, which is why it can hold a number, a quoted string,
    /// or a call.
    #[test]
    fn equs_substitutes_text_and_braces_reach_inside_a_token() {
        let b = |src: &str| {
            asm(&format!("SECTION \"a\", ROM0[0]\n{src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .bytes
        };
        assert_eq!(b("DEF s EQUS \"$41\"\ndb s"), vec![0x41]);
        // `{name}` splices into the middle of a token, which a bare name
        // cannot: `$1{n}` with `n` as `4` is `$14`.
        assert_eq!(b("DEF n EQUS \"4\"\ndb $1{n}"), vec![0x14]);
        // A bare name inside a string literal is left alone.
        assert_eq!(b("DEF s EQUS \"$41\"\ndb \"s\""), b"s");
        // What it holds may be a quoted string, or a call that then folds.
        assert_eq!(b("DEF q EQUS \"\\\"ab\\\"\"\ndb STRLEN(q)"), vec![2]);
        assert_eq!(
            b("DEF j EQUS \"STRCAT(\\\"xy\\\",\\\"z\\\")\"\ndb j"),
            b"xyz"
        );
    }

    /// Fixed-point literals, and the arithmetic that is exact enough to
    /// reproduce. Every value here was read off rgbasm v1.0.3 first.
    #[test]
    fn fixed_point_reads_as_rgbasm_reads_it() {
        let longs = |src: &str| {
            let out = crate::assemble_rgbasm(&format!("SECTION \"a\", ROM0[0]\n{src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"));
            out.bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| i64::from(i32::from_le_bytes(*c)))
                .collect::<Vec<_>>()
        };
        // `1.0` is `$10000`, and the fraction rounds to nearest: `3.7` is
        // `$3B333` because `242483.2` rounds down, and `0.1` is `$199A`
        // because `6553.6` rounds up. Truncation agrees with the first and
        // not the second, which is why both are here.
        assert_eq!(longs("dl 1.0"), vec![0x1_0000]);
        assert_eq!(longs("dl 3.7"), vec![0x3_B333]);
        assert_eq!(longs("dl 0.1, 0.3"), vec![0x199A, 0x4CCD]);
        // A half goes away from zero, which `q1` and `q2` can show and 16 bits
        // of precision cannot reach with a short literal.
        assert_eq!(longs("dl 0.25q1, 1.25q1, 0.125q2"), vec![1, 3, 1]);
        assert_eq!(longs("dl -1.5"), vec![-0x1_8000]);
        // A `q` suffix names another precision.
        assert_eq!(longs("dl 3.7q8"), vec![0x3B3]);

        assert_eq!(longs("dl DIV(1.0, 3.0)"), vec![0x5555]);
        assert_eq!(longs("dl MUL(1.5, 1.5)"), vec![0x2_4000]);
        assert_eq!(longs("dl FMOD(7.5, 2.0)"), vec![0x1_8000]);

        // FLOOR goes toward minus infinity and CEIL away from it, so both move
        // a negative the opposite way from a positive.
        assert_eq!(
            longs("dl FLOOR(3.7)\ndl FLOOR(-3.2)"),
            vec![0x3_0000, -0x4_0000]
        );
        assert_eq!(
            longs("dl CEIL(3.2)\ndl CEIL(-3.2)"),
            vec![0x4_0000, -0x3_0000]
        );
        // A half goes *away from zero*, which is why ROUND is its own node
        // rather than an add-and-mask.
        assert_eq!(
            longs("dl ROUND(3.5)\ndl ROUND(-3.5)"),
            vec![0x4_0000, -0x4_0000]
        );

        // `TZCOUNT` answers a plain integer, not a fixed-point value.
        assert_eq!(longs("dl TZCOUNT(8)\ndl TZCOUNT(1)"), vec![3, 0]);

        let out = crate::assemble_rgbasm("SECTION \"a\", ROM0[0]\ndb HIGH($1234), LOW($1234)\n")
            .expect("byte extraction");
        assert_eq!(out.bytes, vec![0x12, 0x34]);
    }

    /// rgbasm truncates an oversized value and warns; it does not refuse.
    /// This is what replaced the `contract.rs` span case that asserted an
    /// error here — that case was pinning our divergence rather than
    /// rgbasm's behaviour (asm198x#290).
    #[test]
    fn an_oversized_byte_is_a_warning_not_an_error() {
        let out = crate::assemble_rgbasm("SECTION \"a\", ROM0\n        ld a, 300\n")
            .expect("rgbasm truncates rather than refusing");
        assert_eq!(out.bytes, vec![0x3E, 0x2C]);
        let out = crate::assemble_rgbasm("SECTION \"a\", ROM0\n        db 256\n")
            .expect("data truncates too");
        assert_eq!(out.bytes, vec![0x00]);
    }

    /// A section is placed at its own base, whatever order the source wrote
    /// them in — the thing an `org` could never express, since it can only
    /// move forward.
    #[test]
    fn sections_are_placed_by_address_not_by_source_order() {
        let out = crate::assemble_rgbasm(
            "SECTION \"c\",ROM0[$0]\n db $cc\nSECTION \"b\",ROM0[$20]\n db $bb\n\
             SECTION \"a\",ROM0[$10]\n db $aa\n",
        )
        .expect("assembles");
        assert_eq!(out.bytes.len(), 0x21);
        assert_eq!(out.bytes[0x00], 0xCC);
        assert_eq!(out.bytes[0x10], 0xAA);
        assert_eq!(out.bytes[0x20], 0xBB);
    }

    /// A label belongs to its own section, so it resolves against that
    /// section's base rather than a running image offset.
    #[test]
    fn a_label_takes_its_own_sections_base() {
        let out = crate::assemble_rgbasm(
            "SECTION \"c\",ROM0[$0]\n dw far\nSECTION \"f\",ROM0[$30]\nfar: db $99\n",
        )
        .expect("assembles");
        assert_eq!(&out.bytes[..2], &[0x30, 0x00]);
    }

    /// A banked section is addressed at $4000 whichever bank holds it, and
    /// lands at `bank * $4000` in the ROM — the two are different numbers, and
    /// an image position equal to an address cannot hold both.
    #[test]
    fn a_banked_section_is_addressed_and_placed_differently() {
        let out = crate::assemble_rgbasm(
            "SECTION \"f\",ROM0[$0]\n db $00\nSECTION \"p\",ROMX,BANK[2]\nhere:\n db $22\n dw here\n",
        )
        .expect("assembles");
        // Three banks, because bank 2 was used.
        assert_eq!(out.bytes.len(), 3 * 0x4000);
        assert_eq!(out.bytes[0x8000], 0x22);
        // `here` is $4000, not its offset in the ROM.
        assert_eq!(&out.bytes[0x8001..0x8003], &[0x00, 0x40]);
    }

    /// `BANK[n]` is bracketed like an origin and is not one. Reading the whole
    /// line for brackets put the bank number in the origin, and every label in
    /// a banked section came out low.
    #[test]
    fn a_bank_is_not_an_origin() {
        let out = crate::assemble_rgbasm("SECTION \"p\",ROMX,BANK[2]\nhere:\n dw here\n")
            .expect("assembles");
        assert_eq!(&out.bytes[0x8000..0x8002], &[0x00, 0x40]);
    }

    /// `BANK("name")` is answered after the whole program is known, because it
    /// reaches forward: the section it names is often below the reference.
    #[test]
    fn bank_reaches_forward_to_its_section() {
        let out = crate::assemble_rgbasm(
            "SECTION \"f\",ROM0[$0]\n db BANK(\"paged\")\n db BANK(\"f\")\n\
             SECTION \"paged\",ROMX,BANK[2]\n db $22\n",
        )
        .expect("assembles");
        // The forward section's bank, then this one's — ROM0 is bank 0.
        assert_eq!(&out.bytes[..2], &[0x02, 0x00]);
    }

    /// Naming a section that does not exist is an error, not bank 0.
    #[test]
    fn bank_of_an_unknown_section_is_refused() {
        let err = crate::assemble_rgbasm("SECTION \"f\",ROM0[$0]\n db BANK(\"nope\")\n")
            .expect_err("no such section");
        assert!(err.to_string().contains("no section named"), "got `{err}`");
    }

    /// An assertion fires only when false, and sees labels defined below it —
    /// which is why it is evaluated with the finished symbol table rather than
    /// folded while parsing.
    #[test]
    fn an_assertion_fires_only_when_false_and_reaches_forward() {
        assert!(
            crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\n ASSERT fin-beg\nbeg: db 1,2\nfin:\n")
                .is_ok()
        );
        let err =
            crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\n STATIC_ASSERT 0, \"boom\"\n db 1\n")
                .expect_err("false");
        assert!(err.to_string().contains("boom"), "got `{err}`");
        // No message given: the reference's own wording, without one.
        let bare = crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\n ASSERT 0\n").expect_err("false");
        assert!(
            bare.to_string().contains("Assertion failed"),
            "got `{bare}`"
        );
    }

    /// Two sections claiming the same bytes is refused, not silently merged.
    #[test]
    fn overlapping_sections_are_refused() {
        let err = crate::assemble_rgbasm(
            "SECTION \"a\",ROM0[$0]\n db 1,2,3,4\nSECTION \"b\",ROM0[$2]\n db 9\n",
        )
        .expect_err("overlap");
        assert!(err.to_string().contains("overlaps"), "got `{err}`");
    }

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

    /// rgbasm's `EXPORT` asks nothing: a name defined nowhere and referenced
    /// nowhere links to a ROM. The only failure is a *reference* to an
    /// undefined name, which is the ordinary refusal and needs no help from
    /// `EXPORT`. It is the one visibility word in any of the six references
    /// that is honestly accept-and-discard.
    #[test]
    fn export_asks_nothing_of_its_name() {
        assert_eq!(
            bytes("SECTION \"s\",ROM0[0]\nEXPORT foo\nfoo: db 1,2\nEXPORT nope\n"),
            vec![1, 2]
        );
        asm("SECTION \"s\",ROM0[0]\nEXPORT nope\n dw nope\n")
            .expect_err("reading an undefined name still fails");
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
    /// The text layer's lines are the formatter's to hand back, not to lay
    /// out: an `EQUS` holds text spliced into the source, and a `{name}`
    /// splices into the middle of a token. Neither is an expression yet, and
    /// re-rendering either would have to reproduce its quotes and escapes byte
    /// for byte.
    #[test]
    fn the_text_layer_formats_back_as_written() {
        let src = "SECTION \"s\",ROM0[0]\n\
                   DEF q EQUS \"\\\"ab\\\"\"\n\
                   DEF n EQUS \"4\"\n\
                   db $1{n}\n\
                   db STRLEN(q)\n\
                   db STRCAT(\"ab\",\"cd\")\n";
        let formatted = crate::format_rgbasm(src).expect("formats");
        for kept in [
            "DEF q EQUS \"\\\"ab\\\"\"",
            "$1{n}",
            "STRCAT(\"ab\",\"cd\")",
        ] {
            assert!(formatted.contains(kept), "lost `{kept}`:\n{formatted}");
        }
        assert_eq!(
            out(src),
            crate::assemble_rgbasm(&formatted)
                .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
                .bytes,
            "formatting changed the program:\n{formatted}"
        );
    }

    /// rgbasm answers an interpolation of a name it does not hold with
    /// "Interpolated symbol `n` does not exist", so the pass refuses it too
    /// rather than leaving the braces for the expression parser to trip over.
    #[test]
    fn an_unresolved_interpolation_is_refused() {
        let e = crate::assemble_rgbasm("SECTION \"s\",ROM0[0]\n db $1{n}\n")
            .expect_err("rgbasm refuses an interpolation of an undefined symbol");
        assert!(e.to_string().contains("{n}"), "{e}");
    }

    /// An `ELIF` leg is stored as a conditional nested in the else branch, and
    /// it has to come back out flat: `ELSE` before `ELIF` is source rgbasm will
    /// not read, and the one closer the author wrote belongs to the whole
    /// chain — rgbasm takes `ENDC` and refuses `ENDIF`, so a derived closer is
    /// a broken file rather than a cosmetic difference (#346).
    #[test]
    fn an_elif_chain_formats_back_flat() {
        let src =
            "SECTION \"s\",ROM0[0]\n if 0\n db $99\n elif 1\n db $AA\n else\n db $BB\n endc\n";
        let formatted = crate::format_rgbasm(src).expect("formats");
        assert!(formatted.contains("elif 1"), "{formatted}");
        assert!(!formatted.contains("endif"), "derived closer:\n{formatted}");
        assert_eq!(formatted.matches("endc").count(), 1, "{formatted}");
        assert_eq!(
            out(src),
            crate::assemble_rgbasm(&formatted)
                .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
                .bytes,
            "formatting changed the program:\n{formatted}"
        );
    }

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
        // `ASSERT` and `PRINT` used to be here; both are implemented now.
        for d in ["UNION", "OPT b.X", "PUSHS"] {
            let e = err(d);
            assert!(
                e.contains("is a real directive here"),
                "`{d}` should name itself a real rgbasm directive, got: {e}"
            );
        }
        assert!(err("ZZQQ").contains("unknown instruction"));
    }

    /// Only *statement* vocabulary is declared as an operation. rgbasm's
    /// registers, predefined symbols and section attributes are words it knows
    /// in positions that are not this one, and naming them here would describe
    /// them where they never appear.
    ///
    /// A built-in **function** is the one exception, and it is not really one:
    /// `Category::ExpressionWord` names the position rather than claiming the
    /// word begins a line, so a function we implement is declared as what it
    /// is and one we do not stays undeclared. `strlen` is implemented (the
    /// text layer); `cos` is not.
    #[test]
    fn only_statement_vocabulary_is_declared() {
        let declared: Vec<String> = crate::directives::surfaces()
            .into_iter()
            .filter(|s| s.dialect == "rgbasm")
            .flat_map(|s| s.directives)
            .flat_map(|d| d.spellings())
            .map(|s| s.to_ascii_lowercase())
            .collect();
        for absent in ["af", "hli", "sizeof", "cos", "__date__", "_narg", "rom0"] {
            assert!(
                !declared.iter().any(|s| s == absent),
                "`{absent}` is not statement vocabulary"
            );
        }
        // An implemented function is declared as an expression word, never as
        // an operation.
        let kind = |word: &str| {
            crate::directives::surfaces()
                .into_iter()
                .filter(|s| s.dialect == "rgbasm")
                .flat_map(|s| s.directives)
                .find(|d| d.spellings().iter().any(|s| s.eq_ignore_ascii_case(word)))
                .map(|d| d.category)
        };
        for function in ["strlen", "strcat", "high", "mul"] {
            assert_eq!(
                kind(function),
                Some(crate::directives::Category::ExpressionWord),
                "`{function}` is implemented, so it is declared as what it is"
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
