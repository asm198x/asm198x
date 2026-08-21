# A first program

Two complete programs, one per machine family. CI assembles every sample on
this page with the real `asm198x` binary, so they assemble as written.

## Commodore 64, ACME syntax

<!-- sample: acme -->
```asm
; Fill the screen with a character, then colour it.
* = $c000

screen  = $0400
colour  = $d800

        lda #$51            ; a filled circle in the C64 character set
        ldx #$00
loop    sta screen,x
        sta screen + $100,x
        inx
        bne loop

        lda #$01            ; white
        ldx #$00
colours sta colour,x
        sta colour + $100,x
        inx
        bne colours
        rts
```

```sh
asm198x --dialect acme fill.a -o fill.bin
```

Two loops rather than one, because a `bne` counts to 256 and the screen is
1000 bytes: each pass covers a page, and two passes cover most of it. Making
that exact is the reader's exercise, not the assembler's problem.

`--prg` wraps the output with the two-byte load address a C64 expects:

```sh
asm198x --dialect acme --prg fill.a -o fill.prg
```

## ZX Spectrum, pasmo syntax

<!-- sample: pasmo -->
```asm
; Cycle the border through all eight colours.
        org 32768

        ld b, 8             ; eight colours
next:   ld a, b
        dec a
        out ($fe), a        ; the border takes the low three bits
        ld hl, 0
wait:   dec hl
        ld a, h
        or l
        jr nz, wait
        djnz next
        ret
```

```sh
asm198x --dialect pasmo border.asm -o border.bin
```

`--sna` writes a 48K snapshot an emulator will load directly, which needs an
`end` directive naming the entry point:

```sh
asm198x --dialect pasmonext --sna border.asm -o border.sna
```

## When it does not assemble

`lda` takes one byte. Give it two and the assembler says so, naming the line
and the column the value starts at:

<!-- sample: acme, file: fill.a, refuses: value 4660 does not fit in a byte -->
```asm
* = $c000
        lda #$1234
        rts
```

<!-- output: fill.a, output -->
```text
asm198x: fill.a:2:13: error: value 4660 does not fit in a byte
```

The address is `$c000`, so `lda #$1234` is not ambiguous — it is a byte-sized
instruction given a word. Dropping the `#` makes it `lda $1234`, which is an
absolute load from address `$1234`.

## Which dialect?

`--dialect` names the **assembler**, not the machine — see
[Dialects](../reference/dialects.md) for the full table and the conventional choice per
machine. If you have existing source, the answer is whichever assembler it was
written for.
