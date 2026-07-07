# Decision: the Debug198x format's evolution policy and freeze governance

**Status:** Active. The format is **public draft v0.1**; the v1 freeze is a
gated event executed via the checklist below.

**Date:** 2026-07-06.

## What this governs

The Debug198x cross-CPU debug-info format — the `.debug198x` NDJSON sidecar
written by asm198x (`--debug`) and read by consumers, first the Emu198x
importer. The format itself is specified in the org docs repo
([`docs/debug198x.md`](https://github.com/asm198x/docs/blob/main/debug198x.md)),
written for external implementers against the conformance fixture corpus
(`crates/asm198x/tests/fixtures/debug198x/`, enforced always-on by
`tests/debug198x_fixtures.rs`). That page describes the format; this record
governs it — why it is the way it is, and under what rules it may change.

Plan: `docs/plans/2026-07-03-001-feat-debug-info-format-plan.md` (U1–U7).
Sibling governance precedent: [`core-contract-freeze.md`](core-contract-freeze.md)
(the draft-then-freeze pattern this record instantiates for the format).
Related: [`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md) (the
single-binary CLI the artifacts ride), [`syntax-stance.md`](syntax-stance.md)
(the dialect surface the `header.dialect` field names).

## The evolution policy

- **Additive, skip-unknown.** New record types and new fields are added without
  a version break; a conforming reader skips unknown `t` values and ignores
  unknown fields (both spec-normative, both fixture-exercised — AE5). The
  `format_version` bumps incompatibly only for a breaking shape change, which
  after the freeze requires a new decision here.
- **Decimal wire, hex rendering.** Numbers stay decimal JSON integers;
  presentation belongs to tools (KTD3).
- **No fabricated address data.** A producer emits `space` qualifiers only from
  actual placement; flat CPUs carry nothing extra (AE3). The banked/paged shape
  is specified and fixture-validated ahead of any emission path populating it.
- **The dependency direction is one-way.** `debug198x` depends on serde only
  and never on `asm198x`; asm198x writes, consumers read (KTD1).

## Draft v0 posture

The format is public from day one — spec page live, fixtures committed — but
carries **draft status: v0.x, subject to change until the first consumer
ships**. Real consumers routinely falsify format designs; the irreversible
additive-evolution promise (R11) waits for that first contact. Until the
freeze, a shape change is permitted with: spec page updated, fixtures
regenerated, and a dated note in this record.

## The freeze checklist

The v1 freeze is executed by appending a dated section to this record
confirming every item:

1. **First consumption has occurred.**
   - *Primary trigger:* the Emu198x importer (milestone
     [emu198x/emu198x #29 "Debug198x importer"](https://github.com/emu198x/emu198x/milestone/29))
     has exercised the reader end-to-end — symbolized disassembly and
     source-anchored breakpoints against a real asm198x-produced sidecar.
   - *Secondary trigger* (usable by explicit decision-record event at the
     bounded review below): the maintainer's own dev-loop consumption plus a
     reference reader exercising all three R9 lookups (`addr_of`, `symbol_at`,
     `line_at`) against the full fixture corpus.
2. **Fixture coverage matches consumption.** Every CPU family the first
   consumer exercised has a fixture, or the gap is named here and its risk
   accepted per family (see *Coverage and accepted gaps*).
3. **The banked fixture's three validation legs are complete.**
   - Leg 1 — cross-bank `line_at`/`symbol_at` lookups exercised as data:
     ✅ done 2026-07-06 (`banked_fixture_resolves_per_paging_state`).
   - Leg 2 — SLD long-address projection table committed alongside the
     fixture: ✅ done 2026-07-06 (`spectrum128-banked-sld.md`).
   - Leg 3 — the fixture's slot/page expectations cross-checked against
     Emu198x's actual Spectrum 128 paging model: ⏳ **pending, cross-repo**
     (belongs to the importer work in the Emu198x session).
4. **A bounded review has passed:** a deliberate pass over the shape against
   the consuming reader — did contact reveal a field that is wrong, missing,
   or misnamed? Fix while draft; only then freeze. The freeze is never an
   automatic flip the moment a consumer parses a file.

**Bounded-review backstop:** if no importer work has started within **six
months** of this record (by 2027-01-06), re-examine here — either schedule the
consumer, invoke the secondary trigger, or explicitly extend the draft with
reasoning. The draft never waits open-endedly by default.

## Dated notes

- **2026-07-07 — multi-file population (language-surface U9).** The
  multi-file source model reached every emission path: `Header.sources` is
  now populated in the producer's `FileId` order — `sources[0]` = the root
  input, one entry per included file in first-inclusion order, the
  `AssemblyResult.files` convention, so one id space spans the contract and
  the sidecar (KTD2) — and each `line` record's `file` names the record's own
  file. An `incbin` payload is one `line` record covering the whole payload
  at the directive's position; binary assets never appear in `sources`.
  **Data-semantics clarification only**: no field or record shape changed and
  no existing golden was regenerated. The corpus grew two always-on
  multi-file families (`z80-spectrum-multifile` — flat engine, include +
  incbin; `6502-nes-multifile` — ca65 linked, included CHR data). Spec page
  updated in the same change per the draft posture above (plan KTD7).

## Coverage and accepted gaps

The v0 corpus covers: z80-spectrum (flat engine + entry symbol), 6502-c64
(acme), 6502-nes (ca65 linker, multi-segment, non-CPU sections), 68000-amiga
(vasm hunks, relocatable), 65816 (24-bit constant), cp1610-intellivision (the
one word-addressed family — decle units), the hand-authored
spectrum128-banked shape fixture, and — since 2026-07-07 — the two multi-file
families (z80-spectrum-multifile: flat include + incbin; 6502-nes-multifile:
ca65-linked included CHR data) pinning per-file line records and ordered
`sources`.

**Accepted gap:** the asl-syntax flat chips (8080, 6800, 1802, 8048, SC/MP,
F8, 2650, TMS7000, PDP-11, TMS9900, Z8000) share the flat engine's single
capture path with the covered Z80/6502 families and introduce no new record
shape; they are accepted as a class, gaining fixtures incrementally as
consumers reach them (R10 — fixture growth is additive and independent of the
freeze).

## Drift triggers

Re-consult this record before:

- adding, renaming, or removing any field or record type in
  `crates/debug198x/src/lib.rs` — spec page and fixtures move in the same
  change, and post-freeze a breaking change needs a new decision here;
- making a consumer depend on a field the spec marks informational
  (`tool`/`tool_version`);
- emitting a `space` qualifier from a new machine — the paged shape is
  specified; population must match actual hardware placement, and the Emu198x
  paging cross-check (leg 3) is the arbiter for the Spectrum 128;
- treating the `--sym`/`--listing` text renderings as part of this format —
  they are CLI conveniences over the record and carry no stability promise.
