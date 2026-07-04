---
title: Intermediate Representation (AST) - Plan
type: feat
date: 2026-07-04
topic: ir-ast-layer
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Intermediate Representation (AST) - Plan

## Goal Capsule

- **Objective:** Introduce a **source-preserving intermediate representation** — a shared, dialect-neutral **semantic AST** sitting between parsing and byte-lowering — that carries labels, instructions (with *unresolved* operand expressions), directives, and (as idea 4 lands) macro/include/conditional/scope constructs, with `(file, line, column)` provenance, symbol scope, and comments carried as **trivia**. It is the foundation four other ideas independently demand. v1 is the **A+ shape** (semantic AST + comment trivia), deliberately designed to grow toward per-dialect lossless syntax trees later, without committing to that machinery now.
- **Product authority:** Steve Hill. Surfaced by mapping ideas 3–7 — the keystone *under* idea 3's contract. Ambition level (A+, grow-toward-B) chosen 2026-07-04 after reviewing the IR design space (AST vs lossless CST; single vs layered; the rust-analyzer/rowan and Roslyn red-green precedents).
- **Open blockers:** None. The isa/encoding layer underneath is unchanged; this is a new layer *above* it.

---

## Product Contract

### Summary

Today the assembler lowers too early: parsing produces an already-encoding-oriented form (`Statement`/`Operation`, with the addressing mode resolved to a `&'static str`, operands lowered to encoding `Piece`s, comments stripped), so nothing downstream can operate on *source structure*. Mapping ideas 3–7 showed four of them independently need a tree that sits *before* that lowering — the dialect converter (to re-emit source), the language surface (macros expand, includes splice, conditionals prune, locals scope — all tree operations), the contract's diagnostics and dbg198x (provenance is node metadata), and the cycle listing (cost mapped to structure). This plan introduces that tree: a **shared, dialect-neutral semantic AST**. All dialects of a CPU parse into it; it lowers to today's encoding form → bytes (the isa layer is untouched). v1 is the **A+** design — the semantic AST with comments carried as trivia — the cheapest shape that unblocks every near-term consumer while reserving room to grow into a two-layer per-dialect-lossless architecture if full-fidelity conversion or incremental LSP ever become scheduled goals. It deliberately does **not** build per-dialect concrete syntax trees or a red-green/rowan tree now — that would be IDE-scale complexity ahead of need for a solo-maintained assembler.

### Problem Frame

The pipeline's front half throws away exactly what the family's next wave of features needs. Parsing jumps straight to an encoding-oriented representation, so: the converter has "no seam" to reconstruct source (its only reuse route, disassemble→reassemble, loses labels and structure); the language surface has no tree to expand macros, splice includes, or prune conditionals on (the ACME-only conditional preprocessor and the Z80-only local-label mangle are ad-hoc proto-versions of the missing layer); diagnostics and dbg198x can only name a line, never a file/column/scope/expansion-frame, because the byte-lowered statement carries none of that. The absence is invisible from any single feature and obvious across four — which is why it was found by mapping the ideas before planning them. It is the true foundation under the "core contract": idea 3's structured result and its bidirectional `Dialect` trait are really *about* this representation.

### Key Decisions

- **A+ shape: one shared dialect-neutral semantic AST + comment trivia.** A single tree, shared across a CPU's dialects at the semantic level, that defers lowering and carries provenance, scope, and comments-as-trivia. Not a lossless concrete syntax tree, not a red-green tree.
- **Designed to grow toward the two-layer split, without building it.** The chosen semantic AST *is* the shared-semantic layer of the eventual "per-dialect lossless CST → shared semantic AST" architecture. Adding per-dialect CSTs later is therefore additive, not a rewrite. v1 does not build them.
- **The isa/encoding layer is unchanged.** The AST lowers *to* today's `Statement`/`Operation` encoding form and thence to bytes; this is a new layer *above* the encoder, not a replacement of it. Output bytes are identical.
- **The AST is the pivot that makes dialects bidirectional.** *Parse* is source → AST (every dialect, v1). *Emit* is AST → source (built per idea 6, the converter). This is what turns idea 3's `Dialect` trait bidirectional (its R4).
- **Comments ride as trivia from day one.** Carrying comments attached to nodes is the cheap 80% of losslessness — it reserves the converter's later comment-fidelity without paying for full per-dialect CSTs or exact-formatting round-trip now (the same "reserve the room" discipline idea 3 used for expansion frames and banking).
- **Provenance and scope are first-class node metadata.** Every node carries `(file, line, column)` through include chains, reserved room for macro-expansion frames, and symbol scope — precisely what idea 3's C1–C3 and dbg198x require.

### Requirements

- R1. A **source-preserving semantic AST** sits between parse and lowering: dialects parse source → AST; the AST lowers to the existing encoding form → bytes, with **byte-identical output** to today. The isa/encoding layer is unchanged.
- R2. The AST is **dialect-neutral at the semantic level** — shared across a CPU's dialects — representing labels, instructions (mnemonic + *unresolved* operand expressions), directives, and, as idea 4 lands, macro/include/conditional/scope constructs.
- R3. Every AST node carries **source provenance**: `(file, line, column)` through include chains, with reserved room for macro-expansion frames (populated when idea 4's macros land).
- R4. Symbols carry **scope**, so a local label reused in two scopes is two distinct symbols in the AST.
- R5. **Comments are carried as trivia** attached to nodes (not stripped), so the converter and a later fidelity stage can emit them — even though v1 does not fully round-trip exact formatting.
- R6. The AST is the **pivot for bidirectional dialects**: parse (source → AST) ships in v1 for every dialect; emit (AST → source) is enabled by the design and built per idea 6.
- R7. The design is **explicitly extensible toward per-dialect lossless CSTs** (the two-layer architecture) — the AST is that split's shared-semantic layer — as an additive future step, not a rewrite. v1 builds neither CSTs nor a red-green tree.

### Relationship to the family (what this unblocks)

- C1. **Idea 3 (contract):** the AST is the real substance of idea 3's R1 "structured result" at the *source* level (distinct from the byte-level image, which is a lowering of it) and the pivot for R4's bidirectional `Dialect`. Idea 3's contract should be planned with the AST as R1's core.
- C2. **Idea 4 (language surface):** macros/includes/conditionals/locals are AST operations (expand/splice/prune/scope) — idea 4 builds *on* the AST, which is its prerequisite.
- C3. **Idea 6 (converter):** the converter is parse → AST → emit; the AST + emit direction is its foundation (instruction/directive-level now, comment fidelity later via trivia).
- C4. **Idea 5 (cycle listing):** cost annotations hang off AST nodes.
- **Sequencing:** the AST is the **first foundational build** — it sits under idea 3, idea 4, and idea 6. It comes before or with idea 3, and before idea 4/6.

### Acceptance Examples

- AE1. **Covers R1.** A representative program parses to an AST and lowers to **byte-identical** output versus today — the new layer changes structure, not bytes (the whole existing conformance corpus still passes).
- AE2. **Covers R3.** A program that includes a second file produces AST nodes whose provenance names the *included* file, line, and column — not the top-level include line.
- AE3. **Covers R5.** A comment in the source is present as queryable trivia on the AST, not dropped.
- AE4. **Covers R4.** A local label reused in two scopes yields two distinct scoped symbols in the AST.
- AE5. **Covers R6.** A dialect emits AST → source for a simple program, producing source the reference assembler accepts and assembles identically (proving the bidirectional pivot).
- AE6. **Covers R2.** Two dialects of one CPU (e.g. pasmo and sjasmplus) parse an equivalent program to the same shared semantic AST.

### Scope Boundaries

**Deferred for later**

- **Per-dialect lossless CSTs** (the two-layer / approach-B upgrade) — additive when full-fidelity conversion or incremental LSP become scheduled goals.
- **A red-green / incremental tree** (approach C, rust-analyzer style) — IDE-scale; not built.
- **Full formatting round-trip** — v1 keeps comments as trivia but does not reproduce exact whitespace/layout.
- **Macro-expansion-frame population** — the room is reserved (R3); idea 4's macros fill it.
- **The emit direction's full breadth** — parse is v1; emit is enabled-by-design and built out by idea 6.

**Outside this product's identity**

- Replacing the isa/encoding layer — unchanged; the AST lowers *to* it.
- Inventing dialect syntax — dialects stay source-compatible; the AST is internal.
- Competing with the byte-level structured result — that result *is* a lowering of the AST, not a rival representation.

### Dependencies / Assumptions

- Verified across this session's scouts (`/tmp/compound-engineering/ce-brainstorm/macro-engine/grounding.md`, `.../dialect-converter/grounding.md`): parse lowers directly to `Statement`/`Operation` (`engine.rs:346`, `:256`) with the addressing mode resolved to a string and operands lowered to encoding `Piece`s; comments are stripped at parse (`strip_comment`, `z80.rs:29-30`); no source-preserving tree exists; the ACME conditional preprocessor (`acme.rs:214-307`) and the Z80 local-label mangle (`z80.rs:451-496`) are ad-hoc proto-versions of the missing layer.
- **This is the foundation for ideas 3, 4, and 6** (and 5). It should be planned and built first; idea 3's contract (R1/R4) is re-conceived around it, and ideas 4/6 depend on it.
- Design precedents reviewed (external): rust-analyzer/`rowan` and Roslyn (lossless red-green trees + trivia), LLVM (layered IR), tree-sitter (CST grammars). A+ borrows the **trivia** concept without the full lossless-tree machinery.

### Outstanding Questions

- How **dialect-neutral** the shared AST can truly be given per-dialect *semantic* quirks (not just spelling) — where per-dialect semantics live: in the parse→AST lowering, or as node annotations. A real design question for planning, and the crux the two-layer split would eventually resolve.
- Whether the AST **wraps or replaces** the existing `Statement`/`Operation` — is today's form enriched into the AST, or a distinct type the AST lowers to? Planning.
- The exact **trivia model** — which nodes carry comments, leading vs trailing — planning.
- **Sequencing with idea 3:** does the AST fold into idea 3's plan as R1's core, or stand as its own foundational plan idea 3 depends on? (Lean: its own, planned first, since it also serves ideas 4 and 6 independently of idea 3's diagnostics/spec-query.)

### Sources

- Session mapping of ideas 3–7 and their grounding scouts (2026-07-04) — the four consumers that surfaced the need.
- `docs/plans/2026-07-03-003-feat-core-contract-plan.md` (idea 3), `docs/plans/2026-07-04-001-feat-language-surface-plan.md` (idea 4), `docs/plans/2026-07-04-003-feat-dialect-converter-plan.md` (idea 6), `docs/plans/2026-07-04-002-feat-cycle-analyzer-plan.md` (idea 5) — the dependents.
- External prior art: rust-analyzer / `rowan` (lossless red-green trees), Roslyn (red-green + trivia), LLVM (layered IR), tree-sitter (CST grammars).
