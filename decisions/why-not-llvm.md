# Decision: hand-rolled MC-style architecture, not LLVM integration

**Status:** Active. Binding for Asm198x.

**Date:** 2026-06-03.

## The decision

Asm198x assembles and disassembles with a **hand-written, dependency-free
engine** built around the authored [`isa`](../crates/isa) spec — *not* by
integrating LLVM (its MC layer, TableGen-generated backends, or `llvm-mc`).

This is not LLVM-skepticism. We independently arrived at LLVM's **MC-layer
shape**, which is the proven design and a good sign we're on the right track:

| Asm198x | LLVM equivalent |
|---------|-----------------|
| `isa` crate (declarative encoding spec, authored once, drives encode + decode) | TableGen `.td` files → `llvm-tblgen` generates matcher/encoder/decoder |
| dialect front-ends (text → operands) | target `AsmParser` → `MCInst` |
| field-based encoder (base word + bit-field slots) | `MCCodeEmitter` |
| disassembler (match word, most-fixed-bits wins) | `MCDisassembler` (TableGen `DecoderEmitter`) |
| grow-only branch-relaxation fixpoint | `MCAssembler::layout` over `MCRelaxableFragment` |
| same-section → PC-relative, cross-section → relocation | `evaluateFixup` → `MCFixup`/`MCObjectWriter` |
| section/hunk model | `MCSection`/`MCFragment` |
| `asm198x --dialect … --exe` / `--disasm` | `llvm-mc` |

So the question isn't "is LLVM well-built" (it is) — it's "is integrating it
better *for this project's goals*." It is not.

## Why hand-rolled wins here

1. **Byte-identity is the core goal, and LLVM structurally can't deliver it.**
   LLVM produces *its own* correct encoding; it has no notion of matching vasm's
   or acme's exact bytes. Nearly all the hard work (the `add`↔`lea`↔`addq`
   rewrites, `cmp #0`→`tst`, the relaxation fixpoint, `NOP` code-hunk padding,
   `label(pc)` distance, omitting the symbol table) was reproducing *another
   tool's choices*. To get that from LLVM you'd patch its internals to mimic
   vasm's quirks — *harder* than owning a small engine. And the free, exhaustive
   correctness oracle (32/32 Amiga, 80/80 C64, every curriculum unit) only
   exists *because* the bytes must match exactly.

2. **LLVM detonates the single-binary rescue premise.** Asm198x exists because
   period assemblers need dead-OS emulation and modern ones are fragmented — the
   answer is one small, statically-linked, runs-anywhere binary (see the
   umbrella `asm198x-and-shared-isa-spec.md` and `packaging-and-cpu-roadmap.md`).
   LLVM is hundreds of MB, a heavy C++ build, constant version churn. Bundling it
   into a learner-facing tool is the opposite of boring/featherweight, and
   contradicts the single-binary packaging decision.

3. **For the retro CPUs — the actual core — LLVM gives almost nothing.** No Z80
   backend; no acme/ca65/pasmo/vasm *syntax*; no Amiga hunk object writer. (6502
   exists out-of-tree as `llvm-mos`, but that's a C compiler backend, a separate
   fork, and still not byte-identical to acme/ca65.) You'd write the parsers
   anyway, get no byte-identity, and carry the full weight for nearly none of
   the value.

## The honest counterpoint (when LLVM *would* win)

If the goals were different, LLVM/MC is the obvious choice — record this so the
trade-off stays clear, not dogmatic:

- A **broad, modern, full-ISA** assembler with **no byte-identity requirement**
  (e.g. production x86-64 or ARM64). Hand-rolling that long instruction tail is
  a mistake; LLVM's encoders are battle-tested across the whole space.
- Free **linking, object formats, debug info** (lld, ELF/Mach-O/COFF, DWARF).
- **Scale** across dozens of targets from one description.

We chose the same architecture at a hand-holdable scale, for a goal LLVM has no
reason to serve.

## The one idea worth borrowing (not integrating)

If the CPU count grows large, steal **TableGen's spirit**, not LLVM itself:
today we hand-write both the `isa` spec *and* the encode/decode logic; a small
codegen step could *generate* the logic from the spec (still zero-dependency,
still our format). For a handful of curriculum CPUs, hand-writing is clearer and
not worth a generator yet — revisit if 6809/65816/8086/ARM2/TMS9900 all land.

## Drift triggers

- **"Just build on llvm-mc / link LLVM for the new CPU."** No — it can't be
  byte-identical to the reference tool, and it breaks the single-binary premise.
  Re-read this entry and `packaging-and-cpu-roadmap.md`.
- **"Use llvm-mos for the 6502 target."** No — it's a separate C-compiler fork,
  not byte-identical to acme/ca65, and not how we assemble curriculum syntax.
- **"Generate the tables with TableGen."** Borrow the *idea* (codegen from our
  own `isa` spec) only if the CPU count makes boilerplate real — never pull in
  LLVM's toolchain to do it.
- **"A real assembler would have LLVM's optimizer."** Category error: LLVM's
  optimizer is for *compiling*. An assembler's only "optimization" is
  size-optimal encoding + relaxation, which we already match to vasm.
