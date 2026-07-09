> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: Cycle Analyzer - Plan
type: feat
date: 2026-07-04
topic: cycle-analyzer
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Cycle Analyzer - Plan

## Goal Capsule

- **Objective:** Turn the spec's already-validated cycle data into a feature: a **cycle-honest listing** (human table + machine JSON) annotating every line with size and cycle cost (min/max, page-cross, branch-taken) plus per-block/per-label totals, and a **source-level budget assertion** (`cycles(routine) <= N`) that fails the build when a routine blows its cycle budget — the raster/demoscene programmer's contract, checked by the assembler. v1 covers the Form-model CPUs, whose specs already carry validated cycles; the field-packed CPUs are a staged backfill.
- **Product authority:** Steve Hill. Seeded from idea 5 of the 2026-07-03 ideation. Explored before planning idea 3 (the core contract) — the cycle data reaches the analyzer *through* idea 3's spec-query, so this dictates what that API must expose.
- **Open blockers:** None. v1 scope confirmed 2026-07-04: cycle-honest listing **and** the budget assertion, for the Form-model CPUs; the field-packed cycles backfill is a staged follow-on shared with idea 3's spec-query (R5), which hits the same gap.

---

## Product Contract

### Summary

The Form-model instruction specs already carry validated `Cycles` data (base, page-cross, branch-taken), machine-checked in tests and used for *nothing* — no listing or cycle output exists at all. Demoscene and game programmers count cycles by hand, and no retro assembler does uniform cross-CPU cycle analysis. v1 converts that dormant asset into two outputs — a human-readable **cycle-honest listing** and a machine-readable JSON of the same data, annotating every line with size and honest cycle *ranges* (min/max, with page-cross and branch-taken penalties surfaced) plus per-block and per-label totals — and adds a **build-failing budget assertion** so a routine that exceeds its cycle budget fails the assemble, like a test. It is static analysis at assemble time (llvm-mca-style), not execution — Emu198x executes; this measures. Coverage is exactly the CPUs whose spec carries cycles (Form-model in v1); the six field-packed CPUs get a staged cycles backfill. The cross-cutting output: the cycle data reaches the analyzer through idea 3's spec-query (R5, which must expose `Cycles` richly), the budget assertion is a build-failing diagnostic (idea 3 R2 + a stable code), the JSON listing extends the listing surface, and the field-packed backfill is the *same* gap idea 3's R5 already flagged.

### Problem Frame

The project's most-validated, least-exploited asset is its cycle data: every Form-model instruction carries machine-checked `Cycles`, and nothing reads it. Meanwhile cycle-exactness is the defining discipline of the target machines — racing the beam, stable rasters, cycle-counted interrupt handlers — and the people who most want asm198x count cycles by hand in a text editor today. There is no listing output of any kind (not even size), so a programmer can't see what their code costs without assembling, disassembling, and adding it up. And the guarantee that would make an expert switch — "the assembler counts cycles for you, honestly, and fails the build if you blow your budget" — is exactly the thing the spec data is sitting ready to power.

### Key Decisions

- **Form-model first, field-packed backfill staged.** The 13 Form-model CPUs (6502, Z80, 65816, HuC6280, SM83, 8080, 8048, 6800, 1802, SC/MP, F8, 2650, TMS7000) already carry validated cycles, so listing + assertion ship for them now. The six bespoke CPUs (m68k, 6809, PDP-11, TMS9900, Z8000, CP1610) carry none; authoring their cycles from datasheets is a staged follow-on — and it is the *same* backfill idea 3's spec-query needs, so the two share it. (68000 EA-dependent timing is the genuinely hard corner.)
- **Honest ranges, never fabricated single numbers.** Cycle cost is a min/max range wherever timing is data-dependent — branch taken vs not, page-cross vs not — surfaced as such (e.g. `4/5`), not collapsed to one misleading figure. Sourcing only from the validated spec is what keeps it honest; the analyzer covers exactly the CPUs whose spec has cycles and reports "no data" for the rest rather than guessing.
- **The budget assertion rides a magic comment, preserving source-compatibility.** The assertion (`cycles(irq_raster) <= 224`) is asm198x-only, so it cannot be inline dialect syntax without violating the source-compatible stance. It lives in an `; asm198x:` **magic comment** that every reference assembler ignores *as a comment* — so the source still assembles byte-identically under the reference, and asm198x acts on it. (Exact spelling is a planning detail; the magic-comment principle is the decision.)
- **Static analysis at assemble time — this measures, it does not execute.** The sibling boundary holds: Emu198x runs code; the analyzer computes cost from the spec + the assembled structure. v1 is per-line / per-block / per-label straight-line cost; full control-flow worst-case-execution-time (data-dependent loop counts) is out of scope.
- **This makes the spec load-bearing.** Once cycles power a build-failing assertion, the spec's cycle data stops being documentation-grade and becomes correctness-grade — the forcing function that justifies the field-packed backfill and tighter cycle validation.

### Requirements

**v1 — Form-model CPUs**

- R1. A **cycle-honest listing** annotates every source line with its size (bytes) and cycle cost, sourced from the spec's validated `Cycles`, for any Form-model CPU.
- R2. Cycle cost is an honest **range** where timing is data-dependent — branch-taken and page-cross penalties surfaced as min/max, not a single fabricated number.
- R3. The listing carries **per-block and per-label totals** (size and cycles), so a routine's cost is legible without hand-summing.
- R4. Two output forms of the same data: a **human-readable table** and a **machine-readable JSON**.
- R5. A **source-level budget assertion** — `cycles(<label>) <= N` in an `; asm198x:` magic comment — **fails the build** with a diagnostic when the measured cost exceeds the budget, naming the routine, the budget, and the actual cost.
- R6. The analyzer sources cost **only** from the validated spec cycles; it covers exactly the CPUs whose spec carries cycles and reports "no cycle data (backfill pending)" for the rest, never fabricating.

**What the contract (idea 3) must account for**

- C1. Idea 3's **spec-query (R5)** must expose `Cycles` richly — base, page-cross, branch-taken, and the min/max derivation — because the analyzer reads timing through it, not by reaching into `isa` directly.
- C2. The budget assertion is a **build-failing diagnostic** — it uses idea 3's diagnostic model (R2) and a stable error code, so tools and the docs site (idea 7) can key on it.
- C3. The JSON listing is a **rendering of the structured result + spec cycles** — it extends the listing surface (dbg198x's `--listing`) and shares the multi-file source model idea 4 requires (C1 there), so a listing line names its source file.
- C4. The **field-packed cycles backfill** is the same gap idea 3's spec-query flagged (bespoke `Class` tables carry no cycles). Fill it once, serve both; sequence idea 3's R5 and this analyzer's backfill together.

### Acceptance Examples

- AE1. **Covers R1, R3.** A 6502 routine's listing shows per-line size + cycles and a per-label total that matches hand-computed cycles for a known raster kernel.
- AE2. **Covers R5.** A routine carrying `; asm198x: cycles(irq) <= 224` that actually costs 230 **fails the build** with a diagnostic naming the routine, the 224 budget, and the 230 actual; at 220 it passes.
- AE3. **Covers R2.** A conditional branch that may cross a page shows a `4/5` min/max range, and a `lda abs,x` that may page-cross shows its penalty — not a single collapsed number.
- AE4. **Covers R4.** The same program's JSON listing carries the same per-line, per-block, and per-label size/cycle data as the human table.
- AE5. **Covers R6, C4.** Requesting a listing for a field-packed CPU (e.g. TMS9900) reports "no cycle data (backfill pending)" rather than fabricating cycles.

### Scope Boundaries

**Deferred for later**

- The **field-packed cycles backfill** — authoring cycle data for m68k (68000), 6809, PDP-11, TMS9900, Z8000, CP1610 from datasheets; staged, and shared with idea 3's spec-query. 68000 EA-dependent timing is the hardest corner.
- Full **control-flow worst-case-execution-time** — data-dependent loop counts, whole-program WCET across branches. v1 is straight-line per-line/per-block/per-label cost plus explicit budget assertions.

**Outside this product's identity**

- **Execution-based timing** — Emu198x runs code and can report actual cycles; this analyzer computes cost statically from the spec at assemble time. Different verb, sibling boundary.
- The contract's *implementation* — idea 3 owns the spec-query and diagnostic shape; this states the requirements on them (C1–C4).

### Dependencies / Assumptions

- Verified this session (`/tmp/compound-engineering/ce-brainstorm/cycle-analyzer/grounding.md`): `Cycles { base, page_cross, branch_taken }` (all `u8`) at `crates/isa/src/lib.rs:154-162`, hung off `Form` (`lib.rs:94`); read only in `#[cfg(test)]` blocks (`z80.rs`, `mos6502.rs`) — no non-test consumer; the `listing_*` functions (`main.rs:241-298`) are disassembly reconstruction with zero timing (no cycle listing exists); the six bespoke specs — m68k, 6809, PDP-11, TMS9900, Z8000, CP1610 — carry no cycles (`Class`-based tables). The 13 Form-model specs (incl. `mos65816`) carry cycles.
- Consumes idea 3 (core contract, `docs/plans/2026-07-03-003-feat-core-contract-plan.md`): the spec-query (R5) and diagnostic model (R2) — C1–C4 are requirements on it. This is a *post-contract* feature; it does not gate idea 3, but idea 3's R5 design must expose cycles for it.
- Extends the listing surface dbg198x (`docs/plans/2026-07-03-001-feat-debug-info-format-plan.md`) introduces, and shares idea 4's multi-file source model for per-line file attribution.
- The `; asm198x:` magic-comment channel is assumed reference-ignored; confirm each reference treats `;`/dialect-comment content as inert (it does by definition, but incbin/quirky comment syntaxes are worth a planning check).

### Outstanding Questions

- The magic-comment spelling and grammar for the budget assertion (`; asm198x: cycles(x) <= N`) — and whether budgets can also live in a sidecar file for programs that must stay byte-clean of even comments — planning.
- Min/max semantics for CPUs where a single instruction has more than two timing outcomes (e.g. 68000 EA-dependent, when backfilled) — how the range is defined.
- Whether per-block totals attempt any control-flow awareness (straight-line vs across a local branch) in v1, or are strictly straight-line between labels.

### Sources

- `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` — idea 5 ("Cycle-budget analyzer & cycle-honest listing").
- Grounding scout (2026-07-04): `/tmp/compound-engineering/ce-brainstorm/cycle-analyzer/grounding.md`.
- `docs/plans/2026-07-03-003-feat-core-contract-plan.md` (idea 3) — the spec-query/diagnostic consumer relationship (C1–C4); `docs/plans/2026-07-03-001-feat-debug-info-format-plan.md` (dbg198x) — the listing surface this extends; `docs/plans/2026-07-04-001-feat-language-surface-plan.md` (idea 4) — the multi-file source model the listing shares.
- External prior art: llvm-mca (static machine-code timing), WCET analysis from real-time systems.
