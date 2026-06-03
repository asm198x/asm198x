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
| 68000 | Amiga (ST, Genesis) | vasm (mot) | new field-based core | ✅ done |
| 6809 | Dragon, CoCo | **lwasm** | engine seam reused; computed postbyte (indexed) | ✅ done |
| 65816 | SNES, Apple IIgs | **ca65** | **target extension of `mos6502` (like Z80N on Z80)** | ✅ done |
| later | 8080/8085, 8086, ARM2 (Archimedes), TMS9900 (TI-99) | TBD | mixed | open |

**65816:** the full native-mode instruction set + a spec-driven disassembler,
all validated byte-identical against `ca65 --cpu 65816` (linked flat; no SNES
curriculum yet, so representative programs). It is a **target extension** —
`isa::mos6502` (primary) + `isa::mos65816` (extension), the `z80::NEXT`
mechanism. Covered: the `m`/`x` immediate width (`.a8`/`.a16`/`.i8`/`.i16`), all
the new addressing modes (long, long,x, `[dp]`, `[dp],y`, `n,s`, `(n,s),y`,
`(dp)`), the `z:`/`a:`/`f:` size forces with value-based sizing and fall-up,
long calls/jumps, `brl`/`per`/`pea`/`pei`, `stz`/`trb`/`tsb`/`inc a`/`bra`, the
register/stack/control ops, `mvn`/`mvp`, `cop`/`wdm`, and the `^` bank-byte
operator. The engine gained 24-bit operands, a 2-byte PC-relative, and an `i64`
symbol table (for 24-bit addresses). The **disassembler** tracks `m`/`x` width
through `rep`/`sep` and emits the matching width directives, so width-switching
code round-trips assemble→disassemble→reassemble exactly (its one inherent limit
is width set out of band, with no preceding `rep`/`sep`). Deferred: `.smart`
rep/sep width tracking and `@cheap` locals (source-compat conveniences).

**6809:** all addressing modes (inherent, immediate, direct, extended,
short/long relative, and the full indexed set — 5/8/16-bit offsets, auto
inc/dec, accumulator offsets, indirect, extended-indirect, PC-relative), the
register ops (`tfr`/`exg`/`pshs`/`puls`/`pshu`/`pulu`), `org`/`equ`/`fcb`/`fdb`/
`fcc`/`rmb`, and a spec-driven disassembler with assemble→disassemble→reassemble
round-trip. Validated byte-identical against `lwasm --6809 --raw` (curriculum
harness, as representative programs — there is no 6809 curriculum). Deferred
lwasm-isms: the `'c` char literal (needs a shared-tokenizer change) and PCR
8-bit auto-selection for *constant* targets (the size depends on the PC, unknown
at parse time — use `<` to force; labels default to 16-bit, matching lwasm).

Agreed order: **finish 68000 → 6809 → 65816 → reassess.** 65816 is the cheapest
big win (6502 family + ca65 reuse) but is sequenced after 6809 per the owner's
priority.

## Encoding models and the computed-operand seam

Adding the 6809 forced a question that will recur for every CPU after it: *how
does an instruction's operand turn into bytes?* The answer is not one model but a
small taxonomy, and the engine now has one seam that spans it.

**The taxonomy** (what the roadmap CPUs need):

- **Fixed-width slots** — 6502, Z80. The opcode is one or more fixed bytes; each
  operand is a slot of known width (`isa::Form { opcode, operands, … }`). The
  dialect resolves the addressing mode at parse time and hands the engine an
  `Operation::Instruction { mnemonic, mode, operands }`; the engine emits from
  the form. Stable instruction *size* without knowing forward symbol values.
- **Field-packed opcode word** — 68000. Operand fields are packed *into* the
  opcode word(s) (`isa::m68k`). vasm computes this itself (it also owns layout,
  relaxation, sections, relocations, the hunk serializer), bypassing the flat
  engine — the documented "two engines" seam in `syntax-stance.md`.
- **Computed variable-length operand** — 6809 (indexed postbyte), and ahead
  8086 (modrm + prefixes). The opcode is fixed, but the operand bytes are
  *computed* by the dialect (a postbyte, a modrm), not read from a fixed slot.
- **State-selected fixed width** — 65816. The accumulator/index immediate is 8-
  or 16-bit per the `m`/`x` flags, but each width is a *distinct fixed-slot
  form* (`"immediate"` / `"immediate16"`, same opcode). The dialect carries the
  width as parse-time state (from `.a8`/`.a16`/`.i8`/`.i16`) and picks the form;
  the engine stays form-based. This is **not** the computed-operand seam — the
  earlier roadmap mis-filed it there. (The seam was the right tool only for
  genuinely *computed* bytes like the 6809 postbyte.) The two engine additions
  65816 did need — 24-bit operands and a 2-byte PC-relative — are extensions of
  the *fixed-slot* path, not the seam.

**The seam (decided, built):** the engine gained
`Operation::Encoded(Vec<Piece>)`, where a `Piece` is either `Lit(u8)` (a byte the
dialect already computed — opcode, postbyte, later a modrm) or
`Val { expr, bytes, rel, signed }` (a value resolved in pass two at a given
width, optionally a PC-relative branch offset or a signed displacement). A
dialect whose operands are computed builds the pieces itself and still reuses the
engine's two-pass driver, symbol table, `org`, and `equ`. This is deliberately
**general**, not 6809-specific: 8086 will emit `Encoded` pieces the same way.
lwasm is its first consumer and proves it byte-identical. (65816, by contrast,
needed only fixed-slot forms — see the taxonomy above.)

This kept lwasm a *front-end* (parse → pieces) rather than a second bypass engine
like vasm. The boundary that matters: a CPU that also needs its own *layout/
relaxation/relocation* logic (as 68000 did) bypasses the engine; a CPU that only
needs *computed operands* uses the `Encoded` seam and keeps the shared driver.

**True engine unification is deferred, with a trigger.** There are effectively
two assembly drivers today — the flat `engine::assemble` (6502, Z80, 6809) and
vasm's `assemble_core` (68000). Unifying them into one driver is *not* worth it
yet (one data point). **Reassess when a second CPU needs the
layout/relaxation/relocation machinery** (a second `assemble_core`-shaped
backend) — at that point the shared structure is real and provable, not
speculative. Until then, the `Encoded` seam absorbs the cheaper "computed
operand" cases without touching the vasm path.

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
- **"Give 6809 (or 65816, 8086) its own assemble engine like vasm"** — no, unless
  it needs its own layout/relaxation/relocation. Computed operands alone use the
  shared `Operation::Encoded` seam and keep the two-pass driver.
- **"Unify the flat engine and vasm's `assemble_core` now"** — not yet; one
  data point. Reassess only when a *second* CPU needs the layout/relocation
  machinery, so the shared shape is real rather than speculative.
