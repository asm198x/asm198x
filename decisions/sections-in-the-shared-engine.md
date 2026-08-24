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

## Drift triggers

Re-read this record when any of these appear:

- "this dialect has sections, so it needs its own layout pass"
- "just lower the section to an `org`"
- adding a `debug198x::Section` list inside a dialect
- a section directive declared for a dialect whose reference lacks it
- any proposal for an object file, a serialised IR, or a standalone linker
