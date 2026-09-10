# Decision: computed instructions resolve timing after emission

**Status:** Active.

**Date:** 2026-09-10.

The flat engine distinguishes `ComputedInstruction` from `Encoded` data.
Both emit pieces through the same encoder, but only an instruction asks its
dialect for timing from the final operand bytes. The semantic AST preserves
that distinction. The lwasm direct-page selection becomes a computed
instruction after pass one chooses its opcode and width.

This keeps data such as lwasm `fqb` out of instruction accounting, while
letting indexed postbytes, register masks, and resolved addressing modes
select shared Isa198x metadata. The engine owns capture and source attribution;
the dialect identifies the instruction; Isa198x owns the hardware facts.
A missing timing result makes coverage partial and prevents cycle budgets
from treating the captured subset as a complete account.

`CycleRec` gains optional, authoritative `bounds` for computed instructions:
minimum plus optional maximum. An absent maximum means a documented
unbounded wait; missing metadata instead produces no record and partial
coverage. RTI has a finite runtime-dependent range, which must not be
misrepresented as a page-cross or branch-taken penalty.

Existing form records omit `bounds` and retain their JSON shape.
`CycleRec::range()` resolves either shape. New JSON consumers must honor
`bounds` when present. Human listings show unbounded costs as `>=min`, JSON
listings use `max: null`, and cycle ceilings refuse the affected label.
Coverage describes completeness of metadata, not whether execution finishes.

This extends the public draft under `core-contract-freeze.md`; it does not
change the Debug198x sidecar. `CycleRec` and `CycleBounds` are non-exhaustive
output records. Rust callers constructing the formerly exhaustive `CycleRec`
with a literal must migrate; deserialization of older records is preserved.
