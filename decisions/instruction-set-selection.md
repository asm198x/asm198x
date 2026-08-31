# Decision: Instruction-set selection is lexical

**Status:** Active. Binding for Asm198x.

**Date:** 2026-08-31.

## The decision

A dialect supplies the default instruction-set target for an assembly, but it
does not necessarily fix that target for the whole file. When source syntax
selects a CPU, the frontend resolves that selection while walking the live
source and stamps the resulting primary and extension sets on each statement.
Both engine passes use that statement-local pair for form sizing and emission.

This is lexical state, like ACME's character conversion or XOR mask. A switch
in an untaken conditional has no effect; a switch in a live include or macro
persists according to that dialect's source-order rules. Switching back must
remove extensions as well as restore the primary set.

The absence of a statement-local target means “inherit the dialect default”.
It does not mean “default primary set plus whichever extension happens to be
present”: a stamped target is an exact pair, including an explicitly absent
extension.

## Why

ACME's `!cpu` is the forcing case. `!cpu 65816` enables instructions such as
`rtl` and `xba`, while a later `!cpu 6502` refuses them again. Treating the
directive as an assembly-wide setting loses source order; treating processor
names as synonyms silently accepts and rejects the wrong programs.

Keeping selection on statements also preserves the LLVM-alike separation we
want: dialects parse source, ISA specs describe encodings, and the shared
engine consumes an already-resolved target. Adding ARM or another multi-target
dialect does not require a second assembler engine.

## Current coverage

ACME can switch between the documented 6502 set and the shared 65816 extension.
The other ACME processor names remain explicit capability gaps until their
executable ISA sets exist; they are not aliases for 6502. In particular, 6510
requires the undocumented opcode set rather than a renamed documented 6502.

## Drift triggers

- A frontend reads the dialect-wide set after it has accepted a source-level
  CPU switch.
- A pass sizes with one target and emits with another.
- An unsupported processor name is accepted as a “close enough” alias.
- A target switch is moved to command-global state despite appearing inside
  conditional, included, or expanded source.
