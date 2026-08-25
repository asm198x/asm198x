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
| 1802, 2650, 6502, 6800, 65816, 8039, 8048, 8080, F8, SC/MP, TMS7000, Z80, HuC6280, SM83 | **6809, CP-1610, PDP-11, TMS9900, Z8000, Z8001, 68000** — all seven done as of 2026-08-25 |

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
| 68000 | `Form`, but field-based | one per `Form` per allowed address mode — 838 |

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

  > **Settled 2026-08-25 — both, on different axes.** The effective-address
  > mask takes the **product**: each mode differs in length, in extension
  > words, and in what a reference can get wrong, so each is its own row. Size
  > takes a **representative**: `.b`/`.w`/`.l` ride one uniform two-bit field,
  > the same way a register number rides a uniform three-bit field, and
  > expanding it would measure the CPU's operand space rather than the spec's
  > own distinctions. That is the line
  > `a_word_cpu_declares_one_row_per_entry` already draws for the word CPUs,
  > applied to a field-based spec.
  >
  > The result is 838 rows, all 838 arbitrated against `vasm`. The audit found
  > two encoding faults in the assembler that the opcode sweep could not: the
  > `MOVEM` register-list mask emitted after the effective address's extension
  > words instead of directly after the opcode word, and a `d8(PC,Xn)`
  > displacement stored as its target instead of relative to the extension
  > word. Both are PC-relative or ordering faults, and the sweep drops every
  > PC-relative instruction by construction — it keeps only what disassembles
  > identically at two origins. The two mechanisms are complementary here in
  > exactly the way the table below claims.
- **Where the enumeration lives.** A trait in `isa`, or a free function per
  module. `isa` is dependency-free and `&'static`, and that must survive.

  > **Amended 2026-08-25 — dependency-free survives; `&'static` does not, for
  > one field.** `Row::mode` is now `Cow<'static, str>`. Twelve specs author
  > their mode label as a literal beside the encoding and still borrow it; the
  > 68000 authors no label at all, only a bitmask of allowed addressing modes
  > and a slot list, so the thing that tells `(An)+,Dn` from `Dn,(An)` exists
  > only once the two are combined. A `&'static str` could hold that only by
  > leaking it or by authoring a second copy of the spec by hand — and the
  > second copy is what the derived-row rule exists to prevent.
  >
  > What the constraint was *for* is intact, and that is the part that was
  > never stylistic: `Cow` is `std`, so `isa` stays dependency-free; nothing
  > here reaches into `asm198x`; and a consumer reading a spec still compiles
  > no assembler and allocates nothing, because a borrowed label allocates
  > nothing and `Row` is only built when something asks for rows. `Row` loses
  > `Copy` and keeps `Clone`. The whole change cost twelve mechanical edits
  > across seven files, none of them outside this crate and its own tests.

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

**Corrected 2026-08-24, after reading how the audit works.** This section first
said the audit would let "a missing or wrong row be caught rather than noticed".
The *missing* half is wrong, and it mattered enough to fix rather than soften: a
spec-driven audit walks what the spec declares, so a row nobody declared is
exactly as invisible to it as it was to the metric. What found
[#225](https://github.com/asm198x/asm198x/issues/225) was `xtask surface`, which
asks the **reference** for its vocabulary instead of asking us. The two
mechanisms are complementary and are not substitutes:

| question | answered by |
|---|---|
| does the reference have a word we do not? | `xtask surface` |
| has every row we declare been put to a reference? | the form audit |
| do our bytes match over byte space? | the opcode sweeps |

So, accurately:

- **Six CPUs gain a *scored* measure that every declared row has been
  arbitrated.** They have sweep verdicts today, which do put bytes to the real
  assembler — but over byte space, a set with no denominator that ought to
  exist, which is why they are counted and not scored. What is new is that a
  row we declare and never check becomes visible, and the number can fall when
  someone adds a row without arbitrating it.
- The coverage metric stops having a blind spot it cannot report on. That is a
  narrower claim than the one this section made first, and it is the true one.
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
