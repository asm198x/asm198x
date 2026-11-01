# Where we differ

Every differential suite assembles the same source with a real assembler and
compares the bytes. Where they differ and the difference is known, it is
recorded against a join id that ties the recorded fact to the expectation of our
own output, so neither half can quietly disappear. If a tracked difference
stops being a difference, the build fails and says so.

This is that list.

It is here because a claim of parity asks to be believed, and a list can be read
instead. If you are deciding whether to move a project that already builds, what
you need is not reassurance — it is to know what will change.

<!-- generated: xtask divergences --markdown -->
3 tracked differences across 9 recorded cases.

| Difference | CPU | Dialect | Reference tool | Cases |
|---|---|---|---|---|
| [`issue-110`](https://github.com/asm198x/asm198x/issues/110) | 68000 | vasm | vasmm68k_mot | 4 |
| [`issue-984`](https://github.com/asm198x/asm198x/issues/984) | Z80N | pasmonext | pasmo | 1 |
| `sprintf-c-width` | 65816 | ca65-816 | ca65 | 4 |
<!-- /generated -->

## What each one is

**`issue-98` — `:` as a statement separator.** sjasmplus treats a colon as both
a label terminator and a statement separator. We read the label form and not the
separator form.

**`issue-99` — conditions on forward-referenced symbols.** sjasmplus resolves
`IF` against a label defined later in the file, which needs more passes than we
currently make.

**`issue-110` — 68000 optimisation, and this one is settled.** Our output sits
between vasm's two modes. vasm's default deletes `lea (a0),a0` entirely and
rewrites `asl.w #1,d0` as `add.w d0,d0`; `-no-opt` does neither. We apply some
and not others, so no single vasm invocation reproduces our bytes. The issue is
closed as completed — it records the decision rather than tracking a fix.

**`sprintf-c-width` (ca65) — `.sprintf` follows C.** ca65 V2.19 departs from
C's `printf` in a few places: `%x` of a negative number, `%#x` of 0, which side
`%s` and `%c` are padded on in a width, the `+` and space flags on an unsigned
conversion, and a width counted in bytes rather than characters. Lang198x's ca65
front end, which every ca65 dialect here reads through, follows C, and these
cases record ca65's own bytes beside ours.

**`issue-984` — PasmoNext's `PUSH nn` on the Next, a reference bug we fix.**
The Z80N's `PUSH nn` stores its immediate high byte first, as the Next's
documentation and sjasmplus agree. PasmoNext 0.1.3 writes it low byte first, so
its `push $1234` pushes `$3412` on real hardware. Our `pasmonext` emits the
correct bytes rather than copying the bug.

**`canonicalisation-68000-*` — three encodings with more than one spelling.**
The 68000 encodes some operations more than one way, so assembling, then
disassembling, then reassembling can land on the other spelling. Each is pinned
to the exact bytes it concerns.

## What leaves this list

A difference that stops being one. Macros used to be here: twelve recorded
cases across five dialects, where ACME, ca65, lwasm, vasm and sjasmplus all
expanded macros in ways we did not reproduce. They were implemented, their
verdicts became ordinary ones (`cargo xtask close-divergence`), and the rows
went. The corpus holds what is true now; git history holds what was.

## What is not here

Differences nobody has found. The list covers what the differential suites
compare, which is every instruction form the reference tools accept, the
curriculum, and the fuzz and probe corpora — not every program that could ever
be written.

A difference found later gets a join id and appears here. One that turns out to
be a plain bug gets fixed instead, and the recorded verdict goes with it.
