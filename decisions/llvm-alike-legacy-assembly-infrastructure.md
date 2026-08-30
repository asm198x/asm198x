# Decision: Asm198x grows as shared compiler infrastructure for legacy assembly

**Status:** Active. Binding architectural direction for Asm198x.

**Date:** 2026-08-30.

## The decision

Asm198x is not a collection of unrelated assemblers behind one executable. It
grows as **LLVM-alike infrastructure for legacy assembly**: several source
languages and future language front-ends share explicit semantic, machine,
layout, and artifact layers rather than each bringing an end-to-end toolchain.

“LLVM-alike” describes the shape and ambition of the infrastructure. It does
not mean using LLVM, matching LLVM APIs, adopting LLVM IR, or preparing an LLVM
contribution. Those may become useful later, but none is part of this decision.

The immediate work remains reference-compatible assembly. A legacy dialect
must continue to accept genuine source unchanged and reproduce its named
reference tool. The eventual Asm198x house syntax is a separate frontend for
new source, not a replacement interpretation silently applied to old source.

## The intended layers

The architecture distinguishes four concerns even where today's code combines
some of them:

1. **Source representation** — dialect spelling, trivia, macro and conditional
   structure, includes, and formatter fidelity.
2. **Assembly semantics** — symbols, expressions, sections, relocations,
   address spaces, and operations whose meaning no longer depends on spelling.
3. **Machine representation** — concrete instructions and operands, selection
   constraints, instruction sizes, relaxation, and CPU-family semantics.
4. **Artifact representation** — placement, banks, segments, hunks, headers,
   checksums, object records, ROMs, and executables.

These are conceptual boundaries, not a commitment to introduce four public IR
types now. A new shared representation is extracted only after two real
consumers show the common shape. Source-preserving AST nodes and
family-owned native payloads remain legitimate: a useful common substrate does
not require pretending that every target has identical operands or passes.

The existing pieces already point in this direction:

- dialect frontends and the source-preserving AST own source language;
- Isa198x owns executable instruction truth independently of dialect;
- the shared engine owns expressions, symbols, pieces, and fixed-slot layout;
- family-owned payloads carry genuinely multi-pass CISC semantics;
- Asm198x link/layout paths and graduated Format198x libraries own historical
  artifact structures;
- Debug198x carries source and machine identity across tool boundaries.

Future work improves the boundaries between those pieces instead of replacing
them with a speculative universal IR.

## The plug-in test

For a new CPU, dialect, linker, artifact format, disassembler, or future
language frontend, ask:

> Does this addition plug into an explicit reusable layer, or does it create
> another end-to-end assembler?

An end-to-end path can still be correct when the target proves that no existing
seam fits. That exception must identify the missing reusable concept and keep
the target-specific part family-owned. It is evidence for a later extraction,
not permission to generalise from one example.

This test supplements rather than replaces reference arbitration. Reuse proves
architecture; byte identity against a real tool proves behaviour.

## ARM2 is the first deliberate proving ground

ARM2 will be the first new CPU undertaken after this direction was recorded and
the first 32-bit target, so its staged build must test the architecture
deliberately:

- the 32-bit address widening belongs to shared address and expression
  machinery, not an ARM-only parallel engine;
- condition fields, the barrel shifter, register lists, rotated immediates, and
  PC+8 branches use the existing encoded-piece seam where it tells the truth;
- ARM syntax remains a frontend concern, separate from ARMv2 instruction
  semantics;
- little-endian flat output is one artifact consumer, not part of instruction
  identity;
- any ARM-specific escape hatch stays named and bounded rather than being
  promoted immediately into a universal operand model.

The ARM2 work is successful architecturally when it widens the substrate for
later 32-bit targets while adding only the semantics genuinely particular to
ARM. MIPS, SH, PowerPC, V810, and SPARC must be able to reuse the widening
without inheriting ARM's operand model.

## The house-language consequence

Once enough compatibility frontends have exposed what genuinely generalises,
Asm198x may add an explicit house syntax for new programs. It should lower
through the same assembly and machine seams as legacy dialects, gaining common
linkers, diagnostics, debug information, and artifact writers. Legacy modes
remain faithful even where their behaviour is awkward; the house language may
choose coherent modern rules because its dialect is explicit.

Migration between the two is an intentional conversion operation. It is never
an implicit reinterpretation of legacy source.

## What this does not decide

- the concrete design or name of a universal assembly or machine IR;
- that every dialect must lower through every conceptual layer;
- an LLVM dependency, LLVM compatibility layer, or LLVM upstream plan;
- the syntax of the future house language;
- a higher-level compiler frontend;
- a new crate boundary before an independently useful consumer exists.

Those require evidence and their own decisions. This record keeps their design
space open while preventing today's target work from hard-coding another
isolated pipeline.

## Drift triggers

- **“This CPU is easiest as a new assembler beside the others.”** First prove
  why the shared expression, piece, symbol, section, and artifact seams cannot
  carry it.
- **“LLVM-alike means use LLVM's IR.”** No. The analogy is shared layered
  infrastructure; legacy assembly needs byte, relocation, bank, and source
  fidelity that LLVM IR deliberately abstracts away.
- **“One universal operand type will make the architecture clean.”** Not from
  one target. Preserve the family-owned-payload decision and extract only from
  demonstrated commonality.
- **“The house syntax can tidy legacy source while parsing it.”** No. Fidelity
  dialects and the house frontend have different contracts and remain explicit.
- **“Artifact details can stay in the CLI.”** Banks, hunks, relocations,
  headers, and checksums are reusable toolchain semantics, not presentation.
- **“ARM needs a private 32-bit engine.”** Shared widening is the point of ARM
  opening the 32-bit wave; ARM-specific instruction semantics sit above it.

## Related decisions

- [`syntax-stance.md`](syntax-stance.md)
- [`reference-parity-goal.md`](reference-parity-goal.md)
- [`roadmap-sequencing.md`](roadmap-sequencing.md)
- [`ast-native-payload-for-multipass-cisc.md`](ast-native-payload-for-multipass-cisc.md)
- [`arm-staged-build.md`](arm-staged-build.md)
- [`../../../decisions/asm198x-cpu-coverage-roadmap.md`](../../../decisions/asm198x-cpu-coverage-roadmap.md)
