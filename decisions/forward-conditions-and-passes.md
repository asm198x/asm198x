# Decision: sjasmplus conditions resolve across three passes, and we do not converge further than it does

**Status:** Active. Binding for the sjasmplus front-end.

**Date:** 2026-08-23.

## The decision

**sjasmplus resolves a condition against a symbol defined later in the file, so
we do too — by running the walk the same three times, seeding each pass from
the last, and raising the same two advisories.** We do not iterate to a
fixpoint, and we do not refuse a program that fails to reach one.

This relaxes the parse-time-constant rule for conditions, and only for
conditions. `ds` and `incbin` arguments keep it.

## What the reference actually promises

[#99] framed this as the classic convergence problem: conditional assembly
changes how much code is emitted, which changes the addresses the conditions
were resolved against. It asked what the assembler promises when passes do not
converge.

Measured against sjasmplus 1.21.0, the answer is: **nothing.** It warns.

```asm
 IF later < 2
 ld a,1
 ENDIF
later: nop
```

```text
warning[fwdref]: forward reference of symbol: IF later < 2
warning: Label has different value in pass 3: previous value 0 not equal 2
bytes: 3E 01 00
```

Emitting the body moves `later` from 0 to 2, so the condition that admitted the
body is false by the end — and the body is in the binary. sjasmplus ships a
program its own source does not describe, says so twice, and exits zero.

## Why we reproduce that rather than improve on it

Three options were on the table, and the third was chosen deliberately.

**Converge-or-error** — iterate until the statement stream settles, refuse if it
does not — is a *better assembler* and a *worse* asm198x. Every program in the
gap between "sjasmplus accepts it" and "it converges" would assemble there and
fail here, which is the identity claim inverted. The README's rule is that real
source assembles unchanged, not that it assembles correctly.

**Refusing outright** was defensible while we had no way to warn, because
shipping that binary in silence is worse than refusing it. That was the real
blocker, and it is why this waited: the honest version of adoption needs the
warning, and there was no channel for one.

So the channel came first. `Dialect::parse_warned` is defaulted to "no
advisories", so no other dialect changed, and it is the thing that made full
adoption the right answer instead of the reckless one.

## Three passes, not a loop

The count is the reference's: sjasmplus prints "Pass 3 complete" on every file
it reads. Looping further would converge cases it does not, and emit different
bytes from it — which is the failure this record exists to prevent, arriving by
the back door.

A program whose first pass reaches no forward symbol stops there. The later
passes would produce the same statements, and every program that does not use
the feature would otherwise pay three times over.

## Backward references are not forward references

The walk binds each label to its address as it defines it, using the shared
[`engine::next_pc`] rule (see [`acme-zero-page.md`](acme-zero-page.md) for why
that rule is shared and not copied). So a condition below a label folds against
a value.

This is not an optimisation. Without it the walk cannot tell a backward
reference from a forward one — both are simply absent from the constant table —
and it would warn `forward reference of symbol` on source where the reference
says nothing. Matching the bytes was never the whole of matching.

## Drift triggers

- *"Loop until it converges, three is arbitrary."* → Three is the reference's.
  Converging further means emitting bytes it does not.
- *"Refuse a program that does not settle — we can, and it is wrong code."* →
  It is wrong code that sjasmplus builds. Refusing it is the identity claim
  inverted, and the warning is how the reader is told.
- *"Relax the parse-time-constant rule for `ds`/`incbin` too, for
  consistency."* → Consistency is not the argument; the reference is. Probe
  each, and if it resolves them across passes, adopt that with its own record.
- *"The forward path can answer everything, so drop the label binding."* → Then
  a backward reference warns, and the reference does not.

[#99]: https://github.com/asm198x/asm198x/issues/99
[`engine::next_pc`]: ../crates/asm198x/src/engine.rs
