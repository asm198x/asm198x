//! The source-preserving semantic AST — plan unit **U2**
//! (`docs/plans/2026-07-04-005-feat-ir-ast-layer-plan.md`).
//!
//! A new layer *above* the encoder: dialects parse source into this tree, and
//! it lowers to today's [`Statement`](crate::engine::Statement) /
//! [`Operation`](crate::engine::Operation) → bytes (U3 wires the lowering; the
//! `isa`/encoding layer is unchanged — KTD1). This module defines the node
//! types and their provenance/scope/trivia model; no dialect produces it yet,
//! so it is unconsumed until U3.
//!
//! **Neutrality is one tree *type* both dialects lower into, not identical
//! streams** (the U1 gate finding). Where dialects diverge *semantically* —
//! pasmo-vs-sjasmplus local-label scope, oversize policy — each dialect's
//! parse→AST lowering resolves its own meaning into this shared type; the tree
//! carries the *resolved* meaning (a scoped [`Symbol`], a structured
//! [`Operand`]), and per-dialect policy stays a `Dialect` attribute applied at
//! lowering. So the tree needs no per-dialect escape hatch.
#![allow(dead_code)] // the foundation lands ahead of its first consumer (U3)

use crate::engine::Expr;

// ---------------------------------------------------------------------------
// Provenance (R3)
// ---------------------------------------------------------------------------

/// Identifies a source file. v1 is single-file (`FileId(0)`); include chains
/// (idea 4) allocate further ids so a span can name the *included* file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileId(pub(crate) u32);

/// One macro-expansion frame (a rustc-style defined-at / invoked-at record).
/// The room is reserved now; idea 4's macro engine fills it. Empty in v1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExpansionFrame {
    pub(crate) macro_name: String,
    pub(crate) invoked_at: Box<Span>,
}

/// Where a node came from: `(file, line, column)` through the include chain,
/// with reserved room for macro-expansion frames (R3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) file: FileId,
    pub(crate) line: u32,
    pub(crate) col: u32,
    /// Empty in v1; populated when idea 4's macros land, without a type change.
    pub(crate) expansion_frames: Vec<ExpansionFrame>,
}

impl Span {
    /// A single-file v1 span with no expansion frames.
    pub(crate) fn at(line: u32, col: u32) -> Self {
        Span {
            file: FileId(0),
            line,
            col,
            expansion_frames: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Trivia — comments carried, not stripped (R5, KTD5)
// ---------------------------------------------------------------------------

/// A source comment, kept as trivia so emit can reproduce it (R5). The text is
/// the comment body; `span` locates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Comment {
    pub(crate) text: String,
    pub(crate) span: Span,
}

/// Comments attached to an item: own-line comments *before* it, and a same-line
/// comment *after* it. Blank-line and mid-expression comments are out of the v1
/// fidelity floor (KTD5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Trivia {
    pub(crate) leading: Vec<Comment>,
    pub(crate) trailing: Option<Comment>,
}

// ---------------------------------------------------------------------------
// Symbols with scope (R4)
// ---------------------------------------------------------------------------

/// A symbol's scope, resolved by the dialect lowering. A local label reused
/// under two globals becomes two distinct symbols (same `name`, different
/// scope) — the pasmo-vs-sjasmplus divergence the U1 spike proved lives here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// A plain (global) label — and how pasmo treats a leading-`.` name.
    Global,
    /// A local label, qualified by its enclosing global — how sjasmplus (and
    /// the existing `scopes_locals` mechanism) treats a leading-`.` name.
    Local { in_global: String },
}

/// A label definition or symbol reference carrying its resolved [`Scope`], so
/// two same-named locals in different scopes are two distinct symbols (R4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub(crate) name: String,
    pub(crate) scope: Scope,
}

// ---------------------------------------------------------------------------
// Operands — fixed-slot Expr, or an abstract structured operand (KTD5, U1 axis 2)
// ---------------------------------------------------------------------------

/// An auto-increment / -decrement marker on an index register (6809 and kin).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoIndex {
    None,
    Inc1,
    Inc2,
    Dec1,
    Dec2,
}

/// An abstract addressing-mode operand for the computed / field-packed CPUs
/// (6809 indexed, PDP-11, TMS9900, Z8000, CP1610, …). It keeps the *source
/// structure* — register, auto marker, indirection, offset — rather than the
/// pre-computed encoding bytes, so it round-trips to source and lowers to
/// [`Piece`](crate::engine::Piece)s in U6. The U1 spike proved this shape
/// suffices for the 6809 indexed operand with no escape hatch; other CPUs
/// extend it.
#[derive(Clone, Debug)]
pub(crate) struct StructuredOperand {
    pub(crate) reg: String,
    pub(crate) auto: AutoIndex,
    pub(crate) indirect: bool,
    pub(crate) offset: Option<Expr>,
}

/// One instruction operand.
#[derive(Clone, Debug)]
pub(crate) enum Operand {
    /// A fixed-slot operand: the unresolved value plus its **source token text**
    /// (`$0A`, `10`, `%1010` all evaluate to 10 but re-emit distinctly — KTD5).
    Expr { value: Expr, source: String },
    /// A computed / field-packed operand (see [`StructuredOperand`]).
    Structured(StructuredOperand),
}

// ---------------------------------------------------------------------------
// Items and the program
// ---------------------------------------------------------------------------

/// One meaningful thing on a source line, before byte-lowering.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    /// A label definition, carrying its resolved scope.
    Label(Symbol),
    /// An instruction: mnemonic, the isa mode label the dialect resolved (or
    /// `None` before resolution), and its operands.
    Instruction {
        mnemonic: String,
        mode: Option<&'static str>,
        operands: Vec<Operand>,
    },
    /// A directive (`org`, `equ`, `defb`, …) named as written, with its
    /// operands. U3's lowering maps the name to the right `Operation`.
    Directive {
        name: String,
        operands: Vec<Operand>,
    },
}

/// An item with its provenance and attached comment trivia.
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub(crate) item: Item,
    pub(crate) span: Span,
    pub(crate) trivia: Trivia,
}

/// A parsed translation unit: the ordered nodes. Lowers to `Vec<Statement>` in
/// U3 (this module defines the type only).
#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub(crate) nodes: Vec<Node>,
}

// ===========================================================================
// Model tests — construct ASTs by hand and prove the type supports each
// requirement, before U3 wires a parser to populate them.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// AE3 (R5) — a comment is carried as queryable trivia, not dropped.
    #[test]
    fn comment_is_queryable_trivia() {
        let node = Node {
            item: Item::Instruction {
                mnemonic: "nop".into(),
                mode: Some("implied"),
                operands: vec![],
            },
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
        assert_eq!(node.trivia.trailing.as_ref().unwrap().text, "no-op");
    }

    /// AE4 (R4) — a local label reused in two scopes is two distinct symbols.
    #[test]
    fn reused_local_is_two_distinct_scoped_symbols() {
        let first = Symbol {
            name: ".loop".into(),
            scope: Scope::Local {
                in_global: "first".into(),
            },
        };
        let second = Symbol {
            name: ".loop".into(),
            scope: Scope::Local {
                in_global: "second".into(),
            },
        };
        assert_ne!(first, second, "same name, different scope -> distinct");
        assert_eq!(first.name, second.name);
    }

    /// AE2 (R3) — every node carries `(file, line, column)`, with the
    /// expansion-frame stack present and empty in v1.
    #[test]
    fn span_carries_file_line_column() {
        let s = Span::at(7, 9);
        assert_eq!(s.file, FileId(0));
        assert_eq!(s.line, 7);
        assert_eq!(s.col, 9);
        assert!(s.expansion_frames.is_empty(), "reserved, empty in v1");
    }

    /// KTD5 — an operand round-trips its source token text: `$0A`, `10`, and
    /// `%1010` all evaluate to 10 but re-emit distinctly.
    #[test]
    fn operand_preserves_source_spelling() {
        let forms = ["$0A", "10", "%1010"];
        for src in forms {
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
}
