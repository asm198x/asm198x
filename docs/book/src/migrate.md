# Moving an existing project across

You do not port anything. Name the dialect your source is already written in
and assemble it. The rest of this page is how to check that for yourself before
you rely on it.

## Prove it produces the same bytes

The useful first step is not to switch. It is to build both ways and compare.

```sh
acme -f cbm -o theirs.prg main.asm
asm198x --dialect acme --prg main.asm -o ours.prg
cmp theirs.prg ours.prg && echo identical
```

That is the same comparison the differential suites run, on your source instead
of ours. If the files match, nothing about your build has changed except which
binary produced it.

The same shape works for the other front doors:

```sh
pasmo --bin main.asm theirs.bin
asm198x --dialect pasmo main.asm -o ours.bin
cmp theirs.bin ours.bin
```

## Projects in more than one file

`-I` adds a directory to the include search path. It is repeatable, and
searched in the order given:

```sh
asm198x --dialect acme -I src -I lib main.asm -o main.bin
```

Each dialect keeps its own spelling of the include directive, because your
source already has one:

| Dialect | Spelling | Supported |
|---|---|---|
| acme | `!source "file"` | Yes |
| ca65 | `.include "file"` | Yes |
| sjasmplus | `include "file"` | Yes |
| vasm | `include "file"` | Yes |
| pasmo | `include "file"` | **Not yet** |

A multi-file pasmo project will not assemble today. It is the one front door
where the include mechanism has not landed, and it is the thing to check first
if that is your project.

## The output your build already expects

If your build ends by wrapping a flat binary into a loadable file, that step
may not be needed:

| Flag | Produces |
|---|---|
| `--prg` | A C64 program with its two-byte load address |
| `--sna` | A 48K Spectrum snapshot — needs `end <addr>` for the entry point |
| `--exe` | An Amiga hunk executable |

Without one of these you get the flat binary, which is what your existing
packaging step is presumably already fed.

## What might not match

[Where we differ](divergences.md) lists every known difference, what each one
is, and whether it is pending work or a settled position. It is a short list,
and reading it is faster than discovering one.

If your source is refused rather than assembled differently, that is not on
that list — it is a gap, and worth reporting with the source that triggered it.
`--message-format=json` gives a structured diagnostic if that is easier to
attach than a terminal scrape.

## Keeping both for a while

Nothing stops you running both assemblers over the same source in CI and
comparing bytes, exactly as above. That is what this project does against eight
reference assemblers on every change, and it is the cheapest possible way to
find out that something moved.
