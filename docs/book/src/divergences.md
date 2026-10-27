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
16 tracked differences across 32 recorded cases.

| Difference | CPU | Dialect | Reference tool | Cases |
|---|---|---|---|---|
| [`issue-110`](https://github.com/asm198x/asm198x/issues/110) | 68000 | vasm | vasmm68k_mot | 4 |
| [`issue-761`](https://github.com/asm198x/asm198x/issues/761) | 6502 | ca65 | ca65 | 1 |
| [`issue-798`](https://github.com/asm198x/asm198x/issues/798) | 6502 | ca65 | ca65 | 1 |
| [`issue-820`](https://github.com/asm198x/asm198x/issues/820) | 65816 | ca65-816 | ca65 | 1 |
| [`issue-844`](https://github.com/asm198x/asm198x/issues/844) | 6502 | ca65 | ca65 | 1 |
| [`issue-850`](https://github.com/asm198x/asm198x/issues/850) | 6502 | ca65 | ca65 | 1 |
| [`issue-866`](https://github.com/asm198x/asm198x/issues/866) | 6502 | ca65 | ca65 | 2 |
| [`issue-868`](https://github.com/asm198x/asm198x/issues/868) | 6502 | ca65 | ca65 | 1 |
| [`issue-872`](https://github.com/asm198x/asm198x/issues/872) | 6502 | ca65 | ca65 | 1 |
| [`issue-876`](https://github.com/asm198x/asm198x/issues/876) | 6502 | ca65 | ca65 | 1 |
| [`issue-891`](https://github.com/asm198x/asm198x/issues/891) | 6502 | ca65 | ca65 | 1 |
| [`issue-895`](https://github.com/asm198x/asm198x/issues/895) | 65816 | ca65-816 | ca65 | 1 |
| [`issue-918`](https://github.com/asm198x/asm198x/issues/918) | 6502 | ca65 | ca65 | 1 |
| [`issue-928`](https://github.com/asm198x/asm198x/issues/928) | 65816 | ca65-816 | ca65 | 1 |
| [`issue-940`](https://github.com/asm198x/asm198x/issues/940) | 6502 | ca65 | ca65 | 13 |
| [`issue-984`](https://github.com/asm198x/asm198x/issues/984) | Z80N | pasmonext | pasmo | 1 |
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

**`issue-761` to `issue-931` (ca65) — source ca65 V2.19 accepts and our ca65
readers refuse or assemble differently.** Each is one of the open ca65 issues,
recorded as a probe with ca65's own answer under a flat ld65 link: `.set`,
labels on closing directives,
`.sizeof`, qualified label definitions, generated register-spelt names,
`.left` on a quoted list, macro arguments holding a function's comma, and source
after `.end`; and, in the 65816 reader, `.if`, `.define` and `.const` inside a
captured list. The old readers take no parity fixes while the new ca65 front end
(`lang198x/ca65`) is built; it closes these. Each probe fails once it passes.

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
