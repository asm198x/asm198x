# Decision: every ISA spec enumerates its encoding rows, whatever shape it authors them in

**Status:** Active. Binding for Asm198x (accepted 2026-08-24). Gives
[`spec-conformance-and-fuzzing.md`](spec-conformance-and-fuzzing.md) a
denominator on the six CPUs that have none, and gives
[#233](https://github.com/asm198x/asm198x/issues/233) somewhere to put a
per-row marker.

**Date:** 2026-08-24.

## The problem, stated as what it costs

**Six CPUs have no form audit at all.** Not "no percentage" — no per-form
arbitration against a reference tool:

| audited | not audited |
|---|---|
| 1802, 2650, 6502, 6800, 65816, 8039, 8048, 8080, F8, SC/MP, TMS7000, Z80, HuC6280, SM83 | **6809, CP-1610, PDP-11, TMS9900, Z8000, Z8001, 68000** |

`xtask coverage` derives its denominator as
`set.instructions.iter().map(|i| i.forms.len()).sum()`, and the audit iterates
the same forms to build one source line per form and put it to the reference.
Both need [`Form`](../crates/isa/src/lib.rs). A spec that does not use `Form`
gets neither, and appears under *counted, not scored* with sweep, fuzz and
probe verdicts only.

Those other suites are real but they are chosen sets, not enumerations. A
sweep walks byte space and round-trips what disassembles; nothing walks *what
the spec claims* and checks each claim.

**[#225](https://github.com/asm198x/asm198x/issues/225) is what that costs.**
Nine core 6809 instructions were absent — `adca`, `bita`, `cmpd` among them —
and no metric could fall, because a mnemonic the spec does not declare is
missing from its own denominator. It was found by `xtask surface`, which asks
the *reference* for its vocabulary, and only because someone ran it.

## The decision

**Every spec exposes a derived enumeration of its encoding rows.** One row per
distinct encoding the spec declares, carrying at least a mnemonic, a mode
label, and room for per-row metadata such as `undocumented`.

Three things this is not:

1. **Not a rewrite of the authored data.** Each spec keeps the shape that suits
   its CPU. The enumeration is *computed* from it, so there is no second copy
   to drift out of step.
2. **Not `Form` everywhere.** `Form` describes fixed opcode bytes plus operand
   slots. It does not fit a computed postbyte (6809 indexed), a field-packed
   opcode word (CP-1610, PDP-11, TMS9900, Z8000) or an effective-address field
   (68000), which is why those specs are shaped as they are.
3. **Not a new authored artefact.** Nothing new to keep current by hand.

## Why the shape is already there

The docs generator has been doing this for a year in miniature. `Class::encoding()`
and `Class::describe()` live *beside* each spec so the generator does not
restate them, and `xtask/src/instructions.rs` already flattens three specs into
one `WordRow` type to render them together.

And the bespoke specs are less bespoke than their count suggests:

| spec | authored shape | rows it implies |
|---|---|---|
| CP-1610, PDP-11, TMS9900 | `Insn { mnemonic, base: u16, class: Class, summary }` — **structurally identical to each other** | one per entry; the class names the field layout |
| Z8000 | as above plus `modes: u8`, an explicit **bitmask of addressing modes** | one per set bit |
| 6809 | `Insn { mnemonic, kind: Kind }`, where `Kind::Mem` holds four mode slots and `Kind::Branch` two | one per non-empty slot |
| 68000 | `Form`, but field-based | one per `Form`, ~163 today |

Indicative entry counts, for scale rather than precision: 6809 ~121, PDP-11
~97, TMS9900 ~70, CP-1610 ~31, Z8000 ~29. These are small tables. The work is
in the seam, not the volume.

## What has to be settled to do it

Recorded as open rather than guessed, because each one could change the shape:

- **What "mode" means per spec.** For CP-1610 and its two structural twins the
  class *is* the mode. For the 6809 it is the slot. For the Z8000 it is the
  bitmask bit. These want one vocabulary, and it should be the one the dialects
  already use to look forms up.
- **Whether a row can be rendered as source.** The audit's value is that it
  puts each declared row to the real assembler. That needs a canonical operand
  per row — trivial for `Inherent`, a choice for a field-packed word.
- **Whether the 68000 enumerates finitely.** Its 163 `Form` rows do; whether an
  audit over them is meaningful when the effective-address field multiplies each
  one is a separate question, and it may want representatives rather than a
  product.
- **Where the enumeration lives.** A trait in `isa`, or a free function per
  module. `isa` is dependency-free and `&'static`, and that must survive.

  Two constraints follow from that, and they are not stylistic. `isa` is
  consumed by Emu198x over a git dependency, and its promotion to the reserved
  `isa198x` org is a live question governed by
  [`rung1-wiring.md`](../../../decisions/rung1-wiring.md). So the seam must not
  reach for anything in `asm198x` — no `AsmError`, no engine types, no dialect
  vocabulary — and must not require a consumer to compile the assembler to read
  a spec. A seam that couples them would make the extraction a rewrite instead
  of a move, which is the same trap `multi-artifact-output.md` names for
  container formats and Format198x.

## What this is worth

- Six CPUs gain a form audit, so a missing or wrong row can be *caught* rather
  than noticed.
- The coverage metric stops having a blind spot it cannot report on, which is
  the failure `xtask surface` exists to cover one level up — and it is the same
  failure, one layer down.
- [#233](https://github.com/asm198x/asm198x/issues/233) gets a home for
  `undocumented` on the 6809, matching how the Z80 already carries its eight
  marked forms. Three unmarked rows would be worse than none: a row that looks
  datasheet-backed and is not is the spec overstating what it knows.

## Drift triggers

- **"Just add the three opcodes and mark them later"** — later is what produced
  a spec whose gaps only an external tool can see.
- **"Rewrite the bespoke specs as `Form`"** — `Form` cannot express a computed
  postbyte or a packed opcode word. That is why they exist.
- **"The sweeps already cover those CPUs"** — a sweep walks byte space and
  round-trips what decodes. It cannot notice a row the spec never declared.
- **"Give it an invented denominator so every CPU reports a percentage"** —
  `coverage.rs` says why not, and it is right: a percentage over an invented
  total is worse than no percentage.
