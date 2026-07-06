# Decision: multi-pass CISC dialects carry a family-owned structured payload in the AST

**Status:** Active. Binding for how the variable-length CISC dialects (68000
now; x86 and the 68020+/68080 line later) route through the semantic AST.
Extends [`roadmap-sequencing.md`](roadmap-sequencing.md) (the AST layer) and is
constrained by the *seam-first* stance in
[`../../decisions/asm198x-cpu-coverage-roadmap.md`](../../decisions/asm198x-cpu-coverage-roadmap.md).

**Date:** 2026-07-06.

## The goal

**AST for everything.** Every dialect's source flows through the source-preserving
semantic `Program` as its single front-end IR — so the formatter, the dialect
converter, and provenance all hang off one tree, with no dialect special-cased
outside it. The ~19 engine-based dialects already do this: they lower into the
shared `Item`s (`Instruction` / `Encoded` / `Bytes` / …) and `ast::lower` feeds
`engine::assemble`. Two dialects remained outside: the NES `ca65` and `vasm`
(68000), both standalone assemblers that never touch `engine::assemble`.

## Why vasm cannot use the engine shortcut

The field-packed CPUs (PDP-11, TMS9900, Z8000, CP1610) *look* like they might be
hard, but they carry their semantics in the tree the easy way: `parse_op`
computes `Item::Encoded(pieces)` at parse time and `engine::assemble` consumes
it. That works because their encoding is **final at parse time**.

vasm's is not. Branch relaxation (short↔word) and the peephole optimizer
(`addq`/`subq`, `lea`↔`addq`, `cmp #0`→`tst`, PC-relative rewrites) **resize and
rewrite instructions across multiple layout passes**, so vasm must hold its
instructions in an **un-lowered, structured form** (mnemonic + size + structured
effective-address operands) *across* those passes. It cannot pre-compute
`Item::Encoded`, and it cannot use `engine::assemble` (which knows nothing of
sections, relaxation, the optimizer, or the Amiga hunk-exe output). That
un-lowered structured form held between parse and byte-lowering **is exactly what
the IR/AST layer was conceived to be**; vasm is the CPU that forces the real
thing rather than the `Encoded` shortcut.

## The forward-looking finding — the need is a bounded club

Sorting every roadmap CPU by "does it need structured operands held in the tree
across passes?" (the property that rules out `Encoded`+engine):

- **Tier 0/1** (8080, 6800, 1802, 8048, Z8000, TMS7000, SC/MP, F8, 2650, PDP-11,
  TMS9900, CP1610, uPD7800, …) — no. Fixed-slot or `Encoded`/field-packed; all
  pre-compute and use the engine.
- **Tier 2 RISCs** (ARM, MIPS, SH-1/2/4, PowerPC, SPARC, V810, Transputer) — no.
  Fixed-width instructions ⇒ no branch relaxation and no optimizer; the barrel
  shifter, `LDM`/`STM` reglists, and `disp(base)` compute to fields at parse, and
  ARM's literal pool is a PC-relative `Piece`. They fit the **`Encoded` seam**.
- **Tier 3 x86** — **yes.** Variable length (ModR/M + SIB + prefixes), jump
  relaxation, mod-rm operands carried across passes. The one genuine future
  member of vasm's club.
- **68020 / 030 / 040 / 060 / 68080 (Apollo)** — **yes**, as an *extension of the
  68000's own model* (scaled index, memory-indirect `([bd,An,Xn],od)`, bit-field
  ops), not a new shared shape. Committed future work.

So the "structured-operands-in-the-tree" set is a **small, bounded club of
variable-length CISC families: 68000 (+ 68020+/68080) now-and-later, and x86** —
against ~32 other families that route through the existing shared `Item`s and the
`Encoded` seam. Critically, the club members' operand shapes **do not unify**:
68000 effective-addresses, x86 mod/rm/SIB, and (had it qualified) an ARM barrel
shifter are genuinely different. There is no honest universal operand across them.

## The decision — family-owned payload (Option B), not a universal operand (A)

Each variable-length CISC dialect carries its **own** structured statement /
operand type **in the AST `Node`**, so the `Program` is genuine IR (the assembler
reads the tree; it does **not** re-parse the node's source — that would make the
`Program` a fake IR) and the multi-pass assembler consumes it. The 68000 family
owns one such type that the 68020+/68080 line extends; x86 will add its own when
it lands.

**Rejected — a universal shared structured-operand model (Option A):** building
one `ast::Operand` model rich enough for 68000 + x86 + (someday) ARM/SH effective
addresses. It fails three ways:

1. **It trips a binding drift trigger.** The CPU-coverage roadmap's
   [drift triggers](../../decisions/asm198x-cpu-coverage-roadmap.md#drift-triggers)
   say: *"This CPU needs a new `OperandKind` … → First try the computed-operand
   seam (`Operation::Encoded`/`Piece`)."* A universal structured-operand model is
   exactly the new `OperandKind` the project already decided against.
2. **It is speculative over-abstraction (YAGNI).** ~32 of ~35 families never use
   it; the ≤3 that do have shapes that never unify. It would serve
   cross-*family* structural conversion — converting 68000 source to x86 — which
   is meaningless. (The converter only ever works *within* a CPU: dialect-A ↔
   dialect-B for the same chip, where both dialects share that family's payload
   type. Option B supports that fully.)
3. **It is a big change to a type used by all 19 working dialects, for no gain
   to any of them.**

## Consequences

- The shared AST gains a way for a `Node` to hold an un-lowered, family-owned
  statement payload (the exact mechanism — a typed carrier the CISC families
  populate — is settled against the code during implementation, keeping the
  shared layer from depending on dialect encoding internals).
- `vasm`'s front-end produces a `Program`; its layout/relax/optimize/serialize
  passes consume the tree's payloads. The `isa::m68k` encoder, the optimizer, the
  section/hunk model, and the three output paths are **rescued unchanged** —
  byte-identity against `vasmm68k_mot` across the Amiga curriculum is the gate at
  every increment.
- `format_vasm` reads each node's verbatim source + trivia + labels, exactly as
  the other dialects' formatters do.
- The NES `ca65` (standalone assemble + link) follows the same pattern as its own
  increment.
- When x86 and the 68020+/68080 line land, they extend the family-owned payload,
  not a shared operand model — the roadmap's seam-first philosophy preserved.

## Drift triggers

- *"Let's generalise one structured-operand model for all the rich CPUs."* → No;
  see Option A above. The shapes don't unify, ~32 CPUs don't need it, and it trips
  the coverage-roadmap drift trigger. Family-owned payloads.
- *"vasm's formatter can just re-parse the node source to get its statements."* →
  No. That makes the `Program` a fake IR. The tree carries the payload; the
  assembler reads it.
- *"Route vasm through `engine::assemble` like the field-packed CPUs."* → It
  can't — relaxation and the optimizer need un-lowered statements held across
  passes, which `Item::Encoded` (final-at-parse) cannot express, and the engine
  has no sections/optimizer/hunk output.
