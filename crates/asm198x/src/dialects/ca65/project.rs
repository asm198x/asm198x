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
use super::{
    AnonCtx, AsmError, Expr, FileId, Kind, Parsed, SegSwitch, SourceLoader, SourceMap, Stmt,
    Walker, fold_const, is_ident, parse_value, resolve_defined, resolve_ref, segment_switch,
    split_first_word,
};

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
        stmts: Vec::new(),
        label_seg: BTreeMap::new(),
        consts: BTreeMap::new(),
        referenced: BTreeSet::new(),
        loop_vars: Vec::new(),
    };
    project_nodes(&program.nodes, &mut st, &mut sw)?;
    // The reader's end-of-source checks — an unclosed scope or record, a
    // dangling forward anonymous reference — run where its state is final:
    // after the sweep, which is where the reading happened.
    let last = program.nodes.last().map_or(0, |n| n.span.line);
    sw.reader.finish(last)?;
    Ok(Parsed {
        stmts: st.stmts,
        label_seg: st.label_seg,
        consts: st.consts,
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
}

struct IncludeCx<'a> {
    map: &'a mut SourceMap,
    loader: &'a dyn SourceLoader,
    /// The active include chain, for the cycle and depth checks — the same
    /// discipline the eager walk applies, at sweep time.
    stack: Vec<FileId>,
}

impl<'a> Sweep<'a> {
    pub(super) fn single(set: &'static isa::InstructionSet) -> Self {
        Self {
            reader: Walker::new(set, true),
            includes: None,
        }
    }

    pub(super) fn multi(
        set: &'static isa::InstructionSet,
        map: &'a mut SourceMap,
        loader: &'a dyn SourceLoader,
    ) -> Self {
        Self {
            reader: Walker::new(set, true),
            includes: Some(IncludeCx {
                map,
                loader,
                stack: vec![FileId(0)],
            }),
        }
    }
}

/// The projection's running state: everything a later line's fold can see.
struct Projection {
    seg: String,
    /// Segments saved by `.pushseg`, innermost last; `.popseg` restores one.
    seg_stack: Vec<String>,
    stmts: Vec<Stmt>,
    label_seg: BTreeMap<String, String>,
    consts: BTreeMap<String, i64>,
    /// Every name the statements *above this point* have mentioned, which is
    /// what `.ref`/`.referenced` and `.ifref`/`.ifnref` ask about. Dead
    /// branches never reach here, so a use inside one does not count — probed
    /// against V2.18, where `.if 0 / .word L / .endif` leaves `.ref(L)` at 0.
    referenced: BTreeSet<String>,
    /// Loop variables bound by enclosing `.repeat`s, innermost last. Kept apart
    /// from `consts` because ca65 **scopes one to its loop** — `lda #i` after
    /// `.endrepeat` is `Symbol 'i' is undefined`, where acme's `!for` variable
    /// survives its block. Two dialects, two rules.
    loop_vars: Vec<(String, i64)>,
}

impl Projection {
    /// The constants a fold may see here: the file's, plus any enclosing loop
    /// variables shadowing them.
    fn env(&self) -> BTreeMap<String, i64> {
        let mut env = self.consts.clone();
        for (name, value) in &self.loop_vars {
            env.insert(name.clone(), *value);
        }
        env
    }
}

/// Project a run of nodes, folding any conditional or repetition **once, in
/// source order, before layout** — `decisions/conditionals-in-multipass-dialects.md`.
///
/// No layout state is consulted because a ca65 condition cannot reach any: the
/// reference refuses `*` and even a backward label in one, since a ca65 label is
/// relocatable until `ld65` links it and so is never a constant expression.
fn project_nodes(
    nodes: &[crate::ast::Node],
    st: &mut Projection,
    sw: &mut Sweep<'_>,
) -> Result<(), AsmError> {
    use crate::ast::Item;
    for node in nodes {
        // `.end` stopped the reader: nothing after it is read, so nothing
        // after it is projected — a block head included.
        if sw.reader.ended {
            break;
        }
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
                if fold_condition(&head, st, line)? {
                    project_nodes(then_body, st, sw)?;
                } else if let Some(body) = else_body {
                    project_nodes(body, st, sw)?;
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
                continue;
            }
            Some(Item::Repeat { head, body, .. }) => {
                let line = node.span.line as usize;
                let head = read_block_head(node, head, st, sw)?;
                let (count, var) = fold_repeat(&head, st, line)?;
                for i in 0..count {
                    if let Some(name) = &var {
                        st.loop_vars.push((name.clone(), i));
                    }
                    let first = st.stmts.len();
                    let out = project_nodes(body, st, sw);
                    if var.is_some() {
                        // Bake this pass's value into everything the body just
                        // produced, then drop the binding: ca65 scopes a loop
                        // variable to its loop.
                        let vars = st.loop_vars.clone();
                        for stmt in &mut st.stmts[first..] {
                            stmt.kind = map_kind_syms(&stmt.kind, &loop_var_sub(&vars));
                        }
                        st.loop_vars.pop();
                    }
                    out?;
                }
                if sw.reader.ended {
                    return Err(AsmError::at(
                        node.span.clone(),
                        "repetition block is never closed".to_string(),
                    ));
                }
                continue;
            }
            _ => {}
        }
        project_one(node, st)?;
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
    let processed = sw
        .reader
        .preprocess_line(head, line, file)
        .map_err(|e| ca65_flat::stamp_file(e, file))?
        // Only a `.define`/`.undefine` line is consumed whole, and a block
        // head is neither.
        .unwrap_or_default();
    let (symbol, rest) = sw
        .reader
        .block_open(&processed, line)
        .map_err(|e| ca65_flat::stamp_file(e, file))?;
    if let Some(symbol) = symbol {
        st.label_seg
            .insert(symbol.qualified.clone(), st.seg.clone());
        st.stmts.push(Stmt {
            line,
            file,
            seg: st.seg.clone(),
            label: Some(symbol.qualified.clone()),
            kind: Kind::Empty,
        });
    }
    Ok(rest.to_string())
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
    let Some(processed) = sw
        .reader
        .preprocess_line(&node.source, line, file)
        .map_err(|e| ca65_flat::stamp_file(e, file))?
    else {
        return Ok(());
    };
    let start = sw.reader.nodes.len();
    let walked = sw
        .reader
        .walk_line(&processed, line, file)
        .map_err(|e| ca65_flat::stamp_file(e, file));
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
        project_one(n, st)?;
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
        return project_one(&ca65_flat::unresolved_node(d), st);
    }
    resolve_at_sweep(d, st, sw)
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
                    seg: st.seg.clone(),
                    label: Some(symbol.qualified),
                    kind: Kind::Empty,
                });
            }
            let set = sw.reader.set;
            let (id, sub) = {
                let cx = sw.includes.as_mut().expect("checked by the caller");
                let (id, contents) = ca65_flat::open_include(
                    request,
                    &at,
                    cx.map,
                    cx.loader,
                    &cx.stack,
                    &ca65_flat::CA65_SEMANTICS,
                    span.line,
                )?;
                cx.stack.push(id);
                // The target's structure, read the way the root's was: its
                // macros expand (a file expands on its own, #93), its blocks
                // group, its lines stay unread for this sweep.
                let mut w = Walker::structural(set);
                let walked = ca65_flat::walk_file(
                    &mut w,
                    &contents,
                    id,
                    cx.map,
                    cx.loader,
                    &mut cx.stack,
                    &ca65_flat::CA65_SEMANTICS,
                );
                let sub = walked.and_then(|()| w.finish(contents.lines().count() as u32));
                (id, sub)
            };
            let out = sub.and_then(|prog| project_nodes(&prog.nodes, st, sw));
            let cx = sw.includes.as_mut().expect("checked by the caller");
            debug_assert_eq!(cx.stack.last(), Some(&id), "include stack is a stack");
            cx.stack.pop();
            out
        }
        WalkDirective::Incbin {
            ref request,
            offset,
            size,
        } => {
            let payload = {
                let cx = sw.includes.as_mut().expect("checked by the caller");
                ca65_flat::incbin_payload(
                    request,
                    offset,
                    size,
                    &at,
                    cx.map,
                    cx.loader,
                    &cx.stack,
                    &ca65_flat::CA65_SEMANTICS,
                )?
            };
            project_one(
                &crate::ast::Node {
                    operand_span: d.operand_span,
                    label: d.label,
                    item: Some(crate::ast::Item::Binary(payload)),
                    source: d.source,
                    span,
                    trivia: d.trivia,
                },
                st,
            )
        }
    }
}

/// Fold a `.if` / `.ifdef` / `.ifndef` / `.elseif` head.
fn fold_condition(head: &str, st: &Projection, line: usize) -> Result<bool, AsmError> {
    let (word, args) = split_first_word(head.trim());
    let word = word.to_ascii_lowercase();
    let args = args.trim();
    let env = st.env();
    match word.as_str() {
        ".ifdef" | ".ifndef" => {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            // An active text symbol was substituted into the argument before
            // this fold, so a numeric value lands here — which ca65 answers
            // `Identifier expected`, not with a membership test.
            if !is_ident(name) {
                return Err(AsmError::new(
                    line,
                    format!("`{word}` needs an identifier, got `{name}`"),
                ));
            }
            let defined = env.contains_key(name) || st.label_seg.contains_key(name);
            Ok(if word == ".ifdef" { defined } else { !defined })
        }
        // `.ifblank` asks whether anything follows it on the line — which is
        // how a macro tests an argument it may not have been given, the
        // expansion being textual. ca65 counts tokens, not characters, so
        // whitespace alone is still blank.
        ".ifblank" | ".ifnblank" => Ok((word == ".ifblank") == args.is_empty()),
        // The CPU tests. This leg is a 6502 and refuses `.setcpu`, so no
        // reachable source can make one of the others true.
        // `.ifref` asks the same question `.ref` does, from the same set.
        ".ifref" | ".ifnref" => {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            Ok((word == ".ifref") == st.referenced.contains(name))
        }
        ".ifp02" => Ok(true),
        ".ifp4510" | ".ifp816" | ".ifpc02" | ".ifpsc02" => Ok(false),
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
                return Err(AsmError::new(line, format!("`{word}` needs an expression")));
            }
            let expr = parse_value(&AnonCtx::default(), "", args, line)?;
            let expr = map_expr_syms(&expr, &resolve_defined(&st.consts, &st.label_seg));
            Ok((word == ".ifconst") == weighs_nothing(&expr, st))
        }
        ".if" | ".elseif" => {
            if args.is_empty() {
                return Err(AsmError::new(line, format!("`{word}` needs a condition")));
            }
            let expr = parse_value(&AnonCtx::default(), "", args, line)?;
            // `.if .defined(X)` — answered against what stands above this line,
            // which is what makes `.defined` positional in the first place.
            let expr = map_expr_syms(&expr, &resolve_defined(&st.consts, &st.label_seg));
            // ca65: `Constant expression expected` — a condition may not reach
            // forward, and a ca65 label is never constant.
            let value = fold_const(&expr, &env, line).map_err(|_| {
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
        )),
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
    let expr = parse_value(&AnonCtx::default(), "", count_text, line)?;
    let count = fold_const(&expr, &st.env(), line)?;
    // ca65: a negative count is `Range error`, where zero is no iterations.
    if count < 0 {
        return Err(AsmError::new(
            line,
            format!("`.repeat {count}`: a repetition count may not be negative"),
        ));
    }
    Ok((count, var.filter(|v| !v.is_empty())))
}

/// Substitute a `.repeat`'s loop variables into an expression.
///
/// The value has to be **baked in**, not left as a symbol, for the same reason
/// acme's `!for` variable is: ca65 resolves `Expr::Sym` once, in a later pass,
/// against one table — and a loop variable holds a different value on every
/// iteration, so there is no single entry that table could hold.
fn map_expr_syms(e: &Expr, f: &dyn Fn(&str) -> Option<Expr>) -> Expr {
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

/// Substitute the enclosing `.repeat` loop variables. Innermost wins, so a
/// nested `.repeat` may shadow an outer name.
fn loop_var_sub(vars: &[(String, i64)]) -> impl Fn(&str) -> Option<Expr> + '_ {
    move |name| {
        vars.iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| Expr::Num(*v))
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
        Kind::Insn { operand, mnemonic } => Kind::Insn {
            operand: map_operand_syms(operand, f),
            mnemonic: mnemonic.clone(),
        },
        Kind::Empty => Kind::Empty,
        Kind::Res(n, f) => Kind::Res(*n, *f),
        Kind::Constant(name, value) => Kind::Constant(name.clone(), *value),
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

/// Project one ordinary node — the body of the original projection loop.
fn project_one(node: &crate::ast::Node, st: &mut Projection) -> Result<(), AsmError> {
    use crate::ast::{Item, Operand};
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
                if let Some(sym) = node.label.as_ref()
                    && let Ok(v) = fold_const(value, consts, line)
                {
                    consts.insert(sym.qualified.clone(), v);
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
                let kind = map_kind_syms(kind, &resolve_defined(consts, label_seg));
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
                }
                // `.export foo := 7` defines `foo` as well as exporting it, so
                // it is collected here with the `=` constants rather than left
                // to the visibility check, which would then find it undefined.
                if let Kind::Visible {
                    names,
                    define: Some(value),
                    ..
                } = &kind
                    && let Some(name) = names.first()
                    && let Ok(v) = fold_const(value, consts, line)
                {
                    consts.insert(name.clone(), v);
                }
                let label = node.label.as_ref().map(|s| s.qualified.clone());
                if let Some(l) = &label {
                    label_seg.insert(l.clone(), seg.clone());
                }
                stmts.push(Stmt {
                    line,
                    file,
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
                        SegSwitch::To(name) => *seg = name,
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
