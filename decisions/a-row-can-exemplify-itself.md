# Decision: a row that can name itself can show itself

**Status:** Active. Binding for Asm198x (accepted 2026-08-25). Completes
[`every-spec-enumerates-its-forms.md`](every-spec-enumerates-its-forms.md),
whose enumeration this gives a second half.

**Date:** 2026-08-25.

## The problem, from a failure

The Z8000 form audit was attempted and abandoned: 42 of 271 rows placed, 171
with no encoding at all, and 58 whose synthesised bytes `asl` refused. The
corpus was reverted before the branch was dropped.

It failed for one reason. **Every consumer that needs a representative encoding
of a row has been inventing one, outside the spec.** There are three such
inventions in the test file already:

| CPU family | how a representative encoding is found |
|---|---|
| the `Form` specs | `synth`, walking `Form::operands` |
| 6809 | `synth_6809`, matching on `Kind` |
| PDP-11, TMS9900, CP-1610 | search `base \| f` for `f` in `0..=0o77` |
| Z8000 | *nothing that worked* |

Each is a second copy of encoding knowledge that lives properly in two places
already — the spec, and the dialect that encodes for real. The Z8000 is where
that caught up with us, because it has thirteen encodings and no shared shape
to guess at.

## The decision

**A spec that enumerates its rows also exemplifies them.** For each row it
declares, a spec can produce one representative encoding: bytes that are a
valid instance of that row, chosen by the spec rather than by whoever is asking.

Three things this is not:

1. **Not an encoder.** It answers "show me one of these", not "encode this
   source". Arbitrary operands, expression folding and extension-word layout
   stay in the dialect, which is where a user's operands arrive.
2. **Not authored data.** Like the rows themselves it is computed from the
   tables beside it, so there is nothing new to keep current by hand.
3. **Not only for the Z8000.** It replaces the three existing synthesisers,
   which is most of its value: the audit stops carrying per-CPU knowledge it
   has no business holding.

## Why it belongs in `isa`

The house rule this follows is already written into three places in this
crate. `Class::encoding()` and `Class::describe()` live beside each spec
explicitly so the documentation generator does not restate them, and
`Class::name()` joined them in #240 so the generator could stop deriving a name
from a `Debug` derive. The reason given each time is the same: **a fact about
an instruction set belongs with the instruction set.**

"What does one of these look like" is such a fact. It is currently held by a
test.

## Constraints

- `isa` stays dependency-free, `&'static` where it can be, and reaches nothing
  outside itself — the constraint accepted with
  [`every-spec-enumerates-its-forms.md`](every-spec-enumerates-its-forms.md),
  because promoting the crate to the reserved `isa198x` org must stay a move
  rather than a rewrite.
- A spec that cannot exemplify a row says so, and the caller reports it.
  Silence would let an audit claim a row it never checked, which is the failure
  the enumeration work exists to end.

## Sequencing

The Z8000 is the reason and it is deliberately **last**:

1. The seam, and the `Form` specs, which already have `synth` to move.
2. The 6809, whose `synth_6809` moves beside `Kind`.
3. The three word CPUs, whose search becomes a stated field value.
4. The Z8000's thirteen families, where the knowledge is genuinely absent and
   has to be read from the assembler's `encode_*` and the family doc comments.

Each step deletes a synthesiser from the test file, so the change pays before
it reaches the hard part.

## Drift triggers

- **"Just fix the Z8000 synthesiser in the test"** — that is what failed, and
  the fourth copy would be the one that finally proved the pattern wrong.
- **"Move the whole encoder into `isa`"** — no. A representative instance is a
  spec fact; encoding a user's operands is a dialect's job, and the two have
  different inputs.
- **"The audit can search the opcode space instead"** — it can, and the Z8000
  attempt shows what that costs: a heuristic that reads `cpu` and `org` as
  mnemonics, and no way to tell one addressing mode from another.

## Outcome

All four steps landed. The Z8000 finished at **124 of 271 rows arbitrated**,
which is the number the sequencing above predicted would be partial and is
recorded here rather than rounded up.

Three things came out differently than planned.

**The knowledge came from the reference, not from our own encoder.** Step 4
said the thirteen families would be read from the dialect's `encode_*` and the
family doc comments. In practice every shape was settled by assembling a
representative instruction with `asl` and reading the opcode word back — the
same probing the project uses for directive behaviour. Reading our own encoder
would have reproduced our own mistakes; the reference cannot.

**Operand legality turned out to be one rule, not thirteen.** A long operand is
a register pair and needs an even number; a quad needs a multiple of four. Every
family already carries its `Size`, so `lowest_register` states the rule once and
each family's exemplar asks for it. That collapsed four families at a stroke.

**The unplaced rows are unplaced for a stated reason.** Shifts and rotates carry
their count in a *following* word whose legal range no entry states, so
`Shift::exemplar` returns `None` rather than a guess with filler as its count.
The segmented Z8001 was not attempted: its memory modes encode addresses
differently, so it remains at zero and shows as zero.

The audit also did what it was built to do. `LDB` with a byte immediate was the
one genuine byte divergence in 271 rows — we emit the long dyadic form where
`asl` emits the short one-word form, which we cannot decode either
([asm198x#252](https://github.com/asm198x/asm198x/issues/252)). It is an
explicitly named exception in the audit, not part of the anonymous unplaced
count, and the entry comes out when the row arbitrates.
