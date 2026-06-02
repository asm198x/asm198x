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
| 6502 | **acme**, **ca65** | C64 (acme), NES (ca65) | 64tass, dasm |
| Z80 | **PasmoNext** (primary), sjasmplus | Spectrum (pasmonext) | pasmo, z80asm |
| 68000 | **vasm** (mot syntax) | Amiga (vasm) | Devpac/HiSoft |

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
covered (no curriculum use): `!pet`, macros, `!for`/`!zone`. ca65 (the NES
dialect) is the next 6502 front-end; when it lands, the shared 6502
operand-resolution lifts into a `dialects::mos6502` core (the pasmo → sjasmplus
path).

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
