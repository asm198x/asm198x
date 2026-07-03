# CP1610 — a staged, field-based build

**Status:** ✅ **Complete (2026-07-03).** The GI CP1610 (Mattel Intellivision
CPU) landed across six sweep-verified increments, like the Z8000 — every
instruction byte-identical to `asl` (`cpu CP-1600`): the register / implied
groups, the shift / rotate group, the two-decle relative branches, the memory /
immediate addressing modes, `JUMP`/`JSR` (with engine word-addressing), and the
`SDBD` double-byte immediate. Closes the CP1610 half of asm198x/asm198x#11.

## The decle: 10-bit, but byte-aligned

The CP1610's defining oddity is the **10-bit "decle"** word. The umbrella
CPU-coverage roadmap originally parked it in Wave E behind a "sub-byte model"
for that reason. On investigation the framing was wrong:

- `asl` arbitrates it as **`cpu CP-1600`** (the GI CP-1600 generator; the spelling
  is hyphenated — `CP1600` / `CP1610` are rejected), speaking the jzIntv / as1600
  mnemonics that are the homebrew standard.
- `p2bin` emits each decle as a **big-endian 16-bit word** with the top six bits
  zero — the standard Intellivision ROM-image representation.

So the output is byte-aligned and the existing byte-oriented engine handles it
directly. This is the **TMS9900 / PDP-11 field-packed pattern**, not a sub-byte
build. The one twist is that `asl` addresses in *decles* (word units), so a label
is a decle number, not a byte offset — the engine gained an `addr_unit` for this
(2 for the CP1610, 1 everywhere else) in increment 5, once absolute-address
operands made it load-bearing (see increment 5 below). The genuinely sub-byte
machines (HP-Saturn nibble, SM5xx 4-bit) stay in Wave E. See the umbrella
[`asm198x-cpu-coverage-roadmap.md`](../../decisions/asm198x-cpu-coverage-roadmap.md)
§ Wave E for the reclassification.

## Architecture

Like TMS9900, the CP1610 is **field-packed** — operands live in fields inside the
opcode word — so `isa::cp1610` is a **bespoke table** (mnemonic + `base` + a
`Class` fixing the field layout), keyed by both the dialect encoder and a
field-based disassembler, not the `Form` model. Decode tries classes widest-mask
first so a fixed opcode (`NOP` at `0x034`) is matched before a broader field
group sharing its region (`GSWD`'s `0x030` block). `decode` rejects any word
above `0x3FF` — not a valid 10-bit decle.

The sweep makes incremental delivery safe: `spec_sweep_matches_reference` walks
the decle space `0x000..=0x3FF`, disassembles with our decoder, reassembles with
`asl`, and compares. Any group **not yet implemented decodes to `word` data and
is skipped**, so a partial decoder stays self-consistent.

### `asl` CP-1600 number syntax — `relaxed on`

Unlike the other asl-arbitrated targets, asl's `CP-1600` mode does **not** accept
the Intel `H`-suffix hex the rest of the codebase emits — by default it takes only
decimal and its own `x'…'` hex. The sweep caught this: the register instructions
assembled fine, but the listing's `org 01000H` line was rejected as an "invalid
symbol name". Rather than switch CP1610 to a bespoke number style, `listing_cp1610`
emits **`relaxed on`** (asl's directive that enables all integer syntaxes at once),
so `0XXXXH` works and the listings stay consistent with every other CPU — the same
move as the Z8000 listing's `supmode on`. The dialect ignores the `relaxed`
directive on the way back in. (The accepted CPU spelling is also fussy:
**`CP-1600`** with the hyphen; `CP1600` / `CP1610` are rejected.)

## Increments

1. **Register / implied (single-decle)** — ✅ **landed (2026-07-03).** The
   control ops (`HLT`, `SDBD`, `EIS`, `DIS`, `TCI`, `CLRC`, `SETC`, `NOP`,
   `SIN`), register-unary arithmetic (`INCR`/`DECR`/`COMR`/`NEGR`/`ADCR`), status
   transfer (`GSWD` R0–R3 / `RSWD` R0–R7), and the register-register dyadic group
   (`MOVR`/`ADDR`/`SUBR`/`CMPR`/`ANDR`/`XORR`). Four `Class` variants (`Implied`,
   `RegUnary`, `GetStatus`, `RegReg`), all one decle, no extension words. Verified
   by a direct differential, a round-trip test, and the decle-space sweep.
2. **Shifts / rotates** — ✅ **landed (2026-07-03).** The register-only shift
   group `SWAP`/`SLL`/`RLC`/`SLLC`/`SLR`/`SAR`/`RRC`/`SARC` (`Class::Shift`,
   `base | (count-1) << 2 | reg`, R0–R3, count 1 or 2 — a bare register is
   count 1). Single-decle, no extension word, so no engine change. Verified by a
   differential, a round-trip, and the sweep.
3. **Branches** — ✅ **landed (2026-07-03).** The two-decle relative branch group:
   the 16 conditional branches (`B`/`BC`/`BOV`/`BPL`/`BEQ`/`BLT`/`BLE`/`BUSC` and
   the bit-3-negated `NOPP`/`BNC`/`BNOV`/`BMI`/`BNEQ`/`BGE`/`BGT`/`BESC`), the
   external-condition `BEXT target, ec` (bit 4 set, `ec` in the low nibble), and
   `NOPP` (the branch-never two-word no-op, no operand). Required a small **engine
   extension** — a new `Piece::Branch`: the branch is two decles where the *sign*
   of the displacement selects a **direction bit** (`0x20`) in the opcode word
   (forward `EA = PC + d`, backward `EA = PC − d − 1`, `PC = opcode + 2` decles),
   which the linear `Piece::Packed` can't express. The byte distance is divided by
   `unit` (2, bytes per decle) to the decle magnitude, so a label-based branch
   matches `asl` exactly (both compute the same decle displacement). Isolated into
   its own increment so the shared-engine change was reviewed and sweep-checked on
   its own. Branches are **position-dependent**, so — like the TMS9900 jumps —
   they fall out of the sweep and are covered by a differential + a round-trip
   test instead. The disassembler prints byte-address targets, self-consistent
   through the engine.
4. **Memory / immediate modes** — ✅ **landed (2026-07-03).** The seven
   memory-referencing families (`MVO` store; `MVI`/`ADD`/`SUB`/`CMP`/`AND`/`XOR`
   loads + ALU) across all three addressing modes — direct (`mm=0`, a following
   address word), indirect `@R1`–`@R6` (`mm=1..6`), and immediate (`mm=7`, a
   following value word) — plus the `PSHR`/`PULR` R6-stack aliases. The mnemonic
   suffix picks the mode (bare / `@` / `I`), and `MVO` reverses the operand order
   (register first). Direct and immediate operands are **position-independent**
   (absolute address / literal, stored with no scaling), so — unlike the branches
   — they *are* covered by the sweep, whose CP1610 candidates gained a filler
   extension word.
5. **`JUMP` / `JSR` + word-addressing** — ✅ **landed (2026-07-03).** The jump
   family (`J`/`JE`/`JD`, `JSR`/`JSRE`/`JSRD`): a three-decle encoding — `0x0004`,
   then a word carrying the return register (`rr`: R4–R6 = 0–2, or 3 for plain
   `J`) in bits 9:8, the interrupt action (`ii`: none/E/D = 0/1/2) in bits 1:0,
   and `addr >> 10` in bits 7:2; then a word with `addr & 0x3FF`. The address is
   split with `Shr`/`And`/`Shl`/`Or` [`Expr`]s emitted as two value words — no new
   engine `Piece`. This increment also fixed a **latent addressing bug**: `asl`'s
   CP-1600 is **word-addressed** (a label is a decle number), but the engine was
   byte-addressed, so any absolute-address operand referencing a label — direct
   memory (increment 4) *and* the new jumps — came out 2× too large (literals were
   fine; the differentials happened to use them). The fix is an engine
   `addr_unit` (bytes per address unit; 1 everywhere, 2 for the CP1610): the
   location counter advances in decles, so labels, `org`, and absolute operands
   match `asl`. `Piece::Branch` was reworked to the unit-based counter (dropping
   its byte-distance `scale`), and the disassembler is decle-addressed. Jumps are
   position-independent but the `0x0004`-prefixed three-decle form is longer than
   the sweep's candidate, so they are covered by a differential (literals + labels)
   and a round-trip, not the sweep.
6. **`SDBD` double-byte immediate** — ✅ **landed (2026-07-03).** The stateful
   prefix: after `SDBD`, the next **immediate** (mode 7 — `MVII`/`ADDI`/…) is
   emitted as **two low-byte-first decles** (`0x1234` → `0x0034`, `0x0012`);
   direct addresses, indirect modes, and register ops are unaffected. Both sides
   thread an `after_sdbd` flag — the dialect's parse loop sets it after an `SDBD`
   and clears it on any other instruction; the disassembler tracks the previous
   decle being `0x0001` and reads two immediate decles when it was. The `SDBD`
   opcode itself already existed (increment 1); this is only the
   immediate-splitting. The split immediate is built from `And`/`Shr` [`Expr`]s so
   a forward reference still resolves. Covered by a differential (each immediate
   mnemonic under `SDBD`, plus the must-not-split cases) and a round-trip; the
   `SDBD` interaction is a two-instruction sequence the opcode sweep can't batch.

## Reference

Facts from the umbrella primary library: distilled
[`cpu-cp1610-reference.md`](../../reference/by-topic/cpu-cp1610/cpu-cp1610-reference.md)
(register set, addressing modes, SDBD, flags) and the Intellivision manual set in
`reference/by-system/intellivision/`. `asl` (`cpu CP-1600`) is the byte arbiter,
exactly as it is for the other field-based CPUs. Exact opcode encodings are
probed from `asl` — the prose reference defers the opcode map to the CP-1600
User's Manual appendix, so the byte-level ground truth is the reference assembler.
