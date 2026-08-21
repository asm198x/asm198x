# Reading a binary back

`asm198x disasm` turns bytes into source. It works for every dialect, and it
writes in that dialect's own syntax — so what comes out is something you can
feed back in.

## A worked example

<!-- sample: acme, file: fill.a -->
```asm
* = $c000
screen = $0400
fill:
        lda #$51
        ldx #$00
loop:   sta screen,x
        inx
        bne loop
        rts
```

Assembled, then read back with `asm198x disasm -d acme --org 0xc000 fill.a.bin`:

<!-- output: fill.a, disasm --org 0xc000 -->
```asm
        *= $C000
        LDA #$51
        LDX #$00
        STA $0400,X
        INX
        BNE $C004
        RTS
```

Labels and symbol names are gone, because they were never in the file — `screen`
and `loop` lived in the assembler, not in the bytes. What survives is what the
CPU sees.

## `--org` is not cosmetic

A flat binary is bytes and nothing else: it carries no record of where it was
meant to live. `--org` supplies that, and it changes what the listing says:

<!-- output: fill.a, disasm -->
```asm
        *= $0000
        LDA #$51
        LDX #$00
        STA $0400,X
        INX
        BNE $0004
        RTS
```

The instructions are identical — the same bytes decode the same way — but the
`BNE` target moved. A branch encodes a **relative** displacement, so its printed
destination depends entirely on where you say the code starts. Read a C64
program at `$0000` and every branch target in it is wrong by `$C000`.

Absolute operands do not move: `STA $0400,X` reads the same either way, because
`$0400` is in the instruction. It is the relative ones that need you to be
right.

## Bytes that are not instructions

Data does not announce itself. The disassembler decodes what it can and emits
the rest as data, in the dialect's syntax:

<!-- sample: acme, file: mixed.a -->
```asm
* = $c000
        lda #$51
        !byte $ff
        !byte $02
        rts
```

<!-- output: mixed.a, disasm --org 0xc000 -->
```asm
        *= $C000
        LDA #$51
        !byte $FF
        !byte $02
        RTS
```

`$FF` is not a 6502 opcode, so it comes out as `!byte`. That is a disassembly
rather than a failure — but it is also the tell that you are reading a table, a
sprite or a string as though it were code. If a run of `!byte` appears where you
expected instructions, the origin is probably right and the *boundary* is wrong.

## It reassembles

Feeding the listing back to the assembler produces the byte-for-byte original.
That is what makes the output worth trusting, not just reading:

```sh
asm198x --dialect acme fill.a -o original.bin
asm198x disasm --dialect acme --org 0xc000 original.bin > back.a
asm198x --dialect acme back.a -o again.bin
cmp original.bin again.bin && echo identical
```

The same round trip runs in the conformance suite against the reference
assemblers, which is where the disassembler's correctness is established — see
[Why asm198x](../why.md).

## Picking the CPU

`disasm` takes the CPU from `--dialect`: a 6502 dialect disassembles as 6502,
and anything else defaults to Z80. Pass `-d` when the default is wrong, or
`--cpu` to name a chip directly with no dialect at all.
