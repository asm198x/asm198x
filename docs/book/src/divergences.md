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
8 tracked differences across 27 recorded cases.

| Difference | CPU | Dialect | Reference tool | Cases |
|---|---|---|---|---|
| `cop-signature` | 65816 | ca65 | ca65 | 1 |
| [`issue-110`](https://github.com/asm198x/asm198x/issues/110) | 68000 | vasm | vasmm68k_mot | 4 |
| [`issue-984`](https://github.com/asm198x/asm198x/issues/984) | Z80N | pasmonext | pasmo | 1 |
| `m740-bit-branch-a` | M740 | ca65 | ca65 | 16 |
| `m740-inc-dec-a` | M740 | ca65 | ca65 | 2 |
| `m740-ldm-order` | M740 | ca65 | ca65 | 1 |
| `scrcode-cbm-expression` | 6502 | ca65 | ca65 | 1 |
| `sprintf-raw-flags` | 6502 | ca65 | ca65 | 1 |
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

**`m740-inc-dec-a`, `m740-ldm-order` and `m740-bit-branch-a` (ca65's M740) —
the 740 family manual decides.** Mitsubishi's and Renesas's *740 Family
Software Manual*, in both its 1997 and 2006 editions, disagrees with ca65
V2.19 in three places, and Asm198x follows the manual:

- `INC A` is `$3A` and `DEC A` is `$1A`, the reverse of the 65C02's codes,
  which ca65 uses;
- `LDM #$nn,$zz` is `$3C nn zz`, the value before the address, where ca65
  lays the address first;
- an accumulator bit branch, `BBSn a,target` or `BBCn a,target`, is two bytes
  and counts its displacement from the byte after it; ca65 counts from one
  byte further, so its branches land a byte early.

MAME's M740 core agrees with the manual on the first two. The manual is held
in the reference library's `by-topic/cpu-m740/`.

**`sprintf-raw-flags` (ca65) — what `.sprintf` converts stays raw.** ca65
keeps, for each character of a string, whether the source wrote it, and its
character map translates only those. What a `.sprintf` conversion writes is
raw, so `.charmap` leaves it alone. But ca65 V2.19 keeps those flags from the
first `.sprintf` on a line: a second one's conversions are translated, so
`.byte .sprintf("%c", $41), .sprintf("%d", 1)` maps the `1` while
`.byte .sprintf("%d", 1)` on its own does not. Asm198x leaves every
conversion raw, wherever it is on the line.

**`cop-signature` (ca65's 65816) — `COP` takes its signature byte.** ca65
V2.19 writes a lone `$02` for `COP` with no operand. The 65816 reads the byte
after `COP` as its signature whatever it is, so that `COP` runs into the next
instruction. Asm198x refuses it and asks for the signature.

**`scrcode-cbm-expression` (ca65) — a screen code is the value's.** The
`cbm` macro package's `scrcode` turns each argument into a C64 screen code.
For a single number, character or name, ca65 V2.19 and Asm198x agree on all
256 values. For some expressions, ca65's own package does not: `scrcode 1+1`
writes `$02`, though `scrcode 2` writes `$82`, and `scrcode $41+0` is a range
error, though `scrcode $41` writes `$01`. Asm198x's packages are written for
it rather than copied from cc65, and they work out the screen code from the
value of the whole expression.

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
