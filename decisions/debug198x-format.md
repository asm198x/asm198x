# Decision: the Debug198x format's evolution policy and freeze governance

**Status:** Active. The format is **frozen at v1** as of 2026-08-18 — the
additive-evolution guarantee is now an irreversible promise. See *The v1 freeze*
at the end of this record for the evidence each checklist item was confirmed
against. The checklist below is kept as the gate that was applied, not as
outstanding work.

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
     Emu198x's actual Spectrum 128 paging model: ✅ done 2026-08-18
     (emu198x/emu198x#986, `454baf55`; verdict recorded beside each claim in
     `spectrum128-banked-paging-crosscheck.md`).
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

- **2026-08-18 — `space` shapes made forward-compatible; the `bank` shape
  withdrawn (bounded review, checklist item 4, second pass).** With legs 1–3
  complete the Emu198x session was asked to say whether anything looked thinner
  than the checklist implied, and it did: `Space::Bank` was exercised by
  **nothing** — no fixture in the corpus emits it, no producer populates it, no
  consumer matches it. The one thing pinning it was a serialization round-trip
  unit test, which fixes the wire shape and says nothing about whether the shape
  is right. `65816-sample.debug198x`, the fixture that should have exercised it,
  carries `base: 0` and expresses its one far address as a plain `const`.

  Checking how expensive it would be to correct `bank` after a freeze surfaced
  the larger problem. `Space` is `#[serde(untagged)]`, so a shape a reader does
  not recognize is not skipped — it is a **hard parse error that fails the whole
  file**:

  ```
  {"space":{"segment":7,"window":2}}  ->  data did not match any variant of untagged enum Space
  {"t":"quantum","whatever":1}        ->  skipped, as designed
  ```

  The evolution policy promises that *record types* and *fields* are additive,
  and AE5 pins exactly that. A new **variant of an existing enum** is neither, so
  the promise was not broken on its face — but the consequence was that the set
  of `space` shapes would close permanently at v1, uniquely among the format's
  parts, and nothing said so. Discovered by probe, not by reading the policy.

  Two changes, both while draft:

  - **`Space::Unknown` carries an unrecognized shape verbatim.** Skip-unknown
    applied one level down: the record still resolves through its section, the
    qualifier is treated as uninterpretable, and a rewritten file preserves it.
    The cost is strictness — a misspelled qualifier lands in the catch-all rather
    than being rejected — which is the right trade for a data contract whose
    consumers must not refuse files they only partly understand.
  - **`bank` is withdrawn.** Unexercised, and redundant on this model: a 65816
    address resolves to its full 24-bit value (the fixture's `FARBUF` is 98304 =
    `$018000`), so the bank byte is the top 8 bits of something the reader
    already has. `Paged` earns its place because two pages genuinely share a CPU
    address; no equivalent case was ever shown for `bank`. With the catch-all in
    place, adding a real banked shape once a 65816 consumer has a requirement is
    safe — and a file carrying the old shape still loads.

  Freezing v1 with `{slot, page}` alone means freezing the one shape that is
  fixture-pinned, hardware-cross-checked against a real `Memory128K` (leg 3), and
  consumer-exercised.

  **Cost:** `Eq` is gone from `Space`, `SymbolKind`, `Symbol`, `Section`,
  `DebugInfo`, and — cascading through the debug slice — `asm198x`'s `DebugData`
  and `AssemblyResult`. Arbitrary JSON has no total equality. `PartialEq` remains
  everywhere; nothing used these as map keys or set members. `Header` and
  `LineSpan` keep `Eq`. The `AssemblyResult` change touches the core contract,
  which is public draft under
  [`core-contract-freeze.md`](core-contract-freeze.md) and outside the v1.0
  promise per [`v1-scope.md`](v1-scope.md), so it costs no stability claim.

  Every generated golden is unchanged: no producer emitted `bank`, so no fixture
  moves. Spec page updated in the same change per the draft posture.

- **2026-08-18 — `space` on `Section` (bounded review, checklist item 4).**
  First consumption (the Emu198x importer, emu198x/emu198x#741) reported the
  banked fixture's two symbols as indistinguishable
  ([#71](https://github.com/asm198x/asm198x/issues/71)). The reported defect was
  not one: the base map already *is* the paging channel — map only the sections
  paged in and both banks resolve correctly, which
  `banked_fixture_resolves_per_paging_state` has pinned since 2026-07-06 and the
  spec page states outright. The importer had mapped both pages into slot 3 at
  once, a state the hardware cannot be in, and read the resulting record-order
  answer as a reader bug.

  What contact did reveal is the field behind that mistake: **paging was
  expressible but not discoverable.** `space` lived only on address-kind
  symbols, so a consumer holding a real paging state had no stated section →
  (slot, page) mapping to build the base map from — only the inference "scrape
  any symbol out of the section", which a section holding nothing but `line`
  records defeats entirely. `Section` now carries an optional `space` with the
  same two shapes, making the mapping a lookup rather than an inference, and
  giving a `LineSpan` the qualifier it has no room to carry itself. Precedence:
  a record's own `space` is the finer truth where it carries one; the section's
  is the section-wide default.

  **Additive on the wire** — the field is skipped when absent, so every
  generated golden is byte-identical and older readers are unaffected (AE3's
  no-fabrication rule now applies at both levels, asserted for sections as well
  as symbols). Source-breaking for anyone building a `Section` from a struct
  literal, which the draft posture permits. Spec page updated and the hand-authored
  `spectrum128-banked` fixture carries section spaces in the same change;
  `section_space_yields_the_base_map_for_a_paging_state` derives the base map
  from a paging state and asserts an absent page maps nothing rather than
  answering from whichever bank sorts first.

  The crate's own rustdoc gained the banked contract too. It had lived on the
  spec page and in a test but not on `BaseMap` or the three lookups — and the
  first consumer read the crate, not the page. That gap is the reason a
  competent importer got it wrong, and closing it is the durable half of this
  note.

- **2026-08-19 — a trimmed leading gap gets its own section (#90).** On the
  asl-family dialects, `p2bin` starts the image at the lowest *written*
  address, so a leading `org` gap or reservation is absent from the file
  rather than padding it. Fixing the byte-level divergence moves
  `Assembly::origin` above where the source's own addresses start, and that
  collides with the format: `SymbolKind::Label` carries an **unsigned**
  section-relative `offset`, so a label in the dropped region —
  `buf: ds 256` before the code, ordinary assembly style rather than a corner
  case — has no representable location.

  The dropped region therefore becomes **its own section**: `name: "reserved"`,
  `base` at the source's original origin, contributing no bytes to the image.
  Symbols below the boundary are placed in it; line spans for it are dropped,
  since a span describes bytes that exist and these do not. This is a usage
  decision, **not a format change** — no field, record, or `format_version`
  moved, and the freeze holds. It uses `sections` as the list it already is,
  the way a BSS region has always been expressed.

  Two consequences worth stating, because both are the kind that bite later:

  - **`main` keeps section id 0.** The approved sketch numbered the reserved
    section 0 and pushed `main` to 1, which reads better in isolation but
    would renumber the one section every other sidecar has — changing the
    output of every flat assemble on every dialect to accommodate a case
    almost no source hits. A sidecar with no leading gap is byte-identical to
    what shipped before this note; the reserved section takes the next free id
    and appears only when there is one.
  - **A consumer must not assume one section means one contiguous image.**
    Emu198x reads these. A `reserved` section has a `base` and no bytes, so
    anything that maps sections to file offsets by accumulating lengths needs
    to skip it. That is already true of the ca65 and vasm linked paths, so it
    is not a new class of thing — but it now reaches the flat path, which is
    where a consumer is most likely to have assumed otherwise.

  `AssemblyResult::reserved_prefix` carries the boundary to the emission
  layer, additive under R7 and omitted from the JSON when zero.


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

## The v1 freeze

**Executed 2026-08-18.** Every checklist item confirmed below, against evidence
rather than assertion. From this date the additive-evolution guarantee (R11) is
irreversible: new record types and new fields are added without a version break,
a conforming reader skips unknown `t` values and ignores unknown fields, and a
**breaking** shape change requires a new decision in this record plus a
`format_version` bump and a migration path.

### Item 1 — first consumption has occurred

The primary trigger, not the secondary one. The Emu198x importer
(emu198x/emu198x#741, merged) exercises the reader end-to-end against a real
asm198x-produced sidecar (`border-walk.debug198x`, a C64 6502 build), through
both consumption modes the trigger names: **symbolized disassembly**
(`DebugSymbols::symbolise`) and **source-anchored breakpoints**
(`addr_of_line`). The relocatable leg — per-section base maps from actual load
addresses — is exercised separately against `68000-amiga.debug198x` with hunks
placed out of order, so an adjacency assumption cannot hide (emu198x/emu198x#987).

### Item 2 — fixture coverage matches consumption

The consumer exercises the 6502 C64 shape and the Spectrum 128 banked shape;
the corpus covers both. **Named gap, risk accepted:** the corpus holds nine
shapes and one consumer reads two of them. NES, both multi-file families, 65816,
CP1610, plain z80-spectrum and 68000-amiga are pinned by the corpus as a
*producer* contract and have no reader. That is the corpus's job — it pins what
asm198x writes — and R10 keeps fixture growth additive and independent of this
freeze, so a later consumer finds its family already covered.

### Item 3 — the banked fixture's three validation legs

All complete. Legs 1 and 2 on 2026-07-06; **leg 3 on 2026-08-18**
(emu198x/emu198x#986, `454baf55`): six tests driving a real `Memory128K` into
each paging state, no ROMs and no skip path, with nothing asserted from the
spec — every address the sidecar claims is checked by reading the byte back
through the same `MemoryBus` the CPU uses. All five hardware claims hold,
including the load-bearing one (a slot holds one page at a time), whose failure
would have meant the format's model was wrong rather than the fixture's. Verdict
recorded per claim in `spectrum128-banked-paging-crosscheck.md`.

### Item 4 — the bounded review has passed

It ran **twice**, and earned its keep both times. This is the item the record
insisted was never an automatic flip, and it was right to.

- **First pass** (`space` on `Section`): contact revealed that paging was
  expressible but not discoverable — `space` lived only on address-kind symbols,
  so a consumer holding real paging state had no stated section → (slot, page)
  mapping to build a base map from. The reported defect (#71) was *not* one; the
  gap behind it was.
- **Second pass** (`space` shapes made forward-compatible; `bank` withdrawn):
  asked whether anything looked thinner than the checklist implied, the consumer
  found `Space::Bank` exercised by nothing — no fixture, no producer, no reader.
  Probing how expensive a post-freeze correction would be then exposed the
  larger problem: `#[serde(untagged)]` made an unrecognized shape a hard parse
  error, so the *set of space shapes* would have closed permanently at this
  freeze, uniquely among the format's parts, with nothing saying so.

Both were fixed while draft. Freezing today therefore freezes one space shape,
`{slot, page}` — fixture-pinned, hardware-cross-checked, consumer-exercised, and
guarded against the join-key error — with a catch-all that keeps the shape set
open for whatever a later machine needs.

### What is deliberately *not* changed: `format_version` stays `"0.1"`

The freeze is a change of **promise**, not of shape. Nothing about the wire
format moved today, so bumping the version string would misinform every reader:
the spec's own rule is *"branch on `format_version` for a breaking change;
additive changes never bump it"*, and a reader that sees the number move is
entitled to conclude a shape it knows has changed.

It would also break real consumers for no reason. The Emu198x importer validates
`format_version` by **exact match** against the crate constant
(`crates/emu198x-shell/src/debug_info.rs:163`), so a bump makes every
post-freeze sidecar unreadable to any build pinned before it — including the
released Emu198x v0.3.0 — in exchange for no new information.

So `FORMAT_VERSION` stays `"0.1"`, and `"0.1"` now denotes the frozen v1
specification. If that pairing later reads as confusing enough to be worth the
migration, changing it is a breaking change and needs its own decision here,
with the consumer coordinated rather than surprised.

## Drift triggers

Re-consult this record before:

- adding, renaming, or removing any field or record type in
  `crates/debug198x/src/lib.rs` — spec page and fixtures move in the same
  change. **The format is frozen (2026-08-18): additive is still free, but a
  breaking shape change now needs a new decision in this record, a
  `format_version` bump, and a migration path.** "It is still 0.1, so it is
  still draft" is the misreading to watch for — the string is the frozen v1
  spec's version, see *The v1 freeze*;
- **bumping `FORMAT_VERSION`** — it is deliberately unchanged at the freeze, and
  moving it tells every reader a shape they know has changed *and* breaks the
  Emu198x importer's exact-match check. Its own decision, with the consumer
  coordinated first;
- making a consumer depend on a field the spec marks informational
  (`tool`/`tool_version`);
- emitting a `space` qualifier from a new machine — the paged shape is
  specified; population must match actual hardware placement, and the Emu198x
  paging cross-check (leg 3) is the arbiter for the Spectrum 128;
- treating the `--sym`/`--listing` text renderings as part of this format —
  they are CLI conveniences over the record and carry no stability promise.
