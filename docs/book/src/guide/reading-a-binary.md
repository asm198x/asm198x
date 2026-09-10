# Reading a binary back

`asm198x disasm` decodes bytes using the CPU selected by `--dialect`.
For raw 6502 images, `disasm --recover` adds code/data regions, labels, and a
rebuild check before it writes ACME source.

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

For this program, feeding the listing back to the assembler produces the
byte-for-byte original:

```sh
asm198x --dialect acme fill.a -o original.bin
asm198x disasm --dialect acme --org 0xc000 original.bin > back.a
asm198x --dialect acme back.a -o again.bin
cmp original.bin again.bin && echo identical
```

The same round trip runs in the conformance suite against the reference
assemblers, which is where the disassembler's correctness is established — see
[Why asm198x](../why.md).

## Recover, name, and change a program

Want to see the edit run? The [C64 raster-bar walkthrough](https://github.com/asm198x/asm198x/blob/main/examples/recovery/C64.md)
takes a 38-byte program through recovery, an identical rebuild, and a one-byte
colour change, then runs both versions in Emu198x. It includes PRG packaging
and a script for capturing the result without a window.

The repository includes a [28-byte palette-copy program](https://github.com/asm198x/asm198x/blob/main/examples/recovery/palette.a).
Its loop loads eight bytes from a table, calls a subroutine that XORs each
with a mask, and stores them in a buffer at `$0400`. The code occupies
`$2000` through `$2013`; the palette occupies `$2014` through `$201B`.

Build the binary, then recover it with the boundaries you know:

```sh
asm198x -d acme examples/recovery/palette.a -o original.bin
asm198x disasm --recover -d acme --org 0x2000 \
  --code 0x2000:0x201c --data 0x2014:0x201c \
  --label 0x2014:palette --label 0x0400:buffer \
  original.bin -o recovered.a
asm198x -d acme recovered.a -o rebuilt.bin
cmp original.bin rebuilt.bin
```

Ranges use CPU addresses, with an exclusive end. `--data` overrides `--code`,
so the palette stays data even though the code range covered the whole file.
You can instead mark only `--code 0x2000:0x2014`: everything unmarked stays
data. Code and data options are repeatable; their order does not change the
map. Without any code range, the output is entirely data.

The backward branch becomes `BNE l_2002`, and the call becomes `JSR l_200e`.
The table reference becomes `LDA palette,X`. You can give generated addresses
more meaningful names with another `--label ADDRESS:NAME`; names outside the
image, such as `buffer`, become constants. Names must be unique ASCII
identifiers: letters or underscores first, then letters, digits or underscores.

Recovery formats the source through the shared ACME formatter, assembles it,
and checks every byte and the origin before returning it. The `cmp` command
shows the same check independently. Byte identity proves the rebuild; it does
not prove which bytes the original author intended as instructions.

Now copy `recovered.a` to `changed.a`, change `EOR #$00` to `EOR #$0F`, and
assemble it. The low four bits of each copied palette byte will be inverted.
The rebuilt program differs at exactly one file offset, 15: the immediate
mask. You have a named source program you can alter, rather than only a dump.

`-o` must name a new file, so repeating recovery cannot erase your edits.
Omit it to write verified source to stdout. Unknown or truncated instructions
stay as `!byte` data. A label that splits an instruction also makes those
bytes data, with a comment explaining the overlap. Branches wrapping the
16-bit address boundary are retained as bytes with their decoded instruction
in a comment. Low absolute addresses keep their numeric spelling where a
symbol could change the instruction width.

This mode accepts raw base-6502 images and emits ACME source. Supply the
address at which the first byte belongs; strip file headers first. Debug
sidecar names, other output dialects, and banked images are not read by this
mode. Direct targets inside the image get labels; indirect jump destinations
are not guessed, and a target in a data region does not turn it into code.

## Picking the CPU

The ordinary `disasm` command selects the CPU through `--dialect` or `--cpu`.
The verified `--recover` mode currently emits ACME source for the base 6502;
it refuses other dialects instead of emitting source under the wrong name.
