# The command line

`asm198x` assembles retro-CPU source to a flat binary, disassembles one back, or
reformats source in place. One binary, no runtime dependencies, the same
interface on macOS, Linux and Windows.

This page is the reference. `asm198x --help` gives the same information in one
screen.

## Installing

```sh
brew install asm198x/tap/asm198x
```

Installer scripts, platform archives and the reason `cargo install` will not
find it are on [Install](../install.md).

## Operations

The operation is a subcommand:

```
asm198x [asm|disasm|fmt] [options] <input>
```

`dialects` and `version` are queries rather than operations: they take no input
and answer immediately.

**Assembling is the default**, so a bare invocation assembles and `asm` is the
explicit spelling:

```sh
asm198x prog.asm -o prog.bin           # assemble
asm198x asm prog.asm -o prog.bin       # identical
asm198x disasm prog.bin                # disassemble to stdout
asm198x fmt prog.asm                   # reformat, to stdout
asm198x --version                      # which build is this
```

> Before v0.0.12 the operations were the `--disasm` and `--fmt` flags. Those are
> withdrawn; using one now tells you the subcommand to use instead.

### `asm` — assemble

```
asm198x [asm] [--dialect <name>] [--cpu <target>] [-I <dir>]... <input> [-o <out.bin>]
```

Reads one source file and writes a flat binary. With no `-o`, the output takes
the input's name with a `.bin` extension.

### `disasm` — disassemble

```
asm198x disasm [-d <dialect>] [--org <addr>] <input.bin>
```

Writes a listing to stdout. The CPU follows the dialect: a 6502 dialect
disassembles as 6502, otherwise Z80 — so pass `-d` when the default is wrong.
`--org` sets the address the first byte is placed at, which changes how branch
and absolute operands render.

<!-- sample: acme, file: fill.a -->
```asm
* = $c000
        lda #$51
        rts
```

Assembled and read back with `asm198x disasm -d acme --org 0xc000 fill.a.bin`:

<!-- output: fill.a, disasm --org 0xc000 -->
```asm
        *= $C000
        LDA #$51
        RTS
```

The origin is not in the binary — a flat binary is bytes and nothing else — so
`--org` is how you tell the disassembler where they were meant to live. Leave it
out and the same bytes read as if they sat at `$0000`.

### `fmt` — reformat

```
asm198x fmt [--cpu <target>] <input.asm> [-o <out.asm>]
```

Canonical layout: labels at column 0, operations indented, own-line comments on
their own lines. **Comments and operand spelling are preserved verbatim** — the
formatter canonicalises layout, never the text of an operation. Formatting is
idempotent, and formatted source reassembles to the same bytes.

Writes to **stdout** unless `-o` is given; it never rewrites the input in place.
To format a file over itself, write to a new path and move it.

<!-- sample: pasmo, file: border.asm -->
```asm
; Set the border colour.
start:  ld a,1
    out ($fe),a
        ret
```

`asm198x fmt -d pasmo border.asm` writes:

<!-- output: border.asm, fmt -->
```asm
; Set the border colour.
start:
        ld a,1
        out ($fe),a
        ret
```

The label takes its own line, the operations line up, and the comment is
untouched — including its wording and its position above the code.

### `dialects` — list what `--dialect` accepts

```
asm198x dialects              # the table below, as text
asm198x dialects --markdown   # the same table as markdown, on stdout
```

The `--markdown` form is what generates this page's dialect table, so the two
cannot disagree. It writes to stdout to be redirected; everything informational
goes to stderr.

### `version` — report the build

```
asm198x --version        # also -V, or `asm198x version`
```

Prints `asm198x v<version>` — the same `v`-prefixed spelling the site, the
docs and the release tags use. The version is compiled in from the crate
version, so it names the build you are holding.

Added after v0.0.12. Earlier binaries answer none of the three spellings, so if
`asm198x --version` reports an unknown flag, you are on v0.0.12 or older.

## Options

| Option | Applies to | Meaning |
|---|---|---|
| `-o`, `--output <path>` | asm, fmt | Output path. `asm` defaults to the input with a `.bin` extension; `fmt` writes to stdout |
| `-d`, `--dialect <name>` | all | Source syntax — see *Dialects* |
| `--cpu`, `--target <name>` | all | CPU target where a dialect serves more than one (`z80`, `z80n`); with no `--dialect`, names a chip directly — see *Targets* |
| `-I <dir>` | asm | Add an include-search directory. Repeatable; **order is search order** |
| `--equ NAME=VALUE` | asm (pasmo/pasmonext) | Define a command-line constant before conditional assembly. Repeatable; matches Pasmo's spelling |
| `--org <addr>` | disasm | Address of the first byte |
| `--message-format <human\|json>` | asm | `human` (default) or a machine-readable result plus diagnostics on stdout |
| `-h`, `--help` | — | This surface, in one screen |

### Output containers

By default `asm` writes a flat binary. These wrap it for a machine's loader:

| Option | Produces | Requires |
|---|---|---|
| `--sna` | Spectrum 48K snapshot | Z80 dialect; `end <addr>` for the entry point; code at or above `$4000`, since below that is ROM |
| `--prg` | C64 program (2-byte load address prepended) | acme |
| `--gb-rom` | Game Boy cartridge ROM (RGBLINK layout, padding and checksums) | rgbasm |
| `--exe`, `--hunkexe` | Amiga hunk executable | vasm |

### Debug artifacts

| Option | Writes | Default path |
|---|---|---|
| `--debug[=path]` | Debug198x NDJSON sidecar | input + `.debug198x` |
| `--sym[=path]` | Sorted `name = $hex` symbol table | input + `.sym` |
| `--listing[=path]` | Address / bytes / source rows | input + `.lst` |

Available on the flat dialects, plus the ca65 and vasm linked paths for
`--debug` and `--sym`. They describe an assembly, so combining them with `fmt`
or `disasm` is an error rather than a silent no-op.

The sidecar format is specified in
[`debug198x.md`](https://github.com/asm198x/docs/blob/main/debug198x.md) and is frozen
at v1.

## Dialects

`--dialect` selects the **source syntax**, not the CPU. Each front-end matches
an existing assembler, so pick the assembler the source was written for.

The full table — every dialect, what its syntax is for, and every spelling
accepted — is on its own page: [Dialects](dialects.md). It is generated from the
binary, so it cannot fall behind what `--dialect` does.

### Targets

`--cpu` picks a CPU where a dialect serves more than one. Today that is Z80:
`z80` (the pasmo default) and `z80n` (Spectrum Next, the pasmonext default).
**Z80N opcodes follow the target, not the dialect** — `sjasmplus --cpu z80n`
gets them, `pasmonext --cpu z80` does not.

`--cpu` also names a chip **directly** when no `--dialect` is given, so
`asm198x --cpu 6809 prog.asm` is lwasm syntax and `--cpu 6502` is ACME's. Any
name from the dialect table works there. With both given, `--dialect` chooses
the syntax and `--cpu` the target.

## Exit status and diagnostics

`0` on success, non-zero on failure. A diagnostic carries a severity, a
message, a `(file, line, column)` span and a code. The human form prints what it
knows — `asm198x: file:line:col: error: message`, dropping the column where the
parse did not record one — and the full record is on the
`--message-format=json` path.

**Every diagnostic currently carries the code `AssemblyError`.** Codes are
assigned as error sites are classified, and new ones are added without
renumbering the existing ones, so a consumer can switch on a code today and keep
working as more arrive. Until then the severity carries the same information.

**stdout carries output; stderr carries everything else.** `disasm` and `fmt`
write their result to stdout, `asm` writes bytes to a file, and the summary line
and diagnostics go to stderr, so a pipeline gets the artifact and nothing else.

`--message-format=json` is the exception: it puts a machine-readable result on
stdout — bytes, symbols and the full diagnostic list — for a build script or an
editor. The human form stays on stderr.

<!-- sample: acme, file: fill.a, refuses: does not fit in a byte -->
```asm
* = $c000
        !byte $1234
```

That source is refused, and `--message-format=json` reports it like this. The
payload is one line on the wire; it is shown indented here:

<!-- output: fill.a, json -->
```json
[
  {
    "span": {
      "file": 0,
      "line": 2,
      "col": 15,
      "expansion_frames": [],
      "path": "fill.a"
    },
    "code": "AssemblyError",
    "severity": "Error",
    "message": "value 4660 does not fit in a byte",
    "fix": null
  }
]
```

`file` is a file id; v1 assembles a single file, so it is always `0`. A `col` of
`0` means the raising site knew no column — treat the span as the whole line.
`expansion_frames` records the expansions a location came through, innermost
first, and stays empty until a dialect expands macros. `fix` carries a suggested
edit where one is available: a `description`, plus a `replacement` when the fix
is a concrete piece of text to apply at the span.
