---
title: Self-Verifying Docs Site - Plan
type: feat
date: 2026-07-04
topic: docs-site
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Self-Verifying Docs Site - Plan

## Goal Capsule

- **Objective:** A docs site that **cannot drift**: per-CPU instruction references generated from the `isa` crate (opcode, operands, cycles, flags, with a provenance link into the umbrella `reference/` datasheet library), every code sample assembled by the real binary in CI, and the framework + prose lint promoted to a CI gate. v1 ships the generatable-now core; diagnostic explain-pages, the conformance ledger (idea 2), per-dialect directive matrices, and cycle columns (idea 5) are wired as slots, filled as their sources land. Prose that can rot is minimised; generated data that cannot is maximised.
- **Product authority:** Steve Hill. Seeded from idea 7 of the 2026-07-03 ideation (highest confidence, 90%). Explored last because it is the family's downstream integration and publishing point — it consumes nearly everything the other ideas produce.
- **Open blockers:** None. v1 scope confirmed 2026-07-04: the generated instruction reference + samples-assembled-in-CI + the mdBook/lint framework now; explain-pages / ledger / directive-matrices / cycle-columns as slots.

---

## Product Contract

### Summary

The drift the idea exists to prevent has already happened — the README claims a small CPU count against ~19 shipped, and the `docs` repo holds little more than one dialect page and a stale spec-format doc. A hand-written docs site is a second source of truth already stale at 19 CPUs and losing harder at 30. This site generates instead: per-CPU instruction references come straight from the `isa` crate (so a spec change regenerates the page), every code sample is assembled by the real `asm198x` binary in CI (so a sample that stops assembling fails the build), and the mdBook framework plus the existing House198x/Vale prose lint become a CI gate. v1 ships exactly what is generatable today — the instruction reference, samples-in-CI, and the framework — and wires *slots* for the pieces whose sources land later: diagnostic explain-pages keyed by idea 3's stable error codes, the conformance ledger from idea 2, per-dialect directive-support matrices from the conformance corpus, and cycle columns from idea 5. The result is a docs site with the same trust property as the assembler: a correctness artifact, not a rot-prone parallel truth — and the natural public venue for the ledger, the explain-pages, and the spec itself.

### Problem Frame

Docs clarity is trust in this community — WLA-DX's cautionary tale is that banking-model confusion is resented as much as bugs — and asm198x's docs are both thin and already wrong. The naive fix (hand-write pages for 19 CPUs) creates the exact drift now visible in the README, and it compounds with every CPU added. Meanwhile the material to generate from already exists: the `isa` crate carries per-instruction opcode, operands, cycles, and flags with datasheet provenance, and the real binary can assemble every example in CI. What is missing is the generation layer and a site that treats generated data as the source of truth, plus the venue the family's other public artifacts (the conformance ledger, coded explain-pages) need to live.

### Key Decisions

- **Generate from the spec; minimise rottable prose.** Instruction references are generated from `isa`; samples are assembled by the real binary in CI; the ledger, matrices, and cycle data come from their own generated sources. Hand-written narrative is reserved for genuinely editorial content and is never a duplicate of generated data.
- **v1 = generatable-now core + framework, everything else as slots.** The instruction reference, samples-in-CI, and the mdBook + House198x-lint scaffold ship now. Explain-pages, the ledger, directive matrices, and cycle columns are wired as slots that degrade gracefully until idea 2/3/5 fill them — so the site is real now and completes incrementally.
- **mdBook + the existing lint, promoted to a CI gate.** Boring-tech stance: mdBook for the site, and the House198x/Vale lint that already runs as a local commit hook becomes a CI gate on the docs.
- **The site is the family's public face.** It is the publishing venue for idea 2's conformance ledger and idea 3's diagnostic explain-pages, and the home of the spec — provenance links reach down into the umbrella `reference/` datasheet library ("see all the way down").
- **The generator is a build-time consumer of the spec — possibly not through idea 3's serializable spec-query.** A docs generator runs at build time and can read the `isa` tables directly, the way the tests do; it does not obviously need idea 3's *serializable* spec-query (R5). This narrows R5's must-have consumers toward the runtime surfaces (MCP/LSP/playground) — a real input to idea 3's R5 scope (see C1).

### Requirements

**v1 — generatable-now core + framework**

- R1. A **per-CPU instruction reference generated from `isa`** — opcode, operand forms, cycles, flags — with a **provenance link** into the umbrella `reference/` datasheet library, regenerated on any spec change so it cannot drift.
- R2. **Every code sample on the site is assembled by the real `asm198x` binary in CI**; a sample that fails to assemble fails the docs build.
- R3. The **mdBook framework** and the **House198x/Vale lint promoted from a local hook to a CI gate**.
- R4. **Slots wired (not filled in v1)** for: diagnostic explain-pages keyed by idea 3's stable error codes; the idea-2 conformance ledger; per-dialect directive-support matrices from the conformance corpus; cycle columns in the instruction references (idea 5). Slots degrade gracefully (a "pending" state, never a broken link).
- R5. **Every future CPU documents itself** the moment its spec lands — no per-CPU hand-authoring of the reference.

**What idea 3 and the siblings must account for**

- C1. The site consumes the **spec's query surface**. Because it is build-time, it may read `isa` directly rather than through idea 3's serializable spec-query (R5) — which, with idea 5's analyzer possibly doing the same, narrows R5's must-have consumers to the runtime surfaces. Idea 3's R5 scope should note which consumers truly need the *serializable* form.
- C2. Explain-pages need idea 3's **stable, enumerable diagnostic error codes** (R2) — the slot keys on them.
- C3. The ledger slot consumes idea 2's **verdict-pipeline ledger** — this site is already named as the ledger's publishing venue in idea 2.
- C4. Cycle columns consume idea 5's **cycle data**; directive matrices consume the **conformance corpus**.

### Acceptance Examples

- AE1. **Covers R1.** Adding an instruction to a CPU's spec regenerates that CPU's reference page with no hand-edit; the page links its provenance to the datasheet in the umbrella `reference/` library.
- AE2. **Covers R2.** A code sample that fails to assemble fails the CI docs build.
- AE3. **Covers R3.** The House198x lint runs as a CI gate on the docs and fails the build on a prose error.
- AE4. **Covers R4.** An explain-page slot renders a graceful "pending stable error codes" state rather than a broken link, before idea 3 lands.
- AE5. **Covers R5.** A newly-added CPU appears in the generated reference with no hand-authored page.

### Scope Boundaries

**Deferred for later (slots, filled when sources land)**

- Diagnostic **explain-pages** — need idea 3's stable error codes.
- The **conformance ledger** — from idea 2.
- **Per-dialect directive-support matrices** — from the conformance corpus (needs the matrix data shaped).
- **Cycle columns** in the instruction references — from idea 5.

**Outside this product's identity**

- Hand-written narrative that **duplicates generated data** — the drift trap the site exists to avoid.
- Replacing the umbrella `reference/` datasheet library — the site *links into* it, it is not a second copy of it.

### Dependencies / Assumptions

- Verified this session (`/tmp/compound-engineering/ce-brainstorm/docs-site/grounding.md`): `README.md:24` says "Five CPUs" against 19 `isa` modules (`lib.rs:196-214`), and the org profile is staler ("6502 first"); the sibling `docs` repo is near-dead (one dialect page for 19 CPUs, frozen 30 May–4 Jun while 18 CPUs shipped); the `isa` query API (`instruction()`/`find_form()`/`Form{opcode,mode,operands,cycles,flags,undocumented}` + `summary`) is generator-ready **but Form-model only** — the bespoke CPUs (PDP-11, TMS9900, Z8000, CP1610, and m68k/6809) need bespoke handling, the *same* field-packed gap ideas 3 and 5 hit; House198x/Vale runs as a local hook (`MinAlertLevel = suggestion`, "not a gate"), not CI; `AsmError { line, message: String }` (`engine.rs:70`) has no stable codes; directive-matrix data is only partial (`differential.rs` tracks 7 dialects, gaps closed) and a real matrix must derive from the `dialects::*` front-ends.
- **Most downstream idea:** consumes idea 3 (spec-query surface + stable error codes, C1/C2), idea 2 (ledger, C3), idea 5 (cycles, C4). It does not gate any of them; it is their publishing/integration point. The generatable-now core (R1–R3, R5) is unblocked today.
- The `docs` repo is a separate sibling repo in the org container; the dbg198x spec page (idea 1) and the core-contract spec (idea 3) also publish there — the site is the shared home.

### Outstanding Questions

- Whether the generator reads `isa` directly or through idea 3's spec-query — this is a real input to idea 3's R5 consumer set (build-time vs runtime), resolved when idea 3 is planned.
- mdBook preprocessor vs build-script generation for the instruction reference — planning.
- Hosting and deploy pipeline for the site — planning.
- The exact shape of the per-dialect directive-support-matrix data the conformance corpus must expose — planning, when that slot is filled.

### Sources

- `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` — idea 7 ("The self-verifying docs site").
- Grounding scout (2026-07-04): `/tmp/compound-engineering/ce-brainstorm/docs-site/grounding.md`.
- `docs/plans/2026-07-03-003-feat-core-contract-plan.md` (idea 3, spec-query + error codes), `docs/plans/2026-07-03-002-feat-verdict-pipeline-plan.md` (idea 2, the ledger this publishes), `docs/plans/2026-07-04-002-feat-cycle-analyzer-plan.md` (idea 5, cycle columns).
- External prior art: shellcheck's coded wiki (explain-pages keyed by stable codes); mdBook; the umbrella `reference/` library for datasheet provenance.
