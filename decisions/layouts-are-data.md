# Decision: placement layouts are data, and a project may supply its own

**Status:** Active. Binding for Asm198x (accepted 2026-09-01, #483).
Completes the artifact layer of
[`llvm-alike-legacy-assembly-infrastructure.md`](llvm-alike-legacy-assembly-infrastructure.md)
by generalising at the trigger [`syntax-stance.md`](syntax-stance.md) named;
builds on [`sections-in-the-shared-engine.md`](sections-in-the-shared-engine.md)
and stays inside [`assemble-io-model.md`](assemble-io-model.md) principle 3.

**Date:** 2026-09-01.

## The decision

1. **The engine's placement consumes a layout description** — named memory
   areas and the segments assigned into them, expressed in the `Run` model's
   two dimensions (CPU address, file offset) that
   `sections-in-the-shared-engine.md` already established. A layout is a value,
   not a table compiled into a dialect.
2. **`NES_SEGMENTS` becomes the curriculum default layout expressed as that
   value.** Same bytes, same rejections, no behaviour change for any source
   assembled today.
3. **A bounded ld65 `.cfg` reader produces layout descriptions.** `MEMORY`
   areas and `SEGMENTS` assignments first — enough for the pinned
   `bbbradsmith/NES-ca65-example` config — grown by corpus evidence, never the
   full ld65 grammar up front.
4. **The ca65 CLI takes `-C <file>`**, mirroring ld65's own flag. Absent, the
   curriculum default applies exactly as now.
5. **Segment names validate against the active layout, not a constant.** The
   diagnostic stays the honest one it is today — a segment with no memory area
   has nowhere to go — but the list it checks belongs to the layout in force.
   This is ld65's own behaviour ("Missing memory area assignment for segment
   'RODATA'") rather than a policy of ours.

## Why now

Three pressures point at the same missing piece:

- **The trigger fired.** `syntax-stance.md`, when the bounded NES linker went
  in: "If a second linker config or multi-object linking ever appears, this is
  the point to generalise (parse the `.cfg` …) — not before." #430's pinned
  project builds byte-for-byte under ca65 + ld65 with its own `example.cfg`,
  and our ca65 path refuses it at segment validation. That is the second
  config, arrived.
- **The fixed table is already wrong for one of its own users.** The
  `NES_SEGMENTS` doc comment records that the curriculum ships two `nes.cfg`
  variants, the table is the `dash` layout, and a `meet-the-machine` program
  placing a label in `BSS` would land a page high — held off only because
  nothing does it today. Layouts as data ends the divergence's
  unrepresentability instead of waiting for it to fire.
- **The parity backlog is queued behind the layer.** The largest cluster of
  outstanding words (#272–#275) is section, bank, and output vocabulary. Its
  container half already has its record (`multi-artifact-output.md`); its
  placement half has the shared `Run` model; configured placement is the piece
  that lets the rest land once instead of per-dialect.

## What already exists — this record adds one piece, not a layer

The artifact layer named by `llvm-alike-legacy-assembly-infrastructure.md` is
mostly built, in three standing records:

| Piece | Record |
|---|---|
| Placement model (sections, address vs file offset, overlap policy) | `sections-in-the-shared-engine.md` |
| Containers and multi-output (artifact list, library describes / CLI writes, Format198x graduation) | `multi-artifact-output.md` |
| Container-per-platform table and the no-standalone-linker bar | `packaging-and-cpu-roadmap.md` |

What none of them provides is placement *configured by the project being
assembled*. That is this record, and only that.

## What does not change

- **No object format, no standalone linker, no serialised IR.**
  `assemble-io-model.md` principle 3 and `packaging-and-cpu-roadmap.md` § 3
  hold. The ca65 path stays a fused assemble+link producing the final image;
  parsing ld65's config grammar does not entail adopting its object pipeline.
- **Reference parity governs the surface.** A layout admits no directive a
  dialect's reference lacks; the declared-vs-dispatched invariant from
  `sections-in-the-shared-engine.md` is untouched.
- **Layout facts cite upward.** A console's memory map is a hardware fact under
  the family canon; the iNES layout the default expresses is cited to the
  primary library, not transcribed from another tool's output
  (`multi-artifact-output.md`'s rule, applied to layouts).
- **lwasm's `section`/`sect` object mode stays out**, as
  `sections-in-the-shared-engine.md` already scoped.

## Deliberately deferred

- **A logical-vs-physical location counter** (sjasmplus `phase`/`dephase`/
  `disp`). Related address-model work, its own slice with its own probes.
- **Other layout grammars.** rgbasm's bank types and sjasmplus's
  `device`/`page`/`slot` should ride the same layout-as-data model, but each
  arrives with its own reference probes and its own evidence, not by analogy
  from ld65.
- **Artifact load addresses / entry points** stay open exactly as
  `multi-artifact-output.md` left them.

## Proof

- The existing curriculum differential fixtures pass unchanged under the
  default layout — the refactor is invisible or it is wrong.
- The `bradsmith-nes-ca65-example` corpus target assembles and links with its
  pinned `example.cfg` via `-C`, byte-compared against ca65 2.18 + ld65
  (#430's acceptance). Later independent gaps may surface normally.
- A `meet-the-machine` layout expressed as data places `BSS` at `$0200`,
  pinned by test, so the recorded divergence stops being latent.

## Drift triggers

Re-read this record when any of these appear:

- a new layout hard-coded as a table inside a dialect
- "parse the full ld65 grammar so we only do this once"
- a segment list validated against a constant rather than the active layout
- "linking is real now, so add an object file" — the bar in
  `assemble-io-model.md` principle 3 has not moved
- a layout's addresses copied from another tool's output rather than cited to
  the primary library

## Related decisions

- [`syntax-stance.md`](syntax-stance.md) — the trigger this generalises at
- [`sections-in-the-shared-engine.md`](sections-in-the-shared-engine.md) — the placement model this configures
- [`multi-artifact-output.md`](multi-artifact-output.md) — the container half of the artifact layer
- [`assemble-io-model.md`](assemble-io-model.md) — principles 2 and 3
- [`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md) — the no-standalone-linker bar
- [`llvm-alike-legacy-assembly-infrastructure.md`](llvm-alike-legacy-assembly-infrastructure.md) — the layer this completes
