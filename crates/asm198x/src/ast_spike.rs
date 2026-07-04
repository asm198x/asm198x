//! U1 validation spike — **THROWAWAY, test-only** (`#[cfg(test)]`).
//!
//! Plan: `docs/plans/2026-07-04-005-feat-ir-ast-layer-plan.md`, unit **U1**
//! (the HARD GATE). This module answers one question with runnable evidence,
//! then is deleted: **can one dialect-neutral semantic AST hold the family's
//! real divergences without per-consumer escape hatches?** — across two axes:
//!
//! 1. **Semantic divergence** — pasmo vs sjasmplus (oversize policy; local-label
//!    scope). Does it live *outside* the tree (a dialect attribute) so both
//!    dialects share one AST?
//! 2. **Operand-structure divergence** — the fixed-slot `Expr` operand vs the
//!    6809's computed postbyte (`Operation::Encoded`). Can an *abstract
//!    structured operand* AST node lower to the same bytes?
//!
//! Plus the KTD7 cheaper-floor head-to-head: would a minimal *un-lowered
//! statement stream* (today's `Statement`, no semantic AST) suffice for the v1
//! consumer (the formatter)?
//!
//! Deliberately not hardened and not wired into production. The gate decision it
//! produces is written back into the plan's U1 section.
#![allow(dead_code)] // spike models the full operand shape; not all is exercised

use crate::dialect::Dialect;
use crate::dialects::{Lwasm, Pasmo, Sjasmplus};
use crate::engine::{Expr, Operation, Statement};
use crate::{assemble_lwasm, assemble_pasmo, assemble_sjasmplus};

// ---------------------------------------------------------------------------
// A canonical, dialect-agnostic rendering of the existing statement stream.
//
// The finding this exposes: `Statement`/`Operation` is *already* the neutral
// layer for fixed-slot instructions — `mode` is an isa-shared label, operands
// are `Expr`s, and the pasmo/sjasmplus divergences (oversize, comment syntax,
// number syntax) are NOT in it. So "does a neutral tree hold both dialects" is,
// for the fixed-slot core, answered by "both dialects already produce the same
// stream". `canon` makes that testable. (`Expr` derives `Debug`; `Operation`
// and `Statement` do not, so we render by hand.)
// ---------------------------------------------------------------------------

/// Render a statement to a dialect-neutral string (line number omitted — it is
/// incidental, and equal for equal source anyway).
fn canon(s: &Statement) -> String {
    let label = s.label.clone().unwrap_or_else(|| "·".into());
    let op = match &s.op {
        None => "—".into(),
        Some(Operation::Org(e)) => format!("org {e:?}"),
        Some(Operation::Equ(e)) => format!("equ {e:?}"),
        Some(Operation::Bytes(v)) => format!("bytes {v:?}"),
        Some(Operation::Words(v)) => format!("words {v:?}"),
        Some(Operation::Instruction {
            mnemonic,
            mode,
            operands,
        }) => format!("instr {mnemonic} [{mode}] {operands:?}"),
        Some(Operation::Encoded(_)) => "encoded(<computed pieces — operand structure lost>)".into(),
        Some(Operation::Entry(e)) => format!("entry {e:?}"),
        Some(Operation::Align {
            andmask,
            value,
            fill,
        }) => format!("align {andmask} {value} {fill}"),
    };
    format!("{label}: {op}")
}

fn canon_stream(statements: &[Statement]) -> Vec<String> {
    statements.iter().map(canon).collect()
}

// ---------------------------------------------------------------------------
// Axis 2 — an abstract structured 6809 indexed operand, and its lowering.
//
// This is the ONLY axis where today's stream is lowered past source: lwasm
// computes the postbyte into `Operation::Encoded` at parse, so `5,x` is gone.
// Here we model the operand abstractly and show it lowers to the same bytes —
// i.e. the structured-operand variant U2 reserves is sufficient and general
// (register + auto + indirect + offset), no escape hatch.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum IdxReg {
    X,
    Y,
    U,
    S,
}

impl IdxReg {
    /// The 2-bit register field, shifted into postbyte bits 6–5.
    fn rr(self) -> u8 {
        (match self {
            IdxReg::X => 0,
            IdxReg::Y => 1,
            IdxReg::U => 2,
            IdxReg::S => 3,
        }) << 5
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Auto {
    None,
    Inc1,
}

/// The abstract structured operand — what a source-preserving AST node would
/// carry for a 6809 indexed operand, instead of pre-computed `Piece`s.
struct Indexed {
    reg: IdxReg,
    auto: Auto,
    indirect: bool,
    /// Constant offset (this spike only needs the constant, no-label cases).
    offset: Option<i64>,
}

impl Indexed {
    /// Lower to the instruction's bytes (opcode + postbyte + 0/1 extension),
    /// mirroring `lwasm::encode_indexed` for the cases the spike exercises.
    fn lower(&self, opcode: u8) -> Vec<u8> {
        let rr = self.reg.rr();
        let mut out = vec![opcode];
        match self.auto {
            Auto::Inc1 => out.push(0x80 | rr),
            Auto::None => {
                let n = self.offset.unwrap_or(0);
                if !self.indirect && (-16..=15).contains(&n) {
                    // 5-bit embedded offset.
                    out.push(rr | (n as u8 & 0x1F));
                } else {
                    // 8-bit extension (indirect has no 5-bit form).
                    let mut post = 0x88 | rr;
                    if self.indirect {
                        post |= 0x10;
                    }
                    out.push(post);
                    out.push(n as u8);
                }
            }
        }
        out
    }
}

// ===========================================================================
// The spike tests. Each prints its evidence; the gate decision is read off the
// aggregate (see the `gate_decision` summary test).
// ===========================================================================

/// Axis 1a — the oversize divergence (pasmo silent `Truncate` vs sjasmplus
/// `TruncateWarn`) is BYTE-NEUTRAL and lives outside the tree. A byte operand
/// given 511 keeps the low 8 bits (0xFF) in both; only the warning differs.
#[test]
fn oversize_divergence_is_byte_neutral_and_off_tree() {
    let src = "  ld a, 511\n"; // 511 = 0x1FF, truncates to 0xFF
    let p = assemble_pasmo(src).expect("pasmo assembles");
    let s = assemble_sjasmplus(src).expect("sjasmplus assembles");
    assert_eq!(
        p.bytes, s.bytes,
        "oversize divergence must not change bytes"
    );
    assert_eq!(p.bytes, vec![0x3E, 0xFF], "ld a,n truncated to low byte");

    // And the statement streams are structurally identical — the divergence is
    // NOT in the tree (it's `Dialect::oversized_byte_policy`, applied in pass 2).
    let ps = Pasmo { z80n: false }.parse(src).unwrap();
    let ss = Sjasmplus { z80n: false }.parse(src).unwrap();
    assert_eq!(
        canon_stream(&ps),
        canon_stream(&ss),
        "pasmo and sjasmplus produce the SAME neutral statement stream"
    );
}

/// Axis 1b — local-label scope is a GENUINE semantic divergence, and it lives
/// in the parse→AST lowering (a dialect attribute), not a shared tree field and
/// not a per-consumer escape hatch. The SAME source means different things:
/// sjasmplus scopes `.loop` to the current global; pasmo treats `.loop` as an
/// ordinary global, so reusing it is a duplicate. The neutral AST holds each
/// dialect's *true* meaning because the divergence is resolved at lowering
/// (keyed by `Z80Syntax::scopes_locals`) — this is the crux the round-1 review
/// flagged, and it has a clean home.
#[test]
fn local_label_scope_divergence_lives_in_parse_lowering() {
    let two_scope = "\
first:
  ld b, 2
.loop:
  djnz .loop
second:
  ld b, 3
.loop:
  djnz .loop
";
    // sjasmplus scopes: assembles, and the neutral stream carries the two
    // `.loop`s as DISTINCT qualified names (`first.loop` / `second.loop`).
    let s = assemble_sjasmplus(two_scope).expect("sjasmplus scopes locals");
    assert!(!s.bytes.is_empty());
    let ss = Sjasmplus { z80n: false }.parse(two_scope).unwrap();
    let rendered = canon_stream(&ss).join("\n");
    assert!(
        rendered.contains("first.loop") && rendered.contains("second.loop"),
        "sjasmplus scope resolves the two `.loop`s to distinct qualified names:\n{rendered}"
    );

    // pasmo does NOT scope: the reused `.loop` is a duplicate — its documented
    // meaning. The divergence is real and dialect-owned (not a spelling quirk).
    assert!(
        assemble_pasmo(two_scope).is_err(),
        "pasmo does not scope locals — reused `.loop` must be a duplicate error"
    );

    // With pasmo's own meaning (distinct globals) it assembles fine — the AST
    // lowering encodes each dialect's semantics, no escape hatch.
    let pasmo_ok = "\
first:
  ld b, 2
firstloop:
  djnz firstloop
";
    assert!(
        assemble_pasmo(pasmo_ok).is_ok(),
        "pasmo assembles distinct globals"
    );
}

/// Axis 2 — an abstract structured 6809 indexed operand lowers to the SAME
/// bytes as lwasm's computed-postbyte path, across the representative shapes
/// (5-bit embedded, auto-increment, indirect 8-bit). This is the structured
/// operand U2 reserves; it needs no per-consumer escape hatch.
#[test]
fn structured_6809_operand_lowers_byte_identical() {
    // opcode for `lda <indexed>` is 0xA6.
    // 6809 mnemonics are indented; column 0 is a label.
    let cases: &[(&str, Indexed)] = &[
        (
            "        lda 5,x\n",
            Indexed {
                reg: IdxReg::X,
                auto: Auto::None,
                indirect: false,
                offset: Some(5),
            },
        ),
        (
            "        lda ,x+\n",
            Indexed {
                reg: IdxReg::X,
                auto: Auto::Inc1,
                indirect: false,
                offset: None,
            },
        ),
        (
            "        lda [5,x]\n",
            Indexed {
                reg: IdxReg::X,
                auto: Auto::None,
                indirect: true,
                offset: Some(5),
            },
        ),
        (
            "        lda 5,y\n",
            Indexed {
                reg: IdxReg::Y,
                auto: Auto::None,
                indirect: false,
                offset: Some(5),
            },
        ),
    ];
    for (src, operand) in cases {
        let reference = assemble_lwasm(src)
            .unwrap_or_else(|e| panic!("lwasm assembles `{}`: {:?}", src.trim(), e))
            .bytes;
        let ours = operand.lower(0xA6);
        assert_eq!(
            ours,
            reference,
            "structured operand for `{}` must lower byte-identical to lwasm",
            src.trim()
        );
    }
}

/// KTD7 cheaper-floor check — the un-lowered statement stream is INSUFFICIENT
/// for computed-operand CPUs. lwasm's `lda 5,x` is `Operation::Encoded` whose
/// operand structure (`5,x`) is already gone; an "un-lowered stream" that keeps
/// only today's `Operation` cannot recover it to emit faithful source. So the
/// cheaper floor serves fixed-slot fmt only; computed-operand fmt needs the
/// structured-operand AST (Axis 2). Recorded, not asserted as pass/fail beyond
/// confirming the Encoded shape.
#[test]
fn cheaper_floor_is_insufficient_for_computed_operands() {
    let ls = Lwasm.parse("        lda 5,x\n").unwrap();
    let rendered = canon_stream(&ls).join("\n");
    assert!(
        rendered.contains("encoded"),
        "6809 indexed operand is Encoded at parse — source structure lost in the \
         un-lowered stream, so the cheaper floor can't emit faithful 6809 source:\n{rendered}"
    );
}
