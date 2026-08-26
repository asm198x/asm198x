# Decision: a conditional in a multi-pass dialect folds once, in source order, before layout

**Status:** Active. Binding for how ca65 and vasm — and any future native
multi-pass dialect — evaluate conditional assembly and repetition.

**Date:** 2026-08-22.

## The question

[`conditional-assembly-framework.md`](conditional-assembly-framework.md) models
conditional assembly as `Item::Conditional` in the shared tree, evaluated by
`ast::evaluate` over a per-dialect `CondEval`. Its four-step adoption recipe
has a step 3: *route the dialect's assembler through `ast::evaluate`*.

**ca65 and vasm have nothing to route.** They are native multi-pass drivers
([`ast-native-payload-for-multipass-cisc.md`](ast-native-payload-for-multipass-cisc.md)):
they project the tree into their own `Parsed` form and then run their own
layout and emit passes. `ast::evaluate` produces a `Vec<Statement>` for the
shared engine, which is not what either driver consumes.

The worry that made this a decision rather than a task: a conditional changes
how much code is emitted, which changes addresses, which could change a
condition — the classic convergence problem, in a driver that already runs a
relaxation fixpoint.

## The finding: it is not a multi-pass problem

Measured against ca65 V2.18 and vasm 1.9 (m68k mot), 2026-08-22. **Neither
reference lets a condition depend on anything that is not already known where
the condition sits.**

| probe | ca65 | vasm |
|---|---|---|
| constant defined **above** | folds | folds |
| constant defined **below** | `Constant expression expected` | `expression must be constant` |
| forward **label** | `Constant expression expected` | `expression must be constant` |
| **backward** label | `Constant expression expected` | — |
| the location counter `*` | `Constant expression expected` | **folds** |
| `.ifdef` of a name defined below | **false** | — |
| a definition inside an **untaken** branch | invisible afterwards | — |

Two things fall out.

**ca65 conditions cannot see layout at all.** Not the location counter, and not
even a *backward* label — because a ca65 label is relocatable until `ld65` links
it, so it is never a constant expression. A ca65 condition is a function of the
`=` constants above it and nothing else.

**vasm conditions can see `*`, and the value is the pre-relaxation address.**
This is the finding that settles the convergence worry, and it is worth showing
because it is not what you would guess. With ten bytes of filler after a branch
that the optimiser shortens from four bytes to two:

```
        bra t
        ds.b 10
t:
        ifeq *-14        ; fires — the condition sees 14
        dc.b $aa
        endif
```

The assembled file is fourteen bytes and begins `60 0A` — a **two-byte** branch,
so the condition sits at address 12. Yet the condition fires on `*-14`, not
`*-12`; and a `dc.w *` in the same position emits `000C`. So an ordinary
expression resolves against the final layout while a **condition resolves
against the unrelaxed one**.

That is not an inconsistency in vasm. It is the only order that works:
conditions decide what code exists, so they must be settled before the
optimiser starts shrinking it. Deciding them early is what stops a condition
and the relaxation loop feeding each other.

## The decision

**Fold conditions and repetition counts in a single sequential sweep over the
tree, during projection, before layout — never during a layout pass, and never
iterated.**

- **ca65**: `parsed_from_program` walks `Item::Conditional` and `Item::Repeat`,
  folding each head against the `=` constants gathered so far in source order,
  and projects only the live branch or the repeated body. No layout state is
  consulted because ca65 conditions cannot reach any.
- **vasm**: the same sweep, carrying a running **unrelaxed** program counter so
  `*` folds to the pre-relaxation address, matching the probe above.
- **Neither iterates.** A condition is folded once. Its result is fixed before
  the relaxation fixpoint begins, so the fixpoint's grow-only invariant is
  untouched and there is no convergence question to answer.
- **An untaken branch defines nothing** — the same rule the shared walk already
  enforces with its `emit = false` pass, and the rule ca65 was measured to
  follow.

The conditional stays **in the tree** as `Item::Conditional`. This is a
different *evaluator*, not a preprocessor: nothing rewrites the source, and the
formatter still sees both branches. That distinction is what
`conditional-assembly-framework.md`'s drift trigger protects, and it survives.

## Why not the alternatives

Three shapes were written down in
`docs/plans/2026-08-22-003-feat-reference-parity-plan.md`. The measurement
chose between them.

**Give the native driver a `CondEval` and run `ast::evaluate`.** The most
consistent-looking option and the wrong seam. `CondEval::lower` produces
`Statement`s for the shared engine; a native driver wants its own `Parsed`
payload, and the whole point of
[`ast-native-payload-for-multipass-cisc.md`](ast-native-payload-for-multipass-cisc.md)
is that a multi-pass CISC dialect does not lower to the engine's stream. It
would mean either a second lowering that throws its output away, or hoisting
the driver's environment out of the passes that build it.

**Evaluate during the layout pass.** Rejected by the vasm probe, not by taste.
Layout runs to a fixpoint, so a condition folded there would be re-folded with
different addresses on each iteration — and could flip. That is exactly the
oscillation vasm avoids by deciding conditions before relaxation. Choosing this
shape would *introduce* an instability neither reference has.

## What this settles about [#99]

[#99] asks whether "a condition must be a parse-time constant" holds across the
board or is being relaxed for sjasmplus specifically, and says a rule that holds
in two places and not a third is worse than either answer.

It holds for ca65 and vasm, and it holds because **their references enforce
it**: both refuse a forward reference in a condition outright. So the posture is
not a shared limitation with one exception — it is per-dialect fidelity, and
sjasmplus is genuinely the outlier, because sjasmplus really does run multiple
passes and really does resolve a condition against a symbol defined later.

That narrows #99 from "decide a global posture" to "adopt multi-pass conditions
for the one dialect whose reference has them", and leaves ca65 and vasm correct
as they stand.

## Drift triggers

Stop and re-consult if a change would:

- **Fold a condition inside the layout or relaxation pass**, so that it is
  re-evaluated as addresses settle. That is the shape this record rejects, and
  it introduces an oscillation the references do not have.
- **Iterate conditions to a fixpoint** because "the assembler is multi-pass
  anyway". Neither reference permits the forward dependency that would require
  it.
- **Fold vasm's `*` in a condition against the final address.** It is the
  unrelaxed address, measured. Using the final one changes which branch
  assembles.
- **Assume ca65 behaves like vasm here.** ca65 conditions cannot see the
  location counter *or* a backward label; vasm's can see `*`. The two dialects
  differ, and a shared "current address" plumbed into both would let ca65 accept
  source real ca65 refuses.
- **Turn this into a source preprocessor** that rewrites text before the parse.
  The conditional stays in the tree; only the evaluator differs.
  See `conditional-assembly-framework.md`.

## See also

- [`conditional-assembly-framework.md`](conditional-assembly-framework.md) — the
  shared tree and the adoption recipe this extends for a third pipeline.
- [`ast-native-payload-for-multipass-cisc.md`](ast-native-payload-for-multipass-cisc.md)
  — why ca65 and vasm do not lower to the engine's statement stream.
- [`syntax-stance.md`](syntax-stance.md) — per-dialect fidelity, which is why
  ca65 and vasm differ here rather than converging.

[#99]: https://github.com/asm198x/asm198x/issues/99
