# HuC6280 (PC Engine) — assembler + disassembler

**Status:** in progress (2026-07-02). **Issue:** #9.

Add the Hudson HuC6280 — the PC Engine / TurboGrafx-16 CPU — as the next CPU
after the 6502/Z80/68000/6809/65816 set. Picked over the other CPU ideas
(#8 SM83, #10 TMS9900, #11 CP1610) on **reuse leverage + reference-tool
availability**: it is a 65C02 superset, so it reuses the `mos6502` core, the
`ca65` front-end, and the extension mechanism (`extension_set`) exactly as the
65816 did — and `ca65 --cpu huc6280` (already installed) is a full byte-identical
reference.

## Shape

- **`isa::huc6280`** — an extension `InstructionSet` layered over
  [`mos6502`](../crates/isa/src/mos6502.rs), the same way
  [`mos65816`](../crates/isa/src/mos65816.rs) is. It carries what the NMOS 6502
  lacks: the 65C02 additions the HuC6280 inherits (`bra`, `stz`, `phx`/`phy`/
  `plx`/`ply`, `trb`/`tsb`, `(dp)` indirect, `inc a`/`dec a`, …), the Rockwell
  bit ops (`rmb`/`smb`/`bbr`/`bbs`), and the HuC6280-specific instructions.
- **`ca65` huc6280 dialect** — a front-end over the shared `dialects::mos6502`
  core, parameterised with `isa::huc6280` as the extension set (mirrors
  `ca65_816`). Wired as `assemble_ca65_huc6280` / a `--cpu huc6280`-style select.
- **Disassembler** — spec-driven HuC6280 decoding in `isa-disasm`.
- **Conformance** — byte-identical against `ca65 --cpu huc6280`, via the same
  synthesise → disassemble → reassemble-with-reference harness the other CPUs use.

## Staging

1. **Core** — the 65C02 additions + the HuC6280 fixed-slot instructions
   (implied register ops `sax`/`say`/`sxy`/`cla`/`clx`/`cly`/`csl`/`csh`/`set`,
   and everything that fits the existing single-operand form model). Conformance
   for this slice.
2. **Exotic forms** — `st0`/`st1`/`st2` (`#imm`), `tam`/`tma`, `tst`
   (`#imm` + memory), `bsr` (relative), and the block transfers
   `tii`/`tdd`/`tia`/`tai`/`tin` (opcode + three 16-bit little-endian words:
   source, destination, length). Probing `ca65 --cpu huc6280` confirmed every
   one is a fixed-width layout, so they land as multi-operand fixed-slot `Form`s
   — **no** computed-operand seam (`Operation::Encoded`/`Piece`) needed. The
   initial plan expected the block transfers to need the seam; the byte probe
   showed a plain 7-byte fixed encoding instead. Spec-side conformance for this
   slice; dialect parsing of the multi-operand syntax is Phase 3.
3. **Disassembler + full conformance sweep.**

## Provenance

The spec is authored from the primary library like the other CPUs: the
manufacturer's **HuC6280 CMOS 8-bit Microprocessor Software Manual** (Hudson Soft
/ NEC, 110pp) was sourced and added to
[`reference/by-topic/cpu-huc6280/`](../../reference/by-topic/cpu-huc6280/)
(commit `af886a2b` in the reference repo). The spec cites it, and encodings are
cross-checked byte-for-byte against `ca65 --cpu huc6280` — so the manual and the
reference tool agree. (The extract is OCR-fair, so the conformance sweep, not the
scanned opcode columns, is the final arbiter of a byte.)

## Scope note

PC Engine is not a current Code198x/Emu198x platform; this is a capability
addition (issue #9), not a curriculum-driven one. It does not change the
per-surface support matrix for the PC Engine.
