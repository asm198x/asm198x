---
title: Core Contract - Plan
type: feat
date: 2026-07-03
topic: core-contract
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Core Contract - Plan

## Goal Capsule

- **Objective:** Give asm198x one stable, structured, machine-readable **core contract** — a uniform assembly result and span-carrying rustc-style diagnostics — so every current and future CPU is instantly consumable by editors, agents, browsers, and the IDE without redoing surface work per tool. v1 ships the contract (structured result + JSON diagnostics) through the existing CLI; the public `Dialect` trait and the spec-query API land with the **first surface, MCP**, and the LSP / WASM surfaces follow.
- **Product authority:** Steve Hill. Seeded from idea 3 of the 2026-07-03 ideation (`docs/ideation/2026-07-03-asm198x-world-class-ideation.html`) — the verifier's "strongest strategic call." Grounding verified this session.
- **Open blockers:** None. The three decisions the five-persona review (2026-07-03) surfaced are settled: (1) **defer** R4 (public `Dialect`) and R5 (spec-query) out of v1 to the MCP first-surface increment — v1 is R1–R3 + R6–R8; (2) **contract-first** sequencing — this contract's structured result lands before **dbg198x** (its one genuine consumer) builds against it, so dbg198x implementation is paused as a deliberate rework-avoidance trade, bounded on the contract reaching a *designed R1 shape* (not full landing); the verdict pipeline is independent and not paused (round-2 correction); (3) **MCP** is the named first surface. Ready for planning.

---

## Product Contract

### Summary

asm198x today exposes ~26 flat `assemble_*` functions in three different return shapes, a `pub(crate)` `Dialect` trait, and errors that carry only a source *line* — no column, byte offset, stable code, or serialization. This makes the assembler impossible to integrate cleanly: Forge198x is gated on it, no editor / agent / browser can consume it, and every future CPU would need surface work redone. **v1 establishes the contract's load-bearing core**: a uniform structured result across all CPUs/dialects and span-carrying stable-coded JSON-serializable diagnostics (rustc's model, including the machine-applicable-fix flag). Its immediate, standalone payoff is a real CLI improvement — an opt-in `--message-format=json` and column-accurate spans replacing today's line-only errors. Its one genuine downstream consumer is **dbg198x**, which builds against the structured result rather than the pre-contract engine (contract-first — dbg198x is paused pending it; the verdict pipeline is independent and not gated). The public `Dialect` trait and the serializable spec-query API — which only the surfaces consume — land with the **first surface, MCP** (a session-stateful agent surface, matching Emu198x's MCP-first convention); the WASM playground and LSP follow. "The sequencing is the idea": stabilize the core once, and every surface plus every future CPU inherits it.

### Problem Frame

The engine already computes rich data but exposes almost none of it in a usable shape. Errors carry only a source line — a consumer cannot underline the offending token, and there is no stable code to switch on. The `Dialect` trait and the `Statement` stream are `pub(crate)`, invisible to any integrator. The public entry points are ~26 free functions returning three shapes (`Result<Assembly, AsmError>`, `Result<Vec<u8>, AsmError>` for linked/flat output, `Result<(Vec<u8>, Vec<Warning>), AsmError>`), so a consumer must special-case per dialect. Nothing derives serde; there is no JSON mode, no subcommand framework, no LSP/MCP/WASM scaffolding. Forge198x is "gated on Asm198x maturity" and literally cannot integrate today, and the external whitespace the research found — no spec-driven retro LSP, no browser playground carrying a *validated* encoder — stays unfilled.

Two assets make this tractable. The `isa` crate **already** offers a query API (`instruction()`, `find_form()`, `has_mnemonic()`, `Form::len()`) over its zero-dependency `&'static` tables — so spec-query is *extend and serialize*, not build-from-scratch. And the sibling `dbg198x` crate is the **design pattern** (not yet exercised — its own freeze is pending) for a versioned, serde-based, additive/skip-unknown contract that ships as public draft and **freezes at first consumption** — the governance shape a core contract borrows.

### Key Decisions

- **Contract first, then MCP, then the rest.** v1 is the contract's core — the structured result (R1) + rustc diagnostics (R2, R3) + versioning/consumers/inheritance (R6–R8). It lands **before dbg198x** implements — dbg198x is its one genuine consumer and depends on it (contract-first: the foundation lands once, dbg198x builds on it rather than being retrofitted). The verdict pipeline is independent and not gated (round-2 correction). The public `Dialect` trait (R4) and spec-query (R5) — consumed only by surfaces — land with the **first surface, MCP**; the WASM playground and LSP follow. Shipping a surface before the core stabilizes would mean redoing surface work three times — the whole point is to invest once.
- **Diagnostics are rustc-shaped.** Each diagnostic carries a source span (byte offset + line/column), a stable machine-readable code, severity, a human message, and an optional machine-applicable suggested fix. Human-readable output stays the CLI default; `--message-format=json` is opt-in. This rich `Diagnostic` is for **asm198x's own errors**. A reference-assembler rejection the verdict pipeline records is a *different thing* — the foreign tool's identity, its verbatim diagnostic text, and its exit status, with no span in our coordinates and no code we assigned — so it stays its own reference-rejection record (the verdict-pipeline plan already commits to that foreign-text shape). At most the two share a thin envelope (severity + human message); the rich structured fields are asm198x's alone. Not "one diagnostic" — one *envelope*, two records.
- **Public contract, draft-then-freeze (the dbg198x design pattern).** The contract's types go public with `#[non_exhaustive]` from day one, but the semver-stability *promise* stays draft until real surface contact, then freezes via a decision-record checklist. Real surface contact falsifies API designs; the irreversible promise waits for it. **Split freeze (decided 2026-07-04):** the v1 core (R1–R3) freezes at MCP; but R4 (public `Dialect`) and R5 (spec-query) — which ship *with* MCP, the freeze trigger — hold their promise draft *past* MCP until a **second surface** (LSP or WASM) has also consumed them. Otherwise the highest-risk, most integrator-facing API would freeze against a single surface with only a paper cross-surface check — the exact failure the draft-then-freeze safeguard exists to prevent.
- **Spec-query extends the existing `isa` surface, lands with MCP, and keeps `isa` zero-dependency.** The spec-query layer adds *serializable* query results (operands / encoding / cycles / flags by CPU + mnemonic — the data MCP, LSP hover, and the playground need) over the lookups `isa` already exposes. It is not v1 — it ships with the MCP first surface (its first consumer; LSP hover and the playground consume it later too). Serialization lives in a consumer-facing layer, never in `isa` itself, so `isa`'s zero-dependency stance holds. It is a free "extend and serialize" only for the Form-model CPUs; the field-packed CPUs (PDP-11, TMS9900, Z8000, CP1610, m68k) need per-CPU work (see R5).
- **One uniform result shape.** The three ad-hoc return shapes collapse into one structured contract (a result carrying bytes, origin/sections, symbols, start, warnings, and — on failure — diagnostics), so a consumer sees one shape across every CPU and dialect, including the linked (ca65 `.nes`, vasm hunk) and warning cases.
- **Surfaces are subcommands of the one binary.** Per the packaging decision, `asm198x lsp` / `asm198x mcp` are subcommands and the playground is the WASM build of the same core — not separate binaries or repos. (The CLI is hand-rolled today; introducing a subcommand structure is a planning-time detail.)

### Requirements

- R1. One structured assembly result across all CPUs/dialects: assembled bytes, origin/sections, symbols, entry point, warnings, and — on failure — diagnostics, replacing the three ad-hoc return shapes. The linked/bypass outputs (ca65 `.nes` ROM, vasm hunk exe), whose flat `origin` is meaningless, are a *variant* of the result, not forced into the flat shape.
- R2. Diagnostics carry a source span (byte offset + line/column), a stable machine-readable code, severity, a human-readable message, and an optional machine-applicable suggested fix (the rustc model).
- R3. The result and its diagnostics are JSON-serializable; the CLI gains an opt-in `--message-format=json`, with human-readable output unchanged as the default.
- R4. **(Ships with the MCP first surface, not v1.)** The `Dialect` trait is public (`#[non_exhaustive]`) as part of the contract; external third-party dialect authorship is out of scope for now (public-for-the-family first).
- R5. **(Ships with the MCP first surface, not v1.)** A serializable spec-query API answers, for a given CPU + mnemonic, the operand forms / encoding / cycles / flags — the data surfaces need — layered over the existing `isa` query functions, without making `isa` depend on serde. This is a free "extend and serialize" **only for the Form-model CPUs**; the field-packed CPUs (PDP-11, TMS9900, Z8000, CP1610, m68k) expose bespoke `Class`-based tables carrying no cycles/flags today, so their spec-query is a per-CPU follow-on that includes authoring that data from datasheets — not an automatic inherit.
- R6. **dbg198x** is the contract's one genuine consumer: it reads the structured result (R1) rather than the current `Assembly`, so R1's section/symbol shape is **co-designed with dbg198x's record model** (typed symbol kinds, address-space qualifier, `(section, offset)` addressing — its KTD2/KTD7) even though its *implementation* is paused (see Dependencies). The v1-buildable artifact this requirement owns is a thin shared **diagnostic envelope** (severity + human message), distinct from the rich `Diagnostic`, that a resumed dbg198x — or the verdict pipeline, opportunistically — can reuse. The **verdict pipeline is not a hard consumer**: it records foreign-tool rejections in its own `verdict-corpus` crate and does not depend on this contract, so it stays implementation-ready and is *not* paused. Contract-first therefore gates dbg198x alone.
- R7. The public contract carries a version and is additive / skip-unknown (the dbg198x precedent). It ships as public draft and freezes via a decision-record checklist that imports the **full** dbg198x precedent, not just its happy path: a **bounded-review clause** (re-examine the freeze if no surface is scheduled within a set period of v1 shipping, so the draft never waits open-endedly), a **secondary trigger**, and a requirement that the checklist review the shape against the *most demanding* anticipated surface's needs (LSP incremental spans, WASM permalink mapping), not only the first surface to ship. Whether load-bearing sibling consumption (dbg198x, the verdict pipeline) itself fires the freeze — they are crates, not surfaces — is an open decision (see Outstanding Questions); until resolved, those siblings may build against a still-draft contract.
- R8. Every current and future CPU inherits the structured result and diagnostics automatically — adding a CPU lights those up (and thus their share of every future surface) with no per-CPU or per-surface work. Spec-query (R5) inherits automatically only for Form-model CPUs; field-packed CPUs need per-CPU spec-query work, so R8's "automatic for every CPU" holds for the result + diagnostics, not the spec-query surface.

### Surfaces (staged — post-v1, not v1 scope)

**MCP is the first surface** (decided 2026-07-04). It also carries the public `Dialect` trait (R4) and the spec-query API (R5), which land with it rather than in v1's core.

- **`asm198x mcp`** *(first surface)* — a session-stateful agent surface, matching Emu198x's MCP-first family convention; serves the agent-native goal. The lowest-cost surface over the contract. Deliberate tension, named: MCP-first means the contract's first real-world validation serves *agents*, while the learner-facing WASM payoff — the surface tied to Code198x's launch anchor — sequences after it. Accepted on cost/convention grounds; if the learner payoff needs a firmer commitment, WASM should get its own target window even landing second.
- **WASM playground** — a zero-install browser assembler with byte-identical permalinks, embedded in Code198x lesson pages. The highest-visible payoff (removes the biggest beginner drop-off), but **not committed to the Oct-2026 Code198x launch**, and the heaviest surface to build (web UI + permalink infra + curriculum embedding).
- **`asm198x lsp`** — a spec-driven language server (hover = cycles/flags/encoding from the spec itself). Follows, since its main consumer, Forge198x, is still deferred.

### Acceptance Examples

- AE1. **Covers R1, R2, R3.** Given a 6502 program with an out-of-range operand, `asm198x --message-format=json` emits a diagnostic with a byte-offset span, a stable code, and a suggested fix where one applies; the same program assembled clean returns the structured result with bytes + symbols.
- AE2. **Covers R2.** The current line-only error is replaced: a diagnostic points at the exact column/offset of the token, not just its line.
- AE3. **Covers R5. (Deferred to the MCP first-surface increment with R4/R5 — not a v1 gate.)** A spec-query for `lda` on 6502 (a Form-model CPU) returns its operand forms, opcodes, cycles, and flags — serializable — sourced from the `isa` tables, not hand-authored; a query for a field-packed CPU (e.g. TMS9900) is documented as a per-CPU follow-on, not answered automatically.
- AE4. **Covers R6.** The verdict pipeline records a reference-assembler rejection as its own reference-rejection record (tool identity + verbatim text + exit status), reusing at most the shared diagnostic envelope — distinct from asm198x's rich `Diagnostic`.
- AE5. **Covers R4. (Deferred to the MCP first-surface increment with R4/R5 — not a v1 gate.)** The public `Dialect` trait is `#[non_exhaustive]` and documented; adding a family dialect uses it; external authorship is documented as not-yet-supported.
- AE6. **Covers R8.** Adding a new CPU makes its structured result and diagnostics available with no surface-specific code. (Spec-query inheritance is the MCP increment's concern, per R5/R8 — not a v1 gate.)
- AE7. **Covers R7.** The contract's serialized form carries a version field, and a payload with an unknown field deserializes successfully (skip-unknown) rather than erroring — the dbg198x additive precedent.

### Scope Boundaries

**Deferred for later**

- The three surfaces — MCP (first), then the WASM playground and `asm198x lsp` — each its own later increment.
- **R4 (public `Dialect` trait) and R5 (spec-query API)** — deferred out of v1's core; they land with the MCP first surface, their first consumer (LSP and the playground consume them later too).
- LSP incrementality (incremental reparse / partial updates) — deferrable, but not forever.
- External third-party `Dialect` authorship outside the repo.
- crates.io publication of the contract crate — with the first surface, not before (the dbg198x/isa-disasm pattern).

**Outside this product's identity**

- Forge198x owns IDE presentation; Emu198x owns execution and its own MCP; Code198x owns curriculum content (the playground embeds, but Code198x owns the lesson pages); Build198x / Play198x boundaries are unchanged.
- The contract exposes *data*; it does not render UIs. Surface presentation is the surfaces' (and siblings') job.

### Dependencies / Assumptions

- Verified this session (`/tmp/compound-engineering/ce-brainstorm/contract-surfaces/grounding.md`): `Assembly {origin, bytes, symbols, start, warnings}` + `AsmError`/`Warning` carry only a source line, no column/offset/code, none derive serde; `Dialect` + `Statement` are `pub(crate)` (`dialect.rs:25`); three return shapes across ~26 `assemble_*` (`lib.rs`); `isa` already exposes `instruction()`/`find_form()`/`has_mnemonic()`/`Form::len()` over `&'static` tables with no serde; no LSP/MCP/WASM/JSON scaffolding; hand-rolled CLI arg parsing (no clap). (The binary depended only on `isa` + `isa-disasm` until the in-flight dbg198x work added a serde-based crate to its tree — so serde is already arriving on the CLI path, which sharpens the "keep the binary lean" assumption below rather than starting it from zero.)
- dbg198x (idea 1, `docs/plans/2026-07-03-001-feat-debug-info-format-plan.md`) is the governance **design pattern** — not yet exercised, its own freeze pending an Emu198x milestone — for a versioned serde contract that freezes at first consumption, and the contract's one genuine consumer: it reads the structured result, not the current `Assembly`. Its KTD2 ("additive field on the current `Assembly`, no `assemble_*` signature changes") *can* ship without the contract, so contract-first is a deliberate **rework-avoidance trade**, not a hard block — its cost is pausing mature work whose `engine.rs` anchors, churning under CP1610, may rot. The pause is **bounded**: it lifts once the contract reaches a *designed R1 shape*, not full landing. **dbg198x implementation is paused (2026-07-04); `crates/dbg198x/` work stops here.** Only dbg198x's plan needs a dependency note + paused status — a cross-plan edit to make during planning.
- The verdict pipeline (idea 2, `docs/plans/2026-07-03-002-feat-verdict-pipeline-plan.md`) records reference rejections as foreign-tool records — its `verdict-corpus` crate already owns that shape — and does **not** depend on this contract: it may reuse the thin diagnostic envelope opportunistically, but is independent, stays implementation-ready, and is **not paused** (round-2 correction — it is not a gated consumer).
- Keeping the shipped binary lean matters: serde (and later async/LSP/WASM) deps should not land on the default CLI path. The likely shape — the contract types in a crate the surfaces depend on, keeping the base CLI light, mirroring `isa-disasm`/`dbg198x` — is a planning decision, flagged here so it is not assumed away.

### Outstanding Questions

- **[Post-mapping — 2026-07-04] The AST layer reshapes R1/R4.** Mapping ideas 4–7 surfaced a foundational source-preserving IR (`docs/plans/2026-07-04-005-feat-ir-ast-layer-plan.md`): R1's "structured result" substance is really that **AST** (the byte-level image is a lowering of it), and R4's `Dialect` trait must be **bidirectional** (parse *and* emit), because the converter (idea 6) and the language surface (idea 4) build on it. Also: spans must carry `(file,line,column)` + scope + reserved macro-expansion frames (idea 4's C1–C3), and the field-packed cycles/flags backfill is a shared prerequisite with ideas 5/7. The AST is planned first; reconcile R1/R4 with it when this contract goes to `ce-plan`.
- Whether load-bearing sibling (crate) consumption — dbg198x reading the structured result — fires the R7 freeze, or only a *surface* does. If it does, the freeze fires early (at dbg198x, not MCP); if not, dbg198x builds against a still-draft contract. Ties to R7's "crates, not surfaces" note.
- Which contract types are semver-frozen at v1 versus draft, and the exact freeze checklist (mirror the dbg198x freeze-at-first-consumption record, including its bounded-review and secondary-trigger safeguards per R7).
- Where the serializable spec-query layer lives so `isa` stays zero-dependency — a new contract crate vs an `isa` companion crate — planning.
- The subcommand-framework choice (adopt clap vs extend the hand-rolled parser) — planning; note the "add a dependency" rule.
- Whether the WASM playground's byte-identical permalinks are verified against the native build (a tie-in to the verdict pipeline) — later, within the WASM-surface scope.

### Sources

- `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` — idea 3 ("One contract, four surfaces"), the verifier's strongest strategic call.
- Grounding scout (2026-07-03): `/tmp/compound-engineering/ce-brainstorm/contract-surfaces/grounding.md` — contract specifics with `file:line`.
- Sibling plans: `docs/plans/2026-07-03-001-feat-debug-info-format-plan.md` (dbg198x) and `docs/plans/2026-07-03-002-feat-verdict-pipeline-plan.md` (verdict pipeline) — the two already-planned consumers.
- `decisions/packaging-and-cpu-roadmap.md` (single binary + subcommands), `decisions/syntax-stance.md`, and the dbg198x freeze-at-first-consumption pattern.
