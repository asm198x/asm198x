# Decision: CLI packaging, binary strategy, and CPU roadmap

**Status:** Active. Binding for Asm198x.

**Date:** 2026-06-03.

## The decision

Three related calls about *how the toolchain is shaped and where it grows*,
distinct from the per-dialect syntax stance in
[`syntax-stance.md`](syntax-stance.md).

### 1. One library, one binary — held, with a defined reassessment point

The whole toolchain stays a **single library crate** (shared `isa`, engine,
per-CPU cores, dialect front-ends, disassembler, link logic) behind a **single
`asm198x` binary**. Assembler and disassembler are spec-coupled — the
disassembler decodes against the *same* `isa` data the assembler emits from, the
basis of the round-trip guarantee — so splitting them buys nothing and risks
drift.

This is held, not permanent. **Reassess at the Stage-3 checkpoint** (full Amiga
hunk-exe, see `syntax-stance.md` 68000 staging) with real numbers: binary size,
compile time, and whether any *planned* CPU pulls heavy dependencies. So far
every CPU is featherweight (no GUI/graphics/audio deps), so size is not expected
to be the deciding factor.

If a split is ever wanted, the **middle path is Cargo feature flags**, not a
fork: one codebase producing either a fat binary or per-CPU binaries
(`--features m68k`, …) from the same source, never duplicating the shared core.

**Splitting by *architecture* (a binary per CPU) is off the table** unless those
metrics later say otherwise.

### 2. Function split = subcommands, not separate binaries

The natural way to expose assembler / disassembler / linker as distinct
operations is **subcommands** (`asm198x asm …`, `asm198x disasm …`, and
`asm198x link …` if it becomes meaningful), `git`/`cargo` style — *not* separate
`as198x`/`disasm198x`/`link198x` binaries (which can't truly decouple, sharing
the spec) and *not* the current `--disasm` flag once the surface grows.

**Deferred:** keep the `--disasm` flag through the 68000 stages; move to
subcommands at the Stage-3 reassessment, so the CLI isn't churned mid-build.

### 3. Linking stays fused until separate compilation is real

Assemble-and-link is **fused** (one source → final `.nes`/hunk image), matching
the reference tools and the curriculum: `vasm -Fhunkexe` does both in one tool,
and `ca65`+`ld65` is invoked per single source, never multi-object. A standalone
linker (`ld`-style, consuming `.o` files) plus an object format earns its place
**only when something needs separate compilation** — no current consumer does,
so it's deferred (YAGNI). A `link` subcommand may later wrap the fused step, but
it reads one source, not objects, until that changes.

## CPU / assembler roadmap

Ordered by **curriculum-priority + reuse leverage** (the same principle that set
the dialect order). Per-CPU dialect choice is settled by *what Code198x and
Emu198x actually consume*, scanned when that CPU comes up — not by wild
popularity.

| CPU | Machines | Likely dialect(s) | Reuse | Status |
|-----|----------|-------------------|-------|--------|
| 6502 | C64, NES | acme, ca65 | — | ✅ done |
| Z80 | Spectrum | pasmo/pasmonext, sjasmplus | — | ✅ done |
| 68000 | Amiga (ST, Genesis) | vasm (mot) | new field-based core | 🚧 in progress |
| 6809 | Dragon, CoCo | **lwasm** (validate against the vendored `xroar`); asm6809 | byte-opcode engine carries over; new postbyte addressing | next |
| 65816 | SNES, Apple IIgs | ca65 (already speaks it), 64tass | **extends the `mos6502` core; ca65 front-end exists** | after 6809 |
| later | 8080/8085, 8086, ARM2 (Archimedes), TMS9900 (TI-99) | TBD | mixed | open |

Agreed order: **finish 68000 → 6809 → 65816 → reassess.** 65816 is the cheapest
big win (6502 family + ca65 reuse) but is sequenced after 6809 per the owner's
priority.

## Drift triggers

- **"Split into a binary per CPU"** — no; size isn't the constraint and the
  shared core would fork. Use feature flags if a split is ever needed, and only
  after the Stage-3 metrics review.
- **"Make the disassembler its own crate/binary"** — no; it is spec-coupled to
  the assembler. Keep it in the shared library; expose it as a subcommand.
- **"Build a standalone linker / object-file format now"** — not until a real
  separate-compilation need exists. Linking stays fused, matching the reference
  tools.
- **"Pick the most popular assembler for the new CPU"** — no; dialect choice
  follows Code198x/Emu198x consumption, scanned per CPU (re-read the roadmap
  note).
