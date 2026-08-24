# Decision: the IR models what a directive means, not what it is spelled

**Status:** Active. Binding for Asm198x (accepted 2026-08-24). A working rule for the
shared IR, under [`assemble-io-model.md`](assemble-io-model.md) principle 2 (one
internal representation) and [`syntax-stance.md`](syntax-stance.md).

**Date:** 2026-08-24.

## The decision

Two dialects spelling a directive the same way share an `Operation` **only when
the semantics coincide**. Where they differ, each gets its own operation and the
shared spelling is a coincidence of vocabulary, not a shared behaviour.

Approximating one with the other is never the answer, however close they look.

## The case that produced it

`align` is one word across all six references and at least four behaviours:

| reference | `align 4` means |
|---|---|
| ca65, lwasm | pad to a boundary of **4**, and the boundary need not be a power of two |
| vasm | pad to a boundary of **2⁴**, because the operand is an exponent |
| sjasmplus | pad to a boundary of 4, and refuse anything not a power of two |
| ACME (`!align`) | advance to the next address matching a **bit mask**, not a boundary |
| rgbasm | **assert** the section is already aligned; emit nothing, error if not |

The engine already had ACME's, as `Operation::Align { andmask, value, fill }`.
Folding the boundary form into it would have meant treating a modulus as a
mask, which is exact only while every boundary is a power of two — and both
ca65 and lwasm pad to boundaries that are not: `.align 3` after a byte puts the
next item at offset 3, which no mask can express.

So `Operation::AlignTo { modulus, fill }` sits beside `Align` rather than
replacing it, and rgbasm's is not an alignment operation at all — it is a
constraint on a section, and it belongs with sections.

## Why the rule is worth stating

The temptation runs the other way. Two operations named `Align` and `AlignTo`
look like duplication, and the obvious tidy-up is to merge them behind whichever
one is more general. That merge is silently wrong on exactly the source that
distinguishes the two — which is the source the project exists to assemble
unchanged.

The same shape is waiting in `org`, `section`, `struct`, `page` and `assert`,
all of which are in the remaining vocabulary gap with divergent meanings across
references.

## The test

Before giving a word an existing operation, assemble it with **both**
references and compare bytes. Where they agree the operation is shared; where
they differ it is two operations. The manual's wording is not evidence: vasm's
exponent, ca65's segment-relative base and ca65's label-binds-before-the-pad
were each found by running the tool, and each contradicts the obvious reading.

## Drift triggers

Re-read this record when any of these appear:

- "these are both align / both org / both section, merge them"
- an operation gaining a flag or mode to cover a second dialect's meaning
- a shared operation whose doc comment has to explain two behaviours
- reasoning about a directive's semantics from its name or its manual entry
  rather than from the assembled bytes
