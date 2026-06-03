# Decision: spec-conformance audit + differential fuzzing

**Status:** Active. Binding for Asm198x.

**Date:** 2026-06-03.

## The decision

Correctness now rests on **four layers**, not one, each testing a different thing
against the real reference assemblers:

1. **Curated byte-identity** (`tests/curriculum`) — real curriculum programs
   assemble byte-for-byte to the reference tool. Proves *the programs we ship*.
2. **Round-trip** (in the curriculum harness) — assemble → disassemble →
   reassemble (with *our* assembler) reproduces the bytes. Proves *internal
   self-consistency* of the asm/disasm pair.
3. **Spec-conformance audit** (`tests/conformance`, `spec_opcodes_match_reference`)
   — every `(mnemonic, mode) → opcode` in `isa` is checked against the reference
   tool. Proves *the spec data itself*, including modes no curated program uses.
4. **Differential fuzz** (`tests/conformance`, `differential_fuzz`) — random
   multi-instruction programs, reassembled by *both* our assembler and the
   reference, must reproduce the bytes. Proves *operand-value and sequence
   coverage* the curated corpus misses.

### The disassembler-reuse trick

The audit and fuzzer do **not** carry per-dialect "mode → source syntax" tables.
For each form they synthesise canonical bytes (opcode + filler operands),
**disassemble with our disassembler**, then reassemble the text with the
**reference** assembler and require the original bytes back. Swapping the
reference in where the round-trip uses our own assembler makes the reference the
arbiter — so a wrong spec opcode, or disassembler output the real tool rejects,
shows up as a mismatch. This is why the disassemblers are a prerequisite for the
audit (every audited CPU has one).

### What it caught immediately

The 6502 disassembler rendered accumulator mode as `ASL A`; **acme rejects that**
(it wants bare `asl`). The round-trip missed it because *our* parser accepts
`A`. Fixed by rendering accumulator mode as the bare mnemonic. This is the class
of bug the audit exists for: output that is self-consistent but not real-tool
compatible.

## Scope

Two audit techniques, by spec shape:

- **Form-based audit** (`spec_opcodes_match_reference`) — for the `isa::Form`
  specs (`mos6502`, `z80`, `mos65816`): iterate every form, synthesise canonical
  bytes, disassemble, reassemble with the reference. 1124 forms. These have the
  largest opcode tables and the highest hand-authoring risk; `mos65816` was
  authored the prior cycle and is fully verified.
- **Sweep audit** (`spec_sweep_matches_reference`) — for the non-form specs
  (`mos6809` is `Kind`-based; `m68k` is field-based) there are no forms to
  iterate, so it sweeps candidate byte sequences through the disassembler,
  keeps the ones that decode to a **position-independent** instruction (verified
  by disassembling at two origins — this drops PC-relative branches, which can't
  be batched), concatenates them, and reassembles the whole blob in one call. It
  covers the primary opcode space plus, for 6809, the full indexed-postbyte
  space (~390 instructions). On failure it localises by reassembling each alone.
- **Fuzzer** (`differential_fuzz`) — stateless CPUs only (6502, Z80). The
  65816's `m`/`x` width makes a random instruction stream ambiguous to decode,
  so it is covered by the per-form audit and the curriculum round-trip instead.

### Covered

`mos6502`, `z80`, `mos65816` (form audit); `mos6809` and `m68k` (sweep). The
sweeps caught real decoder/spec bugs, now fixed:

- **6809:** invalid indexed postbytes (`$8F`/`$BF`/… — extended indirect is
  exactly `$9F`; single auto-inc/dec has no indirect form) decoded as valid.
- **68000:**
  - ADDI/SUBI/CMPI are now **distinct mnemonics** (`$06`/`$04`/`$0C`), so they
    disassemble correctly; `add #imm,Dn` still uses the ADD-with-immediate-EA
    encoding, and the dialect aliases `add/sub/cmp #imm,<mem>` to the I-form
    (this alias is what keeps the curriculum byte-identical — the split alone
    regresses it).
  - **Size-dependent EA validity:** the decoder rejects An as a byte operand
    (`MOVE.B a0,d0` is illegal); the BTST EA mask drops immediate (you can't
    test a bit in a literal); the MOVEM *store* mask drops postincrement (only
    predecrement is legal for reg→mem).
  - **MOVEM load extension order:** the register-mask word always follows the
    opcode, before any EA displacement — the renderer now reads it up front
    rather than in operand-display order.
  - **bit ops render sizeless** (vasm rejects `btst.b` on a register).
  - The audit assembles the reference with `vasm -no-opt`, so vasm's optimizer
    doesn't transform or delete instructions (it drops `lea (a0),a0` as a no-op).

### 68000 PC-relative EA — fixed

The disassembler now renders `(d16,PC)`/`(d8,PC,Xn)` as the resolved **target**
(`$T(pc)`, where `T` is the extension word's address plus the displacement),
matching how vasm reads `N(pc)` — it takes `T` as the target and re-derives the
displacement `T − PC`. The assembler was already target-aware for `label(pc)`;
it now treats a constant `n(pc)` the same way (vasm does), so both halves agree.

Because the target is position-dependent, these instructions are still excluded
from the *batched* sweep (the two-origin filter drops them, as it does branches —
batching needs position-independent text). They are covered instead by targeted
round-trip tests: a decode assertion in `isa-disasm` and a disasm→assemble→bytes
round-trip in `asm198x`, plus manual confirmation that the disassembly
re-assembles to identical bytes under both our assembler and vasm.

## Why this is the right next investment

Of the candidate "next level" directions — harden at scale, feed Emu198x, reach
real source — this one is **self-contained** (no cross-project coordination),
**protects every future change to every CPU**, and directly de-risks the manual
spec authoring that the 65816 work showed is the soft spot. It also makes the
other directions safer to build on.

## Operational notes

- Both tests are `#[ignore]`d (they need `acme`/`ca65`+`ld65`/`pasmo` installed)
  and degrade gracefully when a tool is absent — safe to run anywhere.
- The fuzzer is **seeded** (a fixed LCG seed): the corpus is a reproducible
  regression set, not nondeterministic. Bump or vary the seed to hunt new cases.

## Drift triggers

- **"The round-trip already proves correctness, skip the reference audit"** — no;
  the round-trip only proves self-consistency. It missed the `ASL A` bug. The
  reference must be the arbiter for spec data.
- **"Add a CPU spec without a conformance audit"** — for a form-based spec, add
  it to `spec_opcodes_match_reference`; for a non-form spec, add it to the
  `spec_sweep_matches_reference` sweep. Never leave a spec silently unaudited —
  if it can't land green yet (as with 68000), document the backlog explicitly.
- **"Make the fuzzer nondeterministic for more coverage"** — keep the committed
  seed fixed (reproducible regressions); vary it only in ad-hoc bug hunts.
