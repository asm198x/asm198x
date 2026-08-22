> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: Reference Parity for Block Constructs - Plan
type: feat
date: 2026-08-22
topic: reference-parity
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: probe-survey
execution: code
---

# Reference Parity for Block Constructs - Plan

## Goal Capsule

- **Objective:** Every dialect assembles the macros, conditionals and
  repetition its own reference has. Nine gaps; one closed.
- **Product authority:** Steve Hill.
- **Open blockers:** None. The demand gate that would have blocked this was
  retired 2026-08-22 (`conditional-assembly-framework.md`).

---

## Product Contract

### Problem Frame

The identity claim is that real-world source for a machine assembles unchanged.
Across three block constructs on the seven main dialects there are **ten cells
where it does not**, and nine of them are ours — the reference has the feature
and we have not built it.

| dialect | macros | conditionals | repetition |
|---|---|---|---|
| pasmo | yes | yes | yes |
| sjasmplus | yes | yes | yes |
| acme | yes | yes | **done 2026-08-22** (`!for`) |
| ca65 | yes | gap (`.if`) | gap (`.repeat`) |
| vasm | yes | gap (`ifne`) | gap (`rept`) |
| rgbasm | gap (`MACRO`) | gap (`IF`/`ENDC`) | gap (`REPT`) |
| lwasm | yes | gap (`ifne`/`endc`) | **the reference has none** |

The tenth cell — lwasm repetition — is an *extension*, gated separately and out
of scope here.

### The thing that makes this eight units and not one

**The dialects are on three different pipelines, and only one of them takes the
adoption recipe as written.**

| pipeline | dialects | what a block construct costs |
|---|---|---|
| `ast::evaluate` already | acme, sjasmplus, pasmo | a parse arm and an evaluator method |
| `ast::lower` (flat) | lwasm, rgbasm | the full four-step adoption, **plus** splitting parse from lowering |
| native multi-pass | ca65, vasm | the recipe does not apply at all |

`ast::lower` has no block structure and rejects an `Item::Conditional`
outright, so a flat dialect must move to `ast::evaluate` — which is the
recorded recipe. What the recipe does not mention, because acme and sjasmplus
did not have it, is the **environment-threading problem**:

> lwasm's walk binds `equ` values *at parse time*, and uses them to choose
> direct vs extended addressing. Move it to `ast::evaluate` unchanged and an
> `equ` inside an **untaken** branch still binds, because the parse ran before
> anything knew the branch was dead. That is a silent sizing error, not a
> refused file.

acme solved this by re-parsing each line at lower time with the live
environment. lwasm and rgbasm need the same split, and it is the larger half of
each unit — not the conditional syntax.

ca65 and vasm are further out. Both are native multi-pass drivers
(`ast-native-payload-for-multipass-cisc.md`): they project the tree to their own
`Parsed` form and run their own layout and emit passes. `CondEval` is not in
that path, so "route the assembler through `ast::evaluate`" has nothing to route.
Their design question is genuinely open — see *Outstanding Questions*.

### What the references actually do

Measured 2026-08-22 against the installed binaries.

**acme `!for`** — done; the survey is in the code's own doc comments. Two
syntaxes: `!for i, n` runs `1..=n` and is empty below 1 (and acme *warns* on
every use of this old form); `!for i, a, b` is inclusive and **descends** when
`b < a`.

**lwasm conditionals** — richer than the others and very lenient:

| form | behaviour |
|---|---|
| `ifne`, `ifeq`, `ifgt`, `ifge`, `iflt`, `ifle` | compare the expression against **zero** |
| `ifdef`, `ifndef` | test a symbol |
| `else` | yes |
| `endc` **and** `endif` | both close |
| case | insensitive (`IFNE` assembles) |
| an **unclosed** block | **accepted** — lwasm just ends the file |
| a **stray closer** | **accepted** — no error |
| junk in an untaken branch | accepted |

The last two matter: pasmo errors on both and lwasm does not, so a shared
"unbalanced block" diagnostic would be wrong for lwasm.

`ifpragma` and `ifstr` are out of scope — pragma strings and string conditions
are their own surfaces with no demand.

**ca65, vasm, rgbasm** — not yet surveyed to this depth. Each unit begins with
its own probe pass, as pasmo's and acme's did, because every one of these
surveys has contradicted what the manual implies.

### Key Decisions

- **One dialect per unit, measured first.** Every probe pass so far has found
  something the manual does not say — pasmo closing a repetition with `ENDM`,
  acme's `!for` descending, lwasm accepting an unclosed block. Implementing
  from documentation and arbitrating afterwards would invert the order that has
  been finding the bugs.
- **The parse/lower split is the unit, not the syntax.** For lwasm and rgbasm,
  recognising `ifne` is an afternoon and moving the environment from parse time
  to lower time is the work. Size the units that way round.
- **ca65 and vasm wait for a design.** Not for demand — the gate is retired —
  but because "how does a native multi-pass driver evaluate a conditional" has
  no answer yet, and guessing one would land a second conditional mechanism.
- **The formatter is in scope for every unit.** A construct that assembles and
  will not format, or that formats to something that will not assemble, is the
  defect class this month has been spent on. The round-trip assertion in
  `differential.rs` covers it automatically once probes are added.

### Requirements

- **R1.** Each dialect assembles its reference's spelling of each construct,
  and refuses the spellings its reference refuses.
- **R2.** An `equ` (or equivalent) inside an untaken branch defines nothing,
  and does not influence a later sizing decision.
- **R3.** Formatted source assembles to the same bytes, and formatting is
  idempotent, for every construct added.
- **R4.** Each construct is declared on its dialect's directive surface, so the
  generated dialect page states it.
- **R5.** Every behaviour is arbitrated byte-for-byte against that dialect's own
  reference before its commit.

### Scope Boundaries

**In scope:** conditionals and repetition for lwasm, rgbasm, ca65 and vasm;
macros for rgbasm.

**Out of scope:**

- **lwasm repetition** — the reference has none. That is an extension, with no
  arbiter, and it is gated by its own drift trigger in
  `conditional-assembly-framework.md`.
- **`ifpragma`, `ifstr`**, rgbasm's `FOR` over string lists, and the rest of
  each reference's fringe. Demand-gated as
  `conditional-assembly-framework.md` still gates *fringe* forms — retiring the
  gate covered whole constructs, not every spelling.
- **Modules and namespaces** — [#93]'s third item, unrelated machinery.

### Outstanding Questions

- ~~**How does a native multi-pass dialect evaluate a conditional?**~~
  **Answered 2026-08-22** in
  [`conditionals-in-multipass-dialects.md`](../../decisions/conditionals-in-multipass-dialects.md):
  **shape 1**, and it is not a multi-pass problem at all. Neither reference
  permits a forward reference in a condition — ca65 says `Constant expression
  expected`, vasm says `expression must be constant` — so a condition folds
  once, sequentially, before layout. ca65 conditions cannot see the location
  counter or even a backward label; vasm's can see `*`, and the value is the
  **pre-relaxation** address, which is what stops a condition and the optimiser
  feeding each other. The three shapes considered were:
  1. **Project through the conditional** — `parsed_from_program` walks
     `Item::Conditional`, folds the condition against the constants gathered so
     far, and projects only the live branch. Keeps the tree model; needs the
     projection to be strictly sequential in a way it may not be today.
  2. **Give the native driver a `CondEval`** — hoist enough of its environment
     to implement the trait, and run `ast::evaluate` to produce the `Parsed`
     input. Most consistent with the other five dialects; the largest change.
  3. **Evaluate during the layout pass** — the pass that already resolves
     sizes. Fits the multi-pass model; risks the conditional being re-evaluated
     per pass with different answers.

  This wants its own decision record, and probably its own probe pass on how
  ca65 itself sequences `.if` against forward references.

---

## Implementation Units

Ordered by what each teaches the next.

### U1. acme `!for` — **landed 2026-08-22**

Closed the only gap on a dialect already running `ast::evaluate`, and built the
loop-variable mechanism: `CondEval::count -> i64` became
`iteration -> Iteration`, either `Times(n)` or `Over { name, values }`.

The loop variable is baked into each use rather than left as a symbol, because
the engine resolves `Expr::Sym` once, in a later pass, against one table — it
cannot express a value that differs per pass. `!set` had the same problem
already solved, so the variable joined that mechanism.

### U2. lwasm conditionals

The full adoption, and the first dialect to need the parse/lower split. Its
surface is surveyed above, including the two lenient postures — an unclosed
block and a stray closer are both accepted — which a shared diagnostic would
get wrong.

**Verified by:** byte-identical output against real `lwasm` for each of the
eight forms, both closers, the case-insensitivity, an `equ` in an untaken
branch defining nothing, and the formatter round trip.

### U3. rgbasm macros, conditionals and repetition

Three gaps on one dialect, and the only remaining macro gap. Macros first,
since `MACRO name` … `ENDM` is a source pre-pass that the block work then has
to coexist with — the ordering that caught out pasmo, where `ENDM` closes both
a macro and a repetition.

Reuses whatever U2 establishes for the parse/lower split.

### U4. ca65 conditionals and repetition — **unblocked**

`parsed_from_program` folds each `Item::Conditional` / `Item::Repeat` head
against the `=` constants gathered so far, in source order, and projects only
the live branch. No layout state: a ca65 condition cannot reach any.

`.repeat n[, var]` gives the loop variable its second consumer, which is the
first real test of whether `Iteration::Over` generalised past acme.

**Verified by:** byte-identical output against real `ca65 + ld65`, including a
forward reference in a condition **refused** as ca65 refuses it, a definition
in an untaken branch staying invisible, and the formatter round trip.

### U5. vasm conditionals and repetition — **unblocked**

The same sweep, carrying a running unrelaxed program counter so `*` folds to
the pre-relaxation address. The probe that pinned this belongs in the corpus:
a `bra` the optimiser shortens, with a condition after it whose outcome differs
between the relaxed and unrelaxed address.

---

## Verification Contract

- Every unit's behaviour is arbitrated against that dialect's own installed
  reference, byte-for-byte, before its commit.
- The differential corpus gains probes per dialect per construct, so both the
  refusal ledger and the round-trip ledger cover them.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -D
  warnings` clean.

## Definition of Done

- The nine cells are closed, or the ones that are not are recorded with why.
- No dialect accepts a spelling its reference refuses.
- The generated dialect pages state each construct, from the declared surface
  rather than by hand.

[#93]: https://github.com/asm198x/asm198x/issues/93
