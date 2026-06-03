# Decision: Source-compatible syntax per machine

**Status:** Active. Binding for Asm198x.

**Date:** 2026-05-30.

## The decision

Each assembler is **source-compatible with real existing dialects**, not a
single unified Asm198x syntax. The instruction *encoding* underneath comes from
the shared [`isa`](../crates/isa) spec; the dialect — directives, literals,
operators, label/scope rules, macros — lives in a front-end above it.

**Dialect is an axis independent of CPU.** Asm198x supports *multiple
first-class dialects per CPU*, not one dominant dialect. A dialect front-end
declares which CPU's `isa` spec it targets; several front-ends may target the
same spec (e.g. acme and ca65 both emit 6502). One CPU, one spec, many possible
front-ends.

**Target CPU is a third axis, independent of dialect.** *Which instructions
exist* is a property of the chip you assemble for, not of the source syntax.
The ZX Spectrum Next's **Z80N** opcodes are the case in point: they are
available when the target is the Next, under *any* dialect — not a trait of the
"pasmonext" syntax (sjasmplus, a different syntax, emits Z80N too). So Asm198x
separates **dialect = syntax** from **target = instruction set**: the engine
takes a primary `isa` set plus an optional extension set (the Z80N set), and a
dialect carries a target flag rather than the extension being baked into a
distinct dialect struct. A dialect may *default* a target for tool fidelity
(pasmo → plain Z80, so it rejects Z80N as the real tool does; pasmonext → Z80N),
but the target is selectable independently (`--cpu z80|z80n`).

The goal: real-world source for a machine assembles **unchanged**. Someone with
a working acme C64 project, a ca65 NES project, or a PasmoNext Spectrum project
should point Asm198x at it and get the same bytes out, without porting syntax.

## Dialect targets, prioritised by curriculum

Priority is set by **what Code198x actually consumes**, not by which dialect is
most popular in the wild. The curriculum's own source is the first body that
must assemble unchanged. A 2026-06-02 scan of the curriculum settled the list:

| CPU | First-class dialects | Curriculum platform(s) | Also consider |
|-----|---------------------|------------------------|----------------|
| 6502 | **acme** ✅, **ca65** ✅ | C64 (acme), NES (ca65) | 64tass, dasm |
| Z80 | **PasmoNext** ✅, sjasmplus ✅ | Spectrum (pasmonext) | pasmo, z80asm |
| 68000 | **vasm** (mot syntax) ✅ | Amiga (vasm) | Devpac/HiSoft |

(✅ = delivered and curriculum-validated byte-identical against the real tool.
For vasm, "byte-identical" is the loadable hunk-executable image; vasm's
optional debug symbol table, written in its internal hash order, is omitted —
see the 2026-06-03 Stage 3 entry below.)

Both 6502 dialects are first-class: the curriculum uses acme for the C64 and
ca65 for the NES, so neither is "also consider." For Z80, **PasmoNext is
primary** — the ZX Spectrum Next fork of pasmo (Julián Albo, modified by C
Kirby) that the curriculum invokes as `pasmonext`. PasmoNext is a syntactic
superset of vanilla pasmo; for standard Z80 the two are byte-identical, so one
standard-Z80 backend serves both, and vanilla pasmo drops to "also consider."
The Z80N extended opcodes PasmoNext adds (MUL, LDIRX, NEXTREG, …) are a deferred
ISA-spec extension, authored when the curriculum uses them — a 2026-06-02 scan
found the corpus uses only standard Z80. sjasmplus stays a first-class second
front-end (popular in the wider scene, useful breadth). This corrects an
earlier version of this record that named sjasmplus primary — the curriculum
does not use it.

These are targets, not commitments to bug-for-bug parity. Where dialects
genuinely conflict, document the choice here.

## Why not a unified house syntax

A unified syntax would be cleaner to teach and document, but it would make
every existing body of source need porting — which defeats the rescue mission
(see [`../../../decisions/asm198x-and-shared-isa-spec.md`](../../../decisions/asm198x-and-shared-isa-spec.md)).
The whole reason Asm198x exists is that the *tools* are hard to run, not that
the *source* is wrong. Keep the source working.

## What is shared vs per-dialect

- **Shared (in `isa`):** opcode encodings, operand layout, cycle counts, flags.
- **Shared (in the engine):** expression evaluation, symbol table, sections,
  output formats, listing — the dialect-agnostic machinery.
- **Per-dialect (in each dialect front-end):** operator syntax, directive names
  and semantics, number/string literal forms, label and scope rules, macro
  syntax. A front-end names the `isa` spec it targets, so a CPU can carry
  several (acme and ca65 both target the 6502 spec).

Do **not** build a data-driven "describe any dialect as config" engine yet.
Write two or three real front-ends first; let the shared parts fall out. Only
extract a declarative dialect descriptor if the variance proves genuinely
tabular — premature generalisation here is the failure mode to avoid.

## Current state

The 6502 **acme** front-end is delivered and validated. It is the first real
6502 dialect (the earlier generic `.org`/`.byte` placeholder has been retired;
`--dialect 6502` now aliases acme). It covers `*=` (with forward-gap
zero-fill), `name = expr`, `!byte`/`!word`/`!fill`, `!text`/`!scr` (screen
codes derived from the binary), arithmetic with C precedence, the `<`/`>`
low/high-byte prefixes, `*` as the program counter, anonymous `-`/`+` labels,
value-based zero-page selection, and conditional assembly
(`!if`/`!ifdef`/`!ifndef` … `{ }` … `else`). **The entire buildable C64
curriculum — all 80 units across starfield (16) and sid-symphony (64) —
assembles byte-identical to `acme -f cbm`, with zero miscompiles.** Not yet
covered (no curriculum use): `!pet`, macros, `!for`/`!zone`.

The **ca65** front-end is also delivered — the NES curriculum's dialect. The
shared 6502 operand-resolution and expression parser now live in a
`dialects::mos6502` core (the pasmo → sjasmplus path realised for 6502), with
acme and ca65 as thin surfaces over it; the one grammar difference, where `<`/
`>` bind, is a `BytePrec` flag. ca65 supports `.segment`, `.byte`/`.word`/
`.res`, `=` constants, `name:` labels, and `@cheap` locals. Because ca65 output
is linked by ld65, Asm198x includes a **bounded linker** for the one fixed
`nes.cfg`: it places segments into the NROM layout (`CODE`@`$8000`,
`VECTORS`@`$FFFA`, zero-page/RAM segments off-file) and emits the 40976-byte
`.nes` (iNES header + 32K PRG + 8K CHR, fill `$00`). **All 32 buildable NES
units (dash + neon-nexus) assemble and link byte-identical to `ca65 + ld65`.**
This is the first link step in Asm198x; it is deliberately minimal (one config,
no object-file format), scoped to the curriculum — see the Log.

The Z80 backend is delivered and validated. The engine ↔ dialect ↔ spec seam is
split; the Z80 `isa` spec covers the **complete documented instruction set**
(base page, ED, CB, and DD/FD IX/IY including DD-CB); and both the `pasmo` and
`pasmonext` front-ends handle real source (arithmetic with C precedence, `equ`
constants, `defb` strings, opcode-embedded and indexed operands). **The entire
Gloaming Spectrum curriculum — all 20 units — assembles byte-identical to the
`pasmonext` binary under both dialects, with zero miscompiles**, and a broad
IX/IY exerciser matches pasmonext too. The **Z80N** (Spectrum Next) extension is
now in as well — its own `isa` set, gated by target, validated opcode-by-opcode
against `pasmonext`; base pasmo (plain-Z80 target) rejects it. A spec-driven
disassembler round-trips standard and Z80N code back to identical bytes. The
location counter `$` (statement-start address) and sjasmplus-style local-label
scoping (a leading-`.` label scoped under the preceding global) are both in,
shared across the Z80 dialects and validated against both binaries.

## Drift triggers

- **"Let's invent one clean Asm198x syntax for all CPUs"** — no; that breaks the
  rescue mission. Re-read "Why not a unified house syntax." Revisit only by
  amending this record.
- **"Put dialect-specific directive handling in the shared engine"** — no;
  dialect lives in the per-CPU front-end, encoding and engine stay shared.
- **"Match this niche assembler instead of the primary target"** — record the
  reason here first; don't silently retarget a backend's dialect.
- **"One dialect per CPU is enough"** — no; dialect is an axis independent of
  CPU and the curriculum needs several per CPU (acme *and* ca65 for 6502).
  Re-read "Dialect targets, prioritised by curriculum."
- **"Build a generic data-driven dialect engine so any syntax just works"** —
  not yet. Write real front-ends first; generalise only if the variance proves
  tabular. See "What is shared vs per-dialect."
- **"Prioritise the most popular dialect in the scene"** — priority is set by
  curriculum consumption, not wild popularity. That is why Z80 leads with
  PasmoNext, not sjasmplus.
- **"The Z80 target is vanilla pasmo"** — no; the curriculum uses **PasmoNext**
  (invoked as `pasmonext`), a Spectrum Next superset of pasmo. Validate against
  the `pasmonext` binary.
- **"Gate the Z80N opcodes on the pasmonext dialect"** — no; instruction-set
  availability is a *target* property (which chip), not a *syntax* one. Gate
  Z80N on the target (Z80 vs Z80N), available under any dialect pointed at the
  Next. Dialects may default a target for fidelity, but the axes stay separate.
  Re-read "Target CPU is a third axis."

## Log

### 2026-06-02 — Multi-dialect amendment

Reframed from "one dominant dialect per CPU" to "dialect is an axis independent
of CPU; multiple first-class dialects per CPU, prioritised by curriculum." A
scan of Code198x showed the curriculum already spans dialects — acme (C64) and
ca65 (NES) for 6502, pasmo for the Spectrum, vasm for the Amiga — so serving the
curriculum *requires* several front-ends per spec. Corrected the Z80 primary
from sjasmplus to **pasmo** (the Spectrum curriculum's assembler); sjasmplus
stays first-class for breadth. Set the active first target to **Z80 + pasmo**,
which also forces the engine/dialect/spec seam while the codebase is small.
Held the line against a premature data-driven dialect engine.

### 2026-06-02 — Z80 target is PasmoNext, not vanilla pasmo

Steve noted the course is "busily using pasmonext." The installed assembler is
PasmoNext v0.1.3 — the ZX Spectrum Next fork of pasmo (Julián Albo, modified by
C Kirby) — and the curriculum invokes `pasmonext`. Renamed the Z80 dialect
target from pasmo to **PasmoNext**; it is a syntactic superset, so for standard
Z80 the byte output is identical and one backend serves both. The Z80N extended
opcodes (MUL, LDIRX, NEXTREG, …) are deferred: a corpus scan found only standard
Z80 in use (apparent `TEST`/`MIRROR` hits were comment/filename noise). Renamed
the code dialect to `pasmonext`/`PasmoNext`; `pasmo` stays a CLI alias.

### 2026-06-02 — Z80N is a target, not a dialect

Steve challenged gating Z80N on the `pasmonext` *dialect*: instruction
availability should not depend on source syntax. He's right — Z80N is the
Spectrum Next chip's extension; *which instructions exist* is a target-CPU
property. The tell: pasmo and pasmonext are syntactically identical (the latter
just adds Z80N), and sjasmplus — a different syntax — emits Z80N too, so "emits
Z80N" is a capability many syntaxes share, i.e. a target axis. Collapsed
`Pasmo`/`PasmoNext` into one pasmo-syntax dialect with a `z80n` target flag;
added `Dialect::extension_set` and an optional engine extension set (Z80N lives
in its own `z80::NEXT`). The CLI separates `--dialect` (syntax) from
`--cpu`/`--target` (z80/z80n); pasmo defaults to plain Z80 (rejecting Z80N for
tool fidelity), pasmonext to Z80N, and `--cpu` overrides. Validating each Z80N
opcode against the binary corrected the lore: `MUL` takes no operand, and
`PUSH nn` is little-endian.

### 2026-06-02 — sjasmplus dialect; shared Z80 syntax core

Added **sjasmplus** as the second first-class Z80 dialect — the first genuinely
*different* syntax (pasmo/pasmonext share a parser). Since the Z80
mnemonic/operand syntax is identical across assemblers, extracted a shared
`dialects::z80` core (operand resolution, expression parser, vocabulary, common
directives, driver) behind a small `Z80Syntax` trait; a dialect now overrides
only **comment style** and **number formats**. pasmo is a ~40-line surface;
sjasmplus adds `//` comments and the `$/0x/h` and `%/0b/b` number formats.
Validated byte-for-byte against the sjasmplus v1.21 binary, and all three
dialects assemble the whole Gloaming curriculum identically. This is the
multi-dialect-per-CPU stance realised: one CPU spec, one shared syntax core,
many thin dialect surfaces. Deferred: `$`-as-PC, real local-label scoping,
sjasm modules/macros.

### 2026-06-02 — `$`-as-PC and local-label scoping landed

Bundled in the two cross-dialect deferrals rather than leaving them per-dialect
debt — both belong in the shared Z80 core, so every dialect gains them at once.
**`$` (location counter)** lowers to a new engine `Expr::Pc`, evaluated to the
statement's start address in both passes; the shared tokenizer emits it for a
bare `$` and still reads `$hex` as a number. Validated against `pasmonext` and
`sjasmplus`: `jr $` → `18 FE`, `ld hl,$`, `jp $+3`, `dw $`, `ld bc,$-1` all
byte-identical. **Local-label scoping** is gated by a new
`Z80Syntax::scopes_locals()` (default off): a leading-`.` label qualifies to
`{global}.{local}` under the most recent global, and bare local references
rewrite to match. sjasmplus turns it on (matching its `.loop`-recurs-per-scope
behaviour); pasmo leaves it off (a leading-`.` name is an ordinary global, and
reuse is a duplicate-label error, as the pasmo binary enforces). sjasm modules
and macros stay deferred — they are sjasm-specific, not a shared-core feature.
All three dialects still assemble the Gloaming curriculum 20/20 byte-identical.

### 2026-06-02 — ACME 6502 dialect delivered; generic placeholder retired

Built the real **acme** front-end (the C64 curriculum's assembler) in five
validated slices: (1) `*=` PC, `name = expr`, `!byte`/`!word`/`!fill`,
arithmetic, `<`/`>`, `*`-as-PC; (2) anonymous `-`/`+` labels; (3) `!text` (raw)
and `!scr` (screen codes derived empirically from the binary); (4) conditional
assembly (`!if`/`!ifdef`/`!ifndef` … `{ }` … `else`) with a parse-time symbol
environment; plus value-based zero-page selection (a `= const` low symbol picks
the short form, matching acme) and constant-folded `!fill` counts. **The whole
buildable C64 curriculum — 80 units (starfield 16 + sid-symphony 64) — assembles
byte-identical to `acme -f cbm`.**

ACME operator precedence and the screen-code table were both derived by probing
the binary, not from memory (the `<`/`>` prefixes bind loosest; lowercase maps
to uppercase screen codes 1–26). The 6502 operand resolution sits inside
`acme.rs` for now, to be lifted into a shared `mos6502` core when ca65 lands
(the pasmo → sjasmplus precedent).

Retired the generic `.org`/`.byte` placeholder — nothing consumed it and it was
never a real dialect. `--dialect 6502`/`mos6502` and `assemble_*`'s 6502 entry
now route to acme; the `ca65` alias is dropped until that front-end exists
(mapping it to acme would silently miscompile NES source).

### 2026-06-02 — ca65 (NES) dialect, shared mos6502 core, and a bounded linker

Delivered ca65, the NES curriculum's dialect, in three slices: (1) extracted the
dialect-agnostic 6502 machinery from acme into a `dialects::mos6502` core
(operand classification, zero-page-vs-absolute, constant folding, the expression
parser), parameterised only by `BytePrec` — ACME's `<`/`>` apply to the whole
expression, ca65's bind tight as unary operators (`>$1234+1` is `$12` vs `$13`,
both verified against the binaries); (2) the ca65 front-end (`.segment`,
`.byte`/`.word`/`.res`, `=`, `name:` and `@cheap` locals scoped to the preceding
global); (3) a **bounded linker**.

**Scope decision (linker).** ca65 emits object files that ld65 links, so a
byte-identical `.nes` requires a link step — new for a project branded
"assemblers + disassemblers." Steve approved building it, bounded to the
curriculum's reality: every NES unit links with the *same* `nes.cfg` (verified:
all 32 are byte-identical), so the NROM layout is encoded directly rather than
parsing config files, and there is no object-file format — assemble and link run
in one in-memory pass over a single source file. Segment bases come from that
layout (`CODE`@`$8000`, `VECTORS`@`$FFFA`, `ZEROPAGE`@`$00`, `OAM`/`BSS`
off-file); the ROM is iNES header + 32K PRG + 8K CHR, fill `$00`. If a second
linker config or multi-object linking ever appears, this is the point to
generalise (parse the `.cfg`, add a relocatable object step) — not before. The
umbrella scope (assemblers + disassemblers) now implicitly includes the minimal
link needed to validate a linked target; revisit the umbrella decision if
linking grows beyond this.

All 32 buildable NES units (dash + neon-nexus) assemble and link byte-identical
to `ca65 + ld65`; ACME stays 80/80. Wired as `assemble_ca65` / `--dialect ca65`
(emitting `.nes`).

### 2026-06-03 — vasm (68000 / Amiga): field-based core + optimizer

Delivered the 68000 / vasm dialect, the Amiga curriculum's toolchain. The
68000 needed a new encoding model: where 6502/Z80 are byte-opcode lookups, the
68000 packs size, registers, and two six-bit effective-address fields into a
16-bit opcode word followed by 0–4 extension words. The `isa::m68k` spec is
therefore **field-based** — a base opcode word plus `Slot` bit-field
descriptors — and the vasm front-end fills those fields. Built in stages,
matched against the real `vasmm68k_mot` at each:

- **Stage 1 — `-no-opt` (flat binary).** Instruction encoding only. Every Amiga
  curriculum unit that `vasm -no-opt` can build (20 of 32) is byte-identical.
  Getting there fixed local-label scoping (`.`-labels scoped to the enclosing
  global), `.s` short branches, ADDA/SUBA/CMPA (`add …,An`), ADDI/SUBI/CMPI
  (immediate to memory), `ds`/`dcb` counts as pass-1 expressions, PC-aware
  `equ` (`len equ *-buf`), and the rule that a bare absolute is `(xxx).L`.

- **Stage 2 — default `-Fbin` (optimizer on).** The mode the shipped artifacts
  use. **All 21 curriculum units that build as a flat binary are now
  byte-identical to `vasm -Fbin`.** The optimizer replicates vasm's decisions
  exactly: PC-relative addressing for in-section label references (kept as
  `(xxx).L` for fixed constants like `$dff000`); short-branch relaxation via a
  grow-only fixpoint loop (a bare `bra`/`bsr`/`bcc` is short by default, grown
  to word only when its displacement overflows a byte); `addq`/`subq` for small
  immediates; `add/sub #d16,An → lea d16(An),An`; dropping a zero `d16(An)`
  displacement to `(An)`; and `cmp #0,<ea> → tst <ea>`. Symbol kind
  (relocatable label vs absolute `equ`) is tracked because only relocatable
  references are PC-relative-eligible. `assemble_with(.., false)` keeps the
  Stage-1 `-no-opt` behaviour for comparison.

- **Stage 3 — `-Fhunkexe -kick1hunks` (done).** **All 32 Amiga curriculum
  units now assemble to hunk executables byte-identical to the loadable image
  of `vasmm68k_mot -Fhunkexe -kick1hunks`.** The shipped
  artifacts are Amiga hunk executables, byte-identical to `vasm -Fhunkexe
  -kick1hunks` (verified against the committed `signal`/`exodus` binaries). One
  obstacle shapes the target: vasm writes a `HUNK_SYMBOL` debug table in its
  internal hash order, which the AmigaDOS loader ignores. Reproducing those
  bytes would mean replicating vasm's symbol hash — brittle and version-tied —
  so the decision (2026-06-03, Steve) is **loadable-image parity**: match
  everything the loader consumes (HUNK_HEADER, CODE/DATA/BSS, RELOC32, END) and
  omit the symbol table. A section-aware core emits per-hunk bytes plus
  relocations: a relocatable reference is PC-relative only within its own hunk,
  else `(xxx).L` with a `HUNK_RELOC32` entry; `move.l #label` always relocates.
  Code hunks pad to a longword with `NOP`. **All 21 single-section units produce
  hunk executables byte-identical to the stripped vasm output.** Wired as
  `assemble_vasm_exe` / `--dialect vasm --exe`.

  The 11 two-section units (`code` + `chipbss`) drove three further features:
  bitwise/shift operators (`& | ^ << >>`) added to the shared expression parser
  with vasm's precedence (shift > `&` > `^` > `|` > `* /` > `+ -`, verified
  against vasm) — gated to vasm, so the 6502 dialects (which use neither these
  nor are affected) stay byte-identical; indexed addressing `d8(An,Xn.size)`
  (brief extension word); the `lea d8(An),An`→`addq`/`subq` rewrite (reverse of
  `add #d,An`→`lea`); and `label(pc)` encoding the distance `label - pc` rather
  than the label's absolute offset. The multi-section container (two hunks,
  cross-hunk `RELOC32`, a BSS hunk) then validated unchanged.

The flat path stays wired as `assemble_vasm` / `--dialect vasm` (`-Fbin`); the
executable is `assemble_vasm_exe` / `--dialect vasm --exe`.
