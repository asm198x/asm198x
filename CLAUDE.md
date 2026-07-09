# Asm198x

Asm198x is the 198x family’s assembler/disassembler workspace: modern, single-binary, cross-platform tooling for the project’s target CPUs. It is a sibling of Code198x and Emu198x, not a child of either.

For umbrella context and cross-project rules, read [`../../CLAUDE.md`](../../CLAUDE.md). For the sibling/project boundary, read [`../../decisions/sibling-project-coordination.md`](../../decisions/sibling-project-coordination.md). For the Asm198x/ISA architecture, read [`../../decisions/asm198x-and-shared-isa-spec.md`](../../decisions/asm198x-and-shared-isa-spec.md).

## Current role

Asm198x owns:

- assembler and disassembler front-ends for retro CPU dialects;
- the shared declarative ISA specification used for instruction encoding;
- the `debug198x` debug-info format emitted by the assembler and consumed by Emu198x;
- the command-line contract for diagnostics, JSON output, formatting, listings, symbols, and debug sidecars.

Hardware facts come from the umbrella primary library at [`../../reference/`](../../reference/) and syntheses at [`../../syntheses/`](../../syntheses/). The ISA spec is the executable distillation of instruction encoding facts and should cite those sources, not emulator code.

## Crate layout

| Crate | Role |
|---|---|
| [`crates/isa`](crates/isa) | Dependency-free declarative instruction-set specs. This is the single source of truth for instruction encoding. |
| [`crates/isa-disasm`](crates/isa-disasm) | Spec-driven disassemblers that depend only on `isa` + std, so Emu198x can consume disassembly without the assembler. |
| [`crates/debug198x`](crates/debug198x) | Debug198x sidecar format: line/address maps, typed symbols, and sections. Governed by [`decisions/debug198x-format.md`](decisions/debug198x-format.md). |
| [`crates/asm198x`](crates/asm198x) | Assembler library, dialect front-ends, shared engine, formatter, diagnostics contract, and `asm198x` CLI. |

Split crates further only when the per-CPU ISA boundary or Emu198x consumption makes the split real.

## Supported CPU and dialect surface

The current implementation covers 8-bit and 16-bit CPU families used across the 198x target machines. The exact source-compatible dialects and aliases live in `crates/asm198x` and the CLI help/tests; keep this file as a routing guide, not a coverage ledger.

Implemented ISA families include:

- MOS 6502 family: 6502, 65816, HuC6280;
- Zilog / Intel lineage: Z80, Z80N, SM83, 8080, Z8000;
- Motorola family: 6800, 6809, 68000;
- microcontroller / early-console CPUs: CDP1802, MCS-48/8048 family, SC/MP, Fairchild F8, Signetics 2650, TMS7000;
- 16-bit systems: PDP-11, TMS9900, CP1610.

When adding or changing a CPU, prefer the existing encoding models before introducing new machinery:

- fixed-slot form tables;
- target extensions over a base ISA;
- computed operands via the existing operand seam;
- field-packed bespoke tables for word-oriented or CISC-like encodings.

The engine ↔ dialect ↔ spec seam is documented in `crates/asm198x/src/lib.rs`. The encoding-model taxonomy and CPU roadmap live in [`../../decisions/packaging-and-cpu-roadmap.md`](../../decisions/packaging-and-cpu-roadmap.md) and [`../../decisions/asm198x-cpu-coverage-roadmap.md`](../../decisions/asm198x-cpu-coverage-roadmap.md).

## Layers above the encoder

The assembler has three shared layers above per-CPU encoding. Check [`decisions/roadmap-sequencing.md`](decisions/roadmap-sequencing.md) before changing their responsibilities.

- **Semantic AST** (`crates/asm198x/src/ast.rs`) — source-preserving tree between parsing and byte-lowering.
- **Formatter** (`--fmt`) — canonical layout via AST emit, with byte-identical/idempotent round trips expected.
- **Core contract** (`crates/asm198x/src/contract.rs`, `crates/asm198x/src/span.rs`) — machine-readable results plus rustc-style diagnostics and JSON output.

Keep new dialects on these shared layers unless a documented native payload path is required for a multi-pass/CISC assembler path.

## Correctness model

Correctness is differential and reference-driven. Tests should either compare against the relevant real assembler/disassembler or prove internal round-trip invariants.

Primary layers:

- curriculum fixtures: byte-identical output against reference tools and assemble → disassemble → reassemble round trips;
- conformance tests: opcode/form sweeps, opcode-space sweeps for non-form specs, targeted position-dependent round trips, and seeded differential fuzzing;
- formatter tests: source formatting must remain idempotent and byte-identical after reassembly;
- contract/debug fixtures: JSON, symbol, listing, and Debug198x outputs must remain stable under their governed contracts.

Reference-tool tests are ignored by default when the external tools are absent. See [`decisions/spec-conformance-and-fuzzing.md`](decisions/spec-conformance-and-fuzzing.md).

## Build-time discipline

Keep assemblers featherweight: no GUI, audio, emulator, or graphics dependencies. The workspace should remain fast to build and test through scoped `default-members`, a lean dependency graph, and appropriate dev-profile settings. If build time regresses, investigate dependencies and profiles before changing repo boundaries.

## Where things live

- [`crates/`](crates/) — Rust workspace.
- [`decisions/`](decisions/) — Asm198x-only decisions.
- [`docs/`](docs/) — plans and implementation notes.
- [`examples/`](examples/) — sample source.
- [`../docs/`](../docs/) — org-level public docs, including external format docs such as Debug198x.
- [`../../decisions/`](../../decisions/) — cross-project decisions.
- [`../../reference/`](../../reference/) and [`../../syntheses/`](../../syntheses/) — hardware source-of-truth layers.

## Working rules for agents

- Read the relevant decision record before changing crate boundaries, ISA modelling, syntax compatibility, diagnostics, or external formats.
- Do not copy facts from emulator implementations into `isa`; use primary references and cite upward.
- Prefer existing dialect/encoding patterns over adding special cases.
- Keep this front door current-state focused. Detailed CPU bring-up notes, dated milestones, and implementation history belong in decisions, plans, or commit history, not here.
