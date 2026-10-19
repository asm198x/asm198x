//! The projection sweep (#486's first legibility split): the phase layer of
//! `decisions/parse-affecting-directives.md` and
//! `decisions/conditionals-in-multipass-dialects.md`. The structural parse in
//! the parent module read the tree eagerly; this sweep reads *meaning* —
//! folding each conditional and repetition once, in source order, and reading
//! every deferred line through the eager reader at the moment it is reached,
//! so the text environment, constants, scopes, and anonymous-label stream are
//! all positional in the order ca65's own single sequential reader would see
//! them. Moved verbatim from the parent; the seam is the boundary, not a
//! rewrite.

use std::collections::{BTreeMap, BTreeSet};

use super::super::ca65_flat::{self, FlatWalk};
use super::super::mos6502;
use super::macros::{self, MacroSyntax};
use super::{
    AsmError, Expr, FileId, Kind, Parsed, SegSwitch, SourceLoader, SourceMap, Stmt, Walker,
    fold_const, is_ident, resolve_defined, resolve_ref, segment_switch, split_first_word,
};
use super::{cheap_key, is_cheap_ident};

/// Project the semantic [`Program`](crate::ast::Program) into the assembler's
/// [`Parsed`] — the assemble+link driver runs on an owned `Vec<Stmt>` plus the
/// label→segment and constant maps. Everything is read straight back out of the
/// tree (nothing is re-parsed): a native [`Kind`] payload becomes a placed
/// statement in the segment tracked from the `.segment` nodes, a label-only node
/// becomes an empty placed statement, and an `Item::Equ` node folds into the
/// constant table in source order.
pub(super) fn parsed_from_program(
    program: &crate::ast::Program,
    mut sw: Sweep<'_>,
) -> Result<Parsed, AsmError> {
    let mut st = Projection {
        seg: "CODE".to_string(),
        seg_stack: Vec::new(),
        // ca65 selects its default CODE segment even for an empty source.
        // A configured start for it therefore reserves a file gap in ld65.
        declared_segments: BTreeSet::from(["CODE".to_string()]),
        stmts: Vec::new(),
        label_seg: BTreeMap::new(),
        consts: BTreeMap::new(),
        const_names: BTreeMap::new(),
        equ_definitions: BTreeSet::new(),
        deferred: super::deferred::Deferred::default(),
        constant_updates: Vec::new(),
        zero_page_names: BTreeSet::new(),
        referenced: BTreeSet::new(),
        loop_vars: Vec::new(),
        token_depth: 0,
        macro_state: macros::MacroState::default(),
        token_captures: Vec::new(),
    };
    project_nodes(&program.nodes, &mut st, &mut sw)?;
    // The reader's end-of-source checks — an unclosed scope or record, a
    // dangling forward anonymous reference — run where its state is final:
    // after the sweep, which is where the reading happened.
    let last = program.nodes.last().map_or(0, |n| n.span.line);
    sw.reader.finish(last)?;
    Ok(Parsed {
        declared_segments: st.declared_segments,
        stmts: st.stmts,
        label_seg: st.label_seg,
        consts: st.consts,
        const_names: st.const_names,
        deferred: st.deferred,
    })
}

/// The projection's reader (#481): structure was read eagerly at parse, and
/// this is the machinery that reads *meaning* lazily — an eager [`Walker`]
/// fed one unread line at a time, in sweep order, so its text environment,
/// constants, scopes, and anonymous-label stream are all positional in the
/// order ca65's own single sequential reader would see them. An untaken
/// branch's lines never pass through it at all.
/// `decisions/parse-affecting-directives.md`.
pub(super) struct Sweep<'a> {
    reader: Walker,
    /// The include context — the multi-file entry's map and loader, so an
    /// `.include` inside a selected branch resolves at the moment the sweep
    /// reads it. `None` on the single-source path, which keeps meaning "one
    /// file, no includes".
    includes: Option<IncludeCx<'a>>,
    root: Option<&'a str>,
    line_starts: BTreeMap<u32, Vec<usize>>,
}

struct IncludeCx<'a> {
    map: &'a mut SourceMap,
    loader: &'a dyn SourceLoader,
    /// The active include chain, for the cycle and depth checks — the same
    /// discipline the eager walk applies, at sweep time.
    stack: Vec<FileId>,
    /// Actual recursive callers, excluding suspended sibling character sources.
    ancestry: Vec<FileId>,
    pending: Vec<PendingInclude>,
}

struct PendingInclude {
    id: FileId,
    contents: String,
}

impl<'a> Sweep<'a> {
    pub(super) fn single(set: &'static isa::InstructionSet, source: &'a str) -> Self {
        Self {
            reader: Walker::new(set, true),
            includes: None,
            root: Some(source),
            line_starts: BTreeMap::new(),
        }
    }

    pub(super) fn multi(
        set: &'static isa::InstructionSet,
        map: &'a mut SourceMap,
        loader: &'a dyn SourceLoader,
    ) -> Self {
        Self {
            reader: Walker::new(set, true),
            root: None,
            line_starts: BTreeMap::new(),
            includes: Some(IncludeCx {
                map,
                loader,
                stack: vec![FileId(0)],
                ancestry: vec![FileId(0)],
                pending: Vec::new(),
            }),
        }
    }
}

// Structural ca65 parsing keeps every physical body line, even blank ones.
// Elseif is a nested conditional sharing its parent's closer (empty close).
fn end_line(node: &crate::ast::Node) -> u32 {
    use crate::ast::Item;
    match &node.item {
        Some(Item::Repeat { body, .. }) => body.last().map_or(node.span.line, end_line) + 1,
        Some(Item::Conditional {
            then_body,
            else_body,
            close,
            ..
        }) => {
            let then_end = then_body.last().map_or(node.span.line, end_line);
            match else_body {
                None => then_end + 1,
                Some(body) if close.is_empty() => body.last().map_or(then_end + 1, end_line),
                Some(body) => body.last().map_or(then_end + 1, end_line) + 1,
            }
        }
        _ => node.span.line,
    }
}

impl Sweep<'_> {
    /// Read original character lines, never generated token-stream coordinates.
    fn admit_range(
        &mut self,
        at: &crate::span::Span,
        first: u32,
        last: u32,
    ) -> Result<(), AsmError> {
        if !at.expansion_frames.is_empty() || first > last {
            return Ok(());
        }
        let source = self
            .includes
            .as_ref()
            .and_then(|cx| cx.map.contents(at.file))
            .or(self.root.filter(|_| at.file == FileId(0)))
            .ok_or_else(|| AsmError::at(at.clone(), "numeric source location is unavailable"))?;
        let starts = self.line_starts.entry(at.file.0).or_insert_with(|| {
            std::iter::once(0)
                .chain(source.match_indices('\n').map(|(i, _)| i + 1))
                .collect()
        });
        for line in first..=last {
            let index = line
                .checked_sub(1)
                .map(|n| n as usize)
                .ok_or_else(|| AsmError::at(at.clone(), "invalid numeric source line"))?;
            let start = *starts
                .get(index)
                .ok_or_else(|| AsmError::at(at.clone(), "numeric source line is unavailable"))?;
            let end = starts.get(index + 1).copied().unwrap_or(source.len());
            let mut span = at.clone();
            span.line = line;
            ca65_flat::validate_numeric_tokens(&source[start..end], &span)?;
        }
        Ok(())
    }
}

/// The projection's running state: everything a later line's fold can see.
struct Projection {
    seg: String,
    /// Segments saved by `.pushseg`, innermost last; `.popseg` restores one.
    seg_stack: Vec<String>,
    declared_segments: BTreeSet<String>,
    stmts: Vec<Stmt>,
    label_seg: BTreeMap<String, String>,
    consts: BTreeMap<String, i64>,
    const_names: BTreeMap<String, String>,
    /// Immutable declarations, even when their RHS cannot fold yet. Populated
    /// only by selected nodes, so alternate branches do not redefine a name.
    equ_definitions: BTreeSet<String>,
    deferred: super::deferred::Deferred,
    constant_updates: Vec<(String, i64)>,
    /// Source-time address-size declarations; exports alone do not hint.
    zero_page_names: BTreeSet<String>,
    /// Every name the statements *above this point* have mentioned, which is
    /// what `.ref`/`.referenced` and `.ifref`/`.ifnref` ask about. Dead
    /// branches never reach here, so a use inside one does not count — probed
    /// against V2.18, where `.if 0 / .word L / .endif` leaves `.ref(L)` at 0.
    referenced: BTreeSet<String>,
    /// Counter tokens belong to their repeated source stream. They are not
    /// numeric symbols: a macro body or a generated `.ident("I")` cannot see
    /// the caller's counter, while an invocation argument can.
    loop_vars: Vec<LoopToken>,
    /// Native macro/repeat token sources outrank opened file character sources.
    token_depth: usize,
    macro_state: macros::MacroState,
    token_captures: Vec<TokenCapture>,
}

struct TokenCapture {
    at: crate::span::Span,
    defines: BTreeMap<String, String>,
}

struct LoopToken {
    name: String,
    value: i64,
    at: crate::span::Span,
}

/// Project a run of nodes, folding any conditional or repetition **once, in
/// source order, before layout** — `decisions/conditionals-in-multipass-dialects.md`.
///
/// No layout state is consulted because a ca65 condition cannot reach any: the
/// reference refuses `*` and even a backward label in one, since a ca65 label is
/// relocatable until `ld65` links it and so is never a constant expression.
fn remember_text_names(nodes: &[crate::ast::Node], names: &mut BTreeSet<String>) {
    use crate::ast::Item;
    for node in nodes {
        names.extend(ca65_flat::defined_names(&node.source));
        match &node.item {
            Some(Item::Conditional {
                then_body,
                else_body,
                ..
            }) => {
                remember_text_names(then_body, names);
                if let Some(body) = else_body {
                    remember_text_names(body, names);
                }
            }
            Some(Item::Repeat { body, .. }) => remember_text_names(body, names),
            _ => {}
        }
    }
}

/// Token boundaries for counter substitution. Unlike scope separators, cheap
/// local markers and numeric prefixes are part of their tokens: @I and $B
/// must not become @0 and $0 when I/B is the active counter.
struct RepeatTokens;

impl super::macros::MacroSyntax for RepeatTokens {
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        super::macros::MacroSyntax::header(&ca65_flat::Ca65Macros, line)
    }

    fn locals(&self, body: &[String]) -> Vec<String> {
        super::macros::MacroSyntax::locals(&ca65_flat::Ca65Macros, body)
    }

    fn fit_arguments(
        &self,
        name: &str,
        params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        super::macros::MacroSyntax::fit_arguments(&ca65_flat::Ca65Macros, name, params, args)
    }

    fn is_symbol_char(&self, c: u8) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'@' | b'$' | b'%')
    }
}

fn repeat_text(source: &str, at: &crate::span::Span, st: &Projection) -> String {
    let (names, values): (Vec<_>, Vec<_>) = st
        .loop_vars
        .iter()
        .rev()
        .filter(|binding| {
            binding.at.file == at.file && binding.at.expansion_frames == at.expansion_frames
        })
        .map(|binding| (binding.name.clone(), binding.value.to_string()))
        .unzip();
    super::macros::substitute(&RepeatTokens, source, &names, &values)
}

fn live_text(
    source: &str,
    st: &Projection,
    reader: &Walker,
    line: usize,
) -> Result<String, AsmError> {
    let target = if reader.extension.is_some() {
        ca65_flat::Target::Csg4510
    } else {
        ca65_flat::Target::Mos6502
    };
    ca65_flat::expand_ca65_line(
        source,
        target,
        &st.consts,
        &reader.text_names,
        reader.repeat_registers.map(|selected| {
            if selected {
                ca65_flat::Target::Csg4510
            } else {
                ca65_flat::Target::Mos6502
            }
        }),
        reader
            .scopes
            .len()
            .saturating_sub(reader.records.iter().filter(|open| open.owns_scope).count()),
        line,
    )
}

fn project_nodes(
    nodes: &[crate::ast::Node],
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<(), AsmError> {
    use crate::ast::Item;
    remember_text_names(nodes, &mut sw.reader.text_names);
    let mut index = 0;
    while index < nodes.len() {
        // `.end` stopped the reader: nothing after it is read, so nothing
        // after it is projected — a block head included.
        if sw.reader.ended {
            break;
        }
        let node = &nodes[index];
        if super::macro_header(&node.source).is_some() {
            let end = (index + 1..nodes.len())
                .find(|&i| ca65_flat::Ca65Macros.is_end(&nodes[i].source))
                .ok_or_else(|| {
                    AsmError::at(node.span.clone(), "macro definition is never closed")
                })?;
            if st.token_depth == 0 {
                sw.admit_range(&node.span, node.span.line, nodes[end].span.line)?;
            }
            register_macro(node, &nodes[index + 1..end], st, sw)?;
            index = end + 1;
            continue;
        }
        // Frames run from the innermost expansion out to the caller. The
        // outermost invocation owns the active token source until it ends.
        if st.token_depth == 0
            && let Some(frame) = node.span.expansion_frames.last()
        {
            let mut end = index + 1;
            while end < nodes.len() && nodes[end].span.expansion_frames.last() == Some(frame) {
                end += 1;
            }
            let watermark = pending_len(sw);
            st.token_depth += 1;
            let out = project_nodes(&nodes[index..end], st, sw);
            st.token_depth -= 1;
            out?;
            drain_includes(watermark, st, sw)?;
            index = end;
            continue;
        }
        index += 1;
        match &node.item {
            Some(Item::Native(payload))
                if matches!(payload.as_any().downcast_ref::<Kind>(), Some(Kind::Unread)) =>
            {
                sweep_line(node, st, sw)?;
                continue;
            }
            Some(Item::Conditional {
                head,
                then_body,
                else_body,
                ..
            }) => {
                let line = node.span.line as usize;
                let head = read_block_head(node, head, st, sw)?;
                let selected =
                    fold_condition(&head, st, &sw.reader, line).map_err(|error| match error {
                        ConditionError::Use(error) => super::statement_error(
                            error,
                            node.span.file,
                            &node.span.expansion_frames,
                        ),
                        ConditionError::Definition(error) => error,
                    })?;
                let then_end = then_body.last().map_or(node.span.line, end_line);
                if selected {
                    project_nodes(then_body, st, sw)?;
                } else {
                    if st.token_depth == 0 {
                        sw.admit_range(&node.span, node.span.line + 1, then_end)?;
                        if let Some(body) = else_body {
                            // A nested elseif owns its head; plain else does not.
                            if !matches!(&node.item, Some(Item::Conditional { close, .. }) if close.is_empty())
                            {
                                sw.admit_range(&node.span, then_end + 1, then_end + 1)?;
                            }
                            project_nodes(body, st, sw)?;
                        }
                    } else if let Some(body) = else_body {
                        project_nodes(body, st, sw)?;
                    }
                }
                // `.end` inside the block: ca65's reader stops before ever
                // seeing the closer, and says so (probed: `Conditional
                // assembly branch was never closed`). The structural parse
                // saw the closer; the sequential read is what governs.
                if sw.reader.ended {
                    return Err(AsmError::at(
                        node.span.clone(),
                        "conditional block is never closed".to_string(),
                    ));
                }
                if st.token_depth == 0 {
                    let first = if selected {
                        then_end + 1
                    } else {
                        end_line(node)
                    };
                    sw.admit_range(&node.span, first, end_line(node))?;
                }
                continue;
            }
            Some(Item::Repeat { head, body, .. }) => {
                let line = node.span.line as usize;
                let head = read_block_head(node, head, st, sw)?;
                let (count, var) = fold_repeat(&head, st, &sw.reader, line).map_err(|e| {
                    super::statement_error(e, node.span.file, &node.span.expansion_frames)
                })?;
                if st.token_depth == 0 {
                    sw.admit_range(&node.span, node.span.line + 1, end_line(node))?;
                }
                let watermark = pending_len(sw);
                st.token_captures.push(TokenCapture {
                    at: node.span.clone(),
                    defines: sw.reader.text_defines.clone(),
                });
                st.token_depth += 1;
                let previous_registers = sw.reader.repeat_registers;
                let registers = previous_registers.or(Some(sw.reader.extension.is_some()));
                for i in 0..count {
                    sw.reader.repeat_registers = registers;
                    if let Some(name) = &var {
                        st.loop_vars.push(LoopToken {
                            name: name.clone(),
                            value: i,
                            at: node.span.clone(),
                        });
                    }
                    let out = project_nodes(body, st, sw);
                    sw.reader.repeat_registers = previous_registers;
                    if var.is_some() {
                        st.loop_vars.pop();
                    }
                    if let Err(error) = out {
                        st.token_captures.pop();
                        st.token_depth -= 1;
                        return Err(error);
                    }
                    // Native .end terminates every enclosing token stream.
                    // Restore this iteration's state, then stop even when the
                    // authored count would otherwise require billions of trips.
                    if sw.reader.ended {
                        break;
                    }
                }
                st.token_captures.pop();
                st.token_depth -= 1;
                if st.token_depth == 0 {
                    drain_includes(watermark, st, sw)?;
                }
                continue;
            }
            _ => {}
        }
        project_selected(node, st, &mut sw.reader)?;
    }
    Ok(())
}

/// Read a block head at the moment the sweep reaches it: substitute the text
/// environment in force, split an optional label, and bind it. ca65 places
/// the label at the head line's address, just as if it occupied its own
/// label-only line.
///
/// # Errors
/// A substitution failure, or a label that does not parse.
fn read_block_head(
    node: &crate::ast::Node,
    head: &str,
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<String, AsmError> {
    let line = node.span.line as usize;
    let file = node.span.file;
    sw.reader.anons.file.set(file);
    if st.token_depth == 0 {
        sw.admit_range(&node.span, node.span.line, node.span.line)?;
    }
    let head = capture_text(head, &node.span, st)?;
    let head = repeat_text(&head, &node.span, st);
    let processed = sw
        .reader
        .preprocess_line(&head, line, file)
        .map_err(|e| super::statement_error(e, file, &node.span.expansion_frames))?
        // Only a `.define`/`.undefine` line is consumed whole, and a block
        // head is neither.
        .unwrap_or_default();
    let processed = repeat_text(&processed, &node.span, st);
    let processed = macros::expand_one_at(
        &ca65_flat::Ca65Macros,
        &processed,
        file,
        line,
        &mut st.macro_state,
    )?
    .text;
    let processed = live_text(&processed, st, &sw.reader, line)
        .map_err(|e| super::statement_error(e, file, &node.span.expansion_frames))?;
    bind_head_label(&processed, node, st, sw).map(str::to_string)
}

/// Place a selected block or macro-definition label at its authored position.
fn bind_head_label<'a>(
    source: &'a str,
    node: &crate::ast::Node,
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<&'a str, AsmError> {
    let line = node.span.line as usize;
    let file = node.span.file;
    sw.reader.anons.file.set(file);
    let (symbol, rest) = sw
        .reader
        .block_open(source, line)
        .map_err(|e| super::statement_error(e, file, &node.span.expansion_frames))?;
    if let Some(symbol) = symbol {
        sw.reader.source_known.insert(symbol.qualified.clone());
        st.deferred.invalidate_size(&symbol.qualified);
        st.label_seg
            .insert(symbol.qualified.clone(), st.seg.clone());
        st.stmts.push(Stmt {
            line,
            file,
            expansion_frames: node.span.expansion_frames.clone(),
            bound_insn: None,
            seg: st.seg.clone(),
            label: Some(symbol.qualified.clone()),
            kind: Kind::Empty,
        });
    }
    Ok(rest)
}

/// Read one unread line (#481): apply the reader's live text environment,
/// parse it, and project whatever it contributes. A walk-handled directive —
/// `.include`/`.incbin` — resolves here, at the point the sweep reads it,
/// which is what lets a missing file inside an untaken branch go unnoticed
/// exactly as ca65's sequential reader never notices it.
///
/// # Errors
/// Any parse failure on the line, or a directive whose target cannot be
/// resolved.
fn sweep_line(
    node: &crate::ast::Node,
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<(), AsmError> {
    let line = node.span.line as usize;
    let file = node.span.file;
    let source = capture_text(&node.source, &node.span, st)?;
    let source = repeat_text(&source, &node.span, st);
    // Captures were admitted in full before execution. A selected .end now
    // stops before text evaluation can read its unused operand tail.
    let source = {
        let code = super::strip_comment(&source).trim_start();
        let end = literal_end(code);
        if let Some(end) = end {
            code[..end].to_string()
        } else {
            source
        }
    };
    if st.token_depth == 0 && node.span.expansion_frames.is_empty() {
        ca65_flat::validate_numeric_tokens(&source, &node.span)?;
    }
    let Some(processed) = sw
        .reader
        .preprocess_line(&source, line, file)
        .map_err(|e| super::statement_error(e, file, &node.span.expansion_frames))?
    else {
        return Ok(());
    };
    let processed = repeat_text(&processed, &node.span, st);
    // The shared expander separates an optional label by whitespace. ca65
    // owns the colon token, including attached/indented call-site labels.
    let separated = super::split_label_head(processed.trim_start())
        .filter(|(_, rest)| !rest.is_empty())
        .map(|(label, rest)| format!("{label} {rest}"));
    let macro_source = separated.as_deref().unwrap_or(&processed);
    let invokes = matches!(
        macros::macro_line(&ca65_flat::Ca65Macros, macro_source.trim_start(), &|name| {
            st.macro_state.defines(name)
        },),
        macros::MacroLine::Invokes
    );
    if invokes && node.span.expansion_frames.len() >= 64 {
        let name = &node.span.expansion_frames[0].macro_name;
        return Err(AsmError::at(
            node.span.clone(),
            format!("macro `{name}` is still expanding after 64 levels — is it recursive?"),
        ));
    }
    let mut expanded = macros::expand_one_at(
        &ca65_flat::Ca65Macros,
        if invokes {
            macro_source.trim_start()
        } else {
            &processed
        },
        file,
        line,
        &mut st.macro_state,
    )
    .map_err(|mut error| {
        if error.span.is_none() {
            error.span = Some(node.span.clone());
        }
        error
    })?;
    if invokes {
        for origin in &mut expanded.origins {
            origin
                .frames
                .extend(node.span.expansion_frames.iter().cloned());
        }
        let mut reader = Walker::structural_source(sw.reader.set, &expanded.text);
        ca65_flat::walk_source_expanded(&mut reader, &expanded.text, file).map_err(|e| {
            ca65_flat::stamp_file(macros::remap_lines(e, Some(&expanded.origins)), file)
        })?;
        let mut program = reader.finish(expanded.text.lines().count() as u32)?;
        macros::place_nodes(&mut program.nodes, Some(&expanded.origins));
        return project_nodes(&program.nodes, st, sw);
    }
    let processed = expanded.text.trim_end_matches('\n');
    let generated_label = ident_arguments(processed.trim_start()).is_some();
    let processed = live_text(processed, st, &sw.reader, line)
        .map_err(|e| super::statement_error(e, file, &node.span.expansion_frames))?;
    let end_label = super::split_label_head(processed.trim())
        .filter(|(_, rest)| super::end_token_len(rest).is_some())
        .map(|(label, _)| label);
    if let Some(label) = end_label {
        let name = label.trim_end_matches(':');
        let upper = name.to_ascii_uppercase();
        // Literal registers and F/Z size prefixes are not identifier tokens.
        // .ident explicitly manufactures an identifier, including a register's
        // spelling. Keep this new end-label admission separate from #853.
        if !generated_label
            && (matches!(upper.as_str(), "A" | "X" | "Y" | "F" | "Z")
                || (sw.reader.extension.is_some() && matches!(upper.as_str(), "S" | "SP")))
        {
            return Err(AsmError::at(
                node.span.clone(),
                format!("reserved label name `{name}`"),
            ));
        }
    }
    let start = sw.reader.nodes.len();
    // Bind through the ordinary label-only walk before stopping. Parsing the
    // remaining .end as an operation would instead report unsupported syntax.
    let walked = sw
        .reader
        .walk_line(end_label.unwrap_or(&processed), line, file)
        .map_err(|e| {
            let mut error = super::statement_error(e, file, &node.span.expansion_frames);
            if let Some(span) = error.span.as_mut() {
                span.expansion_frames
                    .clone_from(&node.span.expansion_frames);
            }
            error
        });
    let mut drained: Vec<crate::ast::Node> = sw.reader.nodes.split_off(start);
    let walked = walked?;
    // A macro-expanded line carries its author's frames on the structural
    // node; the freshly read nodes get the same placement.
    for n in &mut drained {
        n.span
            .expansion_frames
            .clone_from(&node.span.expansion_frames);
        if let Some(sp) = n.operand_span.as_mut() {
            sp.expansion_frames.clone_from(&node.span.expansion_frames);
        }
        project_selected(n, st, &mut sw.reader)?;
    }
    if end_label.is_some() {
        sw.reader.ended = true;
    }
    let Some(mut d) = walked else {
        return Ok(());
    };
    d.span
        .expansion_frames
        .clone_from(&node.span.expansion_frames);
    if let Some(sp) = d.operand_span.as_mut() {
        sp.expansion_frames.clone_from(&node.span.expansion_frames);
    }
    if sw.includes.is_none() {
        // Single-source: the target is never opened (KTD1); the projection's
        // Include/Incbin arms carry the pointer to the multi-file entry.
        return project_selected(&ca65_flat::unresolved_node(d), st, &mut sw.reader);
    }
    resolve_at_sweep(d, st, sw)
}

/// The opening parenthesis of a syntactic .ident call, without evaluating it.
fn ident_arguments(source: &str) -> Option<usize> {
    if !source.get(..6)?.eq_ignore_ascii_case(".ident") {
        return None;
    }
    let rest = source[6..].trim_start_matches([' ', '\t']);
    rest.starts_with('(').then_some(source.len() - rest.len())
}

/// A literal terminator is read before its unused tail. A generated label's
/// expression still needs evaluation; only the source after .end is unread.
fn literal_end(code: &str) -> Option<usize> {
    if let Some(end) = super::end_token_len(code) {
        return Some(end);
    }
    if let Some((_, rest)) = super::split_label_head(code) {
        return super::end_token_len(rest).map(|len| code.len() - rest.len() + len);
    }
    let open = ident_arguments(code)?;
    let mut depth = 0usize;
    let mut quote = None;
    for (at, byte) in code.bytes().enumerate().skip(open) {
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let after = code[at + 1..].trim_start_matches([' ', '\t']);
                    let rest = after.strip_prefix(':')?.trim_start_matches([' ', '\t']);
                    return super::end_token_len(rest).map(|len| code.len() - rest.len() + len);
                }
            }
            _ => {}
        }
    }
    None
}

/// Resolve an `.include`/`.incbin` the sweep just read, under the same depth,
/// cycle, and window rules as the eager walk — then read an include's target
/// the same way the root is read: structure eagerly, meaning through this
/// sweep, so the target shares the environment in force at the include point
/// and everything it defines flows back out.
///
/// # Errors
/// Resolution failures at the directive's operand span; any error the
/// target's own lines raise.
fn resolve_at_sweep(
    d: ca65_flat::DirectiveLine,
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<(), AsmError> {
    use super::ca65_flat::WalkDirective;
    let span = d.span.clone();
    let at = d.operand_span.clone().unwrap_or_else(|| span.clone());
    match d.kind {
        WalkDirective::Include { ref request } => {
            // A label on the include line binds at the include point's
            // address (probe-pinned), so it becomes a label-only node
            // before the target's lines.
            if let Some(symbol) = d.label {
                st.label_seg
                    .insert(symbol.qualified.clone(), st.seg.clone());
                st.stmts.push(Stmt {
                    line: span.line as usize,
                    file: span.file,
                    expansion_frames: span.expansion_frames.clone(),
                    bound_insn: None,
                    seg: st.seg.clone(),
                    label: Some(symbol.qualified),
                    kind: Kind::Empty,
                });
            }
            let opened = {
                let cx = sw.includes.as_mut().expect("checked by the caller");
                let (id, contents) = ca65_flat::open_include_in_stream(
                    request,
                    &at,
                    cx.map,
                    cx.loader,
                    &cx.stack,
                    &cx.ancestry,
                    &ca65_flat::CA65_SEMANTICS,
                    span.line,
                )?;
                cx.stack.push(id);
                PendingInclude { id, contents }
            };
            if st.token_depth != 0 {
                sw.includes.as_mut().expect("checked").pending.push(opened);
                Ok(())
            } else {
                consume_include(opened, st, sw)
            }
        }
        WalkDirective::Incbin {
            ref request,
            offset,
            size,
        } => {
            let payload = {
                let cx = sw.includes.as_mut().expect("checked by the caller");
                let mut search = cx.stack.clone();
                if search.last() != cx.ancestry.last() {
                    search.extend(cx.ancestry.last().copied());
                }
                ca65_flat::incbin_payload(
                    request,
                    offset,
                    size,
                    &at,
                    cx.map,
                    cx.loader,
                    &search,
                    &ca65_flat::CA65_SEMANTICS,
                )?
            };
            project_selected(
                &crate::ast::Node {
                    operand_span: d.operand_span,
                    label: d.label,
                    item: Some(crate::ast::Item::Binary(payload)),
                    source: d.source,
                    span,
                    trivia: d.trivia,
                },
                st,
                &mut sw.reader,
            )
        }
    }
}

fn capture_text(source: &str, at: &crate::span::Span, st: &Projection) -> Result<String, AsmError> {
    let mut text = source.to_string();
    for capture in &st.token_captures {
        if capture.at.file == at.file && capture.at.expansion_frames == at.expansion_frames {
            text = super::substitute_text_defines(&text, &capture.defines, at.line as usize)?;
        }
    }
    Ok(text)
}

fn register_macro(
    node: &crate::ast::Node,
    body: &[crate::ast::Node],
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<(), AsmError> {
    let capture = |raw: &str, at: &crate::span::Span| -> Result<String, AsmError> {
        let raw = capture_text(raw, at, st)?;
        let raw = repeat_text(&raw, at, st);
        let raw = super::substitute_text_defines(&raw, &sw.reader.text_defines, at.line as usize)
            .map_err(|e| super::statement_error(e, at.file, &at.expansion_frames))?;
        Ok(repeat_text(&raw, at, st))
    };
    let head = capture(&node.source, &node.span)?;
    let (name, params) = super::macro_header(&head)
        .ok_or_else(|| AsmError::at(node.span.clone(), "invalid macro definition"))?;
    if !is_ident(&name) || params.iter().any(|name| !is_ident(name)) {
        return Err(AsmError::at(
            node.span.clone(),
            "macro names and parameters must be identifiers",
        ));
    }
    if st.macro_state.defines(&name) {
        return Err(AsmError::at(
            node.span.clone(),
            format!("macro `{name}` is already defined"),
        ));
    }
    let body = body
        .iter()
        .map(|n| capture(&n.source, &n.span))
        .collect::<Result<Vec<_>, _>>()?;
    bind_head_label(&head, node, st, sw)?;
    st.macro_state.register(
        name,
        macros::MacroDef {
            params,
            body,
            defined_at: Some(node.span.clone()),
        },
    );
    Ok(())
}

fn pending_len(sw: &Sweep<'_>) -> usize {
    sw.includes.as_ref().map_or(0, |cx| cx.pending.len())
}

fn drain_includes(
    watermark: usize,
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<(), AsmError> {
    while !sw.reader.ended && pending_len(sw) > watermark {
        let opened = sw
            .includes
            .as_mut()
            .expect("pending files need a loader")
            .pending
            .pop()
            .expect("above the watermark");
        consume_include(opened, st, sw)?;
    }
    Ok(())
}

fn consume_include(
    opened: PendingInclude,
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<(), AsmError> {
    let PendingInclude { id, contents } = opened;
    let set = sw.reader.set;
    let sub = {
        let cx = sw.includes.as_mut().expect("opened files need a loader");
        debug_assert_eq!(cx.stack.last(), Some(&id), "character input is LIFO");
        cx.ancestry.push(id);
        let mut w = Walker::structural(set);
        ca65_flat::walk_file(
            &mut w,
            &contents,
            id,
            cx.map,
            cx.loader,
            &mut cx.stack,
            &ca65_flat::CA65_SEMANTICS,
        )
        .and_then(|()| w.finish(contents.lines().count() as u32))
    };
    let outer_vars = std::mem::take(&mut st.loop_vars);
    let out = sub.and_then(|prog| project_nodes(&prog.nodes, st, sw));
    st.loop_vars = outer_vars;
    let cx = sw.includes.as_mut().expect("opened files need a loader");
    if out.is_ok() && !sw.reader.ended {
        debug_assert_eq!(cx.stack.last(), Some(&id), "file's pending streams drained");
        cx.stack.pop();
    }
    cx.ancestry.pop();
    out
}

/// Fold a `.if` / `.ifdef` / `.ifndef` / `.elseif` head.
/// Cycle diagnostics retain the definition's file and expansion frames.
enum ConditionError {
    Use(AsmError),
    Definition(AsmError),
}

impl From<AsmError> for ConditionError {
    fn from(error: AsmError) -> Self {
        Self::Use(error)
    }
}

fn fold_condition(
    head: &str,
    st: &Projection,
    reader: &Walker,
    line: usize,
) -> Result<bool, ConditionError> {
    let (word, args) = split_first_word(head.trim());
    let word = word.to_ascii_lowercase();
    let args = args.trim();
    let env = &st.consts;
    match word.as_str() {
        ".ifdef" | ".ifndef" => {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            // An active text symbol was substituted into the argument before
            // this fold, so a numeric value lands here — which ca65 answers
            // `Identifier expected`, not with a membership test.
            let key =
                if let Some(local) = name.strip_prefix('@').filter(|name| is_cheap_ident(name)) {
                    cheap_key(reader.scope_env.anchor_str(), local)
                } else if name
                    .strip_prefix("::")
                    .unwrap_or(name)
                    .split("::")
                    .all(is_ident)
                {
                    name.to_string()
                } else {
                    return Err(AsmError::new(
                        line,
                        format!("`{word}` needs an identifier, got `{name}`"),
                    )
                    .into());
                };
            let defined = reader.name_defined(&key, line)?;
            Ok(if word == ".ifdef" { defined } else { !defined })
        }
        // `.ifblank` asks whether anything follows it on the line — which is
        // how a macro tests an argument it may not have been given, the
        // expansion being textual. ca65 counts tokens, not characters, so
        // whitespace alone is still blank.
        ".ifblank" | ".ifnblank" => Ok((word == ".ifblank") == args.is_empty()),
        // Processor tests follow the live reader's selection.
        // `.ifref` asks the same question `.ref` does, from the same set.
        ".ifref" | ".ifnref" => {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            Ok((word == ".ifref") == reader.name_referenced(name, line)?)
        }
        ".ifp02" => Ok(reader.extension.is_none()),
        ".ifp4510" => Ok(reader.extension.is_some()),
        ".ifp816" | ".ifpc02" | ".ifpsc02" => Ok(false),
        // `.ifconst` is not `.if`: it asks whether the expression *is* a
        // constant, not what it comes to, and answers rather than failing when
        // it is not. ca65's rule is the linker's — probed against V2.18:
        //
        //   `.const(LA - LB)` in one segment is 1, across two is 0;
        //   `.const(LA + LA)` and `.const(LA * 2)` are 0;
        //   `.const(LB - LA)` with `LB` *below* the line is 0.
        //
        // So a label is not constant, a difference of two labels above the line
        // in one segment is, and anything that is not linear in the labels is
        // not. See [`segment_weights`].
        ".ifconst" | ".ifnconst" => {
            if args.is_empty() {
                return Err(AsmError::new(line, format!("`{word}` needs an expression")).into());
            }
            let expr = reader.source_value(args, line)?;
            let expr = map_expr_syms(&expr, &resolve_defined(&st.consts, &st.label_seg));
            st.deferred
                .study_source(
                    &expr,
                    &st.consts,
                    &st.label_seg,
                    &st.zero_page_names,
                    st.seg == "ZEROPAGE",
                    line,
                )
                .map_err(ConditionError::Definition)?;
            let constant = fold_const(&expr, env, line).is_ok() || weighs_nothing(&expr, st);
            Ok((word == ".ifconst") == constant)
        }
        ".if" | ".elseif" => {
            if args.is_empty() {
                return Err(AsmError::new(line, format!("`{word}` needs a condition")).into());
            }
            let expr = reader.source_value(args, line)?;
            // `.if .defined(X)` — answered against what stands above this line,
            // which is what makes `.defined` positional in the first place.
            let expr = map_expr_syms(&expr, &resolve_defined(&st.consts, &st.label_seg));
            // ca65: `Constant expression expected` — a condition may not reach
            // forward, and a ca65 label is never constant.
            let value = fold_const(&expr, env, line).map_err(|_| {
                AsmError::new(
                    line,
                    format!(
                        "`{args}` must be a constant here — ca65 folds a condition against the                          `=` constants above it, and refuses a label or a forward reference"
                    ),
                )
            })?;
            Ok(value != 0)
        }
        _ => Err(AsmError::new(
            line,
            format!("internal error: `{head}` is not a conditional head"),
        )
        .into()),
    }
}

/// How much of each segment an expression carries — ca65's constancy rule,
/// which is the linker's.
///
/// `Some(weights)` for an expression that is *linear* in the labels: every
/// label above this line counts +1 in its own segment, `*` counts +1 in the
/// active one, and addition and subtraction combine them. All-zero weights
/// mean the expression is a constant. `None` means it is not linear at all — a
/// label multiplied, shifted, masked or fed to a byte extractor — which ca65
/// answers as not constant too, so the caller need not tell the two apart.
///
/// A name this line has not seen yet counts as a label of *no* segment, which
/// cannot cancel against a real one: `.const(LB - LA)` with `LB` below the line
/// is `0` in ca65, and a name defined nowhere is `0` here where ca65
/// additionally reports the undefined symbol.
fn segment_weights(expr: &Expr, st: &Projection) -> Option<BTreeMap<String, i64>> {
    let mut weights = BTreeMap::new();
    walk_weights(expr, 1, st, &mut weights)?;
    Some(weights)
}

/// Whether an expression stands for a value on its own — all weights present
/// and cancelling.
fn weighs_nothing(expr: &Expr, st: &Projection) -> bool {
    segment_weights(expr, st).is_some_and(|w| w.values().all(|&n| n == 0))
}

fn walk_weights(
    expr: &Expr,
    sign: i64,
    st: &Projection,
    weights: &mut BTreeMap<String, i64>,
) -> Option<()> {
    use crate::engine::BinOp as Op;
    let mut carry = |seg: String| *weights.entry(seg).or_insert(0) += sign;
    match expr {
        Expr::Num(_) => Some(()),
        Expr::Pc => {
            carry(st.seg.clone());
            Some(())
        }
        Expr::Sym(name) => {
            if st.consts.contains_key(name) {
                return Some(());
            }
            carry(st.label_seg.get(name).cloned().unwrap_or_default());
            Some(())
        }
        Expr::Bin(Op::Add, a, b) => {
            walk_weights(a, sign, st, weights)?;
            walk_weights(b, sign, st, weights)
        }
        Expr::Bin(Op::Sub, a, b) => {
            walk_weights(a, sign, st, weights)?;
            walk_weights(b, -sign, st, weights)
        }
        // Every other operator is linear only over operands that already stand
        // for values, so each side is weighed on its own and a label anywhere
        // inside ends it.
        Expr::Bin(_, a, b) => (weighs_nothing(a, st) && weighs_nothing(b, st)).then_some(()),
        Expr::Lo(e)
        | Expr::Hi(e)
        | Expr::Bank(e)
        | Expr::Neg(e)
        | Expr::BitNot(e)
        | Expr::LogNot(e)
        | Expr::FixRound(e)
        | Expr::TrailingZeros(e) => weighs_nothing(e, st).then_some(()),
    }
}

/// Fold a `.repeat n[, var]` head into its count and optional loop variable.
fn fold_repeat(
    head: &str,
    st: &Projection,
    reader: &Walker,
    line: usize,
) -> Result<(i64, Option<String>), AsmError> {
    let (_, args) = split_first_word(head.trim());
    let (count_text, var) = match args.split_once(',') {
        Some((c, v)) => (c.trim(), Some(v.trim().to_string())),
        None => (args.trim(), None),
    };
    if count_text.is_empty() {
        return Err(AsmError::new(line, "`.repeat` needs a count"));
    }
    let expr = reader.source_value(count_text, line)?;
    let count = fold_const(&expr, &st.consts, line)?;
    // ca65: a negative count is `Range error`, where zero is no iterations.
    if count < 0 {
        return Err(AsmError::new(
            line,
            format!("`.repeat {count}`: a repetition count may not be negative"),
        ));
    }
    if let Some(name) = &var
        && !is_ident(name)
    {
        return Err(AsmError::new(
            line,
            "`.repeat` needs an identifier after the comma",
        ));
    }
    Ok((count, var))
}

/// Substitute a `.repeat`'s loop variables into an expression.
///
/// The value has to be **baked in**, not left as a symbol, for the same reason
/// acme's `!for` variable is: ca65 resolves `Expr::Sym` once, in a later pass,
/// against one table — and a loop variable holds a different value on every
/// iteration, so there is no single entry that table could hold.
pub(super) fn map_expr_syms(e: &Expr, f: &dyn Fn(&str) -> Option<Expr>) -> Expr {
    match e {
        Expr::Sym(name) => f(name).unwrap_or_else(|| e.clone()),
        Expr::Lo(i) => Expr::Lo(Box::new(map_expr_syms(i, f))),
        Expr::Hi(i) => Expr::Hi(Box::new(map_expr_syms(i, f))),
        Expr::Bank(i) => Expr::Bank(Box::new(map_expr_syms(i, f))),
        Expr::Neg(i) => Expr::Neg(Box::new(map_expr_syms(i, f))),
        Expr::BitNot(i) => Expr::BitNot(Box::new(map_expr_syms(i, f))),
        Expr::LogNot(i) => Expr::LogNot(Box::new(map_expr_syms(i, f))),
        Expr::FixRound(i) => Expr::FixRound(Box::new(map_expr_syms(i, f))),
        Expr::TrailingZeros(i) => Expr::TrailingZeros(Box::new(map_expr_syms(i, f))),
        Expr::Bin(op, l, r) => Expr::Bin(
            *op,
            Box::new(map_expr_syms(l, f)),
            Box::new(map_expr_syms(r, f)),
        ),
        Expr::Num(_) | Expr::Pc => e.clone(),
    }
}

/// The same, over an operand.
fn map_operand_syms(
    o: &mos6502::OperandSyntax,
    f: &dyn Fn(&str) -> Option<Expr>,
) -> mos6502::OperandSyntax {
    use mos6502::OperandSyntax as O;
    let b = |e: &Expr| map_expr_syms(e, f);
    match o {
        O::None => O::None,
        O::Accumulator => O::Accumulator,
        O::Immediate(e) => O::Immediate(b(e)),
        O::Indirect(e) => O::Indirect(b(e)),
        O::IndexedIndirect(e) => O::IndexedIndirect(b(e)),
        O::IndirectIndexed(e) => O::IndirectIndexed(b(e)),
        O::IndirectIndexedZ(e) => O::IndirectIndexedZ(b(e)),
        O::StackIndirectIndexedY(e) => O::StackIndirectIndexedY(b(e)),
        O::Indexed(e, i) => O::Indexed(b(e), *i),
        O::Direct(e) => O::Direct(b(e)),
    }
}

/// Every name a statement mentions. Written as a map that answers nothing,
/// because [`map_kind_syms`] already knows where the names in a `Kind` are and
/// a second walk would be a second thing to keep in step with it.
pub(super) fn collect_syms(kind: &Kind, out: &mut BTreeSet<String>) {
    let seen = std::cell::RefCell::new(Vec::new());
    let _ = map_kind_syms(kind, &|name: &str| {
        seen.borrow_mut().push(name.to_string());
        None
    });
    out.extend(seen.into_inner());
}

/// The same, over a statement kind.
pub(super) fn map_kind_syms(k: &Kind, f: &dyn Fn(&str) -> Option<Expr>) -> Kind {
    let list = |es: &Vec<Expr>| es.iter().map(|e| map_expr_syms(e, f)).collect();
    match k {
        Kind::Unread => Kind::Unread,
        Kind::Bytes(es) => Kind::Bytes(list(es)),
        Kind::Words(es) => Kind::Words(list(es)),
        Kind::DBytes(es) => Kind::DBytes(list(es)),
        Kind::DWords(es) => Kind::DWords(list(es)),
        Kind::BitBranch {
            form,
            zero_page,
            target,
        } => Kind::BitBranch {
            form,
            zero_page: map_expr_syms(zero_page, f),
            target: map_expr_syms(target, f),
        },
        Kind::Insn {
            operand,
            address_size,
            mnemonic,
            extension,
        } => Kind::Insn {
            operand: map_operand_syms(operand, f),
            address_size: *address_size,
            mnemonic: mnemonic.clone(),
            extension: *extension,
        },
        Kind::Empty => Kind::Empty,
        Kind::Res(n, f) => Kind::Res(*n, *f),
        Kind::Constant(name, value) => Kind::Constant(name.clone(), *value),
        Kind::EnumValue(name, value) => Kind::EnumValue(name.clone(), map_expr_syms(value, f)),
        Kind::Org(e) => Kind::Org(map_expr_syms(e, f)),
        Kind::Reloc => Kind::Reloc,
        Kind::Align(m, f) => Kind::Align(*m, *f),
        Kind::Message(sev, t) => Kind::Message(*sev, t.clone()),
        Kind::Visible {
            rule,
            zero_page,
            names,
            define,
        } => Kind::Visible {
            rule: *rule,
            zero_page: *zero_page,
            names: names.clone(),
            define: define.as_ref().map(|e| map_expr_syms(e, f)),
        },
        Kind::Assert(c, fatal, m) => Kind::Assert(map_expr_syms(c, f), *fatal, m.clone()),
        Kind::Raw(b) => Kind::Raw(b.clone()),
    }
}

/// Track physical bytes while the selected source is read. `.org` changes
/// reported addresses, while alignment continues to measure segment offsets.
fn project_selected(
    node: &crate::ast::Node,
    st: &mut Projection,
    reader: &mut Walker,
) -> Result<(), AsmError> {
    let start = st.stmts.len();
    if let Some(label) = &node.label {
        reader.source_known.insert(label.qualified.clone());
        st.deferred.invalidate_size(&label.qualified);
    }
    project_one(node, st)?;
    let updates = st
        .deferred
        .advance(std::mem::take(&mut st.constant_updates), &mut st.consts);
    for (name, value) in updates {
        reader.consts.insert(name, value);
    }
    for stmt in &st.stmts[start..] {
        let off = reader.source_offsets.entry(stmt.seg.clone()).or_default();
        let size = match &stmt.kind {
            Kind::Bytes(v) => v.len(),
            Kind::Raw(v) => v.len(),
            Kind::Words(v) | Kind::DBytes(v) => v.len() * 2,
            Kind::DWords(v) => v.len() * 4,
            Kind::Res(size, _) => *size,
            Kind::Align(boundary, _) => super::link::alignment_padding(*boundary, *off),
            Kind::BitBranch { form, .. } => form.len(),
            Kind::Insn { .. } => stmt
                .bound_insn
                .as_ref()
                .expect("selected instruction form")
                .form
                .len(),
            _ => 0,
        };
        *off += size as u32;
        if size != 0
            && let Some(open) = reader.scopes.last_mut()
        {
            open.has_output = true;
        }
    }
    Ok(())
}

/// Project one ordinary node — the body of the original projection loop.
fn project_one(node: &crate::ast::Node, st: &mut Projection) -> Result<(), AsmError> {
    use crate::ast::{Item, Operand};
    if let Some(Item::Native(payload)) = &node.item {
        let kind = payload
            .as_any()
            .downcast_ref::<Kind>()
            .expect("ca65 native kind");
        let constant = match kind {
            Kind::EnumValue(name, value) => Some((name, value)),
            Kind::Visible {
                names,
                define: Some(value),
                ..
            } => names.first().map(|name| (name, value)),
            _ => None,
        };
        if let Some((name, value)) = constant {
            // Native retained definitions use the ordinary Equ binding path.
            let definition = crate::ast::Node {
                label: Some(crate::ast::Symbol {
                    name: name.rsplit("::").next().unwrap_or(name).to_string(),
                    qualified: name.clone(),
                    scope: crate::ast::Scope::Global,
                }),
                item: Some(crate::ast::item_from_operation(
                    crate::engine::Operation::Equ(value.clone()),
                )),
                source: String::new(),
                span: node.span.clone(),
                operand_span: node.operand_span.clone(),
                trivia: crate::ast::Trivia::default(),
            };
            project_one(&definition, st)?;
            if let Kind::Visible {
                rule,
                zero_page,
                names,
                ..
            } = kind
            {
                let declaration = crate::ast::Node {
                    label: node.label.clone(),
                    item: Some(Item::Native(Box::new(Kind::Visible {
                        rule: *rule,
                        zero_page: *zero_page,
                        names: names.clone(),
                        define: None,
                    }))),
                    source: node.source.clone(),
                    span: node.span.clone(),
                    operand_span: node.operand_span.clone(),
                    trivia: crate::ast::Trivia::default(),
                };
                return project_one(&declaration, st);
            }
            return Ok(());
        }
    }
    let seg = &mut st.seg;
    let seg_stack = &mut st.seg_stack;
    let stmts = &mut st.stmts;
    let label_seg = &mut st.label_seg;
    let consts = &mut st.consts;
    let referenced = &mut st.referenced;
    {
        let line = node.span.line as usize;
        let file = node.span.file;
        match &node.item {
            Some(Item::Equ(Operand::Expr { value, .. })) => {
                if let Some(sym) = node.label.as_ref() {
                    if consts.contains_key(&sym.qualified)
                        || label_seg.contains_key(&sym.qualified)
                        || !st.equ_definitions.insert(sym.qualified.clone())
                    {
                        return Err(AsmError::at(
                            node.span.clone(),
                            format!("symbol `{}` is already defined", sym.name),
                        ));
                    }
                    let source = st.deferred.study_source(
                        value,
                        consts,
                        label_seg,
                        &st.zero_page_names,
                        seg == "ZEROPAGE",
                        line,
                    )?;
                    let updates = st.deferred.insert(
                        sym.qualified.clone(),
                        value.clone(),
                        node.span.clone(),
                        stmts.len(),
                        source,
                        consts,
                    );
                    st.constant_updates.extend(updates);
                    collect_syms(&Kind::Bytes(vec![value.clone()]), referenced);
                    if sym.name.starts_with('@') {
                        st.const_names
                            .insert(sym.qualified.clone(), sym.name.clone());
                    }
                    // A zero-size private placement retains the definition PC
                    // without adding lexical state to the shared statement.
                    stmts.push(Stmt {
                        line,
                        file,
                        expansion_frames: node.span.expansion_frames.clone(),
                        seg: seg.clone(),
                        label: None,
                        kind: Kind::Empty,
                        bound_insn: None,
                    });
                }
            }
            // An unresolved include/incbin cannot assemble: it needs a loader,
            // which only the multi-file entry has (U5, KTD1). The single-source
            // API keeps meaning "one file, no includes" — with a pointer, not
            // the old `unsupported directive` rejection.
            Some(Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `.include \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `.incbin \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            // A resolved `.incbin` payload (the multi-file walk's lowering):
            // raw bytes at the directive's location in the active segment,
            // with a label on the directive line binding at the payload start.
            Some(Item::Binary(payload)) => {
                let label = node.label.as_ref().map(|s| s.qualified.clone());
                if let Some(l) = &label {
                    label_seg.insert(l.clone(), seg.clone());
                }
                stmts.push(Stmt {
                    line,
                    file,
                    expansion_frames: node.span.expansion_frames.clone(),
                    bound_insn: None,
                    seg: seg.clone(),
                    label,
                    kind: Kind::Raw(payload.clone()),
                });
            }
            Some(Item::Native(payload)) => {
                let kind = payload
                    .as_any()
                    .downcast_ref::<Kind>()
                    .expect("ca65 stores a Kind in every native node");
                // `.defined` is answered here, in source order, against what
                // this point in the file has seen — which is the question ca65
                // asks.
                let kind = map_kind_syms(kind, &|name| {
                    if name
                        .strip_prefix(super::DEFINED_MARK)
                        .is_some_and(|key| st.equ_definitions.contains(key))
                    {
                        Some(Expr::Num(1))
                    } else {
                        resolve_defined(consts, label_seg)(name)
                    }
                });
                // `.ref` is the same question about uses rather than
                // definitions, so it is answered from the same place.
                let kind = map_kind_syms(&kind, &resolve_ref(referenced));
                // Then record what *this* statement mentions, so the next one
                // sees it. Answering first keeps a statement's own names out of
                // its own `.ref`.
                collect_syms(&kind, referenced);
                // A record's constants, folded here so they land in the same
                // map, in the same order, as every `=` in the file.
                if let Kind::Constant(name, value) = &kind {
                    consts.insert(name.clone(), *value);
                    st.constant_updates.push((name.clone(), *value));
                    if let Some(owner) = name.strip_suffix(super::SIZE_SUFFIX) {
                        st.const_names
                            .insert(name.clone(), format!("{owner}{}size", super::LABEL_SEP));
                    }
                }
                if let Kind::Visible {
                    rule,
                    zero_page: true,
                    names,
                    ..
                } = &kind
                    && *rule != super::VisRule::MustBeDefined
                {
                    st.zero_page_names.extend(names.iter().map(|key| {
                        super::split_scope_ref(key).map_or_else(
                            || key.clone(),
                            |(path, name)| {
                                if path.is_empty() {
                                    name.to_string()
                                } else {
                                    format!("{path}::{name}")
                                }
                            },
                        )
                    }));
                }
                let label = node.label.as_ref().map(|s| s.qualified.clone());
                if let Some(l) = &label {
                    label_seg.insert(l.clone(), seg.clone());
                }
                // ca65 fixes the form when it reads the instruction. A later
                // constant or zero-page label must not shorten an earlier use.
                // Resolve only this operand's dependencies, rather than copying
                // the complete namespace for every instruction.
                let bound_insn = if matches!(kind, Kind::Insn { .. }) {
                    let mut names = BTreeSet::new();
                    collect_syms(&kind, &mut names);
                    let targets = super::scope_targets(&names, &|name| {
                        consts.contains_key(name)
                            || label_seg.contains_key(name)
                            || st.zero_page_names.contains(name)
                            || st.deferred.definitions.contains_key(name)
                    });
                    let env: BTreeMap<String, i64> = names
                        .into_iter()
                        .filter_map(|name| {
                            let target = targets.get(&name).unwrap_or(&name);
                            let value = consts.get(target).copied().or_else(|| {
                                (st.zero_page_names.contains(target)
                                    || label_seg.get(target).map(String::as_str)
                                        == Some("ZEROPAGE"))
                                .then_some(0)
                            })?;
                            Some((name, value))
                        })
                        .collect();
                    let (resolved, _) =
                        super::resolve(&isa::mos6502::SET, kind.clone(), &env, 0, line, 0, None)
                            .map_err(|e| {
                                super::statement_error(e, file, &node.span.expansion_frames)
                            })?;
                    let super::Resolved::Insn {
                        mut form,
                        operands,
                        mut unsigned_word,
                    } = resolved
                    else {
                        unreachable!("an instruction resolved to a non-instruction")
                    };
                    if let Kind::Insn {
                        operand,
                        address_size: None,
                        mnemonic,
                        extension,
                    } = &kind
                    {
                        use mos6502::{Index, OperandSyntax};
                        let memory = match operand {
                            OperandSyntax::Direct(value) => Some((value, None)),
                            OperandSyntax::Indexed(value, index) => Some((value, Some(*index))),
                            _ => None,
                        };
                        if let Some((value, index)) = memory {
                            let byte = st
                                .deferred
                                .size_hint(
                                    value,
                                    consts,
                                    label_seg,
                                    &st.zero_page_names,
                                    seg == "ZEROPAGE",
                                    line,
                                )
                                .unwrap_or(false);
                            let mode = match (byte, index) {
                                (true, None) => "zeropage",
                                (true, Some(Index::X)) => "zeropage,x",
                                (true, Some(Index::Y)) => "zeropage,y",
                                (false, None) => "absolute",
                                (false, Some(Index::X)) => "absolute,x",
                                (false, Some(Index::Y)) => "absolute,y",
                            };
                            if let Some(selected) = extension
                                .and_then(|set| set.find_form(mnemonic, mode))
                                .or_else(|| isa::mos6502::SET.find_form(mnemonic, mode))
                            {
                                form = selected;
                                if !byte {
                                    unsigned_word = true;
                                }
                            }
                        }
                    }
                    Some(super::BoundInsn {
                        form,
                        operands,
                        unsigned_word,
                    })
                } else {
                    None
                };
                stmts.push(Stmt {
                    line,
                    file,
                    expansion_frames: node.span.expansion_frames.clone(),
                    bound_insn,
                    seg: seg.clone(),
                    label,
                    kind,
                });
            }
            // Item-less nodes: a `.segment` directive (tracked), a label-only line
            // (an empty placed statement), or a comment-only flush node (skipped).
            _ => {
                if let Some(switch) = segment_switch(&node.source) {
                    match switch {
                        SegSwitch::To(name) => {
                            st.declared_segments.insert(name.clone());
                            *seg = name;
                        }
                        SegSwitch::Push => seg_stack.push(seg.clone()),
                        // ca65 pops an empty stack with a diagnostic of its own;
                        // ours is the parse-time refusal, so an unmatched
                        // `.popseg` here leaves the active segment alone.
                        SegSwitch::Pop => {
                            if let Some(prev) = seg_stack.pop() {
                                *seg = prev;
                            }
                        }
                    }
                } else if let Some(sym) = node.label.as_ref() {
                    label_seg.insert(sym.qualified.clone(), seg.clone());
                    stmts.push(Stmt {
                        line,
                        file,
                        expansion_frames: node.span.expansion_frames.clone(),
                        bound_insn: None,
                        seg: seg.clone(),
                        label: Some(sym.qualified.clone()),
                        kind: Kind::Empty,
                    });
                }
            }
        }
    }
    Ok(())
}
