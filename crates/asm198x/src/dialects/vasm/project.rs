//! The projection sweep (#486's legibility split on the phase seam): the
//! layer of `decisions/conditionals-in-multipass-dialects.md` that folds each
//! conditional and repetition once, in source order, before layout, and
//! projects the semantic tree into the multi-pass driver's statement stream.
//! It is its own file so a change to how a branch folds is found without
//! opening the parse walk or the 68000 encoder. Moved verbatim from the
//! parent; the seam is the boundary, not a rewrite.

use std::collections::BTreeMap;

use super::{
    AsmError, Expr, Line, Stmt, bake_reptn, eval, parse_value, split_first_word, split_operands,
};

/// Project the semantic [`Program`](crate::ast::Program) into the assembler's
/// statement stream — the multi-pass driver runs on an owned `Vec<Line>`. Each
/// node's qualified label and native [`Stmt`] payload (built and qualified in
/// [`parse_program`]) are read straight back out of the tree; nothing is
/// re-parsed. A resolved `incbin` payload (the multi-file walk's lowering)
/// becomes [`Stmt::Raw`]; a label-only node becomes an empty statement
/// carrying its label; the comment-only flush node (no label, no item)
/// carries no statement.
///
/// # Errors
/// An **unresolved** `include`/`incbin` cannot assemble: it needs a loader,
/// which only the multi-file entry has (U6, KTD1). The single-source API
/// keeps meaning "one file, no includes" — with a pointer, not the old
/// unknown-directive rejection.
pub(super) fn lines_from_program(program: &crate::ast::Program) -> Result<Vec<Line>, AsmError> {
    let mut out = Vec::new();
    let mut env = BTreeMap::new();
    // `REPTN` reads -1 outside any repetition, which is vasm's own answer and
    // not an absence — `dc.b REPTN` at the top level emits $FF.
    let mut reptn: Vec<i64> = Vec::new();
    project_lines(&program.nodes, &mut out, &mut env, &mut reptn)?;
    Ok(out)
}

/// vasm's `REPTN`: the innermost repetition's 0-based counter.
pub(super) const REPTN: &str = "REPTN";

/// Project a run of nodes, folding conditionals and repetitions **once, in
/// source order, before layout** — `decisions/conditionals-in-multipass-dialects.md`.
fn project_lines(
    nodes: &[crate::ast::Node],
    out: &mut Vec<Line>,
    env: &mut BTreeMap<String, i64>,
    reptn: &mut Vec<i64>,
) -> Result<bool, AsmError> {
    use crate::ast::Item;
    for node in nodes {
        match &node.item {
            Some(Item::Conditional {
                head,
                then_body,
                else_body,
                ..
            }) => {
                if fold_vasm_condition(head, env, reptn, node.span.line as usize)? {
                    if project_lines(then_body, out, env, reptn)? {
                        return Ok(true);
                    }
                } else if let Some(body) = else_body
                    && project_lines(body, out, env, reptn)?
                {
                    return Ok(true);
                }
                continue;
            }
            Some(Item::Repeat { head, body, .. }) => {
                let line = node.span.line as usize;
                let (_, args) = split_first_word(head.trim());
                let count = eval(
                    &parse_value(args.trim(), line)?,
                    &reptn_env(env, reptn),
                    0,
                    line,
                )?;
                // vasm runs a negative count zero times rather than refusing it,
                // where ca65 answers `Range error`. Measured, not assumed.
                for i in 0..count.max(0) {
                    reptn.push(i);
                    let done = project_lines(body, out, env, reptn);
                    reptn.pop();
                    if done? {
                        return Ok(true);
                    }
                }
                continue;
            }
            _ => {}
        }
        if matches!(
            node.item.as_ref(),
            Some(Item::Native(n)) if matches!(n.as_any().downcast_ref::<Stmt>(), Some(Stmt::End))
        ) {
            return Ok(true);
        }
        project_one_line(node, out, env, reptn)?;
    }
    Ok(false)
}

/// The environment a fold sees: the `equ` constants plus `REPTN`, which is the
/// innermost repetition's counter, or -1 outside every one.
fn reptn_env(env: &BTreeMap<String, i64>, reptn: &[i64]) -> BTreeMap<String, i64> {
    let mut e = env.clone();
    e.insert(REPTN.to_string(), reptn.last().copied().unwrap_or(-1));
    e
}

/// Fold one conditional head.
fn fold_vasm_condition(
    head: &str,
    env: &BTreeMap<String, i64>,
    reptn: &[i64],
    line: usize,
) -> Result<bool, AsmError> {
    let (word, args) = split_first_word(head.trim());
    let word = word.to_ascii_lowercase();
    let args = args.trim();
    let env = reptn_env(env, reptn);
    if word == "ifd" || word == "ifnd" {
        let name = args
            .split_whitespace()
            .next()
            .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
        let defined = env.contains_key(name);
        return Ok(if word == "ifd" { defined } else { !defined });
    }
    // `ifb`/`ifnb` ask whether anything follows, which is how a macro tests an
    // argument it may not have been given — the expansion is textual, so an
    // omitted one leaves nothing behind.
    if word == "ifb" || word == "ifnb" {
        return Ok((word == "ifb") == args.is_empty());
    }
    // `ifc`/`ifnc` compare two pieces of *text*, quoted or not. vasm takes
    // `ifc "a","a"` and `ifc a,a` alike, and compares what is between the
    // quotes when they are there.
    if word == "ifc" || word == "ifnc" {
        let parts = split_operands(args);
        let [a, b] = parts.as_slice() else {
            return Err(AsmError::new(
                line,
                format!("`{word}` compares two pieces of text, separated by a comma"),
            ));
        };
        let unquote = |t: &str| {
            let t = t.trim();
            t.strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
                .unwrap_or(t)
                .to_string()
        };
        return Ok((word == "ifc") == (unquote(a) == unquote(b)));
    }
    if args.is_empty() {
        return Err(AsmError::new(line, format!("`{word}` needs a condition")));
    }
    let expr = parse_value(args, line)?;
    // `*` in a condition folds against the **unrelaxed** address in vasm, which
    // this projection does not compute — it runs before layout, deliberately, so
    // that a condition and the relaxation fixpoint cannot feed each other. A
    // condition that reads `*` is refused rather than folded against the wrong
    // address; see `decisions/conditionals-in-multipass-dialects.md`.
    if mentions_pc(&expr) {
        return Err(AsmError::new(
            line,
            "a condition on the location counter `*` is not supported here — vasm folds \
             one against the *unrelaxed* address, which this pass does not compute",
        ));
    }
    let value = eval(&expr, &env, 0, line).map_err(|_| {
        AsmError::new(
            line,
            format!(
                "`{args}` must be a constant here — vasm folds a condition against the \
                 values above it, and refuses a forward reference"
            ),
        )
    })?;
    Ok(match word.as_str() {
        "if" | "ifne" | "elif" => value != 0,
        "ifeq" => value == 0,
        // `ifmi`/`ifpl` read the value's sign — minus, or plus-or-zero.
        "ifmi" => value < 0,
        "ifpl" => value >= 0,
        "ifgt" => value > 0,
        "ifge" => value >= 0,
        "iflt" => value < 0,
        "ifle" => value <= 0,
        _ => {
            return Err(AsmError::new(
                line,
                format!("internal error: `{head}` is not a conditional head"),
            ));
        }
    })
}

/// Whether an expression reads the location counter.
fn mentions_pc(e: &Expr) -> bool {
    match e {
        Expr::Pc => true,
        Expr::Lo(i)
        | Expr::Hi(i)
        | Expr::Bank(i)
        | Expr::Neg(i)
        | Expr::BitNot(i)
        | Expr::LogNot(i)
        | Expr::FixRound(i)
        | Expr::TrailingZeros(i) => mentions_pc(i),
        Expr::Bin(_, l, r) => mentions_pc(l) || mentions_pc(r),
        Expr::Num(_) | Expr::Sym(_) => false,
    }
}

/// Project one ordinary node.
fn project_one_line(
    node: &crate::ast::Node,
    out: &mut Vec<Line>,
    env: &mut BTreeMap<String, i64>,
    reptn: &[i64],
) -> Result<(), AsmError> {
    use crate::ast::Item;
    {
        let label = node.label.as_ref().map(|s| s.qualified.clone());
        let kind = match &node.item {
            Some(Item::Native(n)) => n
                .as_any()
                .downcast_ref::<Stmt>()
                .expect("vasm stores a Stmt in every native node")
                .clone(),
            Some(Item::Binary(payload)) => Stmt::Raw(payload.clone()),
            Some(Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `include \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `incbin \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            None if label.is_some() => Stmt::Empty,
            // A comment-only flush node, or any other shared item (vasm
            // produces only native/binary items) — nothing to assemble.
            _ => return Ok(()),
        };
        let mut kind = kind;
        bake_reptn(&mut kind, reptn.last().copied().unwrap_or(-1));
        // Bind an `equ` as it is projected, so a later condition folds against
        // what a **taken** branch bound and nothing an untaken one held.
        if let Stmt::Equ(name, e) = &kind
            && let Ok(v) = eval(e, env, 0, node.span.line as usize)
        {
            env.insert(name.clone(), v);
        }
        out.push(Line {
            line: node.span.line as usize,
            file: node.span.file,
            frames: node.span.expansion_frames.clone(),
            label,
            kind,
        });
    }
    Ok(())
}
