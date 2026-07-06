# Decision: sequence the idea-plans foundation-first around four shared seams

**Status:** Active. Binding for how the Asm198x idea-plans are ordered and executed.

**Date:** 2026-07-06.

## The decision

The eight idea-plans (`docs/plans/2026-07-03-001` … `2026-07-04-005`) are executed
**foundation-first**, sequenced by their real dependencies rather than by
enthusiasm. Rework risk is **not spread evenly** across them — it concentrates at
four *shared seams* where two or more ideas meet. The path that implements every
idea without unnecessary rework is: **design each shared seam once, for the known
future, and freeze it before its consumers build on it.** Everything not on a
shared seam is independent and low-risk, and can proceed whenever.

**Layers 0 and 1 are complete as of 2026-07-06** — 0a (the one span model) is
locked, 0b (the AST across every CPU) is done, and the core contract's five
units (U1–U5) have all landed: the unified `AssemblyResult`, rustc-shaped
diagnostics, column-accurate operand spans in the AST-routed dialects,
`--message-format=json`, and the versioned draft-then-freeze governance. R1's
freeze *promise* fires at the MCP surface per
`decisions/core-contract-freeze.md`; until then the contract is a public draft.
The current focus is **Layer 2** — the consumers fan out on the frozen-shape
foundations (dbg198x's contract-first pause condition — a designed, landed R1
shape — is now satisfied).

This record is the map. It does not restate each plan — it says what order they
go in and why, and where the freeze-gates are. Read it before scheduling work
that touches more than one plan.

## The four shared seams (where rework concentrates)

| Seam | Owner (defines it) | Consumers (build on it) | Freeze-gate |
|------|--------------------|-------------------------|-------------|
| **The AST** — the source-preserving semantic tree (`crates/asm198x/src/ast.rs`) | AST plan (`…-005`) | contract diagnostics (`…-003` R2), language surface (`…-001`), converter (`…-003`/idea 6), cycle→source map (`…-002`) | Its *data model* is landed and stable; per-CPU coverage is still completing (Layer 0b) |
| **The span / source model** — `(file, line, col)` + reserved macro-expansion frames | language surface (`…-001`, because includes force multi-file) | contract diagnostics + result (`…-003` R1/R2), dbg198x source model (`…-001`/idea 1) | **Lock before** contract U2/U3 freeze and before dbg198x resumes |
| **The symbol / section model** — typed kinds, address-space qualifier, `(section, offset)` | co-designed dbg198x (`…-001`) ↔ contract R1 (`…-003`) | dbg198x reader (Emu198x), the JSON result | Symbol slice stays **draft past MCP** until a resumed dbg198x actually reads an `AssemblyResult` (contract KTD4) |
| **Field-packed cycles/flags data** — per-CPU timing/flag tables authored from datasheets | (unowned today) | cycle listing (`…-002`/idea 5), spec-query (`…-003` R5) | Author **once**, not per-consumer |

**The seam that bites hardest is the span/source model.** Three ideas (1, 3, 4)
each define or consume "how a byte, symbol, or diagnostic is attributed to a
location." The language-surface plan states it directly: *includes force a
multi-file source model, so every byte/symbol/diagnostic must carry
`(file, line, column)` through an include chain — which dictates the shape of the
contract (R1/R2) and dbg198x's source model.* This session already nearly spawned
**two** span types (`DiagSpan` vs `ast::Span`). If the contract's diagnostics
(U2/U3) freeze a *single-file* span now and idea 4 later makes it multi-file, that
is rework inside a **frozen** contract *and* dbg198x. The fix is cheap if done
now and expensive if deferred: design the one span shape multi-file-and-
expansion-frame-ready before diagnostics freeze. U1's `DiagSpan` already reserves
`file: Option<FileId>` under `#[non_exhaustive]` — that instinct is correct; the
discipline is to make it *the* span, aligned to `ast::Span`, not a second one.

## The layered sequence

**Layer 0 — foundations (current focus; finish before anything above freezes)**
See § Layer 0 in detail below.

**Layer 1 — the core contract** (`…-003`, ✅ complete 2026-07-06) — U1 (unified
`AssemblyResult`), U2 (diagnostics), U3 (column-accurate operand spans), U4
(JSON), and U5 (versioning + freeze governance) all landed 2026-07-06. R1's
freeze fires at MCP (`decisions/core-contract-freeze.md`).

**Layer 2 — the consumers fan out** (on stable foundations, low rework):
- **dbg198x (`…-001`, idea 1)** — resume against frozen R1 + the shared span/
  symbol model. **Paused** today, correctly, pending the contract's designed R1
  shape (contract-first).
- **Verdict pipeline (`…-002`, idea 2)** — genuinely **independent**, gated on
  nothing; reuses only the thin diagnostic envelope opportunistically. Run its
  harness prerequisite (outcome-typed `ref_assemble`, its U2) whenever capacity
  allows — it needs no other layer.
- **Language surface (`…-001`, idea 4)** — on the completed AST; it *finalizes*
  the multi-file source model, so its span decisions must be locked in Layer 0.
- **Converter (`…-003`/idea 6)** — on the completed AST + the structural
  renderer it defines; macro/include/conditional conversion is gated on idea 4.
- **Cycle analyzer (`…-002`/idea 5)** — needs the field-packed cycles/flags data
  authored once (seam 4).

**Layer 3 — surfaces + publishing:**
- **MCP** (the first surface) — carries the public `Dialect` trait (contract R4)
  and spec-query (R5), deferred out of contract v1 to here.
- **WASM playground**, then **`asm198x lsp`**.
- **Docs site (`…-004`, idea 7)** — the most downstream idea; consumes 3/2/5 and
  gates nothing. Its generatable-now core (R1–R3, R5) is unblocked today and can
  start early as a thin skeleton, but its integrations wait on their sources.

## Layer 0 in detail (✅ complete 2026-07-06)

Two pieces. **0a is the hard freeze-gate; 0b is the highest-leverage unblock.**
Both are done — kept here as the record of what the foundation guarantees.

### 0a. Lock the one span / source model

- Define a single span shape — `{ file: FileId, line, col }` with reserved
  macro-expansion frames — used by **`ast::Span`**, the contract's diagnostic
  span (`DiagSpan`), and dbg198x's source records. One type (or two that are
  provably identical by construction), not two that drift.
- Make it **multi-file-ready now**, before includes (idea 4) are built: a single
  file is `FileId(0)`; the include chain populates the rest later without a shape
  change. `#[non_exhaustive]` so expansion frames add additively.
- **Done when:** the contract's U2/U3 diagnostics and a resumed dbg198x can both
  consume the *same* span type, and adding includes (idea 4) requires no change
  to its shape — only new values.
- **Why first:** it is the seam that freezes into two consumers (contract +
  dbg198x). Every day it stays un-locked, U2/U3 and dbg198x risk building against
  a shape that idea 4 will invalidate.

### 0b. Complete the AST across the remaining CPUs — ✅ COMPLETE (2026-07-06)

- **Every dialect now routes assembly through the AST.** All 21 dialect
  front-ends produce an `ast::Program` and assemble from it; `--fmt` covers them
  all (the CLI's unsupported-dialect fallback was removed as unreachable).
- **The mechanical stragglers** (ca65_816, ca65_huc6280, F8, 2650, TMS7000, 8048,
  SC/MP…) used the `parse_ast` + `item_from_operation` recipe.
- **The field-packed tier** (PDP-11, TMS9900, CP1610, Z8000/Z8001) turned out
  *not* to need the "harder tail" work feared here: `item_from_operation` was
  already total over `Operation::Encoded`, so they rode the same recipe — their
  pre-computed pieces route through `Item::Encoded` unchanged.
- **The multi-pass CISC dialects** (vasm/68000 and the NES ca65 assemble+link)
  were the genuinely different case — standalone drivers that never used
  `engine::assemble`. They adopt the AST via a **family-owned native payload**
  (`Item::Native`), the shared tree carrying their un-lowered statements; see
  [`ast-native-payload-for-multipass-cisc.md`](ast-native-payload-for-multipass-cisc.md).
  This is the seam that will carry x86 and the 68020+/68080 line later.
- **Every migration was byte-identity-gated** against its real reference tool
  (incl. the 68000 opcode-space sweep and the NES curriculum vs `ca65`+`ld65`).
- **Note for U3 (column spans):** AST-routed ≠ column-accurate. The per-line
  `parse_program`s populate `Span::at(line, 1)` — real line, column `1`. So U3's
  column work is *not* automatically delivered by 0b; the highest-value operand
  error sites still need the operand's column threaded in. See U3's grounding.
- **This was high-leverage but NOT a hard blocker.** Per contract KTD1, the
  diagnostic span rides the **engine error path, not the AST**, so *every* CPU
  already inherits diagnostics — column-accurate where AST-routed, line-granular
  otherwise, improving incrementally as CPUs migrate. So 0b unblocks the
  **converter** and **language surface** (which need the structural tree) and
  *raises diagnostic quality*; it does not gate the contract. Sequence the
  mechanical stragglers first; take the field-packed tail as its own staged work
  (the `…-005` U6 direction), CPU by CPU, never all-at-once.
- **Validation gate:** the dialect-neutral-AST premise — that semantically
  divergent dialects (oversize policy, local-label scoping, `addr_unit`) share
  one neutral tree without per-consumer escape hatches — **passed** its
  validation spike this session. The foundation is sound; proceed.

## The disciplines that prevent the rework

1. **Freeze-at-first-consumer.** Already the family's discipline (dbg198x is
   paused for exactly this). A shared shape goes public as *draft*
   (`#[non_exhaustive]`, versioned, skip-unknown) and freezes only when a real
   consumer has exercised it. Never let a consumer build against an unfrozen
   shared shape.
2. **One shape per concern, designed for the known future.** One span, one symbol
   model, one result. Additive-by-construction so the future extends rather than
   breaks. Two types for one concern is the drift smell.
3. **Ground every unit against the code before executing it.** The two real traps
   this session — the converter's "emit is a verbatim formatter, not a renderer,"
   and U1's "4 outlier functions vs all 27" — were plan-vs-reality gaps caught
   *only* by reading the code first. A cheap grounding pass per unit is the
   highest-ROI rework preventer in the whole programme.
4. **Finish the AST for a CPU tier before the AST-consumers scale across it.**
   Half-migrated coverage means per-CPU rework in every consumer.
5. **Adopt-on-demand for genuinely independent generality.** Conditionals per
   dialect, and kin, wait for a real consumer (see
   [`conditional-assembly-framework.md`](conditional-assembly-framework.md)) — no
   speculative keyword parsers "for later."

## Drift triggers

Stop and re-consult this record if a change would:

- **Build a Layer 2/3 consumer against a Layer 0/1 shape that is not yet frozen**
  — e.g. wiring dbg198x to a not-yet-designed R1, or the converter to an
  incomplete renderer. That is the retrofit-rework this sequencing exists to
  prevent.
- **Introduce a second span, symbol, or result type** for a concern a shared seam
  already owns. One shape per concern (discipline 2). If `ast::Span` and the
  contract span must differ, say why in this record first.
- **Freeze the diagnostic span as single-file** (no `file`/expansion-frame
  headroom) before the language surface's multi-file model is accounted for.
  Seam 2 is the hardest-biting seam; freezing it narrow is the top rework risk.
- **Schedule a downstream idea (converter, language surface, cycle analyzer,
  docs site) ahead of the Layer-0 foundation it needs** without either the
  foundation being ready or an explicit, recorded reason to reorder.
- **Migrate all remaining CPUs onto the AST in one batch.** The field-packed tier
  is staged CPU-by-CPU (0b); an all-at-once sweep is the anti-pattern the staged
  builds (`z8000-staged-build.md`, `cp1610-staged-build.md`) exist to avoid.

See the per-idea plans in [`../docs/plans/`](../docs/plans/), the contract's
span/symbol decisions (`docs/plans/2026-07-03-003-feat-core-contract-plan.md`
KTD1/KTD4), and [`conditional-assembly-framework.md`](conditional-assembly-framework.md)
(adopt-on-demand). Cross-project consumption triggers (Emu198x reading dbg198x /
`isa`) live in the umbrella `../../decisions/`.
