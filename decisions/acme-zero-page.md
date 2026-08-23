# Decision: ACME sizes zero page from a tracked location counter

**Status:** Active. Binding for the ACME front-end.

**Date:** 2026-08-23.

## The decision

**The ACME walk tracks the location counter as it parses, binds each label to
the address it names, and gives up on knowing the counter the moment it cannot
follow it.** Zero-page operand sizing then falls out of the ordinary constant
fold, because a backward label is an ordinary constant once its address is
known.

The width rule itself is not restated here. [`engine::next_pc`] is the single
definition of how far an operation moves the counter, and the walk calls it —
the engine's own address pass calls the same function.

## The problem

ACME picks zero page when an operand's value fits in a byte:

```asm
* = $0000
lbl     lda #5
        lda lbl     ; A5 00 — zero page
```

We emitted `AD 00 00`. Wrong size *and* wrong byte count, on source with
nothing unusual in it (#128).

The cause was structural rather than a missing case. Sizing folds the operand
against the walk's `env`, and `env` held `=` constants but never label
addresses — because the walk had no addresses. A label was symbolic until the
engine's layout pass, which runs after every mode has already been chosen.

## Why a single forward pass is enough

The obvious objection is that this is the fixpoint every assembler has: a
label's address depends on the sizes of what precedes it, and those sizes
depend on label values.

It is not a fixpoint here, because the dependency only ever points **backward**.
When the walk reaches `lda lbl`, `lbl`'s address was fixed by the widths of the
operations before it, and every one of those widths was already decided when
its own line was walked. Nothing later can move it. So one left-to-right pass
is self-consistent, and no relaxation loop is needed.

A **forward** reference genuinely is unknown, and stays absolute — which is
what ACME does too, warning *"Using oversized addressing mode"* when the value
later turns out to have fit. We now say the same: a channel arrived with
`Dialect::parse_warned` (see
[`forward-conditions-and-passes.md`](forward-conditions-and-passes.md)), and
the candidates are cheap to spot — a forced-absolute literal always folds and a
forward symbol never does, so the two sets are disjoint and only the second can
turn out to have fitted.

## Giving up is the safe direction

`pc` is an `Option`, and `None` is a normal outcome rather than an error: an
`*=` whose expression does not fold yet, or an operation whose form the ISA
cannot supply, leaves the counter unknown for the rest of the walk.

While it is unknown no label address enters `env`, so every label reference
sizes absolute — exactly what this dialect did everywhere before this change.
That asymmetry is the point. **A counter that is only probably right is worse
than no counter at all**: absolute is merely wider than it needed to be, while
a wrong zero-page pick emits the wrong instruction.

## What this does not do

- It does not make the walk a layout pass. It computes no image, resolves no
  forward reference, and the engine's two passes are untouched.
- It does not extend to other dialects. ca65 and vasm run their own multi-pass
  layout and have no need of it; the flat Z80 dialects size nothing from a
  value.

## Drift triggers

- *"Copy the width rule into the walk, it's only a few lines."* → No. Two
  copies of the width rule drift, and a drifted counter is wrong bytes rather
  than a missed optimisation. Call [`engine::next_pc`].
- *"Make `pc` an `i64` and default it to zero."* → That turns "I don't know
  where I am" into a confident wrong answer, which is the one failure mode this
  design is shaped to avoid.
- *"Loop until the sizes settle, so forward references shrink too."* → ACME
  does not shrink them either; it warns and stays wide. Matching the reference
  is the goal, not being cleverer than it.

[`engine::next_pc`]: ../crates/asm198x/src/engine.rs
