# Plan: vasm's default optimisations, applied exactly

**Status:** Proposed and built 2026-11-06. See "Outcome" below.
**Binding records:** `decisions/reference-parity-goal.md` (match the
reference, don't improve on it), `decisions/syntax-stance.md`,
`decisions/ast-native-payload-for-multipass-cisc.md` (vasm keeps its
statements un-lowered across passes), `decisions/verdicts-track-current-tools.md`.
**Issues:** #969 (four Amiga pattern sources, and 335 more at code-samples
`d33e74f3`, differ from `vasmm68k_mot`), #110 (the `-no-opt` divergences).

## The problem

Asm198x's vasm dialect applies some of vasm's default optimisations and not
others, so its bytes sit between `vasmm68k_mot` and `vasmm68k_mot -no-opt`.
At code-samples `d33e74f3`, 335 of the 442 Amiga pattern sources build to a
different hunk executable from vasm 2.0f (M68k backend 2.8c), the version
`ops/arbiter/identities` pins. `-showopt` over those sources lists what vasm
does that we do not: `jmp`/`jsr` to `bra`/`bsr` (3,100), a `jmp` to the next
instruction deleted (2,096), branches shortened to `.b` that we leave `.w`,
`eori #-1` to `not`, `abs.l` to `abs.w`, and branches to the next instruction
deleted. We also do one thing vasm does not: rewrite `jmp label` as
`jmp label(pc)`, and we optimise immediates that hold a relocatable label
(`add.l #label,d0` becomes `addq`).

## What vasm does

Read from vasm 2.0f's `doc/cpu_m68k.texi` (the documented default set) and
`cpus/m68k/cpu.c` (`optimize_oper`, `optimize_instruction`,
`instruction_size`) and `vasm.c` (`resolve_section`), then measured with the
2.0f binary built from the arbiter's tarball. For the 68000, with no
`-opt-*` option, these are on: `opt_gen`, `opt_disp`, `opt_abs`, `opt_pc`,
`opt_bra`, `opt_moveq`, `opt_quick`, `opt_branop`, `opt_lea`, `opt_lquick`,
`opt_immaddr`. `-no-opt` clears every one of them.

**Passes.** Each instruction is sized as it is parsed, with forward
references undefined (pass 0). Then each section is re-walked until no label
moves and no size changes; a label takes its new value in place, so a
backward reference sees this pass's value and a forward one the last pass's.
Every pass optimises a fresh copy of the source instruction, and branch and
`jmp` decisions also read the size the instruction had last pass.

**Values.** An operand is *absolute* when its value does not depend on where
a section loads: a number, an `equ` of one, a label in an `org` section, or a
difference of two labels in one section. A label in a relocatable section is
not absolute. A symbol not yet defined is neither absolute nor
PC-relative-able. A flat binary's code before any `section` lives in an
implicit `org 0` section, so its labels are absolute; a hunk executable's
does not.

**Operands** (`optimize_oper`), each unless the size was written:

- `(0,An)` becomes `(An)`.
- An absolute `abs.l` that fits signed 16 bits becomes `abs.w`.
- Otherwise a same-section operand (in an `org` section, any absolute value)
  becomes `(d16,PC)` when the operand allows it and the displacement from
  the opcode word fits 16 bits.

**Instructions** (stage 2 of `optimize_instruction`; one rule per
instruction, then the operands are optimised again):

- `move.l #x,Dn`, absolute `x`: `moveq` for -128..127; `moveq #x>>1` +
  `add.w Dn,Dn` for even 128..254 and -256..-130; `moveq #x>>16` + `swap`
  for `$10000`..`$7f0000` with a zero low word, or `$ff80ffff`..`$fffeffff`
  with `$ffff`.
- `movea #0,An` becomes `suba.l An,An`; `movea.l #label,An` becomes
  `lea label,An` (and so, usually, `lea label(pc),An`).
- `movem` with no registers is deleted; loading one address register becomes
  `move`; loading two address registers from `(An)+` becomes two `move`s.
- `clr.l Dn` becomes `moveq #0,Dn`.
- `and`/`andi` of `$ff`/`$ffff`/`$ffffffff` at that size becomes `tst`.
- `or`/`ori`/`eori #0` becomes `tst`; `eori #-1` becomes `not`.
- `add`/`sub` of 1..8 becomes `addq`/`subq`; `adda`/`suba #0` is deleted,
  `adda #-1..-8` becomes `subq`, and a 16-bit `adda`/`suba` becomes `lea`.
- `lea (An),An` and `lea 0(An),An` are deleted; `lea d(An),An` with
  -8..8 becomes `addq`/`subq`; `lea 0,An` becomes `suba.l An,An`; a
  displacement past 16 bits becomes `movea.l` + `adda.l` (warning 2048).
- `cmp`/`cmpi #0` becomes `tst`, never for an address register on a 68000.
- `asl #1,Dn` becomes `add Dn,Dn`.
- `jmp`/`jsr` to a same-section label within 16 bits becomes `bra`/`bsr`,
  sized by distance and last size; a `jmp` to the next instruction is
  deleted, and stays deleted.
- An unsized `bra`/`bsr`/`bcc` is sized from its last size: `.w` first, then
  `.b` when the target is within -128..128 (measured from the word form), `.w`
  when not, deleted when it is a `bra`/`bcc` to the next instruction, and past
  16 bits `bra`/`bsr` becomes `jmp`/`jsr` and `bcc` becomes `b!cc.b *+8` +
  `jmp` (warning 2046).
- A written `.s`/`.b` branch to the next instruction becomes `lea (a6),a6`
  (`$4dd6`, warning 2058).
- `<op>.l #x,An` with a 16-bit absolute `x` becomes `<op>.w` (`movea`,
  `cmpa`).

Form choice that is not an optimisation stays on with `-no-opt`: `add`,
`sub`, `cmp`, `and`, `or` and `eor` of an immediate into memory are the
immediate instructions.

## The change

Optimisation moves out of the encoder into one function that does what
`optimize_instruction` does. It takes a source instruction and returns the
concrete instructions to encode (none, one or two), each with its operand
modes settled. Sizing and encoding both read that list, so they cannot
disagree. The relaxation loop becomes vasm's pass loop.

## Steps

Each step is one commit, and each passes `cargo test --workspace` before the
next starts.

1. **Pair `-no-opt` verdicts with a `-no-opt` assembly.** Every vasm form,
   sweep, fuzz and probe verdict is recorded from `vasmm68k_mot -Fbin
   -no-opt`, but replay checks it against our optimising assembler. Add a
   hidden `assemble_vasm_no_opt`, and use it in `assemble_form`,
   `assemble_probe` and the fuzz's canonical check. Make the `add`/`and`/…
   immediate-to-memory form choice independent of optimisation. Close the
   four `issue-110` divergences (`cargo xtask close-divergence issue-110`) and
   rewrite `divergences.md`'s paragraph. Files:
   `crates/asm198x/src/lib.rs`, `crates/asm198x/src/dialects/vasm/mod.rs`,
   `crates/asm198x/tests/support/verdicts.rs`,
   `crates/asm198x/tests/conformance.rs`,
   `crates/asm198x/tests/verdicts/68000.ndjson`, `docs/book/src/divergences.md`.
   Verified by `verdict_replay` (fails before: two `-no-opt` probes rejected,
   four divergences that now match) and `cargo xtask docs --check`.
2. **Probe dialects for vasm's default invocation.** `vasm-bin` (`-Fbin`) and
   `vasm-exe` (`-Fhunkexe -kick1hunks -nosym`) in `differential.rs`, replayed
   against `assemble_vasm` and `assemble_vasm_exe`. Files:
   `crates/asm198x/tests/differential.rs`,
   `crates/asm198x/tests/support/verdicts.rs`. Verified by recording the
   existing issue table's cases against vasm 2.0f and seeing replay fail on
   them (the check can go red), before any fix.
3. **Values that know their base, and vasm's passes.** A vasm symbol table
   (labels in place, `equ`s evaluated when read), absolute/relative/undefined
   classes, the implicit `org 0` section in flat output, pass 0 at parse
   order, per-section passes, and a per-instruction last size. Optimisations
   require absolute values. Files: `crates/asm198x/src/dialects/vasm/mod.rs`,
   new `crates/asm198x/src/dialects/vasm/resolve.rs`. Verified by unit tests
   on the classes and on forward `equ`s, and by replay and the curriculum
   staying green.
4. **Concrete instructions.** `optimize.rs` turns a source instruction into
   encodable ones; `encode` and the sizer read only those. Move today's
   rewrites there unchanged. Verified by replay and curriculum unchanged.
5. **The documented default set**, rule by rule as listed above: operand
   rules, then instruction rules, then `jmp`/`jsr`, then branch sizing and
   deletion, then the out-of-range translations. Each rule gets probes at its
   boundaries in both `vasm-bin` and `vasm-exe` (and relocatable against
   absolute operands in hunk output), recorded against vasm 2.0f. Verified
   by replay going from red to green for each rule's probes.
6. **The four #969 sources and the 335.** A live test builds the four named
   sources from a code-samples checkout with vasm and with Asm198x and
   compares. Then measure all 335 at `d33e74f3` with vasm 2.0f, and list what
   still differs and why.
7. **Docs.** Module documentation and `cargo xtask docs --check`.

## Verification

- Probes for each rule and boundary: `$7fff`/`$8000`/`$ffff8000`/`$ffff7fff`
  for `abs.w`, PC displacement limits, branch distances -128/-130 backward and
  126/128/130 forward, forward and backward, `bra` against `bsr` against
  `bcc` for deletion, a branch next to an instruction against one two bytes
  away, `jmp` -128/127/128, relocatable against absolute operands in hunk
  output, forward `equ`s.
- Recorded with vasm 2.0f built from the arbiter's tarball (sha256
  `c84b2de1…`), reporting the identity `ops/arbiter/identities` lists, with
  no other reference tool on `PATH`, so only vasm's verdicts are rewritten.
- Replay, the curriculum's 106 vasm digests (with code-samples at the pin),
  `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace`, `cargo xtask docs --check`.

## Out of scope

- Optimisations behind `-opt-*` options or for other CPUs (68010+ `clr`,
  `-opt-speed`, ColdFire, Apollo). The dialect refuses `opt` by name and has
  no command-line switch for them, so nothing can turn them on.
- Syntax the dialect does not parse yet: a written displacement size
  (`0.w(a0)`), and the one-operand register shift (`asl d0`).

## Outcome

Built as planned, with steps 3 to 5 in one commit: vasm's pass loop changes
how branches size, so the symbol table, the concrete-instruction encoder and
the rules could not land apart without a commit whose bytes matched neither
the old behaviour nor vasm's.

- **The 62 probes** were recorded against vasm 2.0f built from the arbiter's
  tarball. 44 differed before the change and none after.
- **The Amiga pattern library at code-samples `d33e74f3`:** all 442 sources
  build to vasm 2.0f's hunk executable, the 335 listed in #969 among them.
  103 did before.
- **The curriculum's 106 vasm digests and every `-no-opt` verdict** replay
  unchanged.
- **A label before any `section`** opens vasm's default section: `org 0` in a
  flat binary, an empty code hunk in an executable. Found by the probes, and
  fixed with the rest because it decides whether a label is absolute.
- **`ds` with a forward count** now sizes as vasm's first pass does, instead of
  being refused, because the pass loop reads symbols the way vasm does.

Two approximations, neither reachable by ordinary source:

- vasm re-resolves only the sections that read a moved section's labels; we
  walk every section again until all of them settle at their first walk. A
  settled section is a fixed point, so the extra walks change nothing.
- vasm leaves `RESOLVE_WARN` set for the rest of a section's final pass once
  one instruction has changed size more than five times after the first 200
  passes; we set it per instruction.

vasm prints the "branch out of range changed to jmp" warning once where several
branches are translated; we warn for each. The bytes are the same.
