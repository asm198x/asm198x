# A first program

Five complete programs, one per front door. CI assembles every sample on this
page with the real `asm198x` binary, so they assemble as written.

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

## NES, ca65 syntax

<!-- sample: ca65, file: game.s -->
```asm
; Set the background colour, then stop.
.segment "HEADER"
        .byte "NES", $1a
        .byte 2                 ; 32K of program
        .byte 1                 ; 8K of character data
        .byte $00, $00

.segment "CODE"
reset:
        sei                     ; no interrupts while we set up
        cld
        ldx #$ff
        txs                     ; the stack lives at $01ff downwards

wait1:  bit $2002               ; two frames, for the PPU to warm up
        bpl wait1
wait2:  bit $2002
        bpl wait2

        bit $2002               ; reset the address latch
        lda #$3f                ; palette memory starts at $3f00
        sta $2006
        lda #$00
        sta $2006
        lda #$21                ; a mid blue
        sta $2007

forever:
        jmp forever

nmi:    rti
irq:    rti

.segment "VECTORS"
        .word nmi
        .word reset
        .word irq
```

```sh
asm198x --dialect ca65 game.s -o game.nes
```

That one command assembles **and links**: ca65 normally hands object files to
ld65, and what comes out here is the finished 40,976-byte ROM. The segments are
the interface to the layout — `CODE` and `VECTORS` land where the NROM mapping
puts them. [When assembling is not the last step](linking.md) has the detail.

The two `bit $2002` loops are not decoration. The PPU is not ready to be written
to for the first couple of frames after power-on, and code that skips the wait
works on some emulators and not on hardware.

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

## Amiga, vasm syntax

<!-- sample: vasm, file: demo.s -->
```asm
; Write the background-colour register eight times, with a pause between.
CUSTOM   equ $dff000
COLOR00  equ $180

        section code,code
start:
        lea     CUSTOM,a5           ; the custom chips, as a base register
        moveq   #7,d1               ; eight times round

next:   move.w  d1,d0
        lsl.w   #4,d0               ; shift it up into the colour bits
        move.w  d0,COLOR00(a5)      ; background colour

        move.l  #$20000,d2          ; a pause long enough to notice
wait:   subq.l  #1,d2
        bne.s   wait

        dbra    d1,next

        moveq   #0,d0               ; return code: nothing went wrong
        rts
```

```sh
asm198x --dialect vasm --exe demo.s -o demo
```

`--exe` writes a hunk executable, which is what AmigaDOS loads and runs. Note
what this program does *not* do: it never takes the machine over. With the
operating system still running, anything it writes to a display register is
liable to be written back over by the OS on the next redraw. A demo that wants
the screen to itself has to take it first; the register writes are the part
that comes after.

`rts` returns to AmigaDOS with `d0` as the return code, which is why the program
ends rather than looping forever like the other four.

## Tandy CoCo, lwasm syntax

<!-- sample: lwasm, file: screen.asm -->
```asm
; Write one byte across the first row of the text screen.
SCREEN  equ $0400               ; the text screen is memory, 32 by 16

        org $0e00
start:
        ldx #SCREEN             ; where to write
        ldb #32                 ; one row of cells
        lda #$2a                ; the character code to write
loop:   sta ,x+                 ; store, then step X on one
        decb
        bne loop
        rts
```

```sh
asm198x --dialect lwasm screen.asm -o screen.bin
```

`sta ,x+` is the 6809 idiom the 6502 samples spell with an index register and a
counter: store through X, then advance X. It is the same loop as the C64 one
with the bookkeeping folded into the addressing mode.

The origin is where you intend the program to be loaded, and on a real CoCo you
would choose it to sit clear of BASIC — which depends on how much memory the
machine has. It is a decision about the target, not about the assembler.

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

## What else these are good for

Any of these can go straight back through the other two operations. Tidying the
C64 one, for instance:

```sh
asm198x fmt --dialect acme fill.a -o fill.tmp && mv fill.tmp fill.a
```

Formatting is idempotent and does not change the bytes the file assembles to,
so it is safe on source you have not read — [Keeping source tidy](formatting.md).
And a binary reads back in the dialect's own syntax, which is
[Reading a binary back](reading-a-binary.md).

## Which dialect?

`--dialect` names the **assembler**, not the machine — see
[Dialects](../reference/dialects.md) for the full table and the conventional choice per
machine. If you have existing source, the answer is whichever assembler it was
written for.
