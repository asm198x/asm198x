# Decision: a source may name its output, but not where it lands

**Status:** Active. Binding for ACME's `!to` and `!symbollist`, and for any
later directive in any dialect that names a file the assembler writes.

**Date:** 2026-08-25.

## The decision

A directive that names an output file is a **request**, resolved on three
rules:

1. **The command line chooses.** `-o` and `--sym` win; the directive applies
   only when the corresponding flag is absent.
2. **The first name stands.** A second `!to` or `!symbollist` warns and is
   ignored.
3. **The name stays beside the source.** It is resolved relative to the input's
   directory, and refused if it is absolute or climbs out with `..`.

Rules 1 and 2 are ACME's own, probed against 0.97. Rule 3 is ours.

## Why 1 and 2 are ACME's

ACME takes the first name chosen and answers any later one with *"Output file
already chosen"* — and a `-o` counts as the first, because the command line is
read before the source. So a directive never overrides a flag, and a second
directive never displaces the first:

    $ acme -f plain -o flag.bin both.a
    Warning - both.a, line 2: Output file already chosen.
      flag.bin  written
      dir.bin   absent

The alternative considered was **adding** the named file alongside the flag's,
which is more useful and still byte-compatible — every file ACME writes we
would write identically, and write one more. It was rejected because
this project's claim is that real source behaves the same here as there, and
"the same, plus a file you did not ask for" is a different claim. A tool that
matches the reference is worth more than a tool that improves on it in ways no
source can ask for.

## Why 3 is ours

ACME takes the name as given: `!to "../../x"` writes two directories up, and
an absolute path writes wherever it says. That is reasonable for an assembler
run over source you wrote yourself, and unreasonable for one run over source
you did not — a curriculum toolchain assembles submitted files, and "assemble
this" should not mean "write anywhere on this disk".

So the name is confined to the input's own directory. This **narrows** what is
accepted: a source ACME assembles is refused here. That is a real
source-compatibility gap, taken deliberately, and it is the only one in this
decision. It is narrow — a relative name beside the source, which is what the
directive is for, still works — and the refusal says what it refused and why.

## What is not decided here

The **format** of a symbol list. `!symbollist` chooses where the file goes;
`--sym`'s existing format decides what is in it, and the two are not the same
as ACME's layout. Matching that is a separate question about our own symbol
output, not about this directive.
