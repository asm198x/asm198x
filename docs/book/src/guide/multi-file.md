# Projects in more than one file

`asm198x` resolves includes itself. Point it at the root file, tell it where to
look for the rest, and it walks the tree:

```sh
asm198x --dialect ca65 -I src -I lib main.s -o main.bin
```

`-I` is repeatable and **searched in the order given**. It is the second place
looked, not the first.

## Where a relative include is looked for

The first place is the dialect's anchor, and this is where multi-file projects
surprise people: the anchor is not the same in every dialect. Each of ours is
pinned against the real assembler, so source that resolves under `ca65` resolves
here, and source that does not resolve under `vasm` does not resolve here
either.

<!-- generated: xtask includes --anchors -->
| Dialect | Looked for in | A request with no extension |
|---|---|---|
| `acme` | The including file's own directory | taken as spelled |
| `ca65` | The including file's directory, then each enclosing includer's | taken as spelled |
| `65816` | The including file's directory, then each enclosing includer's | taken as spelled |
| `huc6280` | The including file's directory, then each enclosing includer's | taken as spelled |
| `vasm` | The root input's directory, however deep the request | taken as spelled |
| `lwasm` | The including file's own directory | taken as spelled |
| `rgbasm` | The root input's directory, however deep the request | taken as spelled |
| `pasmo` | No include | — |
| `sjasmplus` | The including file's own directory | taken as spelled |
| `8080` | The including file's own directory | `defs` tries `defs.inc` first |
| `6800` | The including file's own directory | `defs` tries `defs.inc` first |
| `1802` | The including file's own directory | `defs` tries `defs.inc` first |
| `8048` | The including file's own directory | `defs` tries `defs.inc` first |
| `scmp` | The including file's own directory | `defs` tries `defs.inc` first |
| `f8` | The including file's own directory | `defs` tries `defs.inc` first |
| `2650` | The including file's own directory | `defs` tries `defs.inc` first |
| `tms7000` | The including file's own directory | `defs` tries `defs.inc` first |
| `pdp11` | The including file's own directory | `defs` tries `defs.inc` first |
| `tms9900` | The including file's own directory | `defs` tries `defs.inc` first |
| `cp1610` | The including file's own directory | `defs` tries `defs.inc` first |
| `z8000` | The including file's own directory | `defs` tries `defs.inc` first |
<!-- /generated -->

Read the middle column as "before `-I` is consulted at all". A `ca65` project
whose `src/gfx.s` includes `"macros.inc"` finds a copy sitting beside the root
file, because ca65 walks back up the chain of includers. The same project under
`lwasm` does not: `lwasm` looks in `src/` and then stops, and the fix is an `-I`
entry naming the directory rather than moving the file.

The spelling of the include directive differs too — that table is on
[Moving a project across](../migrate.md), beside the binary-include spellings.

## When it goes wrong

A missing target names the request, the file that asked for it, and the line:

<!-- sample: ca65, file: main.s, refuses: file not found -->
```asm
        .include "nowhere.inc"
        rts
```

<!-- output: main.s, output -->
```text
asm198x: main.s:1:18: error: cannot load `nowhere.inc` (requested from main.s): file not found
```

Two failures are reported specifically rather than as a missing file, because
both look like one and neither is:

- **A cycle** — a file that includes something which includes it back. The
  diagnostic prints the whole chain, so you can see which hop closed it.
- **Depth** — includes nested more than 64 levels deep. That is a cycle the
  path names differently at each hop, near enough always, and reporting it as a
  depth limit is more useful than assembling until memory runs out.

A file included twice is read twice, as every reference assembler does. It keeps
one identity internally, so a diagnostic inside it names one file rather than
two — but its lines are assembled on each visit, which is what an include-guard
macro exists to prevent.

## Binary data

The binary-include directive resolves through exactly the same search: the
dialect's anchor first, then the `-I` directories in order.

The window arithmetic does **not** agree across dialects, and it is not a place
to guess. ca65 reads a negative size as "the rest of the file", lwasm counts a
negative offset back from the end, and the asl-syntax chips reject both — each
matching its own reference tool. If you are moving a project across, the
[divergences list](../divergences.md) is where a difference would be recorded if
we had one.

## What the single-file API does not do

The library's one-source entry points assemble one string and cannot resolve
anything, so they say so rather than failing obscurely:

```text
cannot resolve `include "defs.inc"` here — the single-source API assembles one
file; use the multi-file entry point (the CLI resolves includes automatically)
```

The command line always takes the multi-file path. This only reaches you if you
are calling the library directly.
