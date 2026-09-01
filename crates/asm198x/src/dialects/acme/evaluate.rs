//! The ACME evaluator (#486's phase split for acme): the walk that assembles
//! by evaluating the source-preserving conditional AST the parent module
//! parses — folding each conditional and loop once in source order, threading
//! the environment (`=` constants, `!set` variables, zones, the conversion
//! table), resolving `!src`/`!bin` at the moment they are reached live, and
//! lowering every taken line through the parent's `parse_statement`. Moved
//! verbatim from the parent; the seam is the boundary, not a rewrite.

use std::collections::{BTreeMap, BTreeSet};

use super::super::macros;
use super::super::mos6502;
use super::{
    AcmeMacros, Anons, AsmError, Closer, Conditional, Expr, FileId, FmtCx, MAX_INCLUDE_DEPTH,
    Operation, SourceLoader, SourceMap, Statement, Warning, anon_marker, bake_set_vars,
    classify_conditional, close_brace, cpu_selector, eval_condition, fold_const, is_ident,
    is_macro_head, parse_program_in, parse_set, parse_statement, parse_value, petscii, screen_code,
    split_first_word, substitute_anon_refs,
};

// ---------------------------------------------------------------------------
// Assembly by evaluation of the conditional AST (idea 4) — the ACME evaluator
// ---------------------------------------------------------------------------

/// The multi-file context of an include-capable walk (language-surface U4,
/// KTD8): the source map that owns `FileId` allocation and the include graph,
/// the loader seam, and the active include stack for cycle detection.
pub(super) struct MultiCx<'a> {
    pub(super) map: &'a mut SourceMap,
    pub(super) loader: &'a dyn SourceLoader,
    /// The files currently open, root first. Cycle detection is membership —
    /// a file may be included twice *sequentially* (acme re-reads it) but
    /// never while it is still open.
    pub(super) stack: Vec<FileId>,
}

/// An instruction that sized absolute for want of a value, and the value it
/// was waiting on.
struct Oversize {
    expr: Expr,
    line: usize,
    file: FileId,
}

/// ACME's [`CondEval`](crate::ast::CondEval): it owns the environment (`=`/`equ`
/// constants and `!set` variables) and lowers each live line through
/// [`parse_statement`], re-parsing from the node's (label, source) with the
/// current `env` — so a direct/extended choice or an opcode-embedded operand
/// folds against exactly the bindings live at that point. The shared
/// [`evaluate`](crate::ast::evaluate) walk prunes untaken branches; this supplies
/// the ACME-specific condition test and per-line lowering.
///
/// With a [`MultiCx`] wired in, `!src`/`!bin` resolve *inside* this walk
/// (U4, KTD1): the target loads only when its directive is reached live, the
/// included tree evaluates through `self` (so the environment threads through
/// and back out), and anonymous labels register in spliced evaluation order.
/// Without one (the single-source entry points), those directives are an
/// error pointing at the multi-file entry points.
/// The character conversion `!text` applies. `!pet` and `!scr` name their own
/// table and ignore this one; `!raw` bypasses it. The default is `Raw`, which
/// is why `!text` and `!raw` agreed byte-for-byte before `!ct` existed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConvTable {
    Raw,
    Pet,
    Scr,
}

impl ConvTable {
    pub(super) fn convert(self, c: u8) -> u8 {
        match self {
            ConvTable::Raw => c,
            ConvTable::Pet => petscii(c),
            ConvTable::Scr => screen_code(c),
        }
    }

    /// ACME names them `raw`, `pet` and `scr`; anything else is "Unknown
    /// encoding". A quoted operand is a 256-byte table read from a file,
    /// which this does not implement — refused by name rather than silently
    /// treated as one of the three.
    fn named(text: &str, line: usize) -> Result<Self, AsmError> {
        if text.starts_with('"') {
            return Err(AsmError::new(
                line,
                "`!ct` with a table file is not implemented here; the named \
                 encodings `raw`, `pet` and `scr` are",
            ));
        }
        match text.to_ascii_lowercase().as_str() {
            "raw" => Ok(ConvTable::Raw),
            "pet" => Ok(ConvTable::Pet),
            "scr" => Ok(ConvTable::Scr),
            "" => Err(AsmError::new(line, "no string given")),
            other => Err(AsmError::new(line, format!("unknown encoding `{other}`"))),
        }
    }
}

/// What an open `{` block owes its `}`.
enum OpenBlock {
    /// The zone name to go back to.
    Zone(String),
    /// The `!xor` mask to go back to.
    Xor(u8),
    /// The conversion table to go back to.
    Ct(ConvTable),
    /// A `!pseudopc` block, whose restore the engine performs.
    PseudoPc,
}

struct MacroCapture {
    source: String,
    depth: usize,
    file: FileId,
    line: usize,
}

/// Rebuild the code-bearing part of a formatter node. Multi-file macro
/// definitions arrive as source-preserving nodes rather than raw lines, so a
/// label must be put back before the definition is handed to the shared macro
/// collector. In particular, an anonymous `-`/`+` label may be the entire
/// line.
fn node_code(node: &crate::ast::Node) -> String {
    match &node.label {
        Some(sym) if node.source.is_empty() => sym.name.clone(),
        Some(sym) => format!("{} {}", sym.name, node.source),
        None => node.source.clone(),
    }
}

#[derive(Clone, Copy)]
pub(super) struct AcmeTarget {
    pub(super) set: &'static isa::InstructionSet,
    pub(super) ext: Option<&'static isa::InstructionSet>,
}

pub(super) struct AcmeEval<'a> {
    target: AcmeTarget,
    anons: Anons,
    env: BTreeMap<String, i64>,
    /// Names bound by `!set` (rebindable): each use is baked to its current value.
    set_names: BTreeSet<String>,
    /// Where the location counter stands, so a label can be bound to its
    /// address as it is defined — which is what lets a *backward* reference
    /// size to zero page (`decisions/acme-zero-page.md`).
    ///
    /// `None` means "not known here", and the walk falls back to what it did
    /// before: no label address enters `env`, so every label reference sizes
    /// absolute. That is the safe direction, and it is deliberate — a counter
    /// that is merely *probably* right would pick zero page on a bad guess and
    /// emit the wrong bytes, which is worse than the gap being fixed.
    pc: Option<i64>,
    /// Instructions that took an absolute form only because their operand was
    /// not yet resolvable. If the value turns out to fit a byte, ACME says so
    /// — see [`AcmeEval::oversized_warnings`].
    oversize: Vec<Oversize>,
    multi: Option<MultiCx<'a>>,
    /// ACME's macro namespace follows textual `!source` order in both
    /// directions. It therefore lives beside the include stack, rather than
    /// being recreated for each parsed file.
    macro_state: macros::MacroState,
    /// A definition is copied as verbatim AST nodes. The live multi-file walk
    /// gathers those nodes here and registers the definition only when it is
    /// reached, which gives ACME its textual ordering across `!source`.
    macro_capture: Option<MacroCapture>,
    /// The file the walk is currently inside — stamps condition-evaluation
    /// errors, which the shared walk raises without node context.
    current_file: FileId,
    /// The current `!zone` scope prefix (U7): empty in the initial zone (so
    /// zone-free programs keep today's bare `.name` keys), then
    /// `{title}@{ordinal}` after each `!zone` — the ordinal keeps same-title
    /// zones distinct (probe z12b: re-entering a title is a *fresh* zone).
    /// Evaluation state, not parse state: it threads through `!src` like the
    /// rest of the environment (probes za/zb) and an untaken branch's `!zone`
    /// never runs (probe zd).
    zone: String,
    /// How many `!zone` directives the walk has taken — the ordinal source.
    zone_ord: usize,
    /// Enclosing-zone saves for the `!zone { … }` block form: `}` restores
    /// (probe z6b); the line form pushes nothing, so it switches for good
    /// even inside a taken conditional (probe ze).
    /// What each open marker block has to restore. `!zone` and `!xor` both
    /// leave their head and `}` in the tree, so one `}` arm serves both and
    /// the stack is what says which it is closing.
    block_stack: Vec<OpenBlock>,
    /// The conversion table `!text` uses from here on.
    conv: ConvTable,
    /// The `!xor` mask in force, which every statement produced from here on
    /// carries. Masks **combine**: `!xor $f0` then `!xor $0f` gives `$ff`,
    /// and `!xor $ff` twice cancels back to `$00` (probed 2026-08-25).
    xor_mask: u8,
}

impl<'a> AcmeEval<'a> {
    pub(super) fn new(set: &'static isa::InstructionSet, multi: Option<MultiCx<'a>>) -> Self {
        Self {
            target: AcmeTarget { set, ext: None },
            anons: Anons::default(),
            env: BTreeMap::new(),
            set_names: BTreeSet::new(),
            // No origin yet. ACME requires `*=` before code, so the first
            // origin sets this before anything can be sized.
            pc: None,
            oversize: Vec::new(),
            multi,
            macro_state: macros::MacroState::default(),
            macro_capture: None,
            current_file: FileId(0),
            zone: String::new(),
            zone_ord: 0,
            block_stack: Vec::new(),
            conv: ConvTable::Raw,
            xor_mask: 0,
        }
    }

    /// Qualify a definition name into the current zone: a leading-`.` local
    /// becomes `{zone}{name}`; anything else (globals, the `\u{1}` anonymous
    /// definitions) passes through. The initial zone's empty prefix keeps
    /// zone-free programs' keys unchanged.
    fn qualify_name(&self, name: String) -> String {
        if name.starts_with('.') && !self.zone.is_empty() {
            format!("{}{}", self.zone, name)
        } else {
            name
        }
    }

    /// Switch zones for a `!zone`/`!zn` directive (U7). A label on the line
    /// binds first — in the *old* zone (probe zf2). The block form (`args`
    /// ends with the head's `{`) saves the enclosing zone for its `}` marker
    /// to restore; the line form switches for good.
    fn lower_zone(
        &mut self,
        node: &crate::ast::Node,
        args: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        if let Some(label) = self.statement_label(node)? {
            out.push(Statement {
                line,
                file,
                label: Some(label),
                op: None,
                operand_span: None,
                xor_mask: 0,
                instruction_set: Some(self.target.set),
                extension_set: self.target.ext,
            });
        }
        let t = args.trim();
        let (title, block) = match t.strip_suffix('{') {
            Some(rest) => (rest.trim(), true),
            None => (t, false),
        };
        if !title.is_empty() && !is_ident(title) {
            // acme: "Garbage data at end of statement" (probe zh4) — a title
            // is one identifier, or none.
            return Err(stamp_file(
                AsmError::new(line, format!("bad `!zone` title `{title}`")),
                file,
            ));
        }
        if block {
            self.block_stack.push(OpenBlock::Zone(self.zone.clone()));
        }
        self.zone_ord += 1;
        self.zone = format!("{title}@{}", self.zone_ord);
        Ok(())
    }

    /// Resolve every anonymous-label *reference* placeholder left in the
    /// statement stream against the definitions collected during the walk —
    /// the deferred half of the spliced-order model (see [`Anons`]). Call
    /// after the evaluation walk completes.
    pub(super) fn resolve_anon_refs(&self, out: &mut [Statement]) -> Result<(), AsmError> {
        for s in out.iter_mut() {
            if let Some(op) = s.op.take() {
                s.op = Some(substitute_anon_refs(op, &self.anons, s.file, s.line)?);
            }
        }
        Ok(())
    }

    /// The label a directive line binds, as a statement-ready name: an
    /// anonymous `-`/`+` marker resolves to the definition registered for the
    /// current evaluation position; a `.local` qualifies into the current
    /// zone (U7); a plain name passes through.
    fn statement_label(&self, node: &crate::ast::Node) -> Result<Option<String>, AsmError> {
        let Some(sym) = &node.label else {
            return Ok(None);
        };
        if anon_marker(&sym.name).is_some() {
            let def = self.anons.def_here().ok_or_else(|| {
                AsmError::new(
                    node.span.line as usize,
                    "internal: anonymous label not registered",
                )
            })?;
            return Ok(Some(def.name.clone()));
        }
        Ok(Some(self.qualify_name(sym.name.clone())))
    }

    /// Resolve a `!src`/`!source` directive live (U4, KTD1): load the target
    /// through the loader, parse it in its own `FileId`, and evaluate its tree
    /// through `self` — the environment and anonymous-label order thread
    /// straight through. A label on the directive line binds at the include
    /// point (probe-pinned).
    fn lower_include(
        &mut self,
        node: &crate::ast::Node,
        args: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        let at = node
            .operand_span
            .clone()
            .unwrap_or_else(|| node.span.clone());
        // The arg parser knows its line but not its file: stamp here so a
        // malformed `!src` inside an included file names that file.
        let (request, rest) = file_request(args, line, "!src").map_err(|e| stamp_file(e, file))?;
        if !rest.trim().is_empty() {
            return Err(AsmError::at(
                at,
                format!("`!src` takes one file name (unexpected `{}`)", rest.trim()),
            ));
        }
        if let Some(label) = self.statement_label(node)? {
            out.push(Statement {
                line,
                file,
                label: Some(label),
                op: None,
                operand_span: None,
                xor_mask: 0,
                instruction_set: Some(self.target.set),
                extension_set: self.target.ext,
            });
        }
        let Some(mcx) = self.multi.as_mut() else {
            return Err(AsmError::at(
                at,
                format!(
                    "cannot resolve `!src \"{request}\"` here — the single-source \
                     API assembles one file; use the multi-file entry point \
                     (the CLI resolves includes automatically)"
                ),
            ));
        };
        if mcx.stack.len() >= MAX_INCLUDE_DEPTH {
            return Err(AsmError::at(
                at,
                format!("includes nested more than {MAX_INCLUDE_DEPTH} levels deep"),
            ));
        }
        let id = mcx
            .map
            .load(mcx.loader, &request, file, line as u32)
            .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
        if mcx.stack.contains(&id) {
            let chain = mcx
                .stack
                .iter()
                .chain(std::iter::once(&id))
                .map(|f| mcx.map.path(*f).unwrap_or("?"))
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(AsmError::at(at, format!("include cycle: {chain}")));
        }
        let contents = mcx.map.contents(id).unwrap_or_default().to_owned();
        mcx.stack.push(id);
        let program =
            parse_program_in(id, &contents, macros::Expand::No).map_err(|e| stamp_file(e, id))?;
        let saved = self.current_file;
        self.current_file = id;
        let walked = crate::ast::evaluate(self, &program.nodes, true, out);
        self.current_file = saved;
        if let Some(mcx) = self.multi.as_mut() {
            mcx.stack.pop();
        }
        walked
    }

    /// Resolve a `!bin`/`!binary` directive live (U4, KTD8): load the asset
    /// through the loader's binary path (no `FileId` — spans only ever point
    /// into source files) and window it with acme's probe-pinned size/skip
    /// semantics ([`window_bin`]). The payload rides one statement at the
    /// directive's span; a label binds at the payload's start.
    fn lower_incbin(
        &mut self,
        node: &crate::ast::Node,
        args: &str,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;
        let at = node
            .operand_span
            .clone()
            .unwrap_or_else(|| node.span.clone());
        // The arg parser knows its line but not its file: stamp here so a
        // malformed `!bin` inside an included file names that file.
        let (request, size, skip) = bin_args(&self.anons, &self.zone, &self.env, args, line)
            .map_err(|e| stamp_file(e, file))?;
        let label = self.statement_label(node)?;
        let Some(mcx) = self.multi.as_mut() else {
            return Err(AsmError::at(
                at,
                format!(
                    "cannot resolve `!bin \"{request}\"` here — the single-source \
                     API assembles one file; use the multi-file entry point \
                     (the CLI resolves binary inclusions automatically)"
                ),
            ));
        };
        let from = mcx.map.path(file).map(str::to_owned);
        let data = mcx
            .loader
            .load_binary(&request, from.as_deref())
            .map_err(|e| AsmError::at(at.clone(), e.to_string()))?;
        let payload = window_bin(&data, size, skip)
            .map_err(|msg| AsmError::at(at, format!("`{request}`: {msg}")))?;
        out.push(Statement {
            line,
            file,
            label,
            op: Some(Operation::Binary(payload)),
            operand_span: node.operand_span.clone(),
            xor_mask: 0,
            instruction_set: Some(self.target.set),
            extension_set: self.target.ext,
        });
        Ok(())
    }
}

impl crate::ast::CondEval for AcmeEval<'_> {
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        // `!ifdef .name` tests the current zone's binding (probe zh7), so the
        // tested name qualifies exactly as a definition would; `!if`
        // expressions qualify through `parse_value` inside `eval_condition`.
        let defined = |s: String| self.env.contains_key(&self.qualify_name(s));
        let taken = match classify_conditional(head) {
            Some(Conditional::IfDef(s)) => Ok(defined(s)),
            Some(Conditional::IfNDef(s)) => Ok(!defined(s)),
            Some(Conditional::If(e)) => {
                eval_condition(&self.anons, &self.zone, &self.env, &e, line)
            }
            None => Err(AsmError::new(line, format!("bad conditional `{head}`"))),
        };
        // The shared walk raises condition errors without node context, so a
        // failure inside an included file is stamped here (U4).
        taken.map_err(|e| stamp_file(e, self.current_file))
    }

    /// acme's `!for` has **two** syntaxes and they do not agree about anything
    /// except the name coming first. Measured against acme 0.97:
    ///
    /// | form | values | notes |
    /// |---|---|---|
    /// | `!for i, n` | `1 ..= n` | the *old* syntax; acme warns on every use |
    /// | `!for i, a, b` | `a ..= b` | inclusive, and **counts down** when `b < a` |
    ///
    /// So `!for i, 3, 1` gives 3, 2, 1 — not an empty loop, and not 1, 2, 3.
    /// That is the case [`Iteration::Over`] exists to carry: it is a list of
    /// values, never a start plus an index, because no index rule recovers a
    /// descending range without already knowing this one.
    ///
    /// `!for i, 0` is the old form's empty loop, and a negative count there is
    /// an error in acme rather than an empty loop.
    fn iteration(&self, head: &str, line: u32) -> Result<crate::ast::Iteration, AsmError> {
        let line = line as usize;
        let (_, args) = split_first_word(head.trim());
        let mut parts = args.split(',').map(str::trim);
        let name = parts
            .next()
            .filter(|n| !n.is_empty())
            .ok_or_else(|| AsmError::new(line, "`!for` needs a variable name"))?;
        let bounds: Vec<&str> = parts.filter(|p| !p.is_empty()).collect();
        let fold = |text: &str| -> Result<i64, AsmError> {
            fold_const(
                &parse_value(&self.anons, &self.zone, text, line)?,
                &self.env,
                line,
            )
        };
        let values: Vec<i64> = match bounds.as_slice() {
            // Old syntax: 1 up to the count, and **empty** when the count is
            // below 1. Counting down is the three-argument form's rule alone —
            // sharing it here made `!for i, 0` run twice.
            [count] => {
                let n = fold(count)?;
                if n < 0 {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "`!for {name}, {n}`: acme rejects a negative count in the old \
                             two-argument form"
                        ),
                    ));
                }
                (1..=n).collect()
            }
            // New syntax: inclusive both ends, descending when the end is below
            // the start.
            [a, b] => {
                let (first, last) = (fold(a)?, fold(b)?);
                if last >= first {
                    (first..=last).collect()
                } else {
                    (last..=first).rev().collect()
                }
            }
            _ => {
                return Err(AsmError::new(
                    line,
                    "`!for` takes a name and either a count or a start and an end",
                ));
            }
        };
        Ok(crate::ast::Iteration::Over {
            name: self.qualify_name(name.to_string()),
            values,
        })
    }

    /// The loop variable is bound like a `!set` name: **baked into each use at
    /// lower time**, not left as a symbol for the engine to resolve.
    ///
    /// That is forced by when the value exists. A label reaches the engine as
    /// `Expr::Sym` and resolves in a later pass against one symbol table, but a
    /// loop variable holds a different value on every pass and there is no pass
    /// for the engine to resolve it in. `!set` already had this problem and
    /// `bake_set_vars` already solves it, so the loop variable joins that set
    /// rather than growing a second mechanism.
    fn bind_loop_var(&mut self, name: &str, value: i64, _line: u32) -> Result<(), AsmError> {
        self.env.insert(name.to_string(), value);
        self.set_names.insert(name.to_string());
        Ok(())
    }

    fn lower(&mut self, node: &crate::ast::Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        // Stamp the mask in force onto whatever this node produces, rather
        // than at each of the half-dozen places a statement is pushed. One
        // funnel means a new lowering path cannot forget it.
        let first = out.len();
        let result = self.lower_inner(node, out);
        if self.xor_mask != 0 {
            for s in &mut out[first..] {
                s.xor_mask = self.xor_mask;
            }
        }
        result
    }
}

impl AcmeEval<'_> {
    fn lower_inner(
        &mut self,
        node: &crate::ast::Node,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        let file = node.span.file;

        // Multi-file assembly leaves definitions verbatim so they enter the
        // namespace in evaluation order. Gather the formatter AST's copied
        // lines until the definition's matching brace, then let the ordinary
        // ACME collector register it with its real file and header line.
        if let Some(mut capture) = self.macro_capture.take() {
            capture.source.push_str(&node_code(node));
            capture.source.push('\n');
            if close_brace(&node.source, &mut capture.depth).is_some() {
                macros::expand_at(
                    &AcmeMacros,
                    &capture.source,
                    capture.file,
                    capture.line,
                    &mut self.macro_state,
                )?;
            } else {
                self.macro_capture = Some(capture);
            }
            return Ok(());
        }
        if is_macro_head(node.source.trim()) {
            let mut capture = MacroCapture {
                source: format!("{}\n", node.source),
                depth: 0,
                file,
                line,
            };
            if close_brace(&node.source, &mut capture.depth).is_some() {
                macros::expand_at(
                    &AcmeMacros,
                    &capture.source,
                    file,
                    line,
                    &mut self.macro_state,
                )?;
            } else {
                self.macro_capture = Some(capture);
            }
            return Ok(());
        }

        // Expand calls against the namespace at this exact point in the live
        // include walk. Unknown calls remain ordinary source and reach the
        // usual diagnostic path.
        if node.source.trim_start().starts_with('+') {
            let expanded = macros::expand_one_at(
                &AcmeMacros,
                &node.source,
                file,
                line,
                &mut self.macro_state,
            )?;
            if expanded.text.trim_end() != node.source.trim_end() {
                let expansion = Some((expanded.text, expanded.origins));
                let text = macros::expanded_text(&expansion, &node.source);
                let origins = macros::line_origins(&expansion);
                let mut cx = FmtCx {
                    set: &isa::mos6502::SET,
                    file,
                    lines: text.lines().collect(),
                    pos: 0,
                    pending: Vec::new(),
                };
                let (mut nodes, closer) = cx
                    .parse_block()
                    .map_err(|e| macros::remap_lines(e, origins))?;
                if !matches!(closer, Closer::Eof | Closer::EofDirective) {
                    return Err(AsmError::new(line, "unbalanced `}` in macro expansion"));
                }
                macros::place_nodes(&mut nodes, origins);
                // The shared pre-pass emits a label in front of an invocation
                // as its own line. A call learned only after `!source` reaches
                // this live path instead, so reproduce that same node before
                // evaluating the generated body. Otherwise `irq +handler`
                // emits the handler but silently loses `irq`, breaking later
                // pass-two references such as `#<irq` / `#>irq` (#457).
                if node.label.is_some() {
                    let label = crate::ast::Node {
                        operand_span: None,
                        label: node.label.clone(),
                        item: None,
                        source: String::new(),
                        span: node.span.clone(),
                        trivia: crate::ast::Trivia::default(),
                    };
                    crate::ast::evaluate(self, &[label], true, out)?;
                }
                return crate::ast::evaluate(self, &nodes, true, out);
            }
        }
        // Every live line takes the next evaluation-order position (the anon
        // "virtual line"): included files splice their lines here, so `-`/`+`
        // resolution follows the spliced order, never any single file's line
        // numbers — and a definition in an untaken branch never registers,
        // matching acme (probe-pinned, U4).
        self.anons.vline += 1;
        // ACME also accepts an indented anonymous label (the real charset
        // corpus uses a tab before `-`). FmtCx necessarily keeps that as
        // operation source because ordinary named labels are column-sensitive,
        // but parse_statement recognises the anonymous run after canonical
        // reconstruction. Register either AST shape before it does so.
        let anon = node
            .label
            .as_ref()
            .and_then(|sym| anon_marker(&sym.name))
            .or_else(|| anon_marker(split_first_word(node.source.trim()).0));
        if let Some((sign, level)) = anon {
            self.anons.define(sign, level);
        }

        // `!src`/`!bin`/`!zone` are walk-handled (case-insensitive, with
        // their aliases), never parsed as operations: include/incbin
        // resolution must happen inside the live walk (KTD1) or not at all
        // (the single-source pointer), and a zone switch is walk state (U7 —
        // an untaken branch's `!zone` never runs, probe zd). The bare `}`
        // marker closes a `!zone { … }` block, restoring the enclosing zone
        // (probe z6b).
        let (word, args) = split_first_word(node.source.trim());
        match word.to_ascii_lowercase().as_str() {
            "!src" | "!source" => return self.lower_include(node, args, out),
            "!bin" | "!binary" => return self.lower_incbin(node, args, out),
            "!zone" | "!zn" => return self.lower_zone(node, args, out),
            "!pseudopc" if args.trim().ends_with('{') => {
                let target = args.trim().trim_end_matches('{').trim();
                let e = parse_value(&self.anons, &self.zone, target, line)
                    .map_err(|err| stamp_file(err, file))?;
                self.block_stack.push(OpenBlock::PseudoPc);
                out.push(Statement {
                    line,
                    file,
                    label: None,
                    op: Some(Operation::PseudoPc(Some(e))),
                    operand_span: None,
                    xor_mask: 0,
                    instruction_set: Some(self.target.set),
                    extension_set: self.target.ext,
                });
                return Ok(());
            }
            "!ct" | "!convtab" if args.trim().ends_with('{') => {
                let name = args.trim().trim_end_matches('{').trim();
                let conv = ConvTable::named(name, line).map_err(|e| stamp_file(e, file))?;
                self.block_stack.push(OpenBlock::Ct(self.conv));
                self.conv = conv;
                return Ok(());
            }
            "!ct" | "!convtab" => {
                // Replaces rather than combines — unlike `!xor`, whose masks
                // compose. Two directives with a block form, two rules.
                self.conv = ConvTable::named(args.trim(), line).map_err(|e| stamp_file(e, file))?;
                return Ok(());
            }
            "!xor" if args.trim().ends_with('{') => {
                let value = args.trim().trim_end_matches('{').trim();
                let v = self.xor_value(value, line, file)?;
                self.block_stack.push(OpenBlock::Xor(self.xor_mask));
                // Combine rather than replace: the probed rule, and the one
                // that makes a nested `!xor` compose with its parent.
                self.xor_mask ^= v;
                return Ok(());
            }
            "!xor" => {
                let v = self.xor_value(args.trim(), line, file)?;
                // No block, so nothing to restore: it runs to the end of the
                // enclosing `!xor` block, or of the file. Notably *not* to the
                // end of an `!if` or `!zone` — those do not scope it.
                self.xor_mask ^= v;
                return Ok(());
            }
            "}" if args.trim().is_empty() => {
                match self.block_stack.pop().ok_or_else(|| {
                    stamp_file(
                        AsmError::new(line, "internal: `}` closed no marker block"),
                        file,
                    )
                })? {
                    OpenBlock::Zone(zone) => self.zone = zone,
                    OpenBlock::Xor(mask) => self.xor_mask = mask,
                    OpenBlock::Ct(conv) => self.conv = conv,
                    // Unlike the others there is nothing to restore here: the
                    // engine keeps the stack, because only it knows the real
                    // address the block opened at.
                    OpenBlock::PseudoPc => out.push(Statement {
                        line,
                        file,
                        label: None,
                        op: Some(Operation::PseudoPc(None)),
                        operand_span: None,
                        xor_mask: 0,
                        instruction_set: Some(self.target.set),
                        extension_set: self.target.ext,
                    }),
                }
                return Ok(());
            }
            _ => {}
        }

        // Reconstruct the source line from the node's (label, operation source) —
        // canonical whitespace, which the parser treats identically to the
        // original.
        let recon = node_code(node);

        // `!set name = expr` binds/rebinds a variable and emits nothing; later
        // uses are baked to this value. A `.name` is zone-scoped (probe zh6).
        if split_first_word(recon.trim()).0 == "!set" {
            let (name, value) = parse_set(&self.anons, &self.zone, &self.env, &recon, line)
                .map_err(|e| stamp_file(e, file))?;
            let name = self.qualify_name(name);
            self.env.insert(name.clone(), value);
            self.set_names.insert(name);
            return Ok(());
        }

        let (label, op) = parse_statement(
            self.target,
            &self.anons,
            &self.zone,
            &self.env,
            self.conv,
            &recon,
            line,
        )
        .map_err(|e| stamp_file(e, file))?;
        if let Some(cpu) = cpu_selector(&recon) {
            self.target.ext = match cpu.as_str() {
                "6502" => None,
                "6510" | "nmos6502" => Some(&isa::nmos6502_undocumented::SET),
                "65c02" => Some(&isa::mos65c02::SET),
                "r65c02" => Some(&isa::mos65c02::ROCKWELL_SET),
                "w65c02" => Some(&isa::mos65c02::WDC_SET),
                "c64dtv2" => Some(&isa::c64dtv2::SET),
                "65ce02" => Some(&isa::csg65ce02::SET),
                "4502" => Some(&isa::csg65ce02::CSG4502_SET),
                "65816" => Some(&isa::mos65816::SET),
                _ => self.target.ext,
            };
        }
        // A `.name` definition qualifies into the current zone (U7); its
        // references were qualified by `parse_value`.
        let label = label.map(|n| self.qualify_name(n));
        // Bake `!set` variables to their current value; real labels stay symbolic.
        let op = op.map(|o| bake_set_vars(o, &self.env, &self.set_names));
        if let (Some(name), Some(Operation::Equ(e))) = (&label, &op)
            && let Ok(v) = fold_const(e, &self.env, line)
        {
            self.env.insert(name.clone(), v);
        }
        // A plain label names the address the counter is standing on. Binding
        // it here — before the counter moves — is what makes a later reference
        // to it foldable, and so sizeable to zero page when it is low.
        if let (Some(name), Some(pc)) = (&label, self.pc)
            && !matches!(op, Some(Operation::Equ(_)))
        {
            self.env.insert(name.clone(), pc);
        }
        self.note_oversize(op.as_ref(), line, file);
        self.advance(op.as_ref(), line);
        if !(label.is_none() && op.is_none()) {
            out.push(Statement {
                line,
                file,
                label,
                op,
                operand_span: node.operand_span.clone(),
                xor_mask: 0,
                instruction_set: Some(self.target.set),
                extension_set: self.target.ext,
            });
        }
        Ok(())
    }
}

impl AcmeEval<'_> {
    /// The operand of an `!xor`, folded and range-checked.
    ///
    /// ACME **does** range-check this one, unlike `!scrxor`, which silently
    /// takes the low byte of whatever it is given. `!xor 256` is "Number out
    /// of range" while `!scrxor 256, "a"` masks to `$00`. Two directives in
    /// one family, two answers; both are reproduced rather than reconciled.
    fn xor_value(&self, text: &str, line: usize, file: FileId) -> Result<u8, AsmError> {
        let e = parse_value(&self.anons, &self.zone, text, line)
            .map_err(|err| stamp_file(err, file))?;
        let v = fold_const(&e, &self.env, line).map_err(|err| stamp_file(err, file))?;
        if !(-128..=0xFF).contains(&v) {
            return Err(stamp_file(
                AsmError::new(line, format!("number out of range: {v}")),
                file,
            ));
        }
        Ok((v & 0xFF) as u8)
    }

    /// Record an instruction that took an absolute form only because its
    /// operand could not be folded yet.
    ///
    /// **A forced-absolute literal is never one of these**, and that is what
    /// makes the test this cheap. ACME reads `$0000` as 16-bit whatever its
    /// value, but a literal always folds — so an operand that does *not* fold
    /// cannot be one. Unfoldable and forced-absolute are disjoint, and only
    /// the first can turn out to have fitted.
    fn note_oversize(&mut self, op: Option<&Operation>, line: usize, file: FileId) {
        let Some(Operation::Instruction {
            mnemonic,
            mode,
            operands,
        }) = op
        else {
            return;
        };
        let Some(index) = mode.strip_prefix("absolute") else {
            return;
        };
        let [expr] = operands.as_slice() else { return };
        if fold_const(expr, &self.env, line).is_ok() {
            return;
        }
        // Only where a zero-page form existed to be chosen: `lda abs,y` has
        // none, so its width was the CPU's decision and never ours.
        if isa::mos6502::SET
            .find_form(mnemonic, &format!("zeropage{index}"))
            .is_none()
        {
            return;
        }
        self.oversize.push(Oversize {
            expr: expr.clone(),
            line,
            file,
        });
    }

    /// The advisories, once the walk has bound every label: an operand that
    /// sized absolute and turned out to fit a byte.
    ///
    /// ACME's posture as well as its wording — it warns and assembles, so the
    /// bytes are unchanged and the reader is told the instruction came out
    /// wider than it needed to be.
    pub(super) fn oversized_warnings(&self) -> Vec<Warning> {
        self.oversize
            .iter()
            .filter(|o| {
                fold_const(&o.expr, &self.env, o.line).is_ok_and(|v| (0..=0xFF).contains(&v))
            })
            .map(|o| Warning {
                line: o.line,
                message: "using oversized addressing mode".to_string(),
                file: o.file,
                kind: crate::engine::WarningKind::Advisory,
            })
            .collect()
    }

    /// Move the location counter over `op`, or give up on knowing where it is.
    ///
    /// The width comes from [`crate::engine::next_pc`], the same rule the
    /// engine's own address pass uses — a second copy here is how the two
    /// would drift apart, and a drifted counter is wrong bytes rather than a
    /// missed optimisation.
    ///
    /// Giving up is a real outcome and not a failure: an `*=` whose expression
    /// does not fold yet, or an operation whose form the ISA cannot supply,
    /// leaves the counter unknown for the rest of the walk. Every label from
    /// that point on stays symbolic and sizes absolute, which is what this
    /// dialect did everywhere before.
    fn advance(&mut self, op: Option<&Operation>, line: usize) {
        let Some(op) = op else { return };
        if let Operation::Org(e) = op {
            self.pc = fold_const(e, &self.env, line).ok();
            return;
        }
        let Some(pc) = self.pc else { return };
        // ACME's 6502 is one byte per address unit, and it has no CPU where
        // that is not so.
        self.pc = crate::engine::next_pc(op, pc, self.target.set, self.target.ext, 1, line).ok();
    }
}

/// Stamp `file` onto a per-line parse error: the line-oriented helpers
/// (`parse_statement`, the expression parser) know their line but not their
/// file, so the walk supplies it at the per-line boundary (language-surface
/// U4, the z80 walk's convention).
fn stamp_file(mut e: AsmError, file: FileId) -> AsmError {
    match &mut e.span {
        Some(span) => span.file = file,
        None if e.line != 0 => {
            e.span = Some(crate::ast::Span::in_file(file, e.line as u32, 0));
        }
        None => {}
    }
    e
}

/// The file name of a `!src`/`!bin` directive: acme requires `"file"` quotes
/// or the `<file>` library form — a bare token is rejected (probe-pinned:
/// `File name quotes not found`). Returns the name and the remaining text
/// after the closing quote/bracket for the caller's argument handling.
fn file_request<'t>(
    args: &'t str,
    line: usize,
    directive: &str,
) -> Result<(String, &'t str), AsmError> {
    let t = args.trim();
    let (inner, rest) = if let Some(body) = t.strip_prefix('"') {
        let end = body
            .find('"')
            .ok_or_else(|| AsmError::new(line, format!("unterminated `{directive}` file name")))?;
        (&body[..end], &body[end + 1..])
    } else if let Some(body) = t.strip_prefix('<') {
        let end = body
            .find('>')
            .ok_or_else(|| AsmError::new(line, format!("unterminated `{directive}` file name")))?;
        (&body[..end], &body[end + 1..])
    } else {
        return Err(AsmError::new(
            line,
            format!("`{directive}` file name must be quoted (\"file\" or <file>)"),
        ));
    };
    if inner.is_empty() {
        return Err(AsmError::new(
            line,
            format!("`{directive}` needs a file name"),
        ));
    }
    Ok((inner.to_string(), rest))
}

/// Parse `!bin`'s arguments: the file name, then acme's optional
/// `, [size] [, [skip]]` tail — **size first, then skip**, either slot
/// omittable by leaving it empty (`!bin "f", , 2` skips two and reads the
/// rest; probe-pinned). Both fold against the parse-time environment (they
/// set the statement's size, like a `!fill` count).
fn bin_args(
    anons: &Anons,
    zone: &str,
    env: &BTreeMap<String, i64>,
    args: &str,
    line: usize,
) -> Result<(String, Option<i64>, Option<i64>), AsmError> {
    let (name, rest) = file_request(args, line, "!bin")?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((name, None, None));
    }
    let Some(tail) = rest.strip_prefix(',') else {
        return Err(AsmError::new(
            line,
            format!("expected `, size [, skip]` after the `!bin` file name, found `{rest}`"),
        ));
    };
    let pieces = mos6502::split_top_level(tail, ',');
    if pieces.len() > 2 {
        return Err(AsmError::new(
            line,
            "`!bin` takes at most a file name, a size, and a skip",
        ));
    }
    let fold = |what: &str, piece: &str| -> Result<Option<i64>, AsmError> {
        if piece.trim().is_empty() {
            return Ok(None); // an empty slot: acme reads it as "not given"
        }
        let expr = parse_value(anons, zone, piece, line)?;
        fold_const(&expr, env, line).map(Some).map_err(|_| {
            AsmError::new(
                line,
                format!(
                    "`!bin` {what} must be a constant here (a number, an expression \
                     of constants, or a symbol defined above)"
                ),
            )
        })
    };
    let size = fold("size", pieces[0])?;
    let skip = pieces
        .get(1)
        .map(|p| fold("skip", p))
        .transpose()?
        .flatten();
    Ok((name, size, skip))
}

/// Apply acme's `!bin` size/skip window to the loaded asset — probe-pinned
/// (acme 0.97): skip past EOF or a size beyond the available data **pads with
/// zeroes** rather than erroring; a negative skip reads from the start; a
/// negative size is an error; no size means "from skip to EOF" (empty when
/// skip is at or past EOF). `Err` carries the message body; the caller wraps
/// it with the request name and the directive's span.
fn window_bin(data: &[u8], size: Option<i64>, skip: Option<i64>) -> Result<Vec<u8>, String> {
    if let Some(s) = size
        && s < 0
    {
        return Err(format!("negative `!bin` size ({s})"));
    }
    // A negative skip reads from the start of the file (the reference's seek
    // fails and the read position stays at 0).
    let skip = usize::try_from(skip.unwrap_or(0).max(0)).map_err(|_| "skip overflows")?;
    let start = skip.min(data.len());
    Ok(match size {
        None => data[start..].to_vec(),
        Some(s) => {
            let s = usize::try_from(s).map_err(|_| "size overflows")?;
            let end = start.saturating_add(s).min(data.len());
            let mut v = data[start..end].to_vec();
            // acme pads a short read with zeroes to exactly `size` bytes.
            v.resize(s, 0);
            v
        }
    })
}

/// Strip a `;` line comment. A `;` inside a `'c'` char literal or `"..."` string
/// is left alone so it is not mistaken for a comment.
pub(super) fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_char = false;
    let mut in_str = false;
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
