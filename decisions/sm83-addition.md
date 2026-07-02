# SM83 (Game Boy) — assembler + disassembler

**Status:** complete (2026-07-02). **Issue:** #8.

Add the Sharp SM83 / LR35902 — the Game Boy / Game Boy Color CPU — as the
seventh CPU (after 6502, Z80, 68000, 6809, 65816, HuC6280). Picked over the
other open CPU ideas (#10 TMS9900, #11 CP1610) on reach and reference-tool
availability: the Game Boy is one of the most popular retro-dev targets, and
`rgbasm`/`rgblink` (RGBDS) — the canonical Game Boy toolchain — is installed and
serves as a byte-identical reference.

## Shape

- **`isa::sm83`** — a **fresh fixed-slot spec**, *not* an extension of any other.
  The SM83 is 8080-derived and Z80-flavoured, but it drops the Z80's `IX`/`IY`,
  shadow registers, `I`/`R`, interrupt modes, `IN`/`OUT`, block ops, and the
  `S`/`P-V` flags, and adds `LDH`, `LD (HL+)`/`(HL-)`, `LD HL,SP+e`, `ADD SP,e`,
  `SWAP`, and a two-byte `STOP`. The opcode map diverges too far to layer over
  [`isa::z80`] the way HuC6280 layers over `mos6502`, so it stands alone: the
  single-byte main page plus the `CB` bit/rotate page, spelled out explicitly
  (the Z80 spec's convention).
- **Mode labels are `rgbasm` operand templates.** Registers are lower-case
  (`a`, `[hl]`); immediates are the upper-case placeholders `N` (8-bit), `NN`
  (16-bit), `E` (a `jr` target), `D` (a signed `sp` displacement). The case split
  is load-bearing: register `e` is lower-case, the `jr` placeholder `E` is
  upper-case, so they never collide.
- **`dialects::rgbasm`** — the RGBDS front-end, resolving operands to spec labels
  by the same candidate-probe mechanism the Z80 dialect uses. A bare register
  word offers both a register and an address interpretation so a like-named label
  (`jr nz, l`) resolves; one-operand ALU ops get an implicit accumulator
  (`sub b` = `sub a,b`); `ldh` emits the low byte of the high-page address; and
  `rst`/`bit`/`res`/`set` embed their constant in the opcode.
- **Disassembler** — `isa_disasm::disassemble_sm83`/`listing_sm83`, decoding the
  one- and two-byte (`CB`, `STOP`) opcodes and substituting placeholders back
  into rgbasm syntax. The listing's `SECTION` header carries the real origin so
  non-zero-origin code round-trips.

## How it's verified

- **Spec + disassembler** — the SM83 arm of `spec_opcodes_match_reference`
  synthesises every form, renders it with `listing_sm83`, and reassembles with
  `rgbasm`/`rgblink`; byte-identical across all forms.
- **Assemble direction** — unit tests byte-checked against `rgbasm`, an
  assemble→disassemble→reassemble round-trip, and a full-program differential
  confirmed byte-identical to `rgbasm`/`rgblink`.

## Provenance

Authored from Nintendo's **Game Boy Programming Manual v1.1** (304pp), sourced
into the primary library at
[`reference/by-topic/cpu-sm83/`](../../reference/by-topic/cpu-sm83/) alongside a
pre-existing distilled `cpu-sm83-reference.md`. Encodings are cross-checked
byte-for-byte against `rgbasm`.

## Scope notes

- The Game Boy is not a current Code198x/Emu198x platform; this is a capability
  addition (issue #8), not a curriculum-driven one.
- rgbasm's `@` (current-PC) symbol is not yet accepted in expressions (the shared
  6502-family parser uses `*`); labels cover the common case. A follow-up if a
  real source needs it.
- Output is a flat binary at the section origin. Full RGBDS linking (multiple
  banks, `rgbfix` headers/checksums) is out of scope — the same flat-vs-linked
  stance the other dialects take.
