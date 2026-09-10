# Take a binary apart

For a visible result, try the [C64 raster-bar workflow](C64.md): recover a
program, rebuild it, change its colours, and run it in Emu198x.

`palette.a` is a 28-byte 6502 program: 20 bytes of code followed by an
eight-byte palette. A loop loads each palette byte, calls a subroutine that
XORs it with a mask, and writes it into the buffer at `$0400`.

Run these commands from the repository root with the newly built `asm198x`:

```sh
cargo build -p asm198x
mkdir -p target/recovery-demo

target/debug/asm198x -d acme examples/recovery/palette.a -o target/recovery-demo/original.bin

target/debug/asm198x disasm --recover -d acme --org 0x2000 \
  --code 0x2000:0x201c --data 0x2014:0x201c \
  --label 0x2014:palette --label 0x0400:buffer \
  target/recovery-demo/original.bin -o target/recovery-demo/recovered.a

target/debug/asm198x -d acme target/recovery-demo/recovered.a -o target/recovery-demo/rebuilt.bin
cmp target/recovery-demo/original.bin target/recovery-demo/rebuilt.bin
```

The end of each range is exclusive. Data overrides code, so the palette stays
`!byte` data even when the whole image was initially marked as code. Leaving
out both ranges would keep the entire image as data.

Open `target/recovery-demo/recovered.a`. The backward branch names `l_2002`
and the subroutine call names `l_200e`. `palette` and `buffer` are names you
supplied; the binary itself never held them. Recovery has already rebuilt the
source and checked every byte and the origin before writing it. The extra
`cmp` lets you inspect that guarantee yourself.

Now save a copy as `changed.a`, change `EOR #$00` to `EOR #$0F`, and build it:

```sh
target/debug/asm198x -d acme target/recovery-demo/changed.a -o target/recovery-demo/changed.bin
cmp -l target/recovery-demo/original.bin target/recovery-demo/changed.bin
```

`cmp` reports one difference at byte 16 (its positions start at one). This
changes the mask applied to each palette byte, inverting its low four bits.
You have moved from bytes to named source to an intentional edit.

Recovery's `-o` creates a new file: it refuses to overwrite your input or any
existing source, including a hand-edited recovery. Choose another source path
when repeating the experiment. The mode accepts raw base-6502 images and
emits ACME source; it does not load PRG headers, banked ROMs, or debug sidecars.
