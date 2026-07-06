# Decision: `asm198x fmt` is a canonical reformatter, and it re-aligns constant runs

**Status:** Active. Binding for Asm198x's formatter (`asm198x fmt`, the
`format_*` library entry points, and `ast::emit`).

**Date:** 2026-07-05.

## The decision

`asm198x fmt` produces a **canonical layout** — it is a *reformatter*, not a
source-preserving pretty-printer. It re-flows every line to one house style and
discards the author's incidental whitespace. What it preserves is **meaning and
spelling**, not **layout**:

- Labels at column 0; operations indented one tab stop (8 spaces); a single
  space around operators and separators; comments repositioned canonically
  (own-line comments on their own line, a same-line comment trailing its
  operation after a fixed gap).
- **Operand spelling is preserved verbatim** — `$0A` / `10` / `%1010` re-emit as
  written (the whole-line source is carried on the node; KTD5). Canonicalisation
  is about *where tokens sit*, not *how numbers are spelled*.
- Conditional blocks (ACME `!if`/`!ifdef`/`!ifndef`) are canonicalised
  structurally: delimiters (`!if … {`, `} else {`, `}`) at column 0, bodies
  formatted with the normal rules, and the idiomatic one-line guard
  (`!ifndef X { X = 0 }`) kept on one line.
- The result **reassembles byte-identical** to the input (AE1) and re-emitting
  is a fixed point (**idempotent**, AE7).

This is the same posture as `gofmt`, `rustfmt`, and `clang-format`: one house
style, no knobs, deterministic. The point of a formatter is to *end* arguments
about layout, so it does not try to guess or keep the author's spacing.

## The constant-alignment ruling

The one place a naive "collapse everything to single spacing" reflow loses real
value is a **run of aligned constant definitions** — pervasive in 6502 (and
retro assembly generally):

```
VOICE1_WAVE = $21               ; Sawtooth for track 1
VOICE_AD    = $09               ; Attack=0, Decay=9
PULSE_WIDTH = $08
```

The padding before `=` is a deliberate readability feature (a lookup table the
eye can scan). Collapsing it to `VOICE_AD = $09` is byte-identical but a genuine
regression in output quality.

**Ruling: the formatter owns the alignment.** Within a **maximal run of
consecutive `name = value` constant-definition lines**, it aligns each `=` to one
column past the longest name in that run. It neither preserves the author's
ad-hoc spacing (which would be non-canonical and author-dependent) nor collapses
to a single space (which loses the table). This keeps constant tables readable
while staying canonical and idempotent.

Run boundaries:

- A run is a set of **adjacent** constant-definition lines. Any non-constant
  line — including a **blank line** or an **own-line comment** — ends the run.
  Blank-line grouping is itself a deliberate authoring signal (the author
  visually groups related constants), so it is respected: each blank-separated
  group aligns independently.
- Alignment is computed per run from the longest **name** in that run; a run of
  one constant gets a single space (nothing to align to).

Scope of v1: the ruling aligns the **`=`**. Trailing-comment alignment within a
run (so the `;` columns line up too) is a **further refinement**, deliberately
left open — it depends on value widths and is a separate, lower-value pass.

## Rationale

- **Readability is the product.** A formatter whose output is *less* readable
  than the input will not be used, and 6502 curriculum code leans on constant
  tables heavily. Re-alignment is the difference between a formatter people run
  and one they avoid.
- **Canonical beats preserved.** Owning the alignment (rather than preserving
  the author's) keeps `fmt(fmt(x)) == fmt(x)` and makes output independent of how
  carefully the source was typed — the whole reason to have a formatter.
- **Bounded cost.** The only machinery this adds is a run-aware pass in emit
  (look ahead over adjacent constant nodes to size the column). It touches
  nothing else about the reflow.

## Drift triggers

Stop and re-consult this decision if a change would:

- **Preserve the author's incidental whitespace** anywhere ("keep the source
  spacing", "don't move that", "round-trip byte-for-byte including layout"). The
  formatter is canonical; only *meaning and spelling* round-trip, not layout.
- **Collapse aligned constant runs to a single space**, or conversely
  **preserve the author's ad-hoc constant spacing** instead of re-aligning
  canonically. Both are explicitly rejected here.
- **Add a formatter option/knob** for layout. The house style is deliberately
  fixed (gofmt posture); configurability is a non-goal.
- **Normalise operand spelling** (`$0A` → `10`, upper/lower-casing hex). Spelling
  is preserved; only placement is canonical.
- **Deep-indent conditional-block bodies.** Bodies stay at the normal column
  (labels col 0, ops col 8) — ACME detects labels by column, so an indented body
  label would stop being a label and break reassembly. Only the `!if`/`}`/`else`
  delimiters sit at column 0.

## Where this lives in the code

- `ast::emit` / `emit_nodes` / `emit_conditional` — the shared canonical
  renderer (Z80 + the U6-migrated CPUs).
- The constant-run re-alignment pass — added to `emit` when the ACME/6502
  formatter lands (the first dialect with pervasive `name = value` constant
  tables).

See [`syntax-stance.md`](syntax-stance.md) (dialect vs encoding) and the AST
plan `docs/plans/2026-07-04-005-feat-ir-ast-layer-plan.md` (U5/U6, KTD5 the fmt
fidelity floor).
