# When assembling is not the last step

Most dialects here produce a flat binary at one origin: you name a file, you get
its bytes. Two do not, because the real toolchain does not either — and in both
cases `asm198x` performs the step that would otherwise need a second tool.

## NES: ca65 assembles, ld65 links

`ca65` emits object files. A `.nes` ROM is what `ld65` makes of them, using a
config that says where each segment lives. Assembling alone gets you nothing you
can run.

So the ca65 front-end assembles **and links**, in one pass, and hands back the
finished ROM:

<!-- sample: ca65, file: game.s -->
```asm
.segment "CODE"
reset:
        sei
        cld
        ldx #$ff
        txs
loop:   jmp loop

.segment "VECTORS"
        .word reset
        .word reset
        .word reset
```

<!-- output: game.s, output -->
```text
assembled + linked 40976 byte(s) -> game.s.bin
```

40,976 bytes every time: a 16-byte iNES header, 32K of PRG and 8K of CHR, filled
with `$00`. The segments land where the NROM layout puts them — `CODE` at
`$8000`, `VECTORS` at `$FFFA`, `ZEROPAGE` at `$00`, with `OAM` and `BSS`
occupying RAM rather than the file.

**The layout is fixed, not read from a config.** There is no `.cfg` parser and
no object-file format here: one NROM configuration, encoded directly, assembled
and linked in memory in a single pass. That is a deliberate boundary rather than
an unfinished one — the whole NES curriculum links with the same config, so a
general linker would be machinery with one caller. A second config is the point
at which generalising becomes worth it, and it has not arrived.

What this buys you is that the ROM is comparable: it is checked byte-for-byte
against real `ca65 + ld65` output, which a hand-rolled layout could not be. The
figures are on [Why asm198x](../why.md).

## Amiga: hunks, and who chooses the addresses

`--exe` writes an AmigaDOS hunk executable rather than a flat binary:

```sh
asm198x --dialect vasm --exe demo.s -o demo.exe
```

That is the loadable file: a hunk header, a hunk per section, and the reloc32
tables that go with them — matching `vasmm68k_mot -Fhunkexe -kick1hunks` for
everything the AmigaDOS loader consumes. vasm's optional debug symbol table is
not emitted.

**Hunks are relocatable, and this is the part that changes how you write code.**
An executable carries no load address; AmigaDOS decides where each hunk goes at
load time and rewrites the 32-bit references in the reloc32 tables to match. Two
consequences:

- An absolute reference to a label in another section becomes a relocation, so
  the loader can fix it up. That is automatic and costs you nothing.
- A PC-relative reference cannot, because the distance between two hunks is not
  known until they are loaded. PC-relative addressing is therefore used only
  **within** a section.

The debug sidecar follows the same logic: symbols are recorded as
`(section, offset)` with no base address, because there is no address to record
until something loads the file.

## What `--prg` and `--sna` are not

Those two are containers, not link steps. A `.prg` is the flat binary with a
two-byte load address in front of it; a `.sna` is the flat binary sitting at its
own origin inside a snapshot of a 48K Spectrum's memory and registers, which is
why it needs an `end` directive to say where execution starts.

Neither resolves a reference or moves a byte relative to another: your code
assembles to the same bytes with the flag as without it. The distinction matters
when you are deciding which flag to reach for — `--prg` and `--sna` wrap what
you already have, `--exe` and the ca65 path build something you did not.

## Debug artifacts on the linked paths

`--debug` and `--sym` work on both, and describe what the linker produced —
hunks for the Amiga, laid-out segments for the NES — rather than the flat
assembly they came from. `--listing` is a flat-dialect artifact; combining any
of them with `fmt` or `disasm` is an error rather than a silent no-op.
