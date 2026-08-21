> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: Declared Directive Surface - Plan
type: feat
date: 2026-08-18
topic: declared-directive-surface
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: grounding-survey
execution: code
---

# Declared Directive Surface - Plan

## Goal Capsule

- **Objective:** Make each dialect's directive vocabulary **declared data the parser dispatches from**, so it can be read by a generator and cannot drift from what the assembler accepts. Unblocks generated per-dialect documentation — a v1.0 bar item — without creating the hand-maintained second source of truth the docs-site plan exists to prevent.
- **Product authority:** Steve Hill. Seeded by the 2026-08-18 grounding survey below, prompted by the observation that the directive matrix `decisions/v1-scope.md` puts downstream of the verdict corpus cannot actually be generated from it.
- **Open blockers:** One decision inside the plan — the sigil convention (see *Outstanding Questions*). Everything else is settled by the survey.

---

## Product Contract

### Summary

Every dialect's directive vocabulary today exists **only** as `match` arms inside its `parse_op`. Nothing machine-readable states which directives a dialect accepts. This plan introduces a declared surface — a per-dialect list of directive **patterns** — and rewires each parser to dispatch through it, so the declaration is the only way a spelling can be accepted and is therefore provably complete. Scope is deliberately **spelling-level**: which directives exist, in which dialect, in which category. Semantics stay in code.

### Problem Frame

`v1-scope.md` puts per-dialect directive matrices on the v1.0 bar, generated rather than hand-authored, because hand-writing 21 dialect pages is the drift the docs-site plan was written to prevent. But the matrix has three inputs and only one of them is the conformance corpus:

- **our side** — which directives each front-end accepts;
- **agreement** — which of those are arbitrated against the reference (the corpus / the differential probes);
- **known gaps** — they-accept/we-reject, already carried by `gap(dialect, note, body, issue)` in `tests/differential.rs`.

The corpus is keyed on **source text**, not directives, so it cannot supply the first. Tagging verdicts with directives derived from our own parser looks like the fix and is not: the most valuable cells are divergences, and our parser cannot tag what it rejects. The spine has to come from our own surface — and that surface is not declared anywhere.

A generator that greps `match` arms would be exactly the rot-prone parallel truth the docs-site plan forbids. The declaration must be load-bearing, not descriptive.

### Grounding survey (2026-08-18)

Measured across `crates/asm198x/src/dialects/`:

- **206 directive arms, 175 distinct spellings**, in 23 of 24 dialect files. The four with no dispatch of their own delegate rather than differ (`pasmo`/`sjasmplus` → `z80::assemble` parameterised by `Z80Syntax`; `ca65_flat`/`mos6502` are shared infrastructure).
- **Five shapes, three of them families:**
  - *Exact spelling sets* — the bulk.
  - *Stem + size families* — vasm only: `strip_prefix("dcb"|"dc"|"ds")` + a size table, order-dependent (`dcb` must be tested before `dc`), with bare `dc` ≡ `dc.b`.
  - *Sigil families* — acme `!x` (6 arms), ca65 `.x` (13 arms).
  - *Argument-guarded* — **2 of 206**: z80's `end` (bare = ignored, `end <addr>` = `Operation::Entry`) and acme's `}`.
  - *Not a directive* — acme's `}` block close.
- **The same family appears both enumerated and generated.** asl-family, cp1610, pdp11 and tms9900 write `"db" | "defb" | "dc.b"` and `"dw" | "defw" | "dc.w"` longhand; vasm generates `dc.{b,w,l}` from a stem. They are one concept with different size sets — the 8-bit dialects enumerate only the sizes their CPU has.
- **Accept-and-ignore is structurally distinct and mechanical to detect.** All 12 asl-family dialects carry exactly one arm returning `Ok(None)`.

  *Superseded 2026-08-18 by #85.* At survey time those lists were a shared core
  of `cpu`/`end`/`title`/`page`/`name` plus ad-hoc per-chip extras — and probing
  every spelling against every chip showed they were close to **uncorrelated**
  with what asl accepts, in both directions. `name` was in all twelve and
  accepted by none; `cseg` and `sect` were accepted nowhere; `text` was on the
  wrong chip; `listing` and `aseg` are accepted on all twelve and were ignored by
  four and one respectively. #85 aligned them, so the current lists are
  `cpu`/`end`/`title`/`page`/`aseg`/`listing` plus `relaxed` (cp1610) and
  `supmode` (z8000).

  Two things this plan should carry forward. The remaining per-chip entries are
  the last unprobed inconsistency, tracked with the wider semantic pseudo-op
  question in [#87](https://github.com/asm198x/asm198x/issues/87). And the drift
  itself is the argument for this plan: twelve dialects sharing one arbiter
  disagreed with it and with each other, invisibly, until something asked the
  arbiter the same question twelve times. A declared surface probed against its
  reference is what makes that class of error impossible rather than merely
  findable.
- **Sigil handling is already inconsistent in-tree.** acme and ca65 keep the sigil *in* the matched spelling (`"!zone"`, `".incbin"`); the sjasmplus conditionals landed 2026-08-18 strip it *before* matching so dotted and undotted share one arm. Both work; they are different models of one idea.
- **Spellings are not always identifier-shaped.** acme accepts numeric aliases — `"byte" | "by" | "8"`, `"word" | "wo" | "16"`.

### Key Decisions

- **The declaration is a pattern list, not a string list.** Families are first-class, because three of the five shapes found are families. A flat string set would either lose why `dc.b`/`dc.w`/`dc.l` are one directive, or force vasm to enumerate a cross-product it currently generates.
- **vasm's stem+size form is the general case; exact sets are the degenerate case.** This inverts the survey's first instinct (*"let vasm opt out"*). vasm's encoding says the size is a **parameter**, handles bare `dc` explicitly, and scales to any size-suffixed ISA; the 8-bit dialects are the same model written out by hand with a smaller size set. Adopting the family form lets both express the concept identically, and a future Motorola-family target inherits it.
- **The table is the only way in.** Dispatch becomes `match lookup(word) { … }` rather than `match word { … }`, so a spelling the table does not contain cannot be accepted. Completeness is then a property of the code, not a discipline to maintain — a table kept *alongside* the match would re-create the two-sources-of-truth problem this exists to remove.
- **Spelling-level scope only.** "Which directives does dialect X accept" survives every complication the survey found: z80's `end` appears in the row whatever its argument does, and vasm's row lists `dc.b`/`dc.w`/`dc.l` without the table knowing they come from a prefix rule. Declaring *semantics* is where uniformity breaks, and the matrix does not need it.
- **Category is declared, not inferred.** At minimum `Operation` versus `Ignored` (accept-and-ignore), because documenting `title` as a supported directive when it is a no-op would be worse than omitting it. The survey shows this is mechanical to derive today, so it costs nothing to state.
- **Block syntax is not a directive.** acme's `}` is parsed in the same place but is not a vocabulary entry; the declaration must be able to *not* claim it.
- **Behaviour is preserved everywhere except the sigil decision.** This is a restructuring, not a feature. The differential and conformance suites are the guard, and any byte-level change is a defect of this work — except where normalising sigils deliberately widens or narrows an accepted spelling, which is called out per dialect.

### Requirements

- **R1.** A `Directive` pattern type expressing at least: `Exact { spellings }`, `Sized { stem, sizes, bare }`, and the sigil form settled by the Outstanding Question. Each entry carries a category (`Operation` | `Ignored`) and a stable id for a generator to key on.
- **R2.** A per-dialect declared surface, reachable without parsing source text.
- **R3.** Dispatch flows *through* the declaration in every converted dialect — no spelling accepted that the declaration does not contain.
- **R4.** A test proving R3 for every dialect: every declared spelling is accepted, and the parser rejects a spelling absent from the declaration.
- **R5.** Byte-identical output across the whole corpus and curriculum before and after conversion, except where the sigil decision deliberately changes an accepted spelling.
- **R6.** vasm expresses its families as families — not an enumerated cross-product.
- **R7.** An accessor a documentation generator can consume: for each dialect, its directive entries with spellings expanded and categories attached.

### Scope Boundaries

**In scope:** the directive vocabulary and its dispatch; the declaration types; the completeness test; the generator-facing accessor; the sigil convention decision.

**Out of scope:**

- **Directive semantics.** What `ds` *does* stays in code. The declaration says a dialect accepts it, not what it means.
- **Instruction mnemonics.** Those come from the `isa` spec and already have a declared form.
- **The reference side of the matrix.** Whether a reference agrees comes from the corpus and the `gap()` markers, not from here.
- **Rendering the matrix.** That belongs to the docs-site plan's slot; this supplies the spine.
- **Argument-level modelling.** z80's `end` guard stays in code; the declaration records the spelling.

### Dependencies / Assumptions

- Anchors verified 2026-08-18; the survey above is the grounding.
- No dependency on the verdict pipeline (#61). This is the *other* input to the matrix and can proceed alongside or before it.
- The differential and conformance suites run in full on the maintainer's machine (all reference tools present) and are the correctness guard for R5. On CI they do not run — until #61 lands, R5 verification is single-machine, which is precisely that plan's problem and not this one's to fix.
- `Item::Native` dialects (vasm, ca65-NES) parse their own text; their declarations describe the same vocabulary even though the dispatch site differs.

### Outstanding Questions

- ~~**The sigil convention — decide before U5.**~~ **Decided 2026-08-21**, see U5. Two models existed in-tree: sigil-in-the-spelling (acme `"!zone"`, ca65 `".incbin"`) and sigil-stripped-before-match (sjasmplus conditionals, 2026-08-18). A declared surface must pick one. Stripping is tidier for a matrix (one row per directive, sigil as a dialect property) but changes what some dialects accept — if acme strips `!`, does bare `zone` become valid? It must not. So stripping needs to be *conditional on the dialect requiring the sigil*, which is a third model and the likely answer. **This is the one place the plan can change behaviour, so it is decided explicitly, per dialect, with probes.**
- Whether `Ignored` entries need a per-spelling reason for documentation, or whether one category is enough.
- ~~Whether a **`KnownUnsupported`** category is needed.~~ **Answered 2026-08-21: yes.** [#87](https://github.com/asm198x/asm198x/issues/87) asks what to do with asl's semantic pseudo-ops (`radix`, `phase`, `align`, `charset`, …): they cannot be ignored without mis-assembling, and today they fail as *unknown mnemonics*, which misdescribes the problem.

  A second instance settles it. `include` is unimplemented for pasmo, and the diagnostic is `unknown instruction INCLUDE` — which tells a reader their source is invalid when real pasmo assembles it. The two cases differ in cause (a semantic pseudo-op we decline to fake; a directive not yet written) and are identical in effect: the assembler reports "no such thing" for something that demonstrably is a thing.

  A reader cannot act on that. "Not supported yet" is a decision about their project; "not valid syntax" is a bug report they will write against their own source. The category exists to keep those apart, so the declaration carries the spelling with the reason it is refused, and the diagnostic can say so.

  It also makes the gap countable, which is the point of the surface: a `KnownUnsupported` row is visible in a generated matrix, where a missing row is only discoverable by tripping over it — which is how the pasmo gap was found.
- Whether the stable id in R1 is the canonical spelling or a separate symbol — matters only when two dialects share a spelling with different meanings (`end`).

### Sources

- Grounding survey, 2026-08-18 (this document).
- [`decisions/v1-scope.md`](../../decisions/v1-scope.md) — puts generated directive matrices on the v1.0 bar and downstream of the corpus.
- [`docs/plans/2026-07-04-004-feat-docs-site-plan.md`](2026-07-04-004-feat-docs-site-plan.md) — "generate, don't hand-write"; the directive-matrix slot this feeds.
- [`docs/plans/2026-07-03-002-feat-verdict-pipeline-plan.md`](2026-07-03-002-feat-verdict-pipeline-plan.md) — the corpus, and why it cannot supply this spine.
- [`decisions/ast-native-payload-for-multipass-cisc.md`](../../decisions/ast-native-payload-for-multipass-cisc.md) — why vasm and ca65-NES parse their own text.

---

## Planning Contract

### Key Technical Decisions

- **KTD1. Convert vasm second, not last.** The family form is the design's biggest assumption; vasm is the only dialect that exercises it today. If the pattern model is wrong, that must surface at two dialects converted, not twenty-two.
- **KTD2. The declaration lives with its dialect.** A per-dialect `const DIRECTIVES: &[Directive]` beside the parser, not a central registry — the shared engine already parameterises per-dialect behaviour through the `Dialect` trait, and a central table would need a name-collision scheme for spellings that mean different things (`end`).
- **KTD3. Lookup returns a declared id, and the existing arm bodies match on that id.** The bodies do not move. This keeps each conversion mechanically reviewable and keeps the diff readable against a suite that guards bytes.
- **KTD4. `Dialect::directives()` is the generator seam** (R7), defaulting to empty so unconverted dialects are visibly absent rather than silently wrong.

### Sequencing

**U1 → U2 → U3 → U4 → U5 → U6.**

The types and one small conversion prove the shape (U1); vasm proves the family form before the bulk (U2, per KTD1); the asl-family bulk is the repetitive middle (U3); the remaining dialects carry the awkward cases (U4); the sigil decision lands as its own change because it is the only behaviour-visible one (U5); the completeness test and generator seam close it (U6).

---

## Implementation Units

### U1. `Directive` pattern types + lookup, proved on one dialect

- **Landed 2026-08-21.** `crates/asm198x/src/directives.rs` carries `Directive`, `Pattern::Exact`, `Category` (including `KnownUnsupported`, per the answered question below) and `lookup`. cdp1802 dispatches through it. R5 verified: the differential suite and the full corpus replay are byte-identical. `Pattern::Sized` is U2's to add, when vasm needs it.
- **Goal:** The declaration types exist and one dialect dispatches through them.
- **Requirements:** R1, R2, R3 (one dialect).
- **Files:** a new `crates/asm198x/src/directives.rs`; `crates/asm198x/src/dialects/cdp1802.rs` (8 arms, one ignore arm — the smallest complete example).
- **Approach:** Define `Directive`, its category, and `lookup(&[Directive], word) -> Option<DirectiveId>`. Convert cdp1802's `match word` to `match lookup(…)`, arm bodies unchanged.
- **Test scenarios:** cdp1802 assembles byte-identically across its fixtures and differential probes; a spelling absent from the declaration is rejected.

### U2. vasm — the stem+size family form

- **Landed 2026-08-21.** The central assumption holds: `Pattern::Sized { stem, separator, sizes, bare }` expresses `dc`/`dcb`/`ds` as three entries carrying their size vocabulary, and the types needed no revision.
- **Better than the unit asked for.** It required the lookup to *preserve* the `dcb`-before-`dc` ordering as a property of matching. Splitting the word at the separator and matching the **stem** removes the constraint instead: `dcb.w` yields the stem `dcb`, which only one entry claims, so the entries can be declared in any order. `dc` is deliberately declared first, and a test asserts `dcb.w` still reaches `dcb`.
- **A refinement the unit did not anticipate.** Matching recognises the stem and leaves the suffix to the arm body, so `dc.x` still reports `bad data size` instead of falling through to be refused as an unknown mnemonic. `sizes` therefore documents rather than enforces — which is what R7 needs it for anyway.
- **R5 verified:** differential against real vasm, the full corpus replay, and 627 curriculum comparisons with zero new verdicts.
- **Goal:** The family form carries a real generated vocabulary.
- **Requirements:** R1, R6; R5 for vasm.
- **Dependencies:** U1.
- **Files:** `crates/asm198x/src/dialects/vasm.rs` (`parse_op` `:1933-1965`, `data_size` `:2017`).
- **Approach:** Express `dc`/`dcb`/`ds` as `Sized` entries with the `{b,w,l}` size set and bare-form default; the lookup preserves the existing ordering constraint (`dcb` before `dc`) as a property of matching, not of arm order.
- **Test scenarios:** every `dc`/`dcb`/`ds` size spelling assembles as before; bare `dc` still means `dc.b`; `dcb.w` does not parse as `dc` with size `b.w`; the Amiga hunk fixtures and vasm differential probes are byte-identical.
- **Verification:** if the family form cannot express vasm cleanly, stop and revise the types — this is the unit that tests the plan's central assumption.

### U3. The asl-family bulk (12 dialects)

- **Landed 2026-08-21.** All twelve dispatch through their declaration. R7's accessor lands with them: `directives::surfaces()` names every converted dialect and its entries, with invariants held across all of them — unique ids, no spelling claimed twice, every entry spelled at least once, and every declared spelling reaching its own entry.
- **`Exact`, not `Sized`, for the `dc.b`/`dc.w` aliases.** The unit expected `Sized` "where U2's form starts paying for itself". It does not fit: probed, these dialects accept `dc.b` and `dc.w` and neither bare `dc` nor `dc.l`, and the two land in *different* operations (`Bytes` and `Words`). vasm's `dc.b/.w/.l` are one concept with a width parameter; asl's are two concepts that happen to share a stem. So they are declared by concept — `bytes` spelled `db`, `defb`, `dc.b` — which is what a matrix should show.
- **R5 verified:** differential, conformance including its reference-tool runs, and the full corpus replay across 22 instruction sets.
- **Goal:** The uniform middle, converted.
- **Requirements:** R2, R3, R5.
- **Dependencies:** U1, U2.
- **Files:** `i8080`, `m6800`, `cdp1802` (done in U1), `i8048`, `scmp`, `s2650`, `tms7000`, `f8`, `cp1610`, `pdp11`, `tms9900`, `z8000`.
- **Approach:** Mechanical, one shape. Each dialect's single accept-and-ignore arm becomes `Ignored` entries; the `dc.b`/`dc.w` aliases become `Sized` entries with that CPU's size set, which is where U2's form starts paying for itself.
- **Test scenarios:** the full conformance and differential suites, byte-identical.

### U4. The remaining dialects

- **Landed 2026-08-21**, with one piece deliberately left: lwasm, acme, the ca65 family (ca65, 65816, huc6280), rgbasm and the shared z80 base are converted. Eighteen dialect surfaces are declared.
- **The z80 base is a base, not a surface.** `COMMON_DIRECTIVES` carries what pasmo and sjasmplus share, and is not registered in `surfaces()` under either name, because they are not the same: sjasmplus adds `INCLUDE` and the conditionals, and pasmo adds nothing — which is exactly the gap that started this work. Composing each dialect's own entries on top of the base is the remaining piece, and it is what would let `surfaces()` *state* the pasmo include gap rather than leave it to be found by assembling a multi-file project.
- It also retired a duplication: `is_common_directive` and `common_directive` carried the same eleven spellings separately, so adding one meant remembering both.
- **lwasm** spells several concepts two ways — `fcb` and `.byte` are the same directive — so those are alternative spellings rather than a sigil applied to a name, and `Exact` carries both.
- **acme is the first real `Sigilled` user, and the first dialect whose dispatch is split.** Its directives are read in three places: the data and layout ones in `parse_directive`, `!src`/`!bin`/`!zone` walk-handled in `AcmeEval::lower` because a zone switch is evaluation state, and the conditionals in the scanner before parsing. The declaration covers all of them, because it describes the dialect rather than any one parser; only the first group dispatches from it. The original loud fall-through is preserved for a misrouted directive.
- **The sigil is put back for the lookup.** `parse_directive` receives the name already stripped, and matching the bare name would quietly make the sigil optional — the exact behaviour change U5's probes ruled out. Verified: bare `byte` is still refused.
- **R5 verified:** 627 curriculum comparisons including 138 C64 sources against real acme, plus differential, conformance and the corpus replay.
- **Goal:** acme, the ca65 family, rgbasm, z80, lwasm converted.
- **Requirements:** R2, R3, R5.
- **Dependencies:** U3.
- **Files:** `acme.rs`, `ca65.rs`, `ca65_816.rs`, `ca65_huc6280.rs`, `rgbasm.rs`, `z80.rs`, `lwasm.rs`.
- **Approach:** The awkward cases live here: acme's numeric aliases (`"8"`, `"16"`) are ordinary exact spellings; acme's `}` is **not** declared; z80's `end` is declared once with its guard left in code. `pasmo`/`sjasmplus` inherit z80's declaration through `Z80Syntax` rather than declaring their own.
- **Test scenarios:** the curriculum suite (689 `.asm` files) byte-identical; acme's `!8`/`!16` still assemble; z80 `end` with and without an argument unchanged.

### U5. The sigil convention

- **Decided and typed 2026-08-21.** The third model, as the plan suspected: the sigil is a declared property of the entry carrying whether it is **required**. `Pattern::Sigilled { sigil, names, required }` is in `directives.rs` with tests; applying it per dialect is what remains of this unit.
- **Settled by probe, not by preference.** Ours and the reference tools, all three agreeing:

  | Dialect | Sigilled | Bare |
  |---|---|---|
  | acme | `!byte` accepted | refused — real acme: *"Label name not in leftmost column"* |
  | ca65 | `.byte` accepted | refused — real ca65: *"':' expected"* |
  | sjasmplus | `.if` accepted | `if` accepted too |

- **Why not strip everywhere.** In acme and ca65 a bare `byte` is a valid *label definition*. Stripping would not merely accept an extra spelling; it would change what a label means. The plan asked "if acme strips `!`, does bare `zone` become valid? It must not" — the probes show why that is a correctness answer rather than a taste one.
- **Why not sigil-in-the-spelling everywhere.** sjasmplus takes both forms, so every conditional would need two entries and the matrix would lose the tidiness that motivated stripping.
- **Goal:** One model for sigils across the tree, decided and applied.
- **Requirements:** R1, R2; the Outstanding Question.
- **Dependencies:** U4.
- **Files:** `directives.rs`, `acme.rs`, the ca65 family, `z80.rs`.
- **Approach:** Decide per the Outstanding Question — most likely a dialect-level *required* or *optional* sigil, so acme's `!` stays mandatory (bare `zone` must not become valid) while sjasmplus's `.` stays optional. **Probe each affected dialect's reference before changing what is accepted**, and record the probes; this is the only unit that can alter behaviour.
- **Test scenarios:** for each dialect, a bare spelling is accepted or rejected exactly as its reference does; the sjasmplus dotted/undotted mixing landed 2026-08-18 is preserved.

### U6. Completeness test + the generator seam

- **Goal:** R3 is proven, and a generator can read the surface.
- **Requirements:** R4, R7.
- **Dependencies:** U5.
- **Files:** `crates/asm198x/src/dialect.rs` (`directives()`), a new test module.
- **Approach:** For every dialect: assert each declared spelling is accepted, and that a spelling outside the declaration is rejected. `Dialect::directives()` returns the entries with families expanded and categories attached.
- **Test scenarios:** a spelling removed from a declaration makes its dialect's tests fail; a dialect with no declaration is visibly absent from the accessor, not silently empty.

---

## Verification Contract

- **The guard is the existing suites.** Conformance, differential and curriculum must be byte-identical before and after every unit except U5. Any byte change outside U5 is a defect of this work.
- **Run the reference-arbitrated suites (`-- --ignored`) at each unit**, not only at the end — they are the only thing that proves R5, they take ~12 minutes, and a regression is far cheaper to localise one dialect at a time.
- **U2 is a decision point, not just a unit.** If the family form does not express vasm cleanly, revise the types before U3 rather than after.

## Definition of Done

- Every dialect dispatches directives through its declaration; no spelling is accepted that the declaration does not contain (R3, proven by R4).
- vasm's families are declared as families (R6).
- The sigil convention is decided, applied, and its per-dialect behaviour probed against the references (U5).
- `Dialect::directives()` supplies the matrix spine (R7).
- Conformance, differential and curriculum are byte-identical except where U5 deliberately changed an accepted spelling, with each such change named.
