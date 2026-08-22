> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: Repetition - Plan
type: feat
date: 2026-08-22
topic: repetition
artifact_contract: ce-unified-plan/v1
artifact_readiness: blocked-on-decision
product_contract_source: probe-survey
execution: code
---

# Repetition - Plan

## Goal Capsule

- **Objective:** Repetition blocks in every dialect whose reference has them.
  One dialect has it today; five references have it and we refuse all five.
- **Product authority:** Steve Hill.
- **Open blockers:** **All units.** Repetition is not an isolated feature —
  four of the five dialects have no block-structured pipeline at all, and
  putting one under them is the adopt-on-demand recipe that
  `conditional-assembly-framework.md` gates. See *The blocker* below. Nothing
  here should start until that gate is answered.

---

## Product Contract

### Problem Frame

[#93] puts repetition in the same stage as macros, for the same reason: real
source uses it. With macros landed and [#130] closed, repetition is the
remaining half of that stage, and it is the narrower half — the mechanism
already exists.

`Item::Repeat` has been in the AST since the language-surface work, with the
shared `evaluate` folding a count and walking the body that many times. It has
one implementor. sjasmplus assembles `DUP 3` … `EDUP`; every other dialect
answers `unknown instruction` or `unsupported directive` on its reference's own
spelling.

### What the references actually do

Measured 2026-08-22 against the installed references — pasmo, ca65 2.19,
rgbds 1.0.3, vasm (m68k mot), acme 0.97, lwasm — not read from manuals.

| dialect | opens | closes | loop variable | count `0` |
|---|---|---|---|---|
| sjasmplus | `DUP n`, `REPT n` | `EDUP`, `ENDR` | none | 0 iterations |
| pasmo | `REPT n` | **`ENDM`** | none | 0 iterations |
| vasm | `rept n` | `endr` | none | 0 iterations |
| rgbasm | `REPT n`, `FOR v, …` | `ENDR` | `FOR` only | 0 iterations |
| ca65 | `.repeat n[, v]` | `.endrepeat` | optional | 0 iterations |
| acme | `!for v, …` | `}` | **always** | see below |
| lwasm | — none — | | | |

Five findings worth carrying into the code, each of which contradicts what a
manual-reading implementation would have assumed:

1. **pasmo closes a repetition with `ENDM`** — the same keyword that closes a
   macro. Not `ENDR`, which it refuses. A scan for a macro's end must therefore
   already be wrong about a `REPT` nested inside a macro body, and pasmo agrees
   it is: pasmo itself refuses that shape (`Identifier expected but 'M' found`).
   Refusing it too is matching the reference, not a limitation.
2. **pasmo is case-insensitive here** (`rept`, `Rept`, `REPT` all assemble),
   unlike sjasmplus, whose strict all-lower-or-all-upper rule is already
   implemented and is a genuine difference between the two.
3. **ca65 2.19 has no `.rep`/`.endrep` alias** — `'.REP' is not a recognized
   control command`. The alias exists in later cc65; declaring it here would be
   declaring a spelling this reference refuses.
4. **The three loop-variable dialects disagree on every axis.** ca65's variable
   is optional and 0-based. rgbasm's `FOR` is 0-based with an **exclusive**
   stop and an optional step (`FOR v, 0, 6, 2` gives 0, 2, 4). acme's is
   mandatory, 1-based, **inclusive**, and *counts down* when the end is below
   the start — `!for i, 3, 1` gives 3, 2, 1, and `!for i, 1, 0` gives two
   iterations rather than none. A shared "loop variable" abstraction that
   assumed any one of these would be wrong about the other two.
5. **A label in a repeated body is a duplicate-label error in every reference
   measured.** That is already what `Item::Repeat`'s doc comment says and why
   it does not scope locals per iteration. Five more references now agree.

### The blocker: this is the adopt-on-demand gate, not plumbing

The section above was written before reading the pipeline, and its sizing was
wrong. Corrected by measurement:

**Only two dialects run `ast::evaluate` at all** — acme and sjasmplus. Every
other dialect assembles through `ast::lower`, which has no block structure and
which *rejects* an `Item::Repeat` outright ("a repetition block is evaluated by
the dialect, not lowered"). So the mechanism is not waiting to be reused by
pasmo, vasm, rgbasm and ca65; there is nowhere in their pipelines to put it.

The same measurement shows repetition is not the only thing missing. None of
those four supports **conditionals** either, and every one of their references
does:

| dialect | `IF` in the reference | in us |
|---|---|---|
| pasmo | `IF`/`ELSE`/`ENDIF` — assembles | `unknown instruction \`IF\`` |
| ca65 | `.if`/`.endif` | `unsupported directive \`.if\`` |
| rgbasm | `IF`/`ENDC` | `unknown instruction \`IF\`` |
| vasm | `ifne`/`endif` | `unknown instruction \`IFNE\`` |

That absence is a **decision, not an oversight**.
[`conditional-assembly-framework.md`](../../decisions/conditional-assembly-framework.md)
models the evaluator as shared and adopted *per dialect, on demand* — "when a
real program needs it, not speculatively" — and its 2026-07-07 note closes with
"further adopters remain demand-gated". Its four-step recipe is exactly the
work this plan would do: implement the evaluator, recognise the syntax, **route
the dialect's assembler through `ast::evaluate`**, add an `emit` branch.
[`roadmap-sequencing.md`](../../decisions/roadmap-sequencing.md) restates it as
a sequencing principle — "no speculative keyword parsers 'for later'" — and
[`macro-expansion-framework.md`](../../decisions/macro-expansion-framework.md)
names it the same shape.

So doing U1 as written would put a keyword-block pipeline under four dialects
without the gate being opened, which is the thing three decision records
separately forbid.

**The argument for opening it exists and is already accepted elsewhere.** [#93]
made it for macros in as many words: curriculum demand is not the instrument,
because *we wrote the curriculum and it avoids what the toolchain does not
support*, so the driver is external source compatibility. Period source for
these machines uses conditionals and repetition heavily. That reasoning applies
to this gap unchanged — but extending a gate is an amendment to a binding
record, not a judgement call inside a plan.

### Key Decisions

- **The dialects without a loop variable go first, together** — *if the gate
  opens.* pasmo, vasm and rgbasm's plain `REPT` need no new **repetition**
  mechanism once their assembler runs through `ast::evaluate`: they fold a
  count and hand it to the walk that already exists. Getting them there is the
  adoption recipe, and it delivers conditionals at the same time, because it is
  the same pipeline.
- **The loop variable is a separate unit, and it needs a decision first.** The
  shared trait's hook is `fn count(&self, head, line) -> Result<i64, _>`, which
  cannot express "and bind `v` to this value on this iteration". Extending it
  is a change to the interface every dialect implements, so it is proposed
  rather than assumed — see *Outstanding Questions*.
- **lwasm gets nothing, and that is the finding.** lwasm has no repetition
  pseudo-op under any spelling probed. Its formatter and assembler are correct
  to refuse one, and no row should appear on its dialect page.
- **Match each reference's spelling, not a house one.** `v1-scope.md` scopes
  this stage as *adopted against real dialect requirements rather than as a
  universal macro language*. pasmo's `ENDM` closer is the test of whether that
  is meant: the tidy choice is to also accept `ENDR`, and the reference refuses
  it.

### Requirements

- **R1.** Each dialect assembles its reference's repetition spelling, and
  refuses the spellings its reference refuses.
- **R2.** The count is folded against the environment at assembly time, so a
  count naming an `equ` constant works and a count naming a symbol defined
  *later* fails — as pasmo's does.
- **R3.** A repetition inside a conditional that is not taken folds no count
  and defines nothing, matching the existing `emit = false` walk.
- **R4.** The formatter round-trips a repetition block: formatted source
  assembles to the same bytes, and formatting is idempotent.
- **R5.** Each dialect's behaviour is arbitrated byte-for-byte against its own
  reference, not against another dialect's or against our own expectation.

### Scope Boundaries

**In scope:** repetition blocks for pasmo, vasm, rgbasm and ca65; the loop
variable for the three dialects that have one; the directive-surface
declarations and the tests.

**Out of scope:**

- **Opening the adopt-on-demand gate.** This plan cannot grant itself the
  permission three decision records withhold. If the gate opens, the amendment
  is a change to `conditional-assembly-framework.md` with its own reasoning.

- **Modules and namespaces** — the third item in [#93]'s scope list, and
  unrelated machinery.
- **rgbasm's `FOR` over a string list**, `BREAK`, and the rest of rgbds'
  loop surface beyond the numeric forms measured above. Demand-gated, as
  `conditional-assembly-framework.md` requires.
- **`.rep`/`.endrep`** — see finding 3.
- **A repetition nested inside a macro body in pasmo** — the reference refuses
  it (finding 1).

### Dependencies / Assumptions

- The installed references are the arbiters. Where a later version of a
  reference differs (ca65's `.rep` alias), this targets the installed one and
  the divergence is recorded rather than papered over.
- `Item::Repeat` needs no shape change for U1 and U2. Whether it needs one for
  U3 is the open question.

### Outstanding Questions

- **Does the adopt-on-demand gate open for these four dialects?** The blocking
  question, and the user's. If yes, the unit list below is preceded by an
  adoption unit per dialect and the scope roughly doubles — but it delivers
  conditionals as well as repetition, since they are one pipeline. If no, the
  only dialect that can gain repetition is acme, whose `!for` is U3.
- **How should a loop variable reach the body?** Two shapes, and the choice is
  the user's because it changes an interface every dialect implements:
  1. **Widen `count`** to return a description of the iteration — the values
     to run over and the name to bind, with today's count-only case as the
     no-variable form. One hook, and every dialect's answer stays declarative.
  2. **Add a per-iteration hook** the shared `evaluate` calls before each pass,
     leaving `count` alone. Smaller diff, but the iteration state then lives in
     the dialect and two hooks must agree about it.

  Shape 1 is the recommendation: acme's counting-down case is expressible as a
  list of values and awkward as an index the dialect must reinterpret.

---

## Implementation Units

### U1. Repetition for pasmo and vasm — *blocked on the gate*

The two plainest cases **once each dialect is on the evaluate pipeline**. Both
fold a count and have no loop variable; neither has anywhere to put an
`Item::Repeat` today. Each needs the four-step adoption first, which is the
larger half of the unit and is not costed here because the gate may not open.

pasmo's walk recognises `REPT` case-insensitively and closes on `ENDM`,
building an `Item::Repeat`; `count` folds the head's expression through the
existing environment. vasm's does the same for `rept`/`endr`.

Declared on each dialect's directive surface, so the spelling appears on its
generated page.

**Verified by:** byte-identical output against real `pasmo` and real
`vasmm68k_mot` for a nested block, a count naming a constant, and a count of
zero; a forward-referenced count refused in pasmo as pasmo refuses it; the
formatter round trip.

### U2. Repetition for rgbasm's `REPT` — *blocked on the gate*

`REPT n` … `ENDR`, no loop variable, closed by `ENDR` only — `ENDM` is
`Unterminated loop`. Same shape as U1; separate because rgbasm is a separate
family and its `FOR` waits on U3.

**Verified by:** byte-identical output against real `rgbasm`.

### U3. The loop variable — ca65, rgbasm's `FOR`, acme's `!for` (proposed)

**acme's half is the one unit here that the gate does not block**, because acme
already runs `ast::evaluate`. It needs only the trait-shape decision.

Blocked on the *Outstanding Questions* decision. Once the trait's shape is
settled, the three dialects are independent of each other and each is arbitrated
against its own reference, with the three disagreements in finding 4 as the
tests that stop one dialect's rule leaking into another.

acme carries an extra wrinkle: `!for` is a brace block, so its walk is the one
just written for `!macro` — except that a `!for` body **is** code and must be
parsed, not copied.

---

## Verification Contract

- Every unit's behaviour is arbitrated against that dialect's own installed
  reference, byte-for-byte, before its commit.
- The formatter round trip (assemble, format, assemble, compare; then format
  again for idempotence) holds for every repetition shape added.
- The differential corpus gains a probe per dialect per shape, so a regression
  is reported rather than discovered.

## Definition of Done

- Every dialect whose reference has repetition assembles it; lwasm still
  refuses it, on the record.
- No dialect accepts a spelling its reference refuses.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -D
  warnings` clean.

[#93]: https://github.com/asm198x/asm198x/issues/93
[#130]: https://github.com/asm198x/asm198x/issues/130
