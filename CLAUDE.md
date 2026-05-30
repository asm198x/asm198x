# Asm198x

A family of modern, single-binary assemblers and disassemblers for the 198x
family's target CPUs. One of three sibling projects under the `198x/` umbrella;
see [`../CLAUDE.md`](../CLAUDE.md) for umbrella context and cross-project rules,
and [`../decisions/sibling-project-coordination.md`](../decisions/sibling-project-coordination.md)
for the sibling relationship (Asm198x is the third sibling, peer to Code198x
and Emu198x — not a child of either).

## What this is

Modern, statically-linked, cross-platform assemblers to replace toolchains that
are getting hard to run: period assemblers need dead-OS emulation; modern
community ones are fragmented (a different tool and dialect per machine). One
Rust workspace, many crates — *not* one repo per CPU. Boring technology;
rescue over replace.

## Binding architecture

Read [`../decisions/asm198x-and-shared-isa-spec.md`](../decisions/asm198x-and-shared-isa-spec.md)
before changing the crate structure or the ISA layer. The load-bearing points:

- **Shared declarative ISA spec.** The [`isa`](crates/isa) crate is the single
  source of truth for instruction *encoding*. Asm198x consumes it to assemble
  and disassemble; Emu198x validates its hand-written decoders against it. The
  spec is **authored** from the primary reference library (datasheets), **not
  extracted** from any emulator's decode loop.
- **`isa` stays dependency-free and standalone**, so Emu198x can depend on it
  without pulling in the assembler. It lives here for now; promotion to a
  neutral location is deferred until Emu198x actually consumes it.
- **Source-compatible per machine.** The dialect lives in each CPU's parser
  front-end; the encoding underneath is the shared spec. Detail and per-CPU
  dialect targets are in [`decisions/`](decisions/).

## Crate layout

Two crates today; split further only when a second CPU makes a seam real (the
engine-vs-dialect boundary, and per-CPU `isa` crates).

- [`crates/isa`](crates/isa) — instruction-set specs (types + `mos6502`).
- [`crates/asm198x`](crates/asm198x) — engine + 6502 dialect (library) and the
  `asm198x` CLI.

## Build-time discipline

The workspace bakes in the levers that keep builds fast — `default-members`
scoped to the CLI, and a `[profile.dev]` that drops full debuginfo (the biggest
`cargo test` cost). Assemblers are featherweight (no `wgpu`/audio/GUI), so this
should stay in the seconds. If a build ever feels slow, the cause is the
dependency graph or profile — never the repo boundary. (Background: this was
measured on Emu198x, whose pain was `cargo test` linking hundreds of
debuginfo-heavy binaries, not its crate count.)

## Where things live

- [`decisions/`](decisions/) — Asm198x-only decisions (syntax stance, dialect
  targets). Cross-project decisions live in [`../decisions/`](../decisions/).
- [`crates/`](crates/) — the Rust workspace.
- [`examples/`](examples/) — sample source.

Hardware facts come from the umbrella primary library at [`../reference/`](../reference/)
and syntheses at [`../syntheses/`](../syntheses/), per
[`../decisions/shared-hardware-reference-canon.md`](../decisions/shared-hardware-reference-canon.md).
The `isa` spec is the machine-readable distillation of the encoding slice of
those facts; it cites the library, not the other way round.
