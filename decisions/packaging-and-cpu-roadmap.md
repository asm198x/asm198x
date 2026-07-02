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

Beyond YAGNI, the fused stance is **entailed** by the I/O model: an object format
is a serialised non-native intermediate, which
[`assemble-io-model.md`](assemble-io-model.md)'s native-only principle forbids. So
reopening separate compilation is not a casual add — it means adopting an
*existing, validatable* object format (ca65 `.o`, ELF, …), its own high-bar
decision. This section remains the owner; that record explains why it holds.

### Output containers per platform

The fused step emits the platform's final image. Coverage today (the CLI's output
dispatch in `crates/asm198x/src/main.rs`):

| Platform | Container | Status |
|----------|-----------|--------|
| Amiga | hunk executable (`--exe`, the `-Fhunkexe` target) | ✅ emitted |
| NES | `.nes` ROM (bounded ld65 config) | ✅ emitted |
| C64 | `.prg` (CBM, load-address-prefixed) | ✅ emitted (`--prg`, #35) |
| Spectrum | `.sna` snapshot (48K) | ✅ emitted (`--sna`, #31) |

Both launch-platform output containers now exist, so the C64/Spectrum Docker
build images can retire on the assembler side (their remaining hold is
assembler-coverage gaps, not the container). Driver and the keep-vs-retire
decision: umbrella
[`code198x-dev-tooling-migration.md`](../../../decisions/code198x-dev-tooling-migration.md).
What each does:

- **C64 `.prg`** — done (#35). `--prg` prepends the 2-byte little-endian load
  address (the origin) to the flat image — the `acme -f cbm` convention.
  Byte-identical to `acme -f cbm` across the buildable C64 sample corpus.
- **Spectrum `.sna`** — done (#31). `--sna` serializes a 48K snapshot: a 27-byte
  register block (pasmo's defaults — IFF2 set, IM 1, white border, SP `$FFFC`
  with the `end`-directive entry point pushed) plus a 48K RAM image (code at
  `org`, attribute map defaulted to `$38`). Byte-identical to `pasmo --sna` across
  the buildable Spectrum sample corpus (92/92). The engine gained an
  `Operation::Entry` + `Assembly::start` to carry the entry point. 128K remains a
  later add if a unit needs paging.

Either gap can instead be closed on the **Emu198x side** (load a flat image at
`org`; a tape/loader for Spectrum) — the umbrella decision owns that sub-choice,
not this crate. Neither is started; logged here so they aren't rediscovered.

## CPU / assembler roadmap

Ordered by **curriculum-priority + reuse leverage** (the same principle that set
the dialect order). Per-CPU dialect choice is settled by *what Code198x and
Emu198x actually consume*, scanned when that CPU comes up — not by wild
popularity.

| CPU | Machines | Likely dialect(s) | Reuse | Status |
|-----|----------|-------------------|-------|--------|
| 6502 | C64, NES | acme, ca65 | — | ✅ done |
| Z80 | Spectrum | pasmo/pasmonext, sjasmplus | — | ✅ done |
| 68000 | Amiga (ST, Genesis) | vasm (mot) | new field-based core | ✅ done — full base ISA, see [68000-isa-completeness](68000-isa-completeness.md) |
| 6809 | Dragon, CoCo | **lwasm** | engine seam reused; computed postbyte (indexed) | ✅ done |
| 65816 | SNES, Apple IIgs | **ca65** | **target extension of `mos6502` (like Z80N on Z80)** | ✅ done |
| HuC6280 | PC Engine / TurboGrafx-16 | **ca65** (`--cpu huc6280`) | **65C02-superset extension of `mos6502` (65816 pattern); every form is fixed-slot — even the block transfers (opcode + three 16-bit words), so no computed-operand seam needed** | ✅ done — see [huc6280-addition](huc6280-addition.md) (#9) |
| 68020+ | Amiga A1200/CD32 (68EC020), A3000 (030), A4000 (040), accelerators (060) | vasm (mot) | extends the base-68000 core | ⏸ deferred — anticipated (A1200 in scope), holding off until an A1200-class need is real |
| later | 8080/8085, 8086, ARM2 (Archimedes), TMS9900 (TI-99) | TBD | mixed | open |

### The 68k upward path (deferred)

The A1200 (and CD32) are in family scope, so a 68020-class target is anticipated
— but we are holding off until A1200-class dev/emulation actually needs it. When
it comes, this is the shape of the work.

**68020 is the rewrite; 030/040/060 are incremental.** The one large jump is
68000 → 68020: new addressing modes (memory indirect, scaled index, the full
multi-word extension format) plus new instructions (bit-field ops, 32×32
`MULS.L`/`DIVxL.L`, `CAS`/`CAS2`, `PACK`/`UNPK`, `TRAPcc`, `EXTB.L`, `Bcc.L`).
The addressing-mode change is an **EA-decoder rewrite** — larger than every
base-ISA gap combined, and the part to scope before committing; the new
instructions are table/slot work like the base-ISA burndown. After 68020 the
**integer ISA is largely stable** through 030/040/060 — each step mostly adds
*system* instructions and removes/traps a few, not new general-purpose codegen:

- **68010** — small, instruction-only (`MOVE CCR,<ea>`, `MOVEC`, `MOVES`, `RTD`,
  `BKPT`); cheap if a 68010 target ever appears.
- **68030** — adds PMMU instructions (`PMOVE`/`PTEST`/`PLOAD`/`PFLUSH`); drops
  68020's `CALLM`/`RTM`.
- **68040** — adds `MOVE16` and cache ops (`CINV`/`CPUSH`); integrates the FPU.
- **68060** — superscalar; in *hardware* it drops more to software emulation
  (64-bit `MULx.L`/`DIVx.L`, `MOVEP`, `CAS2`, `CHK2`/`CMP2`); adds `LPSTOP`,
  `PLPA`.

**The FPU is its own chunk.** The 68881/68882 coprocessor set — integrated on
040/060 — is ~50 `Fxxx` instructions with the FP0–FP7 register file, FPCR/FPSR/
FPIAR, and a distinct extension-word format (rounding/precision encodings). It is
a self-contained coprocessor ISA, shared across the 040/060 and the external
coprocessor; size it as a separate piece, not folded into a CPU step.

**EC/LC variants are a required axis, not a detail.** Each chip ships in cut-down
forms, and the A1200/CD32 stock CPU is itself one of them:

| Variant | FPU | MMU | Other | Amiga use |
|---------|-----|-----|-------|-----------|
| 68EC020 | — (020 has none on-chip) | — | **24-bit address bus** (16 MB) | **A1200, CD32 (stock)** |
| 68EC030 | — | **disabled** | 32-bit bus, caches | accelerators / cost-reduced |
| 68LC040 | **none** | yes | — | accelerator cards |
| 68EC040 | **none** | **none** | — | accelerators / embedded |
| 68LC060 | **none** | yes | — | accelerator cards |
| 68EC060 | **none** | **none** | — | accelerators / embedded |

For the **assembler**, the variant is mainly a *legal-subset / validation* axis
over the **same encodings**: an `68EC030` target should reject PMMU instructions,
an `68LC040`/`68EC040` should reject the `Fxxx` FPU set, an `68EC040` also rejects
the MMU set. (Whether the real silicon *traps* an absent instruction to software
is the emulator's concern; the assembler still encodes it for the full chip,
matching `vasmm68k_mot -m68040` etc. — so the variant gate sits above the shared
table.) The **EC020's 24-bit address bus** is purely a runtime/emulator behavior
(address wrapping), not an assembler concern — but it is exactly the stock-A1200
case, so Emu198x will need it the moment AGA dev targets the real machine.

See [68000-isa-completeness](68000-isa-completeness.md) § Definition of done.

**What "✅ done" means:** validated byte-identical against the reference tool —
historically on the curriculum corpus, *not* a proof of full-ISA coverage. The
rung-1 cross-check against Emu198x's independent decoders made the distinction
concrete: the **Z80** spec was already genuinely *complete* (~60 mnemonics,
confirmed), while the **68000** spec started as a **curriculum subset** of 46
mnemonics. Because `isa::m68k` is the *shared* spec, those gaps failed **assembly**
(`assemble_vasm` rejected them as "unknown instruction"), not just disassembly.
That gap is now **closed**: the 68000 spec covers the full base ISA, validated
byte-identical against vasm for both assemble and disassemble and swept over ~41k
decodable encodings — see [68000-isa-completeness](68000-isa-completeness.md).
6502/6809/65816 full-ISA completeness is not separately audited (no independent
decoder to cross-check yet).

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

Agreed order: **finish 68000 → 6809 → 65816 → reassess.** All three are now done
(68000 to full base ISA; 6809 and 65816 complete). At the reassess point, the
open candidates are the deferred **68020** (driven by the A1200, held off for
now) and the "later" CPUs.

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
- **"Flat `.bin` is good enough — skip `.prg` / `.sna`"** — only if Emu198x loads
  flat at `org` (a sub-choice the umbrella decision owns). Otherwise the
  C64/Spectrum Docker images can't retire: the container *is* the artifact the
  emulator loads. Tracked under § Output containers per platform.
- **"Pick the most popular assembler for the new CPU"** — no; popularity is never
  the criterion. The gate is **validatability** (a runnable reference to diff
  against), the priority is Code198x/Emu198x consumption, and the goal is breadth
  within that — re-anchored from the older "consumption-only" framing by
  [`assemble-io-model.md`](assemble-io-model.md).
- **"Give 6809 (or 65816, 8086) its own assemble engine like vasm"** — no, unless
  it needs its own layout/relaxation/relocation. Computed operands alone use the
  shared `Operation::Encoded` seam and keep the two-pass driver.
- **"Unify the flat engine and vasm's `assemble_core` now"** — not yet; one
  data point. Reassess only when a *second* CPU needs the layout/relocation
  machinery, so the shared shape is real rather than speculative.
