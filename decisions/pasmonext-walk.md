# Decision: PasmoNext's passes run in Asm198x's walk, over Lang198x's front end

**Status:** Active. Binding for the `pasmo` and `pasmonext` dialects' second
path, and for the path that replaces the shared Z80 core once the gate holds.

**Date:** 2026-10-23.

## The decision

**Lang198x's `lang198x-z80-asm` reads PasmoNext 0.1.3 source and lowers it one
line at a time. Asm198x's walk (`dialects/pasmo_walk.rs`) runs PasmoNext's
passes over it, keeps the symbols and the address, and hands the last pass's
statements to the shared engine.** z80-asm does no layout, runs no passes and
computes no bytes. That keeps the umbrella's
[assembly source layer decision](../../../decisions/assembly-source-layer-moves-to-lang198x.md):
layout, encoding and artifacts stay here.

The walk is the same kind of thing as the SjASMPlus walk in
[`forward-conditions-and-passes.md`](forward-conditions-and-passes.md): a
dialect's own passes, sized with the shared `engine::next_pc`, so lengths have
one source.

## What PasmoNext does, which the walk follows

Read from its `asm.cpp` (`processfile`, `dopass`, `setequorlabel`, `setdefl`,
`setvar`, `getvalue`, `isdefined`) and measured. The measured programs are in
`crates/asm198x/tests/fixtures/pasmonext-0.1.3-programs.txt`, made by
`scripts/measure-pasmonext-programs.py`.

- **Two passes.** Each starts at address 0 with the last pass's symbols, its
  `DEFL` names cleared, and a fresh expander. The second pass's statements are
  the program.
- **A name nobody has defined reads as 0 in pass 1, and is an error after.**
  Where PasmoNext needs a value as soon as it reads the line (`ORG`, `IF`,
  `REPT`, the `DS` count, `RST` and the others z80-asm lists), any definition
  will do, above the line or from the pass before.
- **`DEFINED`** is true only for a name defined above the line in this pass.
- **Symbols:** a label or `EQU` once per pass; `DEFL` again and again, but never
  over a label or `EQU`, nor they over it. A name pass 2 defines that pass 1 did
  not is an error. PasmoNext's own messages are used.
- **A `REPT` counter** is a `DEFL` local to its repeat.
- **The engine gets the walk's values for `EQU`, `DEFL` and counters.** The
  engine reads a `DEFL` where it stands, so `n defl later` would fail there
  although PasmoNext accepts it. The walk has PasmoNext's value, so it says so.

## Three-pass mode is reported, not guessed

PasmoNext adds a third pass when a symbol's value changes in the second
("Switching to 3 pass mode"), and stops with "Phase error" if one changes
again. Only then can an earlier pass's bytes survive in the output, or the last
pass read a value no pass set. No curriculum program (0 of 391) and no verdict
source (0 of 1,230) does this. Until it is built, the walk stops such a program
with an error that names three-pass mode, and the measured programs check that
it does, for every program PasmoNext ran in three passes.

## Drift triggers

- *"Move the pass loop into z80-asm, it knows the lines."* → That made z80-asm
  do layout and Asm198x do it again, and every complication of the first design
  came from the duplicate. Lang198x owns reading the source; this owns layout.
- *"Let the engine compute `EQU` and `DEFL`, it can."* → Not as PasmoNext does
  for `DEFL`, and the walk already has the answer. Fold what the walk knows.
- *"Converge further than PasmoNext, or fill in three-pass mode by
  reasoning."* → Match the reference: measure its third pass first, then build
  it, as its own item.
