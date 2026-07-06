---
title: Language Surface - Plan
type: feat
date: 2026-07-04
topic: language-surface
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Language Surface - Plan

## Goal Capsule

- **Objective:** Give asm198x the language-surface features a real program needs — **includes / incbin, local-label scoping, and conditional assembly** in v1, with **macros as a deliberate second stage** — implemented once at the engine layer with thin per-dialect syntax skins, each differentially validated byte-identical against its reference assembler exactly as the instruction surface already is.
- **Product authority:** Steve Hill. Seeded from idea 4 of the 2026-07-03 ideation (`docs/ideation/2026-07-03-asm198x-world-class-ideation.html`). Includes are a firm v1 requirement (Steve). Explored *before* planning idea 3 (the core contract) on purpose — this surface dictates what the contract's source/span/symbol model must carry.
- **Open blockers:** One product decision parked during a timeout, awaiting confirmation — the v1 scope (recommended: *foundation first* — includes + local-labels + conditionals — with macros as stage 2). A redirect (e.g. macros in v1) reshapes scope and staging, not the cross-cutting contract requirements below.

---

## Product Contract

### Summary

Today asm198x is single-file to its core, has no macros or includes, supports conditional assembly only inside the ACME dialect (a local pre-engine preprocessor), and treats local labels as a Z80-only string mangle. Curriculum-sized programs survive, but serious homebrew and demoscene work hits the wall immediately — the community's two loudest signals are that local-label scoping is the most-praised assembler feature and macros are the escape hatch users reach for. v1 adds the tractable, high-value foundation — **includes/incbin, engine-level local-label scoping, and conditional assembly generalized to every dialect** — built once in the engine with a thin per-dialect skin that maps each reference's *syntax and semantics*, validated dialect-by-dialect against the real tool. Macros (the hardest, most dialect-divergent piece) are a focused second stage on top. The load-bearing consequence, and the reason this is explored first: **includes force a multi-file source model**, so every byte, symbol, and diagnostic must be attributable to *(file, line, column)* through an include chain — which directly dictates the shape of idea 3's contract (R1/R2) and dbg198x's source model.

### Problem Frame

The assembler validates instruction encodings byte-identically across 19 CPUs, but a program larger than a lesson cannot be written in it: no way to split source across files (`include`), pull in binary assets (`incbin`), reuse a label name safely (local scoping), vary a build (`if`/`else`), or factor repetition (macros). What exists is fragmentary and non-uniform — conditional assembly lives only in ACME as a brace-based preprocessor that folds into an `env` before the engine runs; local labels are a Z80-only prefix mangle; directives are hardcoded per-dialect `match` arms with no shared seam; and the whole pipeline reads exactly one file, so a diagnostic can only name a line, never a file. This is the largest single gap between "byte-identical validated" and "usable for a real, non-trivial program," and it is the language surface every incumbent assembler has and asm198x does not.

### Key Decisions

- **Foundation first, macros second.** v1 is includes/incbin + local-label scoping + conditional assembly; macros (MACRO/ENDM, parameters, expansion) and their kin (repeat/DUP/rept, modules) are stage 2. Macros are the hardest and most dialect-divergent piece — the ideation verifier's own caveat is that "N thin skins" badly undersells how much macro *semantics* differ across ca65/sjasmplus/rgbasm. The interlock is one-directional (macros *use* local labels and conditionals, not the reverse), so building the foundation first is the ground a good macro engine stands on, not throwaway work.
- **One engine mechanism, thin per-dialect skins — semantics included.** The engine owns the *mechanism* (source-file inclusion, conditional evaluation, scope resolution, and later macro expansion); each dialect maps its own syntax **and its semantic quirks** onto that mechanism. This explicitly rejects "spelling-only skins": expect a shared core plus real per-dialect semantic work, validated dialect-by-dialect. The ACME conditional preprocessor is the existing seed of this mechanism, to be generalized rather than reinvented.
- **Source-compatible: the reference's syntax, exactly.** Each dialect's `include`/`incbin`/conditional/local-label spelling matches its reference assembler verbatim — no invented syntax (the binding `syntax-stance` decision). ca65 `.include`/`.macro`/`.repeat`, sjasmplus `include`/`MACRO`/`DUP`, rgbasm its own — the assembler is a drop-in, not a new language.
- **Differential validation per dialect.** Every language feature is byte-identical to its reference, proven the same way the instruction surface is — the existing differential harness (`tests/differential.rs`), one probe per feature per dialect, with the gap-marker mechanism for the known cases a reference's behaviour can't be byte-reproduced.
- **Includes force a multi-file source model — this is what idea 3's contract must carry.** A byte can originate in an included file, so spans, symbols, and diagnostics must track *(file, line, column)* through the include chain, replacing today's line-only model. The span model must also **reserve room** for stage-2 macro-expansion frames (a rustc-style defined-at/invoked-at stack) and for **scoped** symbols, so idea 3's contract and dbg198x do not freeze a shape that later needs a breaking change.

### Requirements

**v1 — the foundation**

- R1. **`include` (source inclusion):** an assembly can pull another source file's text in at the include point, per each dialect's syntax, including nested includes, byte-identical to the reference. The reference's include-search-path behaviour is honoured.
- R2. **`incbin` (binary inclusion):** an assembly can insert raw bytes from a file at the current location, byte-identical to the reference, honouring the reference's offset/length options where they exist.
- R3. **Multi-file source model:** every emitted byte, symbol, span, and diagnostic is attributable to *(source file, line, column)* through the include chain — replacing today's line-only model. A diagnostic in an included file names that file, not the top-level line.
- R4. **Local-label scoping as an engine concept:** local labels scope between global labels (and, in stage 2, within a macro expansion), per each dialect's syntax (ca65 cheap `@locals`, sjasmplus, rgbasm `.locals`) — replacing the Z80-only string mangle, byte-identical to each reference.
- R5. **Conditional assembly, generalized:** the `if`/`else`/`endif` family (and `set`/define) works for every dialect per its own syntax — generalizing the ACME-only preprocessor — so only the taken branch emits, byte-identical to the reference.
- R6. **Per-dialect differential validation:** each language feature is covered by a byte-identical differential probe against its reference assembler, exactly like the instruction surface; known non-reproducible cases are gap-marked, not silently skipped.
- R7. **Room reserved for stage 2 and the contract:** the span/diagnostic model carries an (empty in v1) macro-expansion-frame stack and the symbol model carries scope, so the stage-2 macro engine, idea 3's contract, and dbg198x can populate them additively without a breaking change.

**What the contract (idea 3) must account for** — the cross-cutting output of exploring this first:

- C1. Idea 3's structured result (R1) and diagnostics (R2) must key spans on *(file, line, column)*, not line — a multi-file source model, because includes make one image span many files.
- C2. Idea 3's diagnostic/span model must reserve a macro-expansion-frame stack from v0 (populated when stage-2 macros land), the same way its address-space qualifier was reserved for banking.
- C3. Idea 3's symbol representation (R1) and dbg198x's symbol records must carry local-label **scope**, so a local reused in two scopes resolves to two distinct symbols.

### Acceptance Examples

- AE1. **Covers R1, R3.** A ca65 program that `.include`s a second file assembles byte-identical to `ca65`; a deliberate error in the included file produces a diagnostic naming *that file* and its line, not the top-level include line.
- AE2. **Covers R2.** An `incbin` of a binary asset inserts its bytes byte-identical to the reference, and the offset/length form (where the dialect has one) matches.
- AE3. **Covers R4.** A program that reuses a local-label name in two scopes assembles byte-identical to the reference — the locals do not collide — for a dialect whose reference has local scoping.
- AE4. **Covers R5.** A conditional-assembly program emits only the taken branch, byte-identical, for a **non-ACME** dialect (e.g. ca65 or sjasmplus), proving the generalization off ACME.
- AE5. **Covers R6.** Each v1 feature has a differential probe that both our assembler and the reference reproduce byte-identically; a known non-reproducible case is gap-marked.
- AE6. **Covers R7, C1, C2.** A diagnostic carries a *(file, line, column)* span, and the span model serialises an empty expansion-frame stack without a format change — demonstrating the room reserved for stage-2 macros and idea 3's contract.

### Scope Boundaries

**Deferred for later (stage 2 — the macro stage)**

- **Macros** — `MACRO`/`ENDM` (and `.macro`/`.endmacro`, rgbasm's form), parameters, expansion, and the expansion-frame *population* the v1 span model reserves room for. The single hardest, most dialect-divergent piece; its own focused effort on the proven foundation.
- **Repetition** — `repeat`/`rept`/`DUP` and friends (part of the macro stage).
- **Modules / namespaces** — sjasmplus modules and similar (the sjasmplus.rs:16 TODO's third item).

**Outside this product's identity**

- The contract's *implementation* — idea 3 owns the span/source/symbol *shape*; this plan states the **requirements on it** (C1–C3), it does not design it.
- Debug-record shapes — dbg198x owns those; this plan states what the source model must feed them (multi-file, scoped).
- Inventing syntax — every feature spells exactly as its reference does; asm198x adds no language of its own.

### Dependencies / Assumptions

- Verified this session (`/tmp/compound-engineering/ce-brainstorm/macro-engine/grounding.md`): source is single-file — the dialect's `parse(&self, source: &str) -> Vec<Statement>` (`dialect.rs:41`) takes one string, the engine's two passes see only `Statement { line, label, op }` (`engine.rs:346`), and the CLI reads one input (`main.rs:308`); conditional assembly exists only in ACME as a pre-engine brace preprocessor (`acme.rs:214-307`); directives are hardcoded per-dialect `match` arms (`acme.rs:567`, `ca65.rs:589`) with no shared seam; local labels are a Z80-only mangle (`qualify_locals`/`qualify_expr`, `z80.rs:451-496`); diagnostics carry `line: usize` with no source file (`AsmError` `engine.rs:67`, `Warning` `:100`, `LineRec` `:60`); the differential harness (`tests/differential.rs`) is the per-dialect validation path.
- The ACME conditional preprocessor is the existing seed to generalize, not a green field.
- **This brainstorm feeds idea 3 (core contract, `docs/plans/2026-07-03-003-feat-core-contract-plan.md`):** C1–C3 are requirements on that contract's R1/R2. Idea 3's plan should not freeze the span/source/symbol shape without accounting for them — the reason this was explored first.
- Cross-cutting with dbg198x (`docs/plans/2026-07-03-001-feat-debug-info-format-plan.md`): its source model must become multi-file and its symbol records scoped; it already deferred "macro expansion frames," which C2 reserves room for.

### Outstanding Questions

- **[Parked — confirm]** v1 scope: recommended *foundation first* (includes + local-labels + conditionals), macros stage 2. A redirect (macros in v1, or includes-only first) reshapes staging.
- Which dialects get the language surface first, and in what order — differential validation needs a reference per dialect, so the macro/include-rich, high-demand references (sjasmplus, ca65, rgbasm) are the natural start.
- How much per-dialect semantic divergence is "close enough" before the gap-marker mechanism is the honest answer — some dialect semantics (local-uniqueness schemes, conditional-expression evaluation) may not be byte-reproducible.
- Whether every dialect's `incbin` offset/length option is in v1 or a subset — planning.
- Include search-path rules per reference — planning.

### Sources

- `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` — idea 4 ("One macro engine, N dialect skins").
- Grounding scout (2026-07-04): `/tmp/compound-engineering/ce-brainstorm/macro-engine/grounding.md` — the single-file pipeline, ACME preprocessor, Z80 local hack, line-only diagnostics, differential harness, with `file:line`.
- `docs/plans/2026-07-03-003-feat-core-contract-plan.md` (idea 3, core contract) — the consumer of C1–C3; and `docs/plans/2026-07-03-001-feat-debug-info-format-plan.md` (dbg198x) — the debug-record consumer of the multi-file/scoped source model.
- `decisions/syntax-stance.md` — the source-compatible (no invented syntax) constraint every feature honours.
