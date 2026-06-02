# Decision: Source-compatible syntax per machine

**Status:** Active. Binding for Asm198x.

**Date:** 2026-05-30.

## The decision

Each assembler is **source-compatible with real existing dialects**, not a
single unified Asm198x syntax. The instruction *encoding* underneath comes from
the shared [`isa`](../crates/isa) spec; the dialect — directives, literals,
operators, label/scope rules, macros — lives in a front-end above it.

**Dialect is an axis independent of CPU.** Asm198x supports *multiple
first-class dialects per CPU*, not one dominant dialect. A dialect front-end
declares which CPU's `isa` spec it targets; several front-ends may target the
same spec (e.g. acme and ca65 both emit 6502). One CPU, one spec, many possible
front-ends.

The goal: real-world source for a machine assembles **unchanged**. Someone with
a working acme C64 project, a ca65 NES project, or a PasmoNext Spectrum project
should point Asm198x at it and get the same bytes out, without porting syntax.

## Dialect targets, prioritised by curriculum

Priority is set by **what Code198x actually consumes**, not by which dialect is
most popular in the wild. The curriculum's own source is the first body that
must assemble unchanged. A 2026-06-02 scan of the curriculum settled the list:

| CPU | First-class dialects | Curriculum platform(s) | Also consider |
|-----|---------------------|------------------------|----------------|
| 6502 | **acme**, **ca65** | C64 (acme), NES (ca65) | 64tass, dasm |
| Z80 | **PasmoNext** (primary), sjasmplus | Spectrum (pasmonext) | pasmo, z80asm |
| 68000 | **vasm** (mot syntax) | Amiga (vasm) | Devpac/HiSoft |

Both 6502 dialects are first-class: the curriculum uses acme for the C64 and
ca65 for the NES, so neither is "also consider." For Z80, **PasmoNext is
primary** — the ZX Spectrum Next fork of pasmo (Julián Albo, modified by C
Kirby) that the curriculum invokes as `pasmonext`. PasmoNext is a syntactic
superset of vanilla pasmo; for standard Z80 the two are byte-identical, so one
standard-Z80 backend serves both, and vanilla pasmo drops to "also consider."
The Z80N extended opcodes PasmoNext adds (MUL, LDIRX, NEXTREG, …) are a deferred
ISA-spec extension, authored when the curriculum uses them — a 2026-06-02 scan
found the corpus uses only standard Z80. sjasmplus stays a first-class second
front-end (popular in the wider scene, useful breadth). This corrects an
earlier version of this record that named sjasmplus primary — the curriculum
does not use it.

These are targets, not commitments to bug-for-bug parity. Where dialects
genuinely conflict, document the choice here.

## Why not a unified house syntax

A unified syntax would be cleaner to teach and document, but it would make
every existing body of source need porting — which defeats the rescue mission
(see [`../../../decisions/asm198x-and-shared-isa-spec.md`](../../../decisions/asm198x-and-shared-isa-spec.md)).
The whole reason Asm198x exists is that the *tools* are hard to run, not that
the *source* is wrong. Keep the source working.

## What is shared vs per-dialect

- **Shared (in `isa`):** opcode encodings, operand layout, cycle counts, flags.
- **Shared (in the engine):** expression evaluation, symbol table, sections,
  output formats, listing — the dialect-agnostic machinery.
- **Per-dialect (in each dialect front-end):** operator syntax, directive names
  and semantics, number/string literal forms, label and scope rules, macro
  syntax. A front-end names the `isa` spec it targets, so a CPU can carry
  several (acme and ca65 both target the 6502 spec).

Do **not** build a data-driven "describe any dialect as config" engine yet.
Write two or three real front-ends first; let the shared parts fall out. Only
extract a declarative dialect descriptor if the variance proves genuinely
tabular — premature generalisation here is the failure mode to avoid.

## Current state

The existing 6502 front-end is a generic early subset — not yet specific to
acme or ca65. It supports the common addressing modes, labels,
`.org`/`.byte`/`.word`, and `<`/`>` operators. Known gaps before it can claim
compatibility with *any* real dialect: arithmetic expressions, that dialect's
directive set and scoping, segments, macros, string escapes.

The Z80 + PasmoNext backend is the first delivered: the engine ↔ dialect ↔ spec
seam is split, the Z80 `isa` spec covers the base page and the ED group, and the
PasmoNext front-end assembles standard-Z80 source. Remaining for full Spectrum
coverage: the CB (bit/rotate) and DD/FD (IX/IY) prefix groups, and validating
output byte-for-byte against the `pasmonext` binary on real curriculum source.

## Drift triggers

- **"Let's invent one clean Asm198x syntax for all CPUs"** — no; that breaks the
  rescue mission. Re-read "Why not a unified house syntax." Revisit only by
  amending this record.
- **"Put dialect-specific directive handling in the shared engine"** — no;
  dialect lives in the per-CPU front-end, encoding and engine stay shared.
- **"Match this niche assembler instead of the primary target"** — record the
  reason here first; don't silently retarget a backend's dialect.
- **"One dialect per CPU is enough"** — no; dialect is an axis independent of
  CPU and the curriculum needs several per CPU (acme *and* ca65 for 6502).
  Re-read "Dialect targets, prioritised by curriculum."
- **"Build a generic data-driven dialect engine so any syntax just works"** —
  not yet. Write real front-ends first; generalise only if the variance proves
  tabular. See "What is shared vs per-dialect."
- **"Prioritise the most popular dialect in the scene"** — priority is set by
  curriculum consumption, not wild popularity. That is why Z80 leads with
  PasmoNext, not sjasmplus.
- **"The Z80 target is vanilla pasmo"** — no; the curriculum uses **PasmoNext**
  (invoked as `pasmonext`), a Spectrum Next superset of pasmo. Validate against
  the `pasmonext` binary. Z80N extended opcodes are a deferred ISA extension.

## Log

### 2026-06-02 — Multi-dialect amendment

Reframed from "one dominant dialect per CPU" to "dialect is an axis independent
of CPU; multiple first-class dialects per CPU, prioritised by curriculum." A
scan of Code198x showed the curriculum already spans dialects — acme (C64) and
ca65 (NES) for 6502, pasmo for the Spectrum, vasm for the Amiga — so serving the
curriculum *requires* several front-ends per spec. Corrected the Z80 primary
from sjasmplus to **pasmo** (the Spectrum curriculum's assembler); sjasmplus
stays first-class for breadth. Set the active first target to **Z80 + pasmo**,
which also forces the engine/dialect/spec seam while the codebase is small.
Held the line against a premature data-driven dialect engine.

### 2026-06-02 — Z80 target is PasmoNext, not vanilla pasmo

Steve noted the course is "busily using pasmonext." The installed assembler is
PasmoNext v0.1.3 — the ZX Spectrum Next fork of pasmo (Julián Albo, modified by
C Kirby) — and the curriculum invokes `pasmonext`. Renamed the Z80 dialect
target from pasmo to **PasmoNext**; it is a syntactic superset, so for standard
Z80 the byte output is identical and one backend serves both. The Z80N extended
opcodes (MUL, LDIRX, NEXTREG, …) are deferred: a corpus scan found only standard
Z80 in use (apparent `TEST`/`MIRROR` hits were comment/filename noise). Renamed
the code dialect to `pasmonext`/`PasmoNext`; `pasmo` stays a CLI alias.
