# Keeping source tidy

`asm198x fmt` lays source out canonically. It works on every dialect, and it is
built to be safe to run over a project you have not read.

## What it does to your source

<!-- sample: acme, file: messy.a -->
```asm
; Fill a page of screen memory.
    * = $c000
screen = $0400
fill:
        lda #$51
   ldx #$00
loop:  sta screen,x
    inx
       bne loop
        rts
```

<!-- output: messy.a, fmt -->
```asm
; Fill a page of screen memory.
        *= $c000
screen = $0400
fill:
        lda #$51
        ldx #$00
loop:
        sta screen,x
        inx
        bne loop
        rts
```

Labels sit at column 0 and take their own line; operations are indented one
level; an own-line comment stays on its own line. `loop: sta screen,x` becomes
two lines, which is the one change people are surprised by.

**What it does not touch is the point.** Comments keep their wording and their
position. Operand spelling is left exactly as written — `$c000` is not
re-cased, `screen + $100` is not folded, and a number written in binary stays
binary. The formatter arranges lines; it does not rewrite what is on them.

## Why you can run it on a project you have not read

Two properties, both tested:

- **Formatting is idempotent.** Formatting formatted source produces the same
  text, so there is no oscillation between two "canonical" layouts and no
  churn in a diff from running it twice.
- **Formatted source assembles to the same bytes.** Layout is not semantics
  here, so tidying a file cannot change what it builds. You do not have to
  re-verify a project because you formatted it.

## In a workflow

`fmt` writes to **stdout** and never rewrites the input in place. That is
deliberate — a formatter that edits files is a formatter that can lose work —
so formatting over a file is an explicit two steps:

```sh
asm198x fmt --dialect acme fill.a -o fill.tmp && mv fill.tmp fill.a
```

For a check rather than a fix, compare the formatted output against the file. In
CI this fails on anything unformatted without changing the tree:

```sh
asm198x fmt --dialect acme fill.a | diff -u fill.a - || exit 1
```

That is the shape worth reaching for on a shared project: the formatter settles
layout arguments, and the check keeps it settled without anyone having to
remember.

## Notes

`--cpu` applies, for a dialect that serves more than one target. The debug
artifacts do not: `--debug`, `--sym` and `--listing` describe an assembly, so
combining them with `fmt` is an error rather than a silent no-op.
