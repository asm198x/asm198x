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

All four steps landed, and both Z8000 CPUs finished at **271 of 271 rows
arbitrated** — nothing unplaced, nothing excepted, and no row diverging in
bytes. It took five passes to get there, and every pass but the first raised
the number by placing a family rather than by fixing an encoding.

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

**The unplaced rows are unplaced for a stated reason**, and the reason has to be
true. Shifts and rotates were held back on the grounds that no entry stated the
count's legal range. That was wrong — `shift_max` had stated it all along, and
a count of 1 is legal at every size. They are placed now, and the lesson is
that "the spec does not say" is a claim to check in the spec, not a feeling
about it.

The audit also did what it was built to do. `LDB` with a byte immediate was the
one genuine byte divergence in 271 rows — we emit the long dyadic form where
`asl` emits the short one-word form, which we cannot decode either
([asm198x#252](https://github.com/asm198x/asm198x/issues/252)). It is an
explicitly named exception in the audit, not part of the anonymous unplaced
count, and the entry comes out when the row arbitrates.

## Filler is part of the exemplar

The single most common failure was never a wrong opcode word. It was **operand
words that were not a legal instance of the form**, which stop the row
disassembling back to itself and so look exactly like an encoding error:

- a byte immediate must be its byte *replicated* into a word;
- a segmented memory operand must carry a long-form segment word, which `asl`
  writes even where the short form would fit;
- a shift's count travels in a following word, and filler is not a count.

Each cost a round of measurement to find and one line to fix. The rule that
came out of it: **an exemplar is the whole instruction, not its first word.**
Where a family has operand words, the family states them.

## A sibling row can answer for the wrong one

The store forms are the sharpest lesson here. They share a mnemonic with the
loads and differ only in a `store` flag, so a lookup by name alone hands a
store row the *load's* encoding. That still names the right mnemonic, so
`names_row` passes and the audit records a verdict — for the wrong instruction.

The non-segmented CPU read 271 of 271 while doing exactly that. The Z8001
caught it, because its segmented encodings differ enough that the wrong entry
stopped decoding. **An audit that matches on the name alone can be green and
wrong**, and only a second CPU sharing the spec made the difference visible.

## Shifts and rotates

The two kinds carry their count differently, which is why `Shift::exemplar`
returns a pair — the opcode word, and the operand word only one of them has:

- A shift puts a *signed* count in a following word, negative for the right-hand
  variants and confined to the low byte when the operand is a byte
  (`srlb rh1,#1` is `B211 00FF`, `srll rr2,#1` is `B325 FFFF`).
- A rotate has no following word. The count folds into the low nibble as
  `type * 4 + (count - 1) * 2`, which is why `sel` means the rotate *type*
  there and the nibble itself for a shift. Only counts 1 and 2 exist.

Reading `sel` as one thing for both kinds is what made the rotates look broken
on the first Z8000 run: `RL` happens to have type 0 and so came out right by
coincidence, while `RR`, `RLC` and `RRC` did not. A field whose meaning depends
on a sibling field is worth a doc comment on both.

## The segmented Z8001

The Z8001 runs the same audit over the same rows, and reaches the same 125. It
gets its own test rather than sharing the Z8002's, because "the same spec" is a
claim worth checking: the two CPUs differ in how they encode memory, and an
audit that assumed they agreed could not have told us they do.

Three things differ, and all three were already known somewhere in the tree —
the exemplar was the last place to learn them:

- An `@Rn` pointer is a register **pair**, so its field must be even.
- `LDA` loads a 32-bit segmented address, so its destination is a pair too.
  The encoder and decoder both already promoted `Size::Address` to
  `Size::Long` here; the exemplar now does the same.
- A memory operand carries a long-form segment word — bit 15 set, segment in
  bits 14-8, low byte zero. `asl` writes that form even for an offset small
  enough for the short one, so the audit's filler does too.

The last of those is the same lesson as the byte immediate: **filler must be a
legal instance of the form, not just a distinguishable one**, and a row whose
operand words are illegal does not disassemble back to itself. Every failure the Z8001
audit reported on its first run was filler, not spec.
