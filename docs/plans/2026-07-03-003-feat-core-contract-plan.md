---
title: Core Contract - Plan
type: feat
date: 2026-07-03
topic: core-contract
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
planned: 2026-07-05
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

---

## Planning Contract

**Product Contract preservation:** unchanged. R1–R8, the Acceptance Examples, and the scope boundaries are carried verbatim from the `ce-brainstorm` requirements. Planning added the HOW below and resolved the AST-reconciliation Outstanding Question (see KTD1/KTD2).

**v1 scope reminder:** this plan implements **R1, R2, R3, R6 (envelope only), R7, R8**. **R4** (public `Dialect` trait) and **R5** (spec-query) stay deferred to the MCP first-surface increment (KTD7); the surfaces (MCP / WASM / LSP) are separate later plans.

### Grounding (verified 2026-07-05)

- `engine.rs`: `Assembly { origin: u16, bytes, symbols: BTreeMap<String,i64>, start: Option<u16>, warnings: Vec<Warning>, debug: DebugData }`; `DebugData { symbols: Vec<dbg198x::Symbol>, lines: Vec<LineRec { line, offset, length }> }`; `AsmError { line: usize, message: String }`; `Warning { line, message }`. None derive serde. **`dbg198x` is already a dependency** (via `DebugData`), so serde is already on the CLI path.
- `lib.rs` public return shapes: **23× `Result<Assembly, AsmError>`** (the per-dialect `assemble_*`), **3× `Result<Vec<u8>, AsmError>`** (linked: ca65 `.nes`, vasm hunk), **1× `Result<(Vec<u8>, Vec<Warning>), AsmError>`** (`assemble_vasm_warned`) — **27 assemble entry points in all**. The 11× `Result<String>` are the formatters/listings — **not** assembly results, out of R1's scope.
- `AsmError::new(line, msg)` has **344 construction sites** across the dialects. `AsmError`/`Warning` are built only via their `::new` constructors (no struct literals), so adding an optional field keeps every site compiling.
- `ast.rs`: the existing span type is `ast::Span { file, line, col, expansion_frames }` — **line/column, no byte offset**. As of this session the AST-routed dialects (whose parses carry `ast::Node` spans) are **6502/acme, Z80 (pasmo/sjasmplus), 8080, 6800, 1802, SC/MP, rgbasm/SM83, 6809/lwasm**; the field-packed/computed CPUs (PDP-11, TMS9900, Z8000, CP1610, m68k) are **not** AST-routed and have no scoped migration. This is the ground truth for KTD1/U3's column-accuracy scope — take it from the code, not from the companion AST-layer plan, whose prose lags.

### Key Technical Decisions

- **KTD1 — Diagnostics span is optional on `AsmError`, aligned to `ast::Span`, byte-offset best-effort; column accuracy is incremental (resolves the AST Outstanding Question for R2).** `AsmError` gains an optional `span: Option<DiagSpan>`. **`DiagSpan` is named distinctly from the existing `ast::Span` to avoid a type collision**, and its fields **mirror `ast::Span`** so the AST path fills it directly: `{ file: Option<FileId>, line: u32, col: u32, offset: Option<u32> }`. The 344 line-only sites keep compiling unchanged (`span: None` → line-granular). The **AST-routed dialects** (the eight enumerated in Grounding) populate `file`+`line`+`col` from their `ast::Node` spans, so the curriculum-heavy CPUs get column-accurate diagnostics in v1. **Byte `offset` is best-effort, not universal:** `ast::Span` carries no offset today, so `offset` stays `None` on the AST path in v1 and is populated only where a parse already threads a byte cursor — R2/AE1's byte-offset is realized as *line+column now, offset when available*, matching the incremental posture (the alternative — threading a byte cursor through every dialect parse for v1 — is out of scope). The reserved `expansion_frames` (macro provenance, idea 4's C1–C3) are **not** in the v1 `DiagSpan`; they are added when the language surface lands, and `DiagSpan` is `#[non_exhaustive]` (KTD5) so that addition is additive. **Column accuracy for the field-packed/computed CPUs is contingent on an AST migration that is not currently scoped for them** — those CPUs stay line-granular until such a migration exists, so R2's column accuracy and R8's "every CPU inherits diagnostics" hold at *line* granularity for them, not as an automatic side effect. **R8 forces the span onto the engine's error path, not the AST** — not every CPU routes through the AST, and R8 requires every CPU to inherit diagnostics (at whatever granularity it can supply).
- **KTD2 — R1 is a unification: every entry point returns `AssemblyResult`; the substance is already there, but the return type is a deliberate breaking change.** A serde-derivable `AssemblyResult` becomes the one shape, and **all 27 assemble entry points return `Result<AssemblyResult, AsmError>`** — the 23 `Assembly`-returning functions (whose substance already matches) *and* the 3 linked + 1 warned outliers. This **changes 27 public signatures** — an intentional breaking change, acceptable because the contract ships as public **draft** under `#[non_exhaustive]` (KTD5) before any external consumer freezes against it, and dbg198x (the one in-tree consumer) is paused. It is **not** a "keep `Assembly`, add a parallel shape" move: leaving `Assembly` as a public return shape would miss R1 (two shapes persist) — so `Assembly` is either renamed into `AssemblyResult` or demoted to an internal builder that the entry points convert from. **U1 owns updating the in-tree tests that destructure `Assembly`** (the `lib.rs` smoke tests reading `a.origin`/`a.bytes`/`a.symbols`, and `assemble_vasm_warned`'s tuple-destructuring `vasm_*_warns_not_errors` tests) — "the full existing suite stays green" means those tests are migrated to the new shape as part of U1, not that the shape is non-breaking. Linked output (ca65 `.nes`, vasm hunk), whose flat `origin` is meaningless, is a **variant** (`Output::Flat { origin } | Output::Image`), not forced into the flat shape (R1's explicit "variant, not forced"). The AST is **not** exposed as R1's result in v1 — "R1 is really the AST" (the Outstanding Question) is a *converter/idea-6* reconception; v1's R1 is the assembly **output**.
- **KTD3 — Contract types live in a `contract` module inside `crates/asm198x` for v1; extractable to a crate with MCP.** serde is already present (dbg198x), so this adds no new base-CLI weight beyond serde-derive on the new types. Keeping async/LSP/WASM deps off the base path (the "lean binary" assumption) is unaffected — none of those land in v1. The eventual crate split (mirroring `isa-disasm`/`dbg198x`) is a follow-on with the first surface, not v1.
- **KTD4 — R1's symbol/section shape is co-designed with dbg198x's record model; the symbol slice stays draft until dbg198x actually consumes it.** `DebugData` already uses `dbg198x::Symbol`; `AssemblyResult`'s symbol/section exposure reuses dbg198x's typed symbol kinds + `(section, offset)` addressing rather than inventing a parallel model, so a resumed dbg198x reads the result directly (R6). Do not duplicate the symbol model. **But dbg198x is paused and its own model is not yet exercised by real consumption**, so freezing R1's symbol/section slice at MCP would freeze it against an unexercised sibling — the exact draft-then-freeze failure R7 guards against. **Freeze precondition (feeds KTD5/U5):** the symbol/section slice of `AssemblyResult` stays **draft past MCP** until a *resumed dbg198x has actually read an `AssemblyResult`*; the rest of the result (bytes, output variant, warnings) may freeze at MCP on the normal schedule. If a resumed dbg198x revises its symbol model against real Emu198x consumption, the still-draft slice bends to it rather than blocking it.
- **KTD5 — Public contract types are `#[non_exhaustive]` + versioned + skip-unknown from day one; the stability *promise* stays DRAFT until MCP (R7 draft-then-freeze).** A `version` field on the serialized envelope; unknown fields deserialize (skip-unknown) rather than erroring. The semver-freeze is a decision-record checklist deferred to first-surface contact — v1 ships the mechanism, documents the promise as draft.
- **KTD6 — R6 in v1 is only the thin shared diagnostic envelope** (`{ severity, message }`), distinct from the rich `Diagnostic`. dbg198x and the verdict pipeline may reuse it. This plan makes two **cross-plan planning-doc edits** (not code): mark `docs/plans/2026-07-03-001-feat-debug-info-format-plan.md` (dbg198x) **paused pending this contract's designed R1 shape**, and confirm `…-002-…verdict-pipeline…` is **independent / not paused**.
- **KTD7 — R4 and R5 are explicitly out of v1.** No public `Dialect` trait, no spec-query in this plan. Any unit that would touch them is a scope error — route to the MCP increment.

---

## Implementation Units

### U1. The unified structured result (`AssemblyResult`)

- **Goal:** collapse the 3 ad-hoc assembly return shapes into one serde-derivable `AssemblyResult` carrying bytes, output-kind (flat origin vs linked image), symbols, entry point, and warnings — the R1 shape every CPU inherits (R8).
- **Requirements:** R1, R7 (version/non_exhaustive), R8. Covers AE1 (clean path), AE6.
- **Dependencies:** none (first unit).
- **Files:** `crates/asm198x/src/contract.rs` (new — the contract types), `crates/asm198x/src/engine.rs` (re-express `Assembly` as / into `AssemblyResult`), `crates/asm198x/src/lib.rs` (the 4 outlier functions adopt the shape), `crates/asm198x/Cargo.toml` (serde derive), `crates/asm198x/tests/contract.rs` (new).
- **Approach:** define `AssemblyResult { output: Output, symbols, start, warnings }` where `Output::Flat { origin, bytes } | Output::Image { bytes }` handles the linked/bypass cases (KTD2). **The `diagnostics: Vec<Diagnostic>` field is added in U2**, where `Diagnostic` is defined — U1 must not embed a type it doesn't yet own (this keeps U1 genuinely dependency-free; see Dependencies). `#[non_exhaustive]`, `#[derive(Serialize, Deserialize)]`, `#[serde(default)]` skip-unknown. Reuse dbg198x's symbol model for the symbol exposure (KTD4). **All 27 entry points return `Result<AssemblyResult, AsmError>`** — the deliberate breaking change of KTD2; `Assembly` is renamed into / demoted behind `AssemblyResult`, and U1 migrates the in-tree tests that destructure `Assembly` (the `lib.rs` smoke tests and the `assemble_vasm_warned` tuple tests) to the new shape.
- **Execution note:** start from a failing serde round-trip test on `AssemblyResult` (serialize → deserialize → equal), then build the type to satisfy it.
- **Test scenarios:** *Covers AE1 (clean).* A clean 6502 assemble returns `AssemblyResult` with bytes + symbols + `Output::Flat{origin}`. A ca65 `.nes` assemble returns `Output::Image` (no meaningless origin). A vasm-warned assemble carries its warnings in the unified `warnings`. serde round-trip (of the U1 shape, before diagnostics) is identity. *Covers AE6.* the result type is CPU-agnostic — a second CPU's assemble returns the same shape with no per-CPU code. The migrated `lib.rs` smoke tests and vasm-warned tests pass against the new return type.

### U2. The `Diagnostic` model + optional `AsmError` span + shared envelope

- **Goal:** give asm198x's own errors the rustc shape — a span, a stable code, severity, message, and an optional machine-applicable fix — without churning the 344 line-only sites (KTD1), and expose the thin shared envelope (R6).
- **Requirements:** R2, R6 (envelope). Covers AE2, AE4 (envelope reuse).
- **Dependencies:** U1 (this unit **adds** `diagnostics: Vec<Diagnostic>` to `AssemblyResult` once `Diagnostic` exists, and adds its serde round-trip — the field U1 deliberately left off).
- **Files:** `crates/asm198x/src/contract.rs` (Diagnostic, DiagSpan, Severity, Code, Fix, DiagnosticEnvelope; add the `diagnostics` field to `AssemblyResult`), `crates/asm198x/src/engine.rs` (`AsmError` gains `span: Option<DiagSpan>`, `From<AsmError> for Diagnostic`), `crates/asm198x/tests/contract.rs`.
- **Approach:** `Diagnostic { span: Option<DiagSpan>, code: Code, severity: Severity, message: String, fix: Option<Fix> }`, all serde. `Code` is a stable `#[non_exhaustive]` enum of error kinds (start with the current message families; assign codes incrementally — **once assigned, a code's numeric/string identity is stable**, since R2 mandates stable codes; only *new* kinds are added). `DiagSpan { file: Option<FileId>, line, col, offset: Option<u32> }` — the KTD1 shape mirroring `ast::Span`, named distinctly to avoid colliding with `ast::Span`. `AsmError` gains `span: Option<DiagSpan>` defaulting to `None` — **`AsmError::new(line, msg)` keeps its signature** (span `None`); add `AsmError::at(span, msg)` for sites that have a position. `DiagnosticEnvelope { severity, message }` is the R6 thin type; `Diagnostic` derefs/flattens into it. `AsmError → Diagnostic` maps line-only errors to a line-granular span (`file`/`col`/`offset` `None`).
- **Execution note:** characterize first — a test that today's error → Diagnostic keeps the same line + message, so the conversion is provably lossless before columns are added.
- **Test scenarios:** *Covers AE2.* a Diagnostic with a populated span reports `col`/`offset`, not just line. *Covers AE4.* the verdict pipeline can construct a `DiagnosticEnvelope` from a foreign rejection (severity + verbatim text) without touching the rich fields. A line-only `AsmError` converts to a Diagnostic with a line-granular span and a stable code. `fix` is `Some` for a representative fixable case (e.g. an out-of-range byte → suggest masking) and `None` otherwise.

### U3. Populate real spans in the AST-routed dialects

- **Goal:** deliver AE2 column accuracy for the curriculum-heavy CPUs by threading `ast::Node` spans into the diagnostics they raise (KTD1's incremental population).
- **Requirements:** R2 (column accuracy for the common case). Covers AE2 end-to-end.
- **Dependencies:** U2.
- **Files:** `crates/asm198x/src/ast.rs` (error-raising paths carry the node span), the AST-routed dialect front-ends (`dialects/z80.rs`, `dialects/acme.rs`, `dialects/i8080.rs`, `dialects/m6800.rs`, `dialects/cdp1802.rs`, `dialects/scmp.rs`, `dialects/rgbasm.rs`, `dialects/lwasm.rs`) at the sites where a parse/lower error has a known column, `crates/asm198x/tests/contract.rs`.
- **Approach:** where the AST already knows the position (`ast::Node::span`, the per-line parse), construct the `AsmError`/`Diagnostic` with `AsmError::at(DiagSpan { file, line, col, offset: None }, …)` instead of line-only — `offset` stays `None` (the AST span carries no byte offset; KTD1). The eight dialects listed in Files are exactly those AST-routed today (per Grounding); no cross-plan migration is a prerequisite for this unit. Scope to the highest-value error sites (operand/mode errors), not every site — the rest stay line-granular and improve as more CPUs adopt the AST. Do **not** attempt the 344-site sweep (KTD1).
- **Execution note:** proof-first — a failing test asserting the column of a known out-of-range operand in an acme and a Z80 program before wiring the span through.
- **Test scenarios:** *Covers AE2.* an out-of-range operand in a 6502/acme program yields a diagnostic whose `col` points at the operand token, not the line start. Same for a Z80 program. A field-packed CPU's error stays line-granular (documented, not a regression).

### U4. JSON serialization + `--message-format=json`

- **Goal:** make the result and diagnostics machine-consumable and add the opt-in CLI JSON mode, human output unchanged (R3).
- **Requirements:** R3. Covers AE1 (the `--message-format=json` emission).
- **Dependencies:** U1, U2.
- **Files:** `crates/asm198x/src/main.rs` (arg parsing for `--message-format=json`, JSON emit path), `crates/asm198x/src/lib.rs` (a `to_json` on the result if not free from derive), `crates/asm198x/tests/cli_json.rs` (new).
- **Approach:** extend the hand-rolled CLI arg parser with `--message-format=human|json` (default `human`). On `json`, serialize the `AssemblyResult` (success) or the `Vec<Diagnostic>` (failure) to stdout; human output stays byte-for-byte as today when the flag is absent (or `=human`). Do **not** adopt clap in v1 (Outstanding Question → extend the hand-rolled parser; adding a dependency needs its own call).
- **Test scenarios:** *Covers AE1.* `--message-format=json` on a failing program emits a JSON diagnostic with span + stable code + fix-where-applicable; on a clean program, JSON with bytes + symbols. Absent the flag, output is unchanged from today (a golden-file test). Invalid `--message-format=xml` errors cleanly.

### U5. Versioning, skip-unknown, the freeze-draft record, and cross-plan notes

- **Goal:** ship R7's additive/versioned mechanism and record the draft-then-freeze governance, plus the R6 cross-plan status edits.
- **Requirements:** R7, R6 (cross-plan). Covers AE7.
- **Dependencies:** U1, U2.
- **Files:** `crates/asm198x/src/contract.rs` (version constant + field), `crates/asm198x/tests/contract.rs` (skip-unknown test), `decisions/core-contract-freeze.md` (new — the draft-then-freeze checklist, importing the dbg198x precedent incl. bounded-review + secondary-trigger per R7), `docs/plans/2026-07-03-001-feat-debug-info-format-plan.md` (dbg198x — paused note), `docs/plans/2026-07-03-002-feat-verdict-pipeline-plan.md` (independent/not-paused note).
- **Approach:** a `CONTRACT_VERSION` on the serialized envelope; `#[serde(default)]` / `#[non_exhaustive]` already give additive/skip-unknown (U1/U2). The freeze **promise** stays draft — the decision record documents the checklist and that it fires at MCP (not at sibling-crate consumption, pending the Outstanding Question, which this record resolves conservatively: crates may build against a draft contract). **The record also carries the KTD4 carve-out: the symbol/section slice of `AssemblyResult` stays draft *past* MCP until a resumed dbg198x has actually read an `AssemblyResult`** — so the slice co-designed with a paused sibling is not frozen against an unexercised model (mirrors R7's split-freeze reasoning for R4/R5). Make the two cross-plan planning-doc edits (KTD6).
- **Test scenarios:** *Covers AE7.* a serialized payload with an unknown extra field deserializes successfully (skip-unknown) rather than erroring; the payload carries a version field. The decision record exists and imports the bounded-review + secondary-trigger clauses.

---

## Verification Contract

- **Serde round-trip:** `AssemblyResult` and `Diagnostic` serialize → deserialize → equal (U1, U2).
- **Skip-unknown:** an unknown field in a serialized payload deserializes without error, and the payload carries a version (U5, AE7).
- **Column accuracy:** an out-of-range operand in an acme and a Z80 program produces a diagnostic whose `col` points at the token (`offset` may be `None`; KTD1) (U3, AE2).
- **CLI parity:** without `--message-format=json`, CLI output is byte-identical to today (golden file); with it, valid JSON emits for both success and failure (U4, AE1). *(AE3 is R5/spec-query, deferred to the MCP increment — not a v1 gate.)*
- **No regressions:** the full existing suite stays green — `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p asm198x`, and the `#[ignore]`d reference-tool conformance/curriculum guards (byte-identity of every CPU is unaffected — this is an additive result/error layer, KTD1/KTD2).
- **Envelope reuse:** a `DiagnosticEnvelope` constructs from a foreign-tool rejection shape without the rich fields (U2, AE4).

## Definition of Done

- R1 shipped: one `AssemblyResult` shape across all CPUs/dialects (all 27 entry points migrated — a deliberate breaking change under the draft stance), linked output as a variant, serde-derivable; in-tree tests migrated to the new return type (U1).
- R2 shipped: `Diagnostic` with `DiagSpan` (file/line/col, best-effort offset) + stable code + severity + message + optional fix; `AsmError` carries an optional span; AST-routed dialects populate real columns, field-packed CPUs stay line-granular (U2, U3).
- R3 shipped: `--message-format=json` opt-in, human output unchanged as default (U4).
- R6 (v1 slice): the thin `DiagnosticEnvelope` exists and is reusable; dbg198x paused-note and verdict independent-note landed (U2, U5).
- R7 shipped: version field + `#[non_exhaustive]` + skip-unknown; the draft-then-freeze decision record exists with the full dbg198x precedent, including the KTD4 carve-out (symbol/section slice stays draft past MCP until dbg198x consumes it) (U5).
- R8 holds: adding a CPU inherits the result + diagnostics with no per-CPU or per-surface work (U1 — verified by the CPU-agnostic result test).
- R4/R5 remain explicitly deferred (KTD7) — no public `Dialect` trait, no spec-query in this plan.
- Verification Contract gates all pass; the byte-identity of every CPU is unchanged.
