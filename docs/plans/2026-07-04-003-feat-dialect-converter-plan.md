---
title: Dialect Converter - Plan
type: feat
date: 2026-07-04
topic: dialect-converter
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Dialect Converter - Plan

## Goal Capsule

- **Objective:** `asm198x convert --from <dialect> --to <dialect> prog.asm` — read source written in one dialect of a CPU and emit it as a *different existing dialect of the same CPU*, proving each conversion byte-identical (assemble both, diff). v1 is instruction- and directive-level; comments, formatting, and (once idea 4 lands them) macros are not preserved yet. "Rescue over replace" applied to other people's source — decades of code trapped in dying tools' dialects — and nobody with a single-dialect tool can even attempt it.
- **Product authority:** Steve Hill. Seeded from idea 6 of the 2026-07-03 ideation (lowest confidence, 65%, but the highest "nobody else can try" score). Explored before planning idea 3 because it demands the contract's `Dialect` trait be *bidirectional*.
- **Open blockers:** None. v1 fidelity confirmed 2026-07-04: instruction/directive-level, self-verifying; comments/formatting/macros deferred. This is the most downstream and most-deferred idea — its near-term value is the shaping requirement it places on idea 3.

---

## Product Contract

### Summary

asm198x already has N dialect front-ends over one shared internal representation — but they only *parse* (source → internal); there is no *emit* direction (internal → source). The converter adds that direction: `convert --from pasmo --to sjasmplus prog.asm` reads a Z80 program in pasmo syntax and writes it in sjasmplus syntax, and it is **self-verifying** — it emits output only when assembling the input and the output produce byte-identical images, so a conversion is correct by construction or it is a reported error, never silent wrong output. No syntax is invented: both ends are real reference dialects, so the source-compatible stance holds and the output assembles under the real target tool. v1 covers instructions and directives; comments, formatting, and the language constructs of idea 4 (macros/includes/conditionals) are a later fidelity stage. The byte-diff doubles as a continuous cross-dialect consistency audit of the front-ends. The cross-cutting output: this demands idea 3's `Dialect` trait become **bidirectional** (an emit direction, not just parse) and a **convert-grade source-preserving representation** richer than the byte-level structured result — both real shaping requirements on idea 3.

### Problem Frame

Decades of retro source are trapped in the dialects of dying tools — the demoscene in sjasmplus, homebrew in WLA-DX, ROM-hacking in xkas — and no single-dialect assembler can move code between dialects, because none has more than one front-end. asm198x is structurally the only tool that can: it already normalises every dialect to one internal representation, so the missing half is emitting that representation back out as a different real dialect. The self-verification (assemble both, diff the bytes) is a snapshot test built into the feature. What is absent today is the emit direction itself and a representation that preserves enough source structure to render — the current internal form is byte-oriented, lowered toward encoding, and drops comments at parse.

### Key Decisions

- **Instruction/directive-level first, self-verifying.** v1 converts instructions and directives and proves each conversion byte-identical; comments, formatting/layout, and macros/includes/conditionals are not preserved yet (the ideation's own "scope this first, fidelity later" guidance). The common case — an orphaned program someone needs to build under a living tool — works.
- **Self-verification is a hard gate, not a nicety.** The converter emits output only if `assemble(input, from) == assemble(output, to)` byte-for-byte. A conversion it cannot verify is a reported error naming what didn't translate — never silent, possibly-wrong output. This is what makes the feature trustworthy despite being lowest-confidence.
- **Same-CPU dialect translation, not CPU porting.** Both ends are dialects of *one* CPU (Z80: pasmo ↔ sjasmplus; 6502: acme ↔ ca65). Converting across CPUs is porting, a different and much harder thing, explicitly out of scope.
- **No invented syntax.** Both ends are real reference dialects; the output is something the real target assembler accepts. The converter never emits an asm198x-canonical middle dialect.
- **The byte-diff is also an audit.** Running conversions in CI is a continuous cross-dialect consistency check on the front-ends — if two dialects of a CPU disagree, a conversion round-trip catches it.
- **Faithful conversion means the parse-re-emit route, not disassemble-reassemble.** Two routes are mechanically possible: (a) parse the source and re-emit it through a new dialect *render* direction, or (b) assemble to bytes and run the existing disassembler to reconstruct source. Route (b) reuses existing code but produces label-less, structure-less output (`jsr $c012`, not `jsr init`) — useless for source migration. So the converter takes route (a) — which is precisely what mandates idea 3's bidirectional `Dialect` (C1) and a source-preserving representation (C2), because the existing internal form is already lowered *past* usefulness (addressing mode resolved to a `&'static str`, operands lowered to encoding pieces, comments stripped at parse). There is no seam to build on today.

### Requirements

**v1 — instruction/directive-level, self-verifying**

- R1. `convert --from A --to B <file>` reads source in dialect A of a CPU and emits equivalent source in dialect B of the **same** CPU, at instruction and directive level.
- R2. **Self-verifying:** output is emitted only when assembling the input (dialect A) and the output (dialect B) yield byte-identical images; a conversion that cannot verify is a reported error naming what failed, never silent output.
- R3. Conversion is between **dialect pairs of one CPU** (e.g. Z80 pasmo↔sjasmplus, 6502 acme↔ca65) — not across CPUs.
- R4. **No invented syntax** — the output assembles under the real target reference tool; the converter emits a real dialect, never an asm198x-only one.
- R5. v1 covers instructions and directives; **comments, formatting, and macros/includes/conditionals are not preserved**, and any dropped content (e.g. comments) is surfaced, not silently lost.
- R6. The self-verification is reusable as a **cross-dialect consistency audit** — a CI check that a CPU's front-ends agree.

**What idea 3 (contract) and idea 4 (language surface) must account for**

- C1. Idea 3's `Dialect` trait (R4) must be **bidirectional** — an emit direction (internal → source) in addition to parse. Today it is parse-only; the converter renders through the emit direction. This shapes R4's design even though R4 is deferred to the first surface increment.
- C2. The converter needs a **convert-grade source-preserving representation** richer than idea 3's byte-level structured result (R1) — one that keeps labels, directives, and operand structure, not just encoded bytes. Idea 3's R1 alone is insufficient for rendering source.
- C3. Faithful conversion of **idea 4's** macros/includes/conditionals is the deferred fidelity problem; idea 4's language-construct representation must be render-able, so its plan should keep constructs (not just their expansion) available.

### Acceptance Examples

- AE1. **Covers R1, R2, R3, R4.** A Z80 program in pasmo syntax converts to sjasmplus syntax; assembling both yields byte-identical images, and the output assembles under real sjasmplus.
- AE2. **Covers R2.** A construct the converter cannot faithfully translate (the two images would differ) produces a reported error naming the offending construct — not silent wrong output.
- AE3. **Covers R5.** Comments in the input are dropped and the conversion surfaces that they were (no silent loss); a directive with a target-dialect equivalent is translated.
- AE4. **Covers R6.** A conversion run in CI catches a deliberately-introduced inconsistency between two front-ends of the same CPU.
- AE5. **Covers R3, R4.** An acme → ca65 conversion of a 6502 program produces source that real ca65 accepts and assembles identically.

### Scope Boundaries

**Deferred for later**

- **Comment and formatting preservation** — carrying comments and rough layout through so the output reads hand-written, not just assembles identically (a bigger representation change).
- **Macro/include/conditional conversion** — gated on idea 4 landing those constructs; converting them faithfully is the fidelity end-state.
- **Full round-trip fidelity** — the complete "reads like the original" experience.

**Outside this product's identity**

- **Cross-CPU porting** — translating a program from one CPU to another is not dialect conversion.
- **A canonical asm198x dialect** — both ends are always real reference dialects; the converter invents no middle language.

### Dependencies / Assumptions

- Verified this session (`/tmp/compound-engineering/ce-brainstorm/dialect-converter/grounding.md`): `Dialect::parse` (`dialect.rs:41`) is the trait's only source-facing method — **no emit/render direction anywhere**; the shared IR is lowered *past* source-preserving — `Statement` (`engine.rs:346`) keeps only line number, label text, and mnemonic, `Operation::Instruction` (`engine.rs:256`) resolves the addressing mode to a `&'static str` + one `Expr` per operand slot, and `Operation::Encoded` is lowered to encoding pieces; comments are stripped at parse (`strip_comment`, `z80.rs:29-30`); multiple dialects exist per CPU (Z80 pasmo+sjasmplus over `dialects::z80`, 6502 acme+ca65 over `dialects::mos6502`); no `convert` subcommand or IR→source emission exists. The disassembler (`isa-disasm`, bytes→text) never touches `Statement`/`Operation`, so it is the lossy route (b), not a reusable seam.
- **Shaping requirement on idea 3 (`docs/plans/2026-07-03-003-feat-core-contract-plan.md`):** its `Dialect` trait (R4) must gain an emit direction, and a convert-grade representation must exist above the byte-level structured result (C1, C2). Idea 3's R4/R1 plans should account for these even though the converter itself is far downstream.
- **Depends on idea 4 (`docs/plans/2026-07-04-001-feat-language-surface-plan.md`):** its language constructs are what a later fidelity stage converts (C3).
- Lowest-confidence, most-deferred idea; the near-term deliverable of this brainstorm is the dependency-graph finding, not the feature's schedule.

### Outstanding Questions

- Which dialect pairs are in v1 — Z80 pasmo↔sjasmplus and 6502 acme↔ca65 are the obvious first pair per CPU; the full matrix is larger.
- Whether the convert-grade representation is a new layer or an enrichment of the existing `Statement` stream — a real idea-3 shaping question, resolved when idea 3 is planned.
- How a non-verifying conversion is surfaced — the error detail (which construct, why the bytes differ) — planning.
- Whether directive translation needs a per-dialect directive-equivalence map, and how gaps (a directive with no target equivalent) are reported.

### Sources

- `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` — idea 6 ("Dialect-to-dialect source converter — asm198x convert").
- Grounding scout (2026-07-04): `/tmp/compound-engineering/ce-brainstorm/dialect-converter/grounding.md`.
- `docs/plans/2026-07-03-003-feat-core-contract-plan.md` (idea 3) — the bidirectional-`Dialect` + convert-grade-IR shaping requirements (C1, C2); `docs/plans/2026-07-04-001-feat-language-surface-plan.md` (idea 4) — the language constructs a later fidelity stage carries (C3).
- External prior art: Python 2to3, jscodeshift — codemod mechanics with a byte-diff as the built-in snapshot test.
