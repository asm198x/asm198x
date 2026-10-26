# Decision: PasmoNext's passes run in Asm198x's walk, over Lang198x's front end

**Status:** Active. Binding for the `pasmo` and `pasmonext` dialects, whose
default path the walk has been since 2026-10-26. The old PasmoNext layer
still formats source until the formatter moves onto Lang198x's tree, and is
then deleted.

**Date:** 2026-10-23; the walk became the default on 2026-10-26.

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

- **Two passes, or three** (below). Each starts at address 0 with the last
  pass's symbols, its `DEFL` names cleared, and a fresh expander. The last
  pass's statements are the program.
- **A name nobody has defined reads as 0 in pass 1, and is an error after.**
  Where PasmoNext needs a value as soon as it reads the line (`ORG`, `IF`,
  `REPT`, the `DS` count, `RST` and the others z80-asm lists), any definition
  will do, above the line or from the pass before.
- **`DEFINED`** is true only for a name defined above the line in this pass.
- **Symbols:** a label or `EQU` once per pass; `DEFL` again and again, but never
  over a label or `EQU`, nor they over it. A name pass 2 defines that pass 1 did
  not is an error. PasmoNext's own messages are used.
- **Options.** `--nocase` is z80-asm's reading option; the walk takes the
  profile it gives. `--equ` names, read by z80-asm's `predefine`, are defined
  before pass 1, can be neither redefined nor made `DEFL`, and reach the
  engine as `EQU`s before any line. Under `--alocal`, a name starting `_` or
  `@` is local to an automatic scope: reading or defining one opens it, and
  defining any other name ends it, as do `LOCAL`, `PROC`, `ENDP` and a macro
  definition. A line's label is defined before the rest is read, except on
  `ORG`, `EQU` and `DEFL` lines, which read their value first; that order
  decides which scope a name on the line belongs to.
- **Structures and reserved names.** z80-asm reports each as a
  `Definition`; the walk gives the values. Inside `STRUCT name`, each field
  `name.field` is its offset, and at `ENDS`, `name` is the structure's size;
  each `RB` or `RW` name is the reserve counter, which `RSSET` sets. Each is
  defined as `setequorlabel` defines it and reaches the engine as an `EQU`.
  The counter starts at 0 each pass: PasmoNext's carries over, so `RB`
  without `RSSET` ends in a phase error, which is one of its bugs.
- **Messages.** `.ERROR` stops the assembly, in z80-asm. A `.WARNING` is
  a warning and a `MESSAGE` a note, each given on the last pass, so once:
  PasmoNext gives a `.WARNING` on every pass. A `MESSAGE`'s values are read
  there, so a name nobody defines is an error and one defined further on is
  not.
- **The Next's banks.** Each 8K slot holds a bank, PasmoNext's defaults
  from the start of every pass (the ROM, then 10, 11, 4, 5, 0 and 1), which
  `SETBANK` changes; it also sets the `DEFL`s `CURRENTBANK` and
  `CURRENTBANKxxxx`. Each name records the bank of its value's slot when it
  is defined, and `?? name` reads it through `Known::bank`: 0 for a name not
  yet defined in pass 1, an error after. `SAVEBIN` is evaluated on the last
  pass and reaches the engine as `Operation::SaveImage`, which saves memory
  as PasmoNext's stands at the line: what each address last held at or
  before it, or else what it ends holding, or else 0.
- **PasmoNext's obvious bugs are fixed, not copied,** under the umbrella's
  [`reference-bugs-are-fixed-not-copied.md`](../../../decisions/reference-bugs-are-fixed-not-copied.md).
  z80-asm rejects the miscompiles and corrects the Next's `push nn`; the walk's
  gate names each measured program that deviates, such as `org 5 junk`, and
  fails if one stops deviating.
- **A `REPT` counter** is a `DEFL` local to its repeat.
- **Scopes, PasmoNext's `LocalLevel`s.** Every macro call, `REPT` and `IRP` is
  a scope, as is `PROC`…`ENDP`. z80-asm reports `LOCAL`, `PROC`, `ENDP` and
  `PUBLIC` as a scope change on the line and leaves names as written; the walk
  keeps the scopes. From the line after a `LOCAL`, the name means a name of
  its own, made by `genlocalname`: a counter in eight hex digits, starting
  again each pass, so each pass gives each local the same name and no source
  name can clash with one. The walk renames labels and every name in the
  line's statements, and answers `DEFINED` and values through the same
  renaming, so the engine sees the names PasmoNext's symbol file lists.
  - A `REPT` or `IRP` is one scope for all its passes, so a local label in
    one is defined twice. Its counter is a local of that scope too.
  - A `PROC` left open in a macro closes with it; an `ENDP` there cannot
    close one outside. `LOCAL` outside any scope, an unmatched `ENDP`, and a
    `PROC` still open at the end, reported where it was written, are errors.
  - `PUBLIC` changes no meaning. Its names are for the public symbol file,
    which this path does not write yet.
- **Files are opened from the folder PasmoNext runs in,** not from the folder
  of the file that includes them: the pattern library's `mistakes/` programs
  include `caller.inc` from the pattern folder above. The walk asks the loader
  as if for the root, so it resolves from its base.
- **The engine gets the walk's values for `EQU`, `DEFL` and counters.** The
  engine reads a `DEFL` where it stands, so `n defl later` would fail there
  although PasmoNext accepts it. The walk has PasmoNext's value, so it says so.

## Three passes, as PasmoNext runs them

PasmoNext runs a third pass when a label's or `EQU`'s value changes in the
second, saying "Switching to 3 pass mode" on that line, and `--pass3` asks for
three from the start. On the last pass, a value that changes again is "Phase
error in 'name'". The walk follows `setvar` and `processfile`: the last pass's
statements are the program, and a pass 2 that switched is thrown away. A
`MESSAGE` or `SAVEBIN` is read on the last pass only.

Two of PasmoNext's bugs only show here, and the walk fixes both rather than
copying them:

- PasmoNext keeps one memory image across its passes. Bytes an earlier pass
  wrote where the last writes nothing stay in the file: `org x` with `x` still
  0 in pass 1 leaves that pass's byte at 0. The walk writes the last pass's
  bytes only.
- A name that only an earlier pass defined, such as a label inside an `IF`
  the last pass skips, is read with that pass's value. The walk refuses it,
  saying so.

No curriculum program (0 of 391) and no verdict source (0 of 1,230) runs three
passes. The measured programs that do are named in the gate, with the
deviation each shows.

## Drift triggers

- *"Move the pass loop into z80-asm, it knows the lines."* → That made z80-asm
  do layout and Asm198x do it again, and every complication of the first design
  came from the duplicate. Lang198x owns reading the source; this owns layout.
- *"Rename `LOCAL` names in z80-asm, its expander knows the macro calls."* →
  PasmoNext's local names hang on its symbols and its passes, which are this
  walk's. z80-asm reports the scope change; this keeps the scopes.
- *"Let the engine compute `EQU` and `DEFL`, it can."* → Not as PasmoNext does
  for `DEFL`, and the walk already has the answer. Fold what the walk knows.
- *"Converge further than PasmoNext, or run passes until values settle."* →
  Match the reference: two passes, a third only where PasmoNext runs one, and
  its phase error after that.
