# Z8000 — a staged, field-based build

**Status:** In progress (Wave B). Branch `feat/z8000`. Foundation landed
(sourced manual + decoded encoding model); implementation is staged in verified
increments.

## Why staged, when the other CPUs were one-shot

Every CPU landed so far (8080 … TMS9900) was a single-session, byte-identical
deliverable of roughly one spec + one dialect + one disassembler (~1–1.5k lines).
The **Z8000 is 3–4× that**: the Zilog manual counts **110 distinct instruction
types**, each with a subset of **eight addressing modes** (R, IM, IR, DA, X, RA,
BA, BX), **word / byte / long** sizes, and a **segmented (Z8001) vs non-segmented
(Z8002)** split that widens address operands. Attempting the whole ISA in one
pass invites exactly the bugs and "almost-there" churn the coding-cadence rules
warn against. So it is built as **verified increments**, each committed only when
the opcode-space sweep is green.

The sweep makes incremental delivery safe: `spec_sweep_matches_reference`
disassembles the opcode space, reassembles with `asl`, and compares. Any
instruction group **not yet implemented decodes to `word` data and is skipped**,
so a partial decoder is always self-consistent — each increment adds a group and
the sweep proves it byte-identical without disturbing the rest.

## Architecture

Like [PDP-11](../../decisions/asm198x-cpu-coverage-roadmap.md) and TMS9900, the
Z8000 is **field-packed** — operands live in fields inside the opcode word — so
`isa::z8000` is a **bespoke table** (mnemonic + base + a `Class` fixing the
field layout), keyed by both the dialect and a field-based disassembler, riding
the **computed-operand seam**. Big-endian; `asl`'s `h`-suffix hex (the 8080
lexer). Target `cpu Z8002` first.

## The decoded core: the dyadic family

The arithmetic / logic / load family (`ADD`/`SUB`/`OR`/`AND`/`XOR`/`CP`/`LD` +
`ADC`/`SBC` + byte forms) shares one first-word format:

```
  MM ooooo b   ssss dddd
```

- `MM` (bits 15–14) = addressing-mode group: `00` → **IR** (`@Rs`) if `ssss ≠ 0`
  else **IM** (immediate word follows); `01` → **DA** (address word) if
  `ssss = 0` else **X** (`addr(Rs)`, address word); `10` → **R** (`Rs`).
- `ooooo` (bits 13–9) = operation: `ADD 0x00`, `SUB 0x01`, `OR 0x02`, `AND 0x03`,
  `XOR 0x04`, `CP 0x05`, `LD 0x10`, `ADC 0x15`, `SBC 0x17`.
- `b` (bit 8) = word (1) / byte (0). Byte registers are `RHn = n`, `RLn = 8+n`.
- second byte = `ssss dddd` (source field, destination register).

Verified against `asl`: `ld r1,r2 = A121`, `add r1,#5 = 0101 0005`,
`add r1,1234h(r2) = 4121 1234`, `addb rl1,rl2 = 80A9`.

## Increment plan

1. **Dyadic word/byte core** (this increment): the family above across R / IM /
   IR / DA / X. Establishes the addressing-mode parser + the field decoder.
2. **Long dyadic + `LDL`/`LDA`/`LDR`/`CLR`/`EX`** — the long-size opcodes
   (`LDL = 0x94`, `ADDL`, …) and the load variants.
3. **Program control** — `JP cc,dst`, `CALL`, `JR cc`, `DJNZ`, `CALR`, `RET cc`
   (the condition-code field + relative displacements, a `Piece::Packed` scale).
4. **Single-operand** — `INC`/`DEC`/`NEG`/`COM`/`TEST`/`PUSH`/`POP`/`CLR`.
5. **Shifts / rotates** — `SLA`/`SRA`/`SLL`/`SRL`/`RL`/`RR`/`RLC`/`RRC` (count).
6. **Bit** — `BIT`/`SET`/`RES`, static and dynamic.
7. **Multiply / divide, block, string, I/O, CPU control** — the remainder.
8. **Segmented Z8001** — widen DA/X/RA address operands to segmented addresses
   as a target-extension over the non-segmented base.

Each step: probe `asl` for the group's exact encodings, add the `Class` +
table rows + dialect arm + decoder arm, extend the round-trip test, keep the
sweep green, commit.
