# CP1610 — a staged, field-based build

**Status:** 🚧 **In progress (started 2026-07-03).** The GI CP1610 (Mattel
Intellivision CPU) is built as sweep-verified increments, like the Z8000.
**Increments 1–4** — the single-decle register / implied groups, the
register-only shift / rotate group, the two-decle relative branches, and the
memory / immediate addressing modes — have landed, byte-identical to `asl`
(`cpu CP-1600`). Remaining: `JUMP`/`JSR` and the `SDBD` double-byte immediate.
Closes the CP1610 half of asm198x/asm198x#11 when complete.

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
build. `asl` addresses in *decles* (word units), which will matter for the
PC-relative branch displacements (a later increment); increment 1 has no
address-dependent operands, so it is unaffected. The genuinely sub-byte machines
(HP-Saturn nibble, SM5xx 4-bit) stay in Wave E. See the umbrella
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
5. **`JUMP` / `JSR`** — the jump family (`J`/`JE`/`JD`, `JSR`/`JSRE`/`JSRD`), a
   three-decle encoding (`0x0004` prefix, then a register/interrupt/address word
   pair) distinct from everything else.
6. **`SDBD` double-byte immediate** — the stateful prefix: after `SDBD`, the next
   immediate is emitted as **two low-byte-first decles** (`0x1234` → `0x0034`,
   `0x0012`), so both the dialect and the disassembler must track the preceding
   `SDBD`. The `SDBD` opcode itself already exists (increment 1); this is only the
   immediate-splitting. Also where the `0x0035` / `0x0037` NOP/SIN variants and
   the exact `asl` data directive get pinned.

## Reference

Facts from the umbrella primary library: distilled
[`cpu-cp1610-reference.md`](../../reference/by-topic/cpu-cp1610/cpu-cp1610-reference.md)
(register set, addressing modes, SDBD, flags) and the Intellivision manual set in
`reference/by-system/intellivision/`. `asl` (`cpu CP-1600`) is the byte arbiter,
exactly as it is for the other field-based CPUs. Exact opcode encodings are
probed from `asl` — the prose reference defers the opcode map to the CP-1600
User's Manual appendix, so the byte-level ground truth is the reference assembler.
