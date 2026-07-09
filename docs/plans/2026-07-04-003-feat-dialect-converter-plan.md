> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: Dialect Converter - Plan
type: feat
date: 2026-07-04
topic: dialect-converter
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
planned: 2026-07-06
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

**Resolved during planning (2026-07-06):**

- ~~Which dialect pairs are in v1~~ → **KTD4/KTD6:** Z80 pasmo→sjasmplus and 6502 acme→ca65, in the two Acceptance-Example directions; computed-operand CPUs and reverse directions deferred.
- ~~Convert-grade representation: new layer vs `Statement` enrichment~~ → **KTD1:** the `ast::Program` (already built) is the render source; the `Operation` stream was considered and rejected (it drops local-label scope).
- ~~How a non-verifying conversion is surfaced~~ → **KTD3/U1:** an `AsmError` naming the first unrenderable node or the diverging byte offset; no output emitted.
- ~~Directive-equivalence: shared map vs per-dialect~~ → **KTD5:** per-renderer, no shared cross-dialect table; a missing equivalent is a reported error.

**Open (deferred to implementation):**

- **Number-spelling loss is accepted for v1** but the per-operand `source` slot (`ast::Operand::Expr.source`, reserved) is where a later fidelity stage would restore radix — noted so U5 doesn't try to preserve spelling prematurely.
- **The exact shared origin for the 6502 neutral corpus** (KTD8) — a single conventional base (e.g. `$0200` or `$c000`) the neutral test programs pin via their `*=`/`.org`, so both flat images align. Chosen in U7 when the flat ca65 path lands.

### Sources

- `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` — idea 6 ("Dialect-to-dialect source converter — asm198x convert").
- Grounding scout (2026-07-04): `/tmp/compound-engineering/ce-brainstorm/dialect-converter/grounding.md`.
- `docs/plans/2026-07-03-003-feat-core-contract-plan.md` (idea 3) — the bidirectional-`Dialect` + convert-grade-IR shaping requirements (C1, C2); `docs/plans/2026-07-04-001-feat-language-surface-plan.md` (idea 4) — the language constructs a later fidelity stage carries (C3).
- External prior art: Python 2to3, jscodeshift — codemod mechanics with a byte-diff as the built-in snapshot test.

---

## Planning Contract

**Product Contract preservation:** R1–R6, C1–C3, and the Acceptance Examples are unchanged. **The requirements-era "no seam to build on today" framing is corrected** by the grounding below (not a product-scope change) — it appears both in the Dependencies/Assumptions note ("the shared IR is lowered past source-preserving") and in the Key Decisions route-(a) bullet ("There is no seam to build on today"), and both were written before the AST layer existed. The AST now *is* source-preserving (C2), so those statements are superseded; but the **emit direction the AST provides is a same-dialect verbatim formatter, not a cross-dialect structural renderer** — so the converter's core work is building that renderer, not wiring up an existing one. This is why the plan is larger than a thin "parse→emit→diff": see KTD2. Read the Product Contract's pre-AST seam claims as historical context, superseded by this Grounding.

### Grounding (verified 2026-07-06, against the AST built this session)

- **The render source exists and is faithful.** `ast::Item::Instruction { mnemonic: String, mode: &'static str, operands: Vec<Operand> }` carries an instruction structurally (`ast.rs:200`). `Operand::Expr { value: Expr, source: String }` (`ast.rs:157`) — and `Expr` (`engine.rs:153`) is `Num(i64) | Sym(String) | Pc | Lo | Hi | Bank | Neg | Bin(BinOp,…)`. **`Expr::Sym` preserves symbol names**, so a structural render keeps `lda init`, not `lda $c012` — the disassemble-reassemble route (b) the Product Contract rejects is genuinely avoided. Both dialects of one CPU resolve to the **same `isa` `mode` labels** (acme and ca65 both target `isa::mos6502`), so the renderer keys on the shared mode label.
- **The render direction does NOT exist.** `ast::emit(program, equ_label_colon: bool)` (`ast.rs:449`) renders every operation from **`node.source`** — the raw operation text *verbatim in the original dialect* (`ast.rs:508/524/534/548`) — canonicalising layout only. The `Dialect` trait (`dialect.rs:25`) has **no `render`/`emit` method**; the single per-dialect emit divergence is one bool, `equ_label_colon` (`dialect.rs:77`). The structured `Item`/`Operand` payloads are used only to *classify* a line (`is_equ`, `is Conditional`), never to *regenerate* its text. So emit is a same-dialect formatter; a cross-dialect renderer is net-new.
- **Per-dialect AST readiness is uneven.** The Z80 dialects populate structural `Item::Instruction` (`z80.rs:192`, `item: op.map(item_from_operation)`), so **pasmo/sjasmplus are render-ready**. But **acme leaves instructions `item: None`** (`acme.rs:345/365`) — its formatter only round-tripped verbatim source, so acme instructions are unstructured in the AST. And **ca65 has no `parse_ast`** at all (only `parse` → `Statement`). This shapes the 6502 pair's units (U4).
- **The self-verify gate reuses existing assembly.** Every dialect has `assemble_*` (`lib.rs`); the gate assembles both sides and diffs bytes. Wrinkle: the ca65 front-end can emit a linked `.nes` container, not a flat image — the diff must compare the **flat code image** on both sides (see Open Questions).
- **Computed-operand CPUs are out.** The 6809 and field-packed CPUs carry `Item::Encoded(Vec<Piece>)` — pre-computed encoding bytes, not structured operands — so structural render would mean reversing the encoding. v1's two pairs are both fixed-slot `Item::Instruction`; Encoded CPUs are deferred (KTD4).

### Key Technical Decisions

- **KTD1 — Render from the AST `ast::Program`, not the byte-lowered `Operation`/`Statement` stream.** The AST keeps label **scope** (`ast::Scope` — needed to translate local-label syntax, e.g. acme `.loop` ↔ ca65 `@loop`), directive structure, and **comment trivia** (`ast::Node::trivia` — `parse_ast` preserves comments *now*, which is how the formatter round-trips them; it is the `parse`→`Statement` path, not `parse_ast`, that strips them per the grounding's `strip_comment` note). So AE3's dropped-comment surfacing has a home: v1 does not *render* comments into the target (R5 defers formatting), but it *detects* their presence from `node.trivia` and surfaces the drop. The AST is the convert-grade representation the Product Contract's C2 names. The `Operation` stream (uniform across all dialects, no `parse_ast` gap) was **considered and rejected** as the v1 source: it drops local-label scope, which real conversion needs. The trade is the acme/ca65 readiness work in U4 — accepted because scope-correct local labels are table-stakes, and the AST is where deferred comment/formatting fidelity (R5) later plugs in without re-architecting.
- **KTD2 — The core artifact is a per-dialect structural `Renderer` — the real emit direction C1 asks for.** Add a `render` seam (a `Dialect::render_line(&Node) -> Result<String, AsmError>` method, or a per-dialect `render` module the converter dispatches to) that is the **inverse of the dialect's mode-resolution**: `render_expr(&Expr)` — covering **every** `Expr` variant: Num→canonical, Sym→verbatim name, **`Pc`→the dialect's location-counter sigil (`$` in pasmo/sjasmplus, `*` in acme/ca65 — a real cross-dialect divergence)**, `Lo`/`Hi`/`Bank`→the dialect's low/high/bank operator, `Neg`→negation, `Bin`→operator — plus `render_instruction(mnemonic, mode, &[Operand])` (mode label → the target dialect's addressing syntax), and `render_directive(&Item)` (org/equ/bytes/words → the target keyword). This is distinct from `ast::emit`'s verbatim formatter and is what makes A-source render as B. It does **not** touch `ast::emit` (the formatter keeps its verbatim path for `asm198x fmt`).
- **KTD3 — Self-verification is a hard gate (R2).** The converter emits output only if `assemble(input, from)` and `assemble(rendered_output, to)` produce byte-identical **flat code images at the same origin**; otherwise it reports an error naming the untranslatable construct and emits nothing. Number spelling/radix is *not* preserved (`Expr::Num` is a value; the per-operand `source` slot that would restore it stays empty in v1) — acceptable because byte-identity, not textual identity, is the contract (R5 defers formatting). The Z80 pair reuses the existing `assemble_pasmo`/`assemble_sjasmplus` directly (both flat at org 0). The 6502 pair needs KTD8's flat ca65 path.
- **KTD8 — acme→ca65 is a pure dialect conversion at a shared origin, which requires a flat ca65 assembly path (U7) and a neutral 6502 corpus, not the platform curricula.** The existing `assemble_ca65` emits **only** a linked NES `.nes` ROM (`ca65.rs`, `PRG_BASE=0x8000`, CODE at `$8000`, `VECTORS` at `$FFFA`), while `assemble_acme` emits a flat image at the source's `*=` origin — so they can never be byte-identical (absolute operands encode different addresses at `$8000` vs `$c000`, and the acme-C64 / ca65-NES curricula share no program). The verify gate therefore compares a **flat ca65 image that honours the source `.org`** (U7 adds this flat path, bypassing the NES linker) against the acme flat image at the **same** origin, over a **neutral 6502 test corpus** (pure 6502, no platform vectors/scaffolding) the plan authors — *not* the platform-specific curriculum. This is dialect conversion (R3, same CPU), explicitly **not** C64→NES platform porting (out of scope, Scope Boundaries). If U7's flat ca65 path proves larger than budgeted, the fallback is to narrow v1 to the Z80 pair (which alone proves the machinery) and defer the 6502 pair.
- **KTD4 — v1 is the two fixed-slot dialect pairs; computed-operand CPUs are deferred.** Z80 pasmo↔sjasmplus and 6502 acme↔ca65 only. The 6809 and field-packed CPUs (`Item::Encoded`) need the encoding reversed to render structurally — a separate, larger effort, explicitly out of v1.
- **KTD5 — Directive equivalence lives in each renderer, not a shared cross-dialect map.** Each dialect's renderer knows its own directive spelling; there is no central A↔B table. A directive with no target-dialect equivalent is a reported error (R5, AE3), surfaced the same way as any untranslatable construct. (Resolves the Outstanding Question on a per-dialect equivalence map: per-renderer, no shared map.)
- **KTD6 — v1 directionality follows the two Acceptance Examples: pasmo→sjasmplus and acme→ca65.** Each pair's reverse direction is a follow-on. This matters because ca65 is a v1 render **target** (needs a renderer) but not a v1 render **source** (would need a `parse_ast` it lacks); acme→ca65 sidesteps the missing ca65 `parse_ast`. The Z80 pair, sharing one core, is effectively bidirectional for free but AE1 pins the pasmo→sjasmplus direction.
- **KTD7 — `asm198x convert --from <dialect> --to <dialect> <file>` is a subcommand of the one binary** (per `decisions/packaging-and-cpu-roadmap.md`), extending the hand-rolled CLI arg parser (no new dependency).

---

## Implementation Units

### U1. The render seam + expression renderer + self-verify harness

- **Goal:** the shared foundation — an `Expr` renderer, the per-dialect render seam (KTD2), and the byte-identity verify gate (KTD3) — proven end-to-end on a trivial single-instruction program before any real dialect renderer exists.
- **Requirements:** R2 (self-verify), C1 (the render direction). Covers AE2 (the error path).
- **Dependencies:** none (first unit).
- **Files:** `crates/asm198x/src/convert.rs` (new — the render seam, the verify gate, `render_expr`), `crates/asm198x/src/dialect.rs` (add the `render_line` trait method, defaulting to an "unsupported for this dialect" error), `crates/asm198x/tests/convert.rs` (new).
- **Approach:** define `render_expr(&Expr, &dyn Dialect) -> String` (Num→canonical hex/decimal, Sym→name, `Lo`/`Hi`/`Bank`/`Neg`→the dialect's operator, `Bin`→infix). Add `Dialect::render_line(&self, node: &ast::Node) -> Result<String, AsmError>` defaulting to `Err` ("dialect has no converter renderer yet"). The verify gate: `convert(source, from, to)` → `from.parse_ast(source)` → for each node `to.render_line(node)` → assemble both sides via the existing `assemble_*` → diff **flat code images** → return the rendered text on match, or an `AsmError` naming the first node that failed to render or the byte offset where images diverged.
- **Execution note:** start from a failing test — a one-line program that renders and verifies — then build the seam to green.
- **Test scenarios:** *Covers AE2.* a node whose dialect returns the default `render_line` error surfaces a reported error naming the construct, and emits nothing. `render_expr` round-trips every variant — `Sym`, `Num`, `Pc` (the `$`/`*` divergence), `Lo(Sym)`, `Hi`, `Bank`, `Neg`, and `Bin` — to the expected target text. The verify gate returns an error (not a panic) when the two assembled images differ by one byte.

### U2. The Z80 renderer (pasmo / sjasmplus) — the proving-ground pair

- **Goal:** render structural Z80 `Item::Instruction` + directives + local labels into pasmo and sjasmplus syntax, proving pasmo→sjasmplus byte-identical on the curriculum corpus (AE1). The Z80 pair is thin at the instruction level (near-identical syntax) — deliberately, so it validates the whole machinery cheaply before the meaty 6502 pair.
- **Requirements:** R1, R3, R4. Covers AE1.
- **Dependencies:** U1.
- **Files:** `crates/asm198x/src/dialects/z80.rs` (implement `render_line` for the Z80 dialects), `crates/asm198x/tests/convert.rs`.
- **Approach:** implement `render_line` over `Item::Instruction` (map the `isa::z80` mode label + operands to Z80 operand syntax via `render_expr`), `Item::Org`/`Equ`/`Bytes`/`Words` (the pasmo/sjasmplus directive keywords: `org`, `equ`, `defb`/`db`, `defw`/`dw`), and local-label rendering from `ast::Scope`. Where pasmo and sjasmplus diverge (directive spelling, local-label sigil, `equ` colon per `equ_label_colon`), branch on the target dialect.
- **Test scenarios:** *Covers AE1.* a pasmo Z80 program renders to sjasmplus and both assemble byte-identical (curriculum corpus, `#[ignore]`d against the real tools). A program using a local label renders the target's local-label syntax and stays byte-identical. A directive (`defb` ↔ `db`) renders the target spelling.

### U3. The `convert` subcommand + error surfacing

- **Goal:** the user-facing `asm198x convert --from A --to B file`, wiring parse_ast→render→self-verify and reporting untranslatable constructs (R1, R2).
- **Requirements:** R1, R2. Covers AE1, AE2.
- **Dependencies:** U1, U2.
- **Files:** `crates/asm198x/src/main.rs` (the `convert` subcommand + arg parsing), `crates/asm198x/tests/cli_convert.rs` (new).
- **Approach:** extend the hand-rolled CLI (KTD7) with `convert --from <dialect> --to <dialect> <file>`, resolving each dialect name to its front-end, calling `convert::convert`, writing the rendered source to stdout on success or the reported error to stderr (non-zero exit) on a non-verifying conversion. Reject an unknown dialect and a cross-CPU pair (`--from` and `--to` targeting different CPUs — R3) with a clear message.
- **Test scenarios:** *Covers AE1.* `convert --from pasmo --to sjasmplus prog.asm` writes sjasmplus source to stdout and exits 0. *Covers AE2.* a program the converter cannot verify exits non-zero and names the construct on stderr, writing no source to stdout. A cross-CPU pair (`--from pasmo --to acme`) errors clearly. An unknown dialect name errors clearly.

### U4. Structural-instruction upgrade for acme (the 6502-source readiness gap)

- **Goal:** populate `Item::Instruction` in acme's `parse_ast` (today `item: None` for instructions) so acme is a structural conversion source, without disturbing the acme formatter's byte-identity or idempotence.
- **Requirements:** R1 (enables acme as a render source). Prerequisite for U5's acme→ca65.
- **Dependencies:** U1.
- **Files:** `crates/asm198x/src/dialects/acme.rs` (populate `item` from the parsed instruction `Operation` via `item_from_operation`, as `z80.rs` does), `crates/asm198x/tests/convert.rs`, and the existing acme formatter tests as the guard.
- **Approach:** where acme's `parse_ast` currently sets `item: None` for an instruction line (`acme.rs:345/365`), populate `Item::Instruction` from the parsed instruction, keeping `node.source` intact so the formatter's verbatim path is unchanged (`ast::emit` still renders from `node.source`, so formatter output must not change). **Env-coupling caveat:** acme's full mode resolution (`mos6502::resolve_mode`, `acme.rs:868`) needs a size environment (constants + zp-pinned labels) to pick zeropage-vs-absolute, but `parse_ast`/`parse_program` is deliberately syntactic and builds no such env. So the AST captures the **syntactic addressing category** (immediate / indexed / indirect / direct — resolvable without an env) and the renderer emits **symbolic operands**, letting the *target* dialect re-resolve zp-vs-abs size. Where the two dialects' size defaults diverge for a given operand, the bytes differ and the self-verify gate (KTD3) reports it — never silent wrong output.
- **Execution note:** characterize first — the acme formatter's existing byte-identity + idempotence tests must stay green after `item` is populated; run them as the regression guard before and after.
- **Test scenarios:** an acme instruction line now carries `Item::Instruction` with the right mnemonic/mode/operands. The full acme formatter suite (byte-identity + idempotence across the C64 curriculum corpus) is unchanged. A structured acme instruction renders (via U5) to valid ca65.

### U7. A flat ca65 assembly path for the verify gate

- **Goal:** give ca65 a flat-image assembly path that honours the source `.org` and bypasses the NES linker (KTD8), so acme and ca65 output can be compared at the same origin. Prerequisite for the 6502 verify gate.
- **Requirements:** R2 (makes the 6502 self-verify reachable).
- **Dependencies:** U1.
- **Files:** `crates/asm198x/src/dialects/ca65.rs` (a flat-assembly entry beside the existing NES-linked path), `crates/asm198x/src/lib.rs` (expose it), `crates/asm198x/tests/convert.rs`.
- **Approach:** add a ca65 assembly mode that emits a flat image at the `.org` origin — assemble the CODE segment without the fixed NES config (no 16-byte header, no `$8000`/`VECTORS` layout, no gap-fill). This is the ca65 analogue of the flat path acme/pasmo already use; the existing NES-linked `assemble_ca65` is untouched (its curriculum use stands). Confirm against real `ca65` assembling to a flat binary (ca65 + a flat ld65 config, or object-then-flat) so R4 holds.
- **Execution note:** the existing NES-linked ca65 path and its curriculum tests must stay green — this adds a path, it does not change the linked one.
- **Test scenarios:** a neutral 6502 program with `.org $c000` assembles to a flat image based at `$c000` (no NES header), byte-identical to real ca65's flat output. The existing NES-linked ca65 curriculum tests are unchanged.

### U5. The ca65 renderer + the acme→ca65 conversion — the real-divergence pair

- **Goal:** render structural 6502 `Item::Instruction` + directives + labels into **ca65** syntax, proving **acme→ca65** byte-identical over a neutral 6502 corpus at a shared origin (AE5) — the pair where genuine dialect divergence lives. (The acme *renderer* — needed only for the deferred ca65→acme reverse — is out of v1; acme is the render **source** here, structured by U4.)
- **Requirements:** R1, R3, R4. Covers AE5, AE3 (directive translation + dropped-comment surfacing).
- **Dependencies:** U1, U4 (acme structural source), U7 (flat ca65 image).
- **Files:** `crates/asm198x/src/dialects/ca65.rs` (implement `render_line`), `crates/asm198x/tests/convert.rs`, plus a small neutral 6502 test corpus under `crates/asm198x/tests/` (pure 6502, no platform vectors).
- **Approach:** implement ca65's `render_line` — the `isa::mos6502` mode label + operands → ca65 addressing syntax (immediate `#`, `zp`, `zp,x`, `(zp),y`, absolute, …), directives (acme `!byte`/`!word`/`* = $addr`/`!fill` → ca65 `.byte`/`.word`/`.org`/`.res`), anonymous labels (acme `+`/`-` → ca65 `:+`/`:-`), and local labels (acme `.name` → ca65 `@name`) from `ast::Scope`. The self-verify gate assembles acme flat and ca65 flat (U7) at the same origin and diffs (KTD3/KTD8). Surface dropped comments by inspecting `node.trivia` (KTD1) — v1 does not render them but reports that they were dropped (R5, AE3).
- **Test scenarios:** *Covers AE5.* a neutral acme 6502 program renders to ca65 and both assemble byte-identical (flat images at a shared origin, `#[ignore]`d against real acme + ca65). *Covers AE3.* a directive with a ca65 equivalent (`!byte`→`.byte`) translates; input comments (seen in `node.trivia`) are dropped and the conversion surfaces that, not silently. An anonymous-label program (`+`/`-`) renders ca65 `:+`/`:-` and stays byte-identical. A local label renders `@name`.

### U6. The CI cross-dialect consistency audit

- **Goal:** make the self-verification reusable as a CI check that a CPU's front-ends agree (R6) — the byte-diff as a continuous front-end audit.
- **Requirements:** R6. Covers AE4.
- **Dependencies:** U2, U5.
- **Files:** `crates/asm198x/tests/convert.rs` (or a dedicated `tests/convert_audit.rs`), reusing the curriculum corpus.
- **Approach:** a test that converts each curriculum program across its CPU's dialect pair and asserts byte-identity — so a divergence introduced into one front-end is caught by a failing round-trip. Reuses the U1 verify gate; degrades gracefully (`#[ignore]`d) when the reference tools are absent, like the existing conformance suite.
- **Test scenarios:** *Covers AE4.* a deliberately-introduced inconsistency between two front-ends of one CPU makes a conversion round-trip fail. With the front-ends consistent, the audit passes across the corpus.

---

## Verification Contract

- **Self-verify gate:** a conversion emits output only when the two assembled flat images are byte-identical; a divergence or an unrenderable node yields an `AsmError`, never silent output (U1/U3, R2, AE2).
- **Z80 pair:** pasmo→sjasmplus is byte-identical across the curriculum corpus and assembles under real sjasmplus (U2, AE1).
- **6502 pair:** acme→ca65 is byte-identical over a neutral 6502 corpus at a shared origin (flat images via U7's flat ca65 path) and assembles under real ca65 (U5, U7, AE5).
- **ca65 flat path:** a neutral 6502 program assembles to a flat image at its `.org` (no NES header) byte-identical to real ca65's flat output; the existing NES-linked ca65 curriculum tests are unchanged (U7).
- **acme regression:** the acme formatter's byte-identity + idempotence suite stays green after the structural-instruction upgrade (U4).
- **CLI:** `convert --from --to` writes source on success, reports the construct and exits non-zero on a non-verifying conversion, and rejects cross-CPU pairs and unknown dialects (U3).
- **CI audit:** the cross-dialect round-trip catches a deliberately-introduced front-end inconsistency (U6, AE4).
- **No regressions:** `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p asm198x` stay green; `ast::emit` (the formatter) is untouched, so every existing formatter/assembly guard is unaffected.

## Definition of Done

- R1: `convert --from A --to B <file>` renders instruction- and directive-level source in dialect B of the same CPU, for both v1 pairs in the AE directions — pasmo→sjasmplus (U2) and acme→ca65 (U5), the latter over a flat ca65 image at a shared origin (U7).
- R2: self-verifying — output only on byte-identical images; untranslatable constructs are reported, never silent (U1, U3).
- R3: conversion is same-CPU only; cross-CPU pairs are rejected (U3). acme→ca65 is dialect conversion, not C64→NES porting (KTD8).
- R4: output assembles under the real target reference tool; no invented syntax (U2, U5, U7 — verified against real pasmo/sjasmplus/acme/ca65 incl. ca65's flat output).
- R5: comments/formatting/macros not preserved; dropped comments are surfaced, not silently lost (U5, AE3).
- R6: the self-verification runs in CI as a cross-dialect front-end audit (U6).
- C1 completed: the `Dialect` render direction exists (the structural renderer, KTD2) — not just the parse direction.
- C2 satisfied: the convert-grade source-preserving representation is the pre-existing `ast::Program` (KTD1) — met by prior work, consumed here, not built by a unit.
- Computed-operand CPUs (6809+, `Item::Encoded`), the acme *renderer* / each pair's reverse direction remain explicitly deferred (KTD4, KTD6).
- Verification Contract gates pass; `ast::emit` and all existing guards are unchanged.
