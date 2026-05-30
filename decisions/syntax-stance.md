# Decision: Source-compatible syntax per machine

**Status:** Active. Binding for Asm198x.

**Date:** 2026-05-30.

## The decision

Each CPU's assembler is **source-compatible with that machine's dominant
existing dialect**, not a single unified Asm198x syntax. The dialect lives in
the per-CPU parser front-end; the instruction *encoding* underneath comes from
the shared [`isa`](../crates/isa) spec.

The goal: real-world source for a machine assembles **unchanged**. Someone with
a working ca65 6502 project or a sjasmplus Z80 project should be able to point
Asm198x at it and get the same bytes out, without porting their syntax.

## Per-machine dialect targets

These name the dialect each backend aims to match. The first listed is the
primary target; others are compatibility aspirations.

| CPU | Primary dialect target | Also consider |
|-----|------------------------|----------------|
| 6502 | ca65 (cc65 suite) | ACME, 64tass |
| Z80 | sjasmplus | pasmo, z80asm |
| 68000 | vasm (mot syntax) | Devpac/HiSoft |

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
- **Per-dialect (in each CPU front-end):** operator syntax, directive names and
  semantics, number/string literal forms, label and scope rules, macro syntax.

## Current state

The 6502 front-end is an early subset, not yet ca65-compatible. It supports the
common addressing modes, labels, `.org`/`.byte`/`.word`, and `<`/`>` operators.
Known gaps before it can claim ca65 compatibility: arithmetic expressions,
ca65's directive set and scoping, segments, macros, string escapes.

## Drift triggers

- **"Let's invent one clean Asm198x syntax for all CPUs"** — no; that breaks the
  rescue mission. Re-read "Why not a unified house syntax." Revisit only by
  amending this record.
- **"Put dialect-specific directive handling in the shared engine"** — no;
  dialect lives in the per-CPU front-end, encoding and engine stay shared.
- **"Match this niche assembler instead of the primary target"** — record the
  reason here first; don't silently retarget a backend's dialect.
