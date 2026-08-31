# Decision: sections belong to the shared engine, not to the dialects that need them

**Status:** Active. Binding for Asm198x (accepted 2026-08-24). Generalises the bounded
linker sanctioned in [`syntax-stance.md`](syntax-stance.md) at the trigger that
record named. Restores principle 2 of
[`assemble-io-model.md`](assemble-io-model.md) and stays inside its principle 3.

**Date:** 2026-08-24.

## The decision

The engine's placement pass models a program as **a list of sections**. A dialect
with no section concept is a program with exactly one section, based at its
origin.

Sections are an **internal** model. They change nothing about what source
Asm198x accepts.

## Why now

Two dialects — vasm and ca65 — do not run through the shared engine. They are
exactly the two dialects with sections, and that is not a coincidence:
`Assembly` is one `origin` and one contiguous `bytes`, so a dialect with
sections cannot say what it means in that type and has to leave.

**Amended 2026-08-24, after doing it.** That reading was right for ca65 and
only half right for vasm — see "What actually moved" below.

Leaving costs each of them its own layout pass, its own label placement, and its
own construction of the debug section table. Today that is two hand-rolls, which
is tolerable. rgbasm's banks and sjasmplus's `device`/`page`/`slot` are now in
scope, and lwasm's `section` is under scope review; taking them the way vasm and
ca65 were taken makes it five.

`syntax-stance.md` named this exact moment when the bounded NES linker went in:

> If a second linker config or multi-object linking ever appears, this is the
> point to generalise (parse the `.cfg`, add a relocatable object step) — not
> before.

vasm's hunk path was the second layout model and went in without generalising.
This record stops at the trigger rather than passing it a third time.

It also restores `assemble-io-model.md` principle 2, which says there is no
"pasmo engine" or "sjasmplus engine" because front-ends sit over one core. There
is currently a vasm engine and a ca65 engine.

## Why it is safe to model sections everywhere

**Model and surface are gated by different mechanisms.** A dialect reaches an
operation only by declaring its spelling in that dialect's own `DIRECTIVES`
table (`crate::directives`). The IR is already richer than any single dialect's
surface: `Operation::AlignTo` exists and ca65's 65816 front-end cannot reach it,
because ca65-816 does not declare `.align`.

So a section-aware IR gives ACME nothing new to write. Nobody can say
`!section` unless ACME could.

## The guard: no superset by the back door

Reference parity governs the surface, unchanged. The invariant:

> No dialect declares a section directive its reference assembler lacks.

Enforced as a surface invariant beside the existing declared-vs-dispatched
checks, which have caught this class of drift repeatedly.

## What does not change

- **`Assembly` stays flat**, and the `AssemblyResult` contract does not move.
  The twenty-one dialects on the shared engine see nothing. A flat dialect's
  result is its single section, flattened — which is what the engine's own
  documentation already claims it is ("the flat engine is a single implicit
  section 0, based at `origin`").
- **Native output only** (`assemble-io-model.md` principle 3). Sections live in
  the IR and are consumed by the native serialisers. No object format, no
  serialised IR, no standalone linker. That bar is unchanged and is owned by
  `packaging-and-cpu-roadmap.md` § 3.
- **lwasm's `section`/`sect`** is out of this record. It is lwasm's object mode
  for `lwlink`, which is a scope question about object formats, not a placement
  question.

## What this consolidates

Three places currently express a section model the engine refuses to hold:

- `listing.rs` already carries `DebugCapture` and `DebugCaptureMulti` — shared,
  multi-section, collected by both ca65 and vasm. The section-aware layer exists
  one level too high.
- rgbasm's `SECTION` lowers to an `Operation::Org` when it pins an address, and
  to nothing when it does not.
- `Assembly::reserved_prefix` exists because a flat image cannot represent a gap
  at the front of itself (#90).

## Alternatives considered

- **Leave it.** Two hand-rolls is tolerable and the change touches the placement
  pass and the debug capture. Rejected because the scope decision above puts
  three more dialects in the queue, and each departure re-derives the same three
  things independently.
- **Give `Assembly` sections.** Rejected: it is a contract change for
  twenty-one dialects that do not have sections, to serve five that do.
- **A standalone linker and object format.** Forbidden by
  `assemble-io-model.md` principle 3 and out of scope per
  `packaging-and-cpu-roadmap.md` § 3.

## What actually moved

Recorded after implementation, because the prediction above was not quite
right and the next person should not go looking for a consolidation that is
not there.

**The model needed a dimension the record did not anticipate.** A section's
address and its position in the image are different numbers. A Game Boy `ROMX`
section is addressed at `$4000` in whichever bank holds it and lands at
`bank * $4000` in the ROM; a NES `HEADER` is at file offset 0 and is not CPU
addressable at all. So a run carries both, and `image_base`/`image_size` moved
to the dialect as the container facts they are.

**ca65 mapped exactly.** Its `link` placed four segments at file offsets
hard-coded in the function body; those became data beside the addresses they
belong with, and `in_file` stopped being a separate boolean — a segment is in
the file exactly when it has an offset to be at.

**vasm had one third of what the record assumed.** Its flat path moved, and
fixed a wrong claim on the way: `flatten_one_section` refused a second section
because "a flat binary holds one section", which is not what vasm does — it
lays several into one image and refuses only where two *overlap*. But vasm does
not bypass the engine only because it has sections:

- its **hunk executable** is a container — per-hunk headers, relocation blocks,
  longword padding — not an image with sections laid into it, and principle 3
  of `assemble-io-model.md` puts native serialisers in the dialect;
- its **multipass** is branch relaxation, which is an optimiser and not
  placement at all.

Neither belongs in a shared layout. The claim "the dialects that bypass the
engine are exactly the dialects that have sections" should be read as: that is
*a* reason all of them bypass it, and the only one this record addresses.

**The payoff is in the dialects that came after.** rgbasm's sections, its banks
and its ROM sizing were built on the shared model rather than a fourth private
layout, which is the growth this record existed to stop.

**Amended 2026-08-31: address runs are not source sections.** ACME source has no
section directive, but it may move `*` backwards and write several regions in
any order. Those regions reuse the engine's internal `Run` placement model and
still flatten to one image; they do not acquire section names or expose a new
language surface. ACME also permits overlaps, with later written bytes winning,
so overlap policy is dialect-gated while ca65, vasm, and rgbasm retain their
refusal. An `org` gap is not part of a run: placement fills it only after all
written regions have been overlaid, preventing a later gap from erasing earlier
data (#463).

## Drift triggers

Re-read this record when any of these appear:

- "this dialect has sections, so it needs its own layout pass"
- "just lower the section to an `org`"
- adding a `debug198x::Section` list inside a dialect
- a section directive declared for a dialect whose reference lacks it
- any proposal for an object file, a serialised IR, or a standalone linker
