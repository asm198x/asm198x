//! The source-preserving semantic AST — plan units **U2** (types) and **U3**
//! (the Z80 front-end lowers into it), see
//! `docs/plans/2026-07-04-005-feat-ir-ast-layer-plan.md`.
//!
//! A layer *above* the encoder: a dialect parses source into a [`Program`], and
//! [`lower`] turns it into today's [`Statement`](crate::engine::Statement) /
//! [`Operation`](crate::engine::Operation) stream, which the existing two-pass
//! driver assembles unchanged (KTD1 — the `isa`/encoding layer is untouched,
//! output bytes are identical). U3 routes the Z80 dialects through this and U6
//! begins migrating the flat-engine CPUs (the Intel 8080 first — the first
//! fixed-slot CPU); the rest stay on direct lowering behind the dialect
//! boundary (KTD6) until migrated.
//!
//! **Neutrality is one tree *type* both dialects lower into, not identical
//! streams** (the U1 gate finding). Where dialects diverge *semantically* —
//! pasmo-vs-sjasmplus local-label scope — each dialect resolves its own meaning
//! into this shared type at parse: a leading-`.` label becomes a [`Scope::Local`]
//! [`Symbol`] under sjasmplus and a [`Scope::Global`] one under pasmo. Per-dialect
//! *policy* (oversize, `addr_unit`) stays a `Dialect` attribute the driver
//! applies in pass 2, not tree content. So the tree needs no escape hatch.
//!
//! The only code here ahead of its consumer is the computed-operand path
//! ([`StructuredOperand`], [`AutoIndex`], [`Operand::Structured`], and the
//! per-operand `source` slot), reserved for U6's field-packed CPUs — each
//! carries a scoped `allow(dead_code)`, so the rest of the module keeps normal
//! dead-code detection (CI is `-D warnings`).

use crate::engine::{AsmError, Expr, Operation, Piece, Statement};

// ---------------------------------------------------------------------------
// Provenance (R3)
// ---------------------------------------------------------------------------

// The span/source model is the shared [`crate::span`] module — one span type
// across the AST, the engine's `AsmError`, and the public `Diagnostic`
// (`decisions/roadmap-sequencing.md` § the span/source seam). Re-exported here so
// the AST's long-standing `crate::ast::Span` / `Span::at` references keep
// resolving unchanged.
pub(crate) use crate::span::Span;

// ---------------------------------------------------------------------------
// Trivia — comments carried, not stripped (R5, KTD5). Populated in U4.
// ---------------------------------------------------------------------------

/// A source comment, kept as trivia so emit can reproduce it (R5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Comment {
    pub(crate) text: String,
    pub(crate) span: Span,
}

/// Comments attached to a node: own-line comments *before* it, and a same-line
/// comment *after* it. Blank-line and mid-expression comments are out of the v1
/// fidelity floor (KTD5). Empty until U4 stops the parser stripping comments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Trivia {
    pub(crate) leading: Vec<Comment>,
    pub(crate) trailing: Option<Comment>,
}

// ---------------------------------------------------------------------------
// Symbols with scope (R4)
// ---------------------------------------------------------------------------

/// A symbol's scope, resolved by the dialect at parse. A local label reused
/// under two globals becomes two distinct symbols — the pasmo-vs-sjasmplus
/// divergence the U1 spike proved lives here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// A plain global label — and how pasmo treats a leading-`.` name.
    Global,
    /// A local label, qualified by its enclosing global — how sjasmplus (and
    /// the existing `scopes_locals` mechanism) treats a leading-`.` name.
    Local { in_global: String },
}

/// A label definition carrying its **source** name, its resolved [`Scope`], and
/// the **qualified** name lowering emits (`first.loop` for a local, the plain
/// name for a global). Two same-named locals in different scopes are distinct
/// symbols (R4); keeping the source name lets emit reproduce `.loop` (U5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub(crate) name: String,
    pub(crate) scope: Scope,
    pub(crate) qualified: String,
}

// ---------------------------------------------------------------------------
// Operands — fixed-slot Expr, or an abstract structured operand (KTD5, U1 axis 2)
// ---------------------------------------------------------------------------

/// An auto-increment / -decrement marker on an index register (6809 and kin).
#[allow(dead_code)] // reserved for U6's computed-operand CPUs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoIndex {
    None,
    Inc1,
    Inc2,
    Dec1,
    Dec2,
}

/// An abstract addressing-mode operand for the computed / field-packed CPUs
/// (6809 indexed, PDP-11, …). It keeps the *source structure* — register, auto
/// marker, indirection, offset — rather than pre-computed encoding bytes, so it
/// round-trips to source and lowers to [`Piece`](crate::engine::Piece)s in U6.
/// The U1 spike proved this shape suffices for the 6809 with no escape hatch.
#[allow(dead_code)] // reserved for U6's computed-operand CPUs
#[derive(Clone, Debug)]
pub(crate) struct StructuredOperand {
    pub(crate) reg: String,
    pub(crate) auto: AutoIndex,
    pub(crate) indirect: bool,
    pub(crate) offset: Option<Expr>,
}

/// One instruction or directive operand.
#[allow(dead_code)] // the `source` slot and `Structured` variant are reserved for U6
#[derive(Clone, Debug)]
pub(crate) enum Operand {
    /// A fixed-slot operand: the value plus a slot for its **source token text**
    /// (`$0A`, `10`, `%1010` all evaluate to 10 but re-emit distinctly — KTD5).
    /// U5's formatter round-trips spelling via the whole-line
    /// [`Node::source`](Node), so this per-operand slot is **reserved for U6+**
    /// (per-operand structural emit — the converter, refactoring) and is empty
    /// today. It is populated when a consumer needs operand-level, not
    /// line-level, source.
    Expr { value: Expr, source: String },
    /// A computed / field-packed operand (see [`StructuredOperand`]); the 6809
    /// and kin use this from U6.
    Structured(StructuredOperand),
}

impl Operand {
    /// A fixed-slot operand with no captured per-operand source text (the
    /// default; the formatter round-trips spelling via the whole-line
    /// `Node::source`, so operand-level source stays empty until U6+ needs it).
    fn expr(value: Expr) -> Self {
        Operand::Expr {
            value,
            source: String::new(),
        }
    }

    /// The value of a fixed-slot operand. A structured operand cannot lower to a
    /// single fixed-slot value; it should have been carried as an
    /// [`Item::Encoded`] instead, so reaching here is an internal error, not a
    /// panic (a computed-operand dialect wraps its pre-computed pieces).
    fn into_value(self) -> Result<Expr, AsmError> {
        match self {
            Operand::Expr { value, .. } => Ok(value),
            Operand::Structured(_) => Err(AsmError::new(
                0,
                "internal error: a structured operand cannot lower to a fixed-slot value",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Items and the program
// ---------------------------------------------------------------------------

/// One operation on a source line, before byte-lowering. The variants mirror the
/// [`Operation`](crate::engine::Operation) kinds a dialect produces. Instructions
/// carry structured [`Operand`]s (source text + structured variants), though the
/// fixed-slot dialects populate only the value.
// No `Clone`/`Debug` derive: `Item::Encoded` carries `Vec<Piece>`, and `Piece`
// (an engine encoding type) derives neither — keeping the encoder untouched
// (KTD1). Nothing clones or debug-prints the tree, so the bounds aren't needed.
pub(crate) enum Item {
    Instruction {
        mnemonic: String,
        mode: &'static str,
        operands: Vec<Operand>,
    },
    Org(Operand),
    Equ(Operand),
    Bytes(Vec<Operand>),
    Words(Vec<Operand>),
    Entry(Operand),
    /// A dialect-computed instruction encoding (a 6809 postbyte + extension,
    /// a field-packed word, …), carried verbatim so a computed-operand CPU
    /// (U6) round-trips byte-identical; the formatter re-emits it via
    /// [`Node::source`](Node).
    Encoded(Vec<Piece>),
    /// ACME's `!align` — a PC-dependent pad the engine resolves (U6).
    Align {
        andmask: i64,
        value: i64,
        fill: u8,
    },
    /// A conditional-assembly block (ACME `!if`/`!ifdef`/`!ifndef` … `{ … }` …
    /// `else { … }`), kept as **tree structure** so the formatter reflects the
    /// block shape and idea 4's evaluator can prune branches. `head` is the
    /// verbatim directive + condition (`!if DEBUG = 1`, `!ifndef FOO`);
    /// `then_body`/`else_body` are the nested nodes; `inline` records that the
    /// source wrote the whole block on one line (the idiomatic
    /// `!ifndef X { X = 0 }` guard), which the formatter preserves.
    ///
    /// ACME assembles by **evaluating** this tree (`dialects::acme::evaluate`
    /// prunes the untaken branch and threads `env`), not through the generic
    /// [`lower`] — so `lower` rejects a conditional. No dialect routes a
    /// conditional through `lower`, so the rejection never fires; it guards
    /// against a future one doing so by mistake.
    Conditional {
        head: String,
        then_body: Vec<Node>,
        else_body: Option<Vec<Node>>,
        inline: bool,
    },
}

/// A source line reduced to an optional (scoped) label and an optional
/// operation, with provenance and attached comment trivia. Mirrors
/// [`Statement`](crate::engine::Statement), enriched.
pub(crate) struct Node {
    pub(crate) label: Option<Symbol>,
    pub(crate) item: Option<Item>,
    /// The operation's raw source text (comment-stripped, trimmed), kept
    /// verbatim so the formatter (U5) round-trips operand spelling and
    /// source-form local names. Empty for a label-only line.
    pub(crate) source: String,
    pub(crate) span: Span,
    pub(crate) trivia: Trivia,
}

/// A parsed translation unit: the ordered nodes.
#[derive(Default)]
pub(crate) struct Program {
    pub(crate) nodes: Vec<Node>,
}

// ---------------------------------------------------------------------------
// Conditional evaluation — the shared framework (idea 4, generalised)
// ---------------------------------------------------------------------------

/// A dialect's conditional-assembly semantics, for the shared [`evaluate`] walk.
/// The reusable part is the tree walk (prune the untaken branch, thread the
/// live/skipped flag); the two dialect-specific parts are **evaluating a
/// condition** and **lowering one content line** (which also carries the
/// dialect's environment — the `equ`/`=`/`!set` bindings a later condition tests).
///
/// A dialect gains conditionals by implementing this over an evaluator that owns
/// its environment, then calling [`evaluate`]; the ACME (brace) and — when a
/// keyword dialect adopts them — `IF … ENDIF` styles share this one walk.
pub(crate) trait CondEval {
    /// Evaluate a conditional head (`!if DEBUG = 1`, `IF DEBUG`, `IFDEF FOO`)
    /// against the current environment: `true` if the then-branch is taken.
    /// `line` is the head's source line, for diagnostics.
    ///
    /// # Errors
    /// A malformed condition or an undefined symbol required to fold it.
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError>;

    /// Lower one content node (an instruction, directive, or `equ`/`=`/`!set`)
    /// to zero or more statements, updating the environment so a later condition
    /// sees the binding. Only ever called for a **live** node.
    ///
    /// # Errors
    /// Any per-line parse, range, or fold failure.
    fn lower(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError>;
}

/// Assemble by evaluating the conditional tree: prune the untaken branch of each
/// [`Item::Conditional`] and lower every live content line through `dialect`. A
/// skipped branch is walked with `emit = false` so it defines nothing — the rule
/// ACME's old `process_block` used, now shared by any conditional dialect (idea
/// 4). Blank/comment nodes (no label, no source) carry nothing and are skipped.
pub(crate) fn evaluate<D: CondEval>(
    dialect: &mut D,
    nodes: &[Node],
    emit: bool,
    out: &mut Vec<Statement>,
) -> Result<(), AsmError> {
    for node in nodes {
        if let Some(Item::Conditional {
            head,
            then_body,
            else_body,
            ..
        }) = &node.item
        {
            let taken = if emit {
                dialect.eval(head, node.span.line)?
            } else {
                false
            };
            evaluate(dialect, then_body, emit && taken, out)?;
            if let Some(else_body) = else_body {
                evaluate(dialect, else_body, emit && !taken, out)?;
            }
        } else if emit && (node.label.is_some() || !node.source.is_empty()) {
            dialect.lower(node, out)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Lowering: AST -> the engine's Statement/Operation stream (U3, KTD1)
// ---------------------------------------------------------------------------

/// Lower an AST to the engine's statement stream. Label definitions emit their
/// **qualified** name (so scope resolves exactly as the old string-mangle did),
/// operations lower to their [`Operation`], and the driver assembles the result
/// unchanged — byte-identical to direct parsing (AE1).
pub(crate) fn lower(program: Program) -> Result<Vec<Statement>, AsmError> {
    program
        .nodes
        .into_iter()
        .map(|node| {
            Ok(Statement {
                line: node.span.line as usize,
                label: node.label.map(|s| s.qualified),
                op: node.item.map(lower_item).transpose()?,
            })
        })
        .collect()
}

fn lower_item(item: Item) -> Result<Operation, AsmError> {
    Ok(match item {
        Item::Instruction {
            mnemonic,
            mode,
            operands,
        } => Operation::Instruction {
            mnemonic,
            mode,
            operands: operands
                .into_iter()
                .map(Operand::into_value)
                .collect::<Result<_, _>>()?,
        },
        Item::Org(o) => Operation::Org(o.into_value()?),
        Item::Equ(o) => Operation::Equ(o.into_value()?),
        Item::Bytes(v) => Operation::Bytes(
            v.into_iter()
                .map(Operand::into_value)
                .collect::<Result<_, _>>()?,
        ),
        Item::Words(v) => Operation::Words(
            v.into_iter()
                .map(Operand::into_value)
                .collect::<Result<_, _>>()?,
        ),
        Item::Entry(o) => Operation::Entry(o.into_value()?),
        Item::Encoded(pieces) => Operation::Encoded(pieces),
        Item::Align {
            andmask,
            value,
            fill,
        } => Operation::Align {
            andmask,
            value,
            fill,
        },
        // No dialect lowers a conditional through the generic path — ACME
        // evaluates the tree in `dialects::acme::evaluate` — so this is
        // unreachable in practice; it guards against a mis-routed future dialect.
        Item::Conditional { .. } => {
            return Err(AsmError::new(
                0,
                "internal error: a conditional block is evaluated by the dialect, not lowered",
            ));
        }
    })
}

/// Build an [`Item`] from any [`Operation`] a dialect produced — total over the
/// operation set, so a computed-operand CPU (`Encoded`) or ACME (`Align`) routes
/// through the AST without a special case.
pub(crate) fn item_from_operation(op: Operation) -> Item {
    match op {
        Operation::Instruction {
            mnemonic,
            mode,
            operands,
        } => Item::Instruction {
            mnemonic,
            mode,
            operands: operands.into_iter().map(Operand::expr).collect(),
        },
        Operation::Org(e) => Item::Org(Operand::expr(e)),
        Operation::Equ(e) => Item::Equ(Operand::expr(e)),
        Operation::Bytes(v) => Item::Bytes(v.into_iter().map(Operand::expr).collect()),
        Operation::Words(v) => Item::Words(v.into_iter().map(Operand::expr).collect()),
        Operation::Entry(e) => Item::Entry(Operand::expr(e)),
        Operation::Encoded(pieces) => Item::Encoded(pieces),
        Operation::Align {
            andmask,
            value,
            fill,
        } => Item::Align {
            andmask,
            value,
            fill,
        },
    }
}

// ---------------------------------------------------------------------------
// Emit: AST -> canonical same-dialect source — the U5 formatter (AE7, KTD5)
// ---------------------------------------------------------------------------

/// Emit an AST back to canonical source in its own dialect (`asm198x fmt`). The
/// formatter canonicalises *layout* — labels at column 0, operations indented,
/// own-line comments on their own lines, a same-line comment trailing its
/// operation — while preserving each operation's source text verbatim, so
/// operand spelling and source-form local names round-trip (KTD5). The result
/// assembles byte-identical to the original (comments never reach the encoder,
/// AE1) and re-emitting is a fixed point (idempotent, AE7).
///
/// `equ_label_colon` is the one per-dialect emit divergence (U6): a Z80 `equ`
/// label keeps a colon (`name: equ …`) so a name colliding with a mnemonic
/// re-parses as a label, but an Intel-8080 `equ` label takes **no** colon
/// (`name equ …`) — its `equ` keyword already disambiguates, and a colon would
/// fail to reassemble.
pub(crate) fn emit(program: &Program, equ_label_colon: bool) -> String {
    let mut out = String::new();
    emit_nodes(&program.nodes, &mut out, equ_label_colon, "");
    out
}

/// Emit a node list — the whole program, or one conditional block's body.
/// Bodies use the same layout rules as the top level: only a conditional's
/// `!if`/`}`/`else` delimiters sit at column 0 (ACME detects labels by column,
/// so a body label must stay at column 0 too — bodies are not deeper-indented).
/// `comment_indent` is prefixed to own-line comments so a body's comments align
/// with its indented operations (empty at the top level); blank-line markers are
/// never indented, so a blank line stays truly empty.
fn emit_nodes(nodes: &[Node], out: &mut String, equ_label_colon: bool, comment_indent: &str) {
    const INDENT: &str = "        ";
    // Per-node `=` alignment width for constant-definition runs (the ruling: the
    // formatter owns the alignment of a `name = value` table). Zero for the
    // colon form (Z80 `name: equ …`), which is not re-aligned.
    let widths = equ_run_widths(nodes, equ_label_colon);
    for (i, node) in nodes.iter().enumerate() {
        for c in &node.trivia.leading {
            if !c.text.is_empty() {
                out.push_str(comment_indent);
            }
            out.push_str(&c.text);
            out.push('\n');
        }
        // A same-line comment trails its operation with a canonical gap.
        let trailing = |out: &mut String| {
            if let Some(c) = &node.trivia.trailing {
                out.push_str("   ");
                out.push_str(&c.text);
            }
        };
        // A conditional block renders its delimiters at column 0 and recurses
        // into its bodies (the idea-4 tree shape).
        if let Some(Item::Conditional {
            head,
            then_body,
            else_body,
            inline,
        }) = &node.item
        {
            emit_conditional(
                node,
                head,
                then_body,
                else_body.as_deref(),
                *inline,
                out,
                equ_label_colon,
            );
            continue;
        }
        let label = node.label.as_ref().map(|s| s.name.as_str());
        let is_equ = matches!(node.item, Some(Item::Equ(_)));
        // A node "has an operation" if it carries an item (a real operation) or
        // just verbatim op source (an ACME directive/instruction the formatter
        // does not lower — it keeps the source and no item).
        let has_op = node.item.is_some() || !node.source.is_empty();
        match label {
            // `equ` binds its label to a value on the same line, so the label
            // stays there. The colon (Z80) forces a mnemonic-colliding name to
            // stay a label; the no-colon form (ACME `name = value`) is re-aligned
            // to its run's width so constant tables keep their columns.
            Some(name) if is_equ => {
                out.push_str(name);
                if equ_label_colon {
                    out.push_str(": ");
                } else {
                    for _ in name.len()..widths[i] {
                        out.push(' ');
                    }
                    out.push(' ');
                }
                out.push_str(&node.source);
                trailing(&mut *out);
                out.push('\n');
            }
            // Label + operation: the label (bare for an anonymous `-`/`+`) on its
            // own line, the operation indented.
            Some(name) if has_op => {
                push_label(out, name);
                out.push('\n');
                out.push_str(INDENT);
                out.push_str(&node.source);
                trailing(&mut *out);
                out.push('\n');
            }
            // Label-only line.
            Some(name) => {
                push_label(out, name);
                trailing(&mut *out);
                out.push('\n');
            }
            // Operation with no label — an ACME directive preserved verbatim, or
            // a rgbasm no-address `SECTION`; indented like any operation.
            None if !node.source.is_empty() => {
                out.push_str(INDENT);
                out.push_str(&node.source);
                trailing(&mut *out);
                out.push('\n');
            }
            // No label, no operation — a bare trailing-comment line (the EOF-flush
            // node).
            None => {
                if let Some(c) = &node.trivia.trailing {
                    out.push_str(&c.text);
                    out.push('\n');
                }
            }
        }
    }
}

/// Push a label, with a `:` unless it is an **anonymous** label (an all-`-` or
/// all-`+` run, ACME's `-`/`+`/`++` targets), which are written bare.
fn push_label(out: &mut String, name: &str) {
    out.push_str(name);
    if !is_anon_label(name) {
        out.push(':');
    }
}

/// An anonymous label — a non-empty run of all `-` or all `+` (ACME). These
/// re-emit bare (no colon); a real identifier never looks like this.
fn is_anon_label(name: &str) -> bool {
    !name.is_empty() && (name.bytes().all(|b| b == b'-') || name.bytes().all(|b| b == b'+'))
}

/// Compute, per node, the `=` alignment column for a constant-definition run —
/// one past the longest name in a maximal set of adjacent (consecutive source
/// line) `name = value` nodes. Zero for the colon form (not re-aligned) and for
/// non-constant nodes.
fn equ_run_widths(nodes: &[Node], equ_label_colon: bool) -> Vec<usize> {
    let mut widths = vec![0usize; nodes.len()];
    if equ_label_colon {
        return widths;
    }
    let is_const = |n: &Node| n.label.is_some() && matches!(n.item, Some(Item::Equ(_)));
    let mut i = 0;
    while i < nodes.len() {
        if !is_const(&nodes[i]) {
            i += 1;
            continue;
        }
        // Extend the run while the next node is also a constant on the very next
        // source line (a blank line or a comment breaks the run — the ruling).
        let mut j = i + 1;
        while j < nodes.len()
            && is_const(&nodes[j])
            && nodes[j].span.line == nodes[j - 1].span.line + 1
        {
            j += 1;
        }
        let w = (i..j)
            .map(|k| nodes[k].label.as_ref().map_or(0, |s| s.name.len()))
            .max()
            .unwrap_or(0);
        for width in &mut widths[i..j] {
            *width = w;
        }
        i = j;
    }
    widths
}

/// Emit one conditional block. Delimiters (`!if … {`, `} else {`, `}`) sit at
/// column 0; each body formats with the normal rules. The idiomatic one-line
/// guard (`!ifndef X { X = 0 }`) is preserved when the source wrote it inline
/// and the body is a single simple node — expanding it would be less idiomatic.
fn emit_conditional(
    node: &Node,
    head: &str,
    then_body: &[Node],
    else_body: Option<&[Node]>,
    inline: bool,
    out: &mut String,
    equ_label_colon: bool,
) {
    let trailing = |out: &mut String| {
        if let Some(c) = &node.trivia.trailing {
            out.push_str("   ");
            out.push_str(&c.text);
        }
    };
    if inline
        && else_body.is_none()
        && then_body.len() == 1
        && let Some(body) = inline_render(&then_body[0])
    {
        out.push_str(head);
        out.push_str(" { ");
        out.push_str(&body);
        out.push_str(" }");
        trailing(&mut *out);
        out.push('\n');
        return;
    }
    out.push_str(head);
    out.push_str(" {");
    trailing(&mut *out);
    out.push('\n');
    // Body comments align with the body's col-8 operations.
    const BODY: &str = "        ";
    emit_nodes(then_body, out, equ_label_colon, BODY);
    if let Some(else_body) = else_body {
        out.push_str("} else {\n");
        emit_nodes(else_body, out, equ_label_colon, BODY);
    }
    out.push_str("}\n");
}

/// Render a single node inline (`X = 0`, `nop`) for the one-line guard idiom.
/// `None` when it can't be safely inlined — a nested block, or attached comments
/// a one-liner would drop.
fn inline_render(node: &Node) -> Option<String> {
    if matches!(node.item, Some(Item::Conditional { .. }))
        || !node.trivia.leading.is_empty()
        || node.trivia.trailing.is_some()
    {
        return None;
    }
    let label = node.label.as_ref().map(|s| s.name.as_str());
    Some(match (label, node.source.as_str()) {
        (Some(name), "") => name.to_string(),
        (Some(name), src) => format!("{name} {src}"),
        (None, src) => src.to_string(),
    })
}

// ===========================================================================
// Model tests — construct ASTs by hand and prove the type + lowering support
// each requirement. (U3 adds full-program byte-identity tests via the Z80
// front-end; see `ast_lowering_tests`.)
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// AE3 (R5) — a comment is carried as queryable trivia, not dropped.
    #[test]
    fn comment_is_queryable_trivia() {
        let node = Node {
            label: None,
            item: Some(Item::Instruction {
                mnemonic: "nop".into(),
                mode: "implied",
                operands: vec![],
            }),
            source: "nop".into(),
            span: Span::at(2, 5),
            trivia: Trivia {
                leading: vec![Comment {
                    text: "set up the loop".into(),
                    span: Span::at(1, 1),
                }],
                trailing: Some(Comment {
                    text: "no-op".into(),
                    span: Span::at(2, 12),
                }),
            },
        };
        assert_eq!(node.trivia.leading.len(), 1);
        assert_eq!(node.trivia.leading[0].text, "set up the loop");
        assert_eq!(
            node.trivia.trailing.as_ref().expect("has trailing").text,
            "no-op"
        );
    }

    /// AE4 (R4) — a local label reused in two scopes is two distinct symbols,
    /// with distinct qualified names but the same source name.
    #[test]
    fn reused_local_is_two_distinct_scoped_symbols() {
        let first = Symbol {
            name: ".loop".into(),
            scope: Scope::Local {
                in_global: "first".into(),
            },
            qualified: "first.loop".into(),
        };
        let second = Symbol {
            name: ".loop".into(),
            scope: Scope::Local {
                in_global: "second".into(),
            },
            qualified: "second.loop".into(),
        };
        assert_ne!(
            first, second,
            "same source name, different scope -> distinct"
        );
        assert_eq!(first.name, second.name);
        assert_ne!(first.qualified, second.qualified);
    }

    /// AE2 (R3) — every node carries `(file, line, column)`, with the
    /// expansion-frame stack present and empty in v1.
    #[test]
    fn span_carries_file_line_column() {
        let s = Span::at(7, 9);
        assert_eq!(s.file, crate::span::FileId(0));
        assert_eq!(s.line, 7);
        assert_eq!(s.col, 9);
        assert!(s.expansion_frames.is_empty(), "reserved, empty in v1");
    }

    /// KTD5 — an operand round-trips its source token text.
    #[test]
    fn operand_preserves_source_spelling() {
        for src in ["$0A", "10", "%1010"] {
            let op = Operand::Expr {
                value: Expr::Num(10),
                source: src.to_string(),
            };
            let Operand::Expr { source, .. } = &op else {
                panic!("expected an Expr operand");
            };
            assert_eq!(source, src, "source spelling preserved");
        }
    }

    /// U1 axis 2 (KTD5) — the structured-operand variant models a 6809 indexed
    /// operand's source structure (register + auto + indirect + offset).
    #[test]
    fn structured_operand_models_6809_indexed() {
        let op = Operand::Structured(StructuredOperand {
            reg: "x".into(),
            auto: AutoIndex::Inc1,
            indirect: false,
            offset: None,
        });
        let Operand::Structured(s) = &op else {
            panic!("expected a structured operand");
        };
        assert_eq!(s.reg, "x");
        assert_eq!(s.auto, AutoIndex::Inc1);
        assert!(!s.indirect);
    }

    /// AE6 — pasmo and sjasmplus lower an equivalent program to identical bytes
    /// (they share the AST lowering; full structural equality is proven by the
    /// reference differential in `tests/conformance.rs`).
    #[test]
    fn pasmo_and_sjasmplus_share_the_lowering() {
        let src = "  ld a, 5\n  ld b, a\n  ret\n";
        let p = crate::assemble_pasmo(src).expect("assembles");
        let s = crate::assemble_sjasmplus(src).expect("assembles");
        assert_eq!(p.bytes, s.bytes);
    }

    /// U3 regression — sjasmplus scoped locals still resolve through the AST
    /// refactor (two `.loop`s in two scopes assemble), and pasmo still rejects a
    /// reused `.loop` (its non-scoping meaning is preserved).
    #[test]
    fn scoped_locals_survive_the_ast_refactor() {
        let src = "\
first:
  ld b, 2
.loop:
  djnz .loop
second:
  ld b, 3
.loop:
  djnz .loop
";
        assert!(crate::assemble_sjasmplus(src).is_ok(), "sjasmplus scopes");
        assert!(
            crate::assemble_pasmo(src).is_err(),
            "pasmo still rejects reused `.loop`"
        );
    }

    /// KTD6 seam — a non-migrated CPU (6502 via acme) still assembles through
    /// the direct-lowering path; it never touches the AST.
    #[test]
    fn seam_leaves_other_cpus_on_direct_lowering() {
        let src = "*=$c000\n  lda #$05\n  rts\n";
        assert!(crate::assemble_acme(src).is_ok(), "6502 direct path intact");
    }

    /// Lowering round-trips an Operation through an Item byte-for-byte (the
    /// mechanism U3 relies on for AE1).
    #[test]
    fn item_round_trips_an_instruction() {
        let node = Node {
            label: Some(Symbol {
                name: "start".into(),
                scope: Scope::Global,
                qualified: "start".into(),
            }),
            item: Some(item_from_operation(Operation::Instruction {
                mnemonic: "ld".into(),
                mode: "a,n",
                operands: vec![Expr::Num(0x42)],
            })),
            source: "ld a,$42".into(),
            span: Span::at(1, 1),
            trivia: Trivia::default(),
        };
        let statements = lower(Program { nodes: vec![node] }).expect("lowers");
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].label.as_deref(), Some("start"));
        match &statements[0].op {
            Some(Operation::Instruction {
                mnemonic,
                mode,
                operands,
            }) => {
                assert_eq!(mnemonic, "ld");
                assert_eq!(*mode, "a,n");
                assert!(matches!(operands.as_slice(), [Expr::Num(0x42)]));
            }
            _ => panic!("expected an instruction"),
        }
    }

    /// U6 — lowering is total over the operation set: a computed-operand
    /// `Encoded` and an ACME `Align` round-trip through an `Item` without a
    /// panic (the paths that were `unreachable!` before U6).
    #[test]
    fn encoded_and_align_round_trip() {
        let encoded =
            item_from_operation(Operation::Encoded(vec![Piece::Lit(0xA6), Piece::Lit(5)]));
        assert!(matches!(
            lower_item(encoded).expect("lowers"),
            Operation::Encoded(_)
        ));

        let align = item_from_operation(Operation::Align {
            andmask: 0xFF,
            value: 0,
            fill: 0,
        });
        assert!(matches!(
            lower_item(align).expect("lowers"),
            Operation::Align { andmask: 0xFF, .. }
        ));
    }

    /// Idea 4 promotion — a conditional block renders its delimiters at column 0
    /// with the body formatted normally, and the one-line guard idiom is kept on
    /// one line.
    #[test]
    fn conditional_block_emits_with_delimiters_at_column_zero() {
        // A multi-line `!if` with a body op.
        let body = Node {
            label: None,
            item: Some(Item::Instruction {
                mnemonic: "jsr".into(),
                mode: "abs",
                operands: vec![],
            }),
            source: "jsr show_menu".into(),
            span: Span::at(2, 1),
            trivia: Trivia::default(),
        };
        let cond = Node {
            label: None,
            item: Some(Item::Conditional {
                head: "!if DEBUG = 1".into(),
                then_body: vec![body],
                else_body: None,
                inline: false,
            }),
            source: String::new(),
            span: Span::at(1, 1),
            trivia: Trivia::default(),
        };
        let out = emit(&Program { nodes: vec![cond] }, false);
        assert_eq!(out, "!if DEBUG = 1 {\n        jsr show_menu\n}\n");

        // The inline guard idiom stays on one line.
        let guard_body = Node {
            label: Some(Symbol {
                name: "FOO".into(),
                scope: Scope::Global,
                qualified: "FOO".into(),
            }),
            item: Some(Item::Equ(Operand::expr(Expr::Num(0)))),
            source: "= 0".into(),
            span: Span::at(1, 1),
            trivia: Trivia::default(),
        };
        let guard = Node {
            label: None,
            item: Some(Item::Conditional {
                head: "!ifndef FOO".into(),
                then_body: vec![guard_body],
                else_body: None,
                inline: true,
            }),
            source: String::new(),
            span: Span::at(1, 1),
            trivia: Trivia::default(),
        };
        assert_eq!(
            emit(&Program { nodes: vec![guard] }, false),
            "!ifndef FOO { FOO = 0 }\n"
        );
    }

    /// A conditional block is formatter-only for now: lowering rejects it (ACME
    /// assembles through its preprocessor). Never hit in practice — no lowering
    /// path constructs one.
    #[test]
    fn conditional_block_rejects_lowering() {
        let cond = Item::Conditional {
            head: "!if X = 1".into(),
            then_body: vec![],
            else_body: None,
            inline: false,
        };
        assert!(lower_item(cond).is_err(), "formatter-only, not lowerable");
    }

    /// A structured operand cannot lower to a fixed-slot value — it returns an
    /// internal error rather than panicking (the U6 graceful-failure path).
    #[test]
    fn structured_operand_in_instruction_errors_not_panics() {
        let bad = Item::Instruction {
            mnemonic: "x".into(),
            mode: "",
            operands: vec![Operand::Structured(StructuredOperand {
                reg: "x".into(),
                auto: AutoIndex::None,
                indirect: false,
                offset: None,
            })],
        };
        assert!(lower_item(bad).is_err(), "graceful error, not a panic");
    }
}
