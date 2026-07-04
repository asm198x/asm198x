//! The source-preserving semantic AST — plan units **U2** (types) and **U3**
//! (the Z80 front-end lowers into it), see
//! `docs/plans/2026-07-04-005-feat-ir-ast-layer-plan.md`.
//!
//! A layer *above* the encoder: a dialect parses source into a [`Program`], and
//! [`lower`] turns it into today's [`Statement`](crate::engine::Statement) /
//! [`Operation`](crate::engine::Operation) stream, which the existing two-pass
//! driver assembles unchanged (KTD1 — the `isa`/encoding layer is untouched,
//! output bytes are identical). U3 routes the Z80 dialects through this; other
//! CPUs stay on direct lowering behind the dialect boundary (KTD6).
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

use crate::engine::{Expr, Operation, Statement};

// ---------------------------------------------------------------------------
// Provenance (R3)
// ---------------------------------------------------------------------------

/// Identifies a source file. v1 is single-file (`FileId(0)`); include chains
/// (idea 4) allocate further ids so a span can name the *included* file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileId(pub(crate) u32);

/// One macro-expansion frame (a rustc-style defined-at / invoked-at record).
/// Reserved now; idea 4's macro engine fills it. Empty in v1.
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

    /// The value of a fixed-slot operand. Panics on a structured operand — the
    /// Z80 never produces one, so U3's lowering never hits this.
    fn into_value(self) -> Expr {
        match self {
            Operand::Expr { value, .. } => value,
            Operand::Structured(_) => {
                unreachable!("a fixed-slot dialect never produces a structured operand")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Items and the program
// ---------------------------------------------------------------------------

/// One operation on a source line, before byte-lowering. The variants mirror the
/// [`Operation`](crate::engine::Operation) kinds the Z80 dialects produce; other
/// CPUs' kinds (`Encoded`, `Align`) are added when U6/ACME migrate. Instructions
/// carry structured [`Operand`]s (source text + structured variants) even though
/// U3 populates only the fixed-slot value.
#[derive(Clone, Debug)]
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
}

/// A source line reduced to an optional (scoped) label and an optional
/// operation, with provenance and attached comment trivia. Mirrors
/// [`Statement`](crate::engine::Statement), enriched.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub(crate) nodes: Vec<Node>,
}

// ---------------------------------------------------------------------------
// Lowering: AST -> the engine's Statement/Operation stream (U3, KTD1)
// ---------------------------------------------------------------------------

/// Lower an AST to the engine's statement stream. Label definitions emit their
/// **qualified** name (so scope resolves exactly as the old string-mangle did),
/// operations lower to their [`Operation`], and the driver assembles the result
/// unchanged — byte-identical to direct parsing (AE1).
pub(crate) fn lower(program: Program) -> Vec<Statement> {
    program
        .nodes
        .into_iter()
        .map(|node| Statement {
            line: node.span.line as usize,
            label: node.label.map(|s| s.qualified),
            op: node.item.map(lower_item),
        })
        .collect()
}

fn lower_item(item: Item) -> Operation {
    match item {
        Item::Instruction {
            mnemonic,
            mode,
            operands,
        } => Operation::Instruction {
            mnemonic,
            mode,
            operands: operands.into_iter().map(Operand::into_value).collect(),
        },
        Item::Org(o) => Operation::Org(o.into_value()),
        Item::Equ(o) => Operation::Equ(o.into_value()),
        Item::Bytes(v) => Operation::Bytes(v.into_iter().map(Operand::into_value).collect()),
        Item::Words(v) => Operation::Words(v.into_iter().map(Operand::into_value).collect()),
        Item::Entry(o) => Operation::Entry(o.into_value()),
    }
}

/// Build an [`Item`] from an [`Operation`] a Z80 dialect produced. The Z80 never
/// emits `Encoded` (it has no computed operands) or `Align` (ACME-only), so those
/// are unreachable here; U6 and the ACME migration add them.
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
        Operation::Encoded(_) | Operation::Align { .. } => {
            unreachable!("the Z80 dialects never emit Encoded or Align operations")
        }
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
pub(crate) fn emit(program: &Program) -> String {
    const INDENT: &str = "        ";
    let mut out = String::new();
    for node in &program.nodes {
        for c in &node.trivia.leading {
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
        let label = node.label.as_ref().map(|s| s.name.as_str());
        match (label, node.item.as_ref()) {
            // `equ` binds its label to a value on the same statement, so its
            // label must stay on the operation's line (it cannot be split off).
            // The colon is required: a bare `name` whose spelling collides with a
            // mnemonic or directive (`in`, `di`, `end`, `set`, …) would re-parse
            // as an instruction, but a `name:` token is always a label.
            (Some(name), Some(Item::Equ(_))) => {
                out.push_str(name);
                out.push_str(": ");
                out.push_str(&node.source);
                trailing(&mut out);
                out.push('\n');
            }
            // Label + other operation: label on its own line, operation indented.
            (Some(name), Some(_)) => {
                out.push_str(name);
                out.push_str(":\n");
                out.push_str(INDENT);
                out.push_str(&node.source);
                trailing(&mut out);
                out.push('\n');
            }
            // Label-only line.
            (Some(name), None) => {
                out.push_str(name);
                out.push(':');
                trailing(&mut out);
                out.push('\n');
            }
            // Operation with no label.
            (None, Some(_)) => {
                out.push_str(INDENT);
                out.push_str(&node.source);
                trailing(&mut out);
                out.push('\n');
            }
            // No label, no operation — a bare trailing comment (rare).
            (None, None) => {
                if let Some(c) = &node.trivia.trailing {
                    out.push_str(&c.text);
                    out.push('\n');
                }
            }
        }
    }
    out
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
        assert_eq!(s.file, FileId(0));
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
        let statements = lower(Program { nodes: vec![node] });
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
}
