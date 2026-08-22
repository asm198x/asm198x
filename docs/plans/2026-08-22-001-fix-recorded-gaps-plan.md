> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: Recorded Gaps - Plan
type: fix
date: 2026-08-22
topic: recorded-gaps
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: grounding-survey
execution: code
---

# Recorded Gaps - Plan

## Goal Capsule

- **Objective:** Close the gaps two plans deliberately recorded rather than
  fixed, now that the surfaces which expose them exist. Every item below is
  already written down somewhere; this collects them, says which are worth
  fixing, and puts them in an order.
- **Product authority:** Steve Hill.
- **Open blockers:** None. Each unit is independent of the others.

---

## Product Contract

### Problem Frame

Two plans finished this week — the declared directive surface and the docs
adoption narrative — and both made a habit of recording a gap instead of
widening to fix it. That was the right call each time: a plan that grows to
absorb everything it touches never lands. But the notes are now scattered
across two plan files, one decision record and three issue threads, and some of
them describe a *user-facing* defect rather than an internal untidiness.

The generated dialect pages change the calculus for several of them. A gap that
was previously discoverable only by tripping over it is now a row, an absence,
or a wrong-looking cell on a published page.

### The gaps, as recorded

**G1. `KnownUnsupported` has no members.** The category was added in U1 and its
count asserted at zero ever since. It exists for two confirmed cases: pasmo's
unimplemented `include`, and asl's semantic pseudo-ops (#87). Both are real and
neither is declared, because declaring one changes a user-facing diagnostic.

*Why it matters.* Today both refuse as an **unknown mnemonic**, which tells a
reader their source is invalid when it is not. "Not supported yet" is a decision
about their project; "not valid syntax" is a bug report they will write against
their own file. The category exists precisely to keep those apart.

**G2. An unknown word is refused three ways, and one names nothing.** Recorded
in U6. `unknown instruction \`x\``; `` `x` has no form for operands `1` `` (the
Z80 family and rgbasm, which names the word without ever saying it is not a
word); and the ca65 family's `no suitable addressing mode for this operand`,
which names nothing at all.

*Why it matters.* A reader given the third cannot tell whether they mistyped the
mnemonic or the operand.

**G3. sjasmplus reports a stray block closer two ways.** Recorded in U7. A stray
`ENDIF` says "without a matching IF"; a stray `ENDM` is an unknown mnemonic. One
dialect, two standards for the same shape of mistake, because the conditional
walk recognises its closer outside a block and the macro scanner does not.

**G6. acme does not warn on an indented label.** Found while probing G2. Real
acme accepts `        frobnicate` as a label and warns "Label name not in
leftmost column"; we accept it silently. The bytes agree — this is a missing
non-fatal warning, not a divergence in what assembles — so it is recorded and
not scheduled.

**G4. Five of six formatters refuse a file containing macros** ([#130]). U7
touched the edge of this: `.macro` on the ca65 formatter path now reports
"declared but not dispatched here" instead of "unsupported directive", which is
a truer message for the same limitation and not a fix.

- **Closed 2026-08-22**, outside this plan and against its own scope note below,
  because it was asked for directly. All five formatters copy a definition
  through instead of reading it: ca65, vasm, lwasm, pasmo, acme. The shared
  parts are the `Item::Verbatim` node and `macros::macro_line`.
  `FORMATTER_GAPS` is empty and stays in the tree as the strictest form of its
  own test.

**G5. The dialect pages have no arbitration column.** Recorded in the generator
itself. The corpus's `dialect` field is a *suite* label, not a `--dialect` name:
`asl` covers twelve chips at once, `vasm-bin`/`vasm-exe` are output legs, and
`pasmonext`/`z80n` are targets. Joining "which tool arbitrates this dialect, and
over how many verdicts" onto a dialect page needs a `--dialect`→CPU mapping that
no source owns.

*Why it matters.* It is the one column that would make a dialect page answer
"how much do you actually check this one", which is the question a reader
adopting a less-travelled dialect brings.

### Key Decisions

- **G1 is worth doing and G2 is worth doing; the rest are worth deciding
  separately.** G1 and G2 both change what a user reads when something fails,
  and a diagnostic that misdescribes the problem costs more than a missing
  feature: it sends the reader to the wrong place.
- **G1 before G2.** Declaring the `KnownUnsupported` spellings gives the
  diagnostic work a concrete case to be right about, and its rows appear on the
  dialect pages immediately.
- **G5 is a modelling question, not a plumbing one.** The mapping should be
  *derived* — the assembler already knows which instruction set each dialect
  targets — rather than hand-written next to the generator. Hand-writing it is
  the failure the pages exist to avoid.
- **G4 is #93's, not this plan's.** Macros are the v1 bar's item 4 and mid-flight
  under an epic of their own. Folding the formatter half into a gap-tidying plan
  would take it out of the sequence it belongs to.

### Requirements

- **R1.** pasmo's `include` and the asl semantic pseudo-ops settled by #87 are
  declared `KnownUnsupported`, and refuse with a diagnostic that says the
  spelling is recognised and not implemented.
- **R2.** The `KnownUnsupported` count is no longer asserted at zero; the
  invariant becomes that every such entry has a diagnostic naming it as
  unimplemented.
- **R3.** An unknown word is refused the same way in every dialect, and the
  message names the word.
- **R4.** Byte-identical output across the corpus and curriculum. These are
  diagnostics changes; any byte change is a defect of this work.

### Scope Boundaries

**In scope:** the `KnownUnsupported` declarations and their diagnostic; the
unknown-word message; the tests pinning both.

**Out of scope:**

- **Implementing pasmo's include.** Declaring it as unsupported is a statement
  about today, not a substitute for the feature. If it lands, the declaration
  changes with it and a test fails to make someone do that.
- **The macro formatter work (G4)** — #93's.
- **G3**, unless it falls out of G1 for free. It is one dialect and one
  diagnostic, and it is recorded.

### Dependencies / Assumptions

- #87 decides *which* asl pseudo-ops are declined rather than ignored. R1 is
  blocked on that decision for the asl half; the pasmo half is not.

### Outstanding Questions

- **G5's mapping: derived from where?** `Dialect::instruction_set()` exists and
  the trait is crate-private, so the accessor needs designing rather than
  exposing. The `includes::resolution()` pattern is the precedent — derived
  where it can be, stated and test-held where it cannot.
- **Does G2's fix change a message the contract pins?** The diagnostics are
  user-facing text; the core contract governs their *shape*, not their wording.
  Worth confirming before changing three dialect families' output.

---

## Implementation Units

### U1. Declare pasmo's include as `KnownUnsupported`

- **Landed 2026-08-22.** The category has a member after two plans, and pasmo's
  page carries a row saying "Recognised, and not implemented" where it carried
  nothing.
- **Derived, not restated.** `Z80Syntax::own_directives()` hands dispatch the
  dialect's own declaration, and `parse_op` refuses a `KnownUnsupported` entry
  from that — so the diagnostic follows the category rather than a second list
  of words to refuse specially, which is the drift the surface exists to remove.
- **It broke the migration table, which is the useful part.** The generated
  include table renders a spelling per dialect, so declaring pasmo's `include`
  made that page claim pasmo supports it. Fixed by rendering the category:
  a `KnownUnsupported` entry shows the spelling **and** "not implemented",
  because "pasmo spells it `include` and we do not read it" is a different fact
  from "pasmo has no include", and a dash for both would lose exactly the
  distinction this unit adds.
- **The invariant changed shape.** `nothing_is_declared_unsupported_yet` asserted
  a count of zero; what matters is not how many there are but that each draws a
  diagnostic naming it unimplemented, so that is what is asserted now — plus a
  guard that the category has not emptied again.
- **R4 verified:** differential, corpus replay and the curriculum comparisons
  byte-identical, zero new verdicts.
- **The asl half is still #87's**, and is not part of this.
- **Goal:** The one case that needs no other decision first.
- **Requirements:** R1, R2, R4 (pasmo half).
- **Files:** `dialects/pasmo.rs`, `dialects/z80.rs`, `directives.rs`.
- **Approach:** Declare `include` with `Category::KnownUnsupported`; the z80
  walk refuses it by category rather than falling through to mnemonic
  resolution. The dialect page gains a row saying "recognised, not implemented"
  where it currently says nothing.
- **Test scenarios:** ` include "x"` reports a recognised-but-unimplemented
  directive rather than `unknown instruction INCLUDE`; the surface invariant
  that counted zero becomes one that checks the diagnostic; byte output
  unchanged.

### U2. One wording for an unknown word

- **Landed 2026-08-22**, and the probe found a different set of dialects than
  G2 recorded.
- **G2 was wrong about who.** It named "the Z80 family and rgbasm". The Z80
  family is fine — `parse_op` checks `has_mnemonic` before resolving operands.
  The five that were not: **rgbasm** and **8080** ("has no form for operands",
  implying the word exists), **65816** ("no suitable addressing mode", naming
  nothing), **huc6280** ("requires an operand"), and **tms7000** ("expected two
  operands"). Two of those five were not in the note at all.
- **The fix is the same in each:** check the mnemonic exists before touching
  operands. An unknown word reaching mode resolution is what produces a message
  about the operand.
- **Order matters where a mnemonic is computed rather than held in the spec.**
  tms7000's guard sits *after* the `TRAP n` arm — `TRAP` is encoded as
  `0xFF - n` in the dialect and the spec carries no such mnemonic, so a guard
  above it refuses a real instruction. Its unit test caught that immediately.
  The condition-code aliases are remapped first for the same reason.
- **acme names the operand, and that is correct.** An indented bare word is a
  *label* there, so `frobnicate 1` reads as a label plus a mnemonic `1` — which
  is what real acme does (probed 2026-08-22: it assembles with the warning
  "Label name not in leftmost column"). The unknown instruction genuinely is
  `1`.
- **The R3 test tightened** from "an undeclared spelling fails" to "an
  undeclared spelling is refused as an unknown instruction", which is only
  assertable now that the wording is one thing.
- **R4 verified:** differential, conformance, corpus replay and the curriculum
  comparisons byte-identical, zero new verdicts.
- **Goal:** The same refusal, naming the word, in every dialect.
- **Requirements:** R3, R4.
- **Dependencies:** U1, for a concrete contrast — an unknown word and a known
  unsupported one must not read alike.
- **Files:** `dialects/rgbasm.rs`, `dialects/z80.rs`, `dialects/i8080.rs`, the
  ca65 family.
- **Approach:** The Z80-family and rgbasm paths build operand combinations
  before checking the mnemonic exists; check first, so an unrecognised word is
  refused as one. The ca65 family's `no suitable addressing mode` is raised
  where the word is known but the operand is not — the unknown-word case needs
  separating from it.
- **Test scenarios:** the existing `an_undeclared_spelling_assembles_nowhere`
  tightens from "fails" to "fails with the word named"; the differential and
  curriculum suites are byte-identical.

### U3. The dialect→CPU accessor, and the arbitration column (proposed)

- **Goal:** A dialect page can say what arbitrates it.
- **Requirements:** G5.
- **Dependencies:** the Outstanding Question above.
- **Approach:** Derive the mapping from each dialect's `instruction_set()`
  rather than restating it, then key the corpus join on CPU — which is the field
  the corpus does own.

---

## Verification Contract

- **The existing suites are the guard.** Conformance, differential and
  curriculum byte-identical before and after every unit. These change what the
  assembler *says*, never what it emits.
- **Run the reference-arbitrated suites at each unit**, not only at the end.

## Definition of Done

- pasmo's include is a row on its dialect page rather than an absence, and its
  diagnostic says which it is (R1).
- No dialect refuses an unknown word without naming it (R3).
- The `KnownUnsupported` count is no longer zero, and is no longer asserted to
  be (R2).
- Byte output unchanged everywhere (R4).

[#130]: https://github.com/asm198x/asm198x/issues/130
