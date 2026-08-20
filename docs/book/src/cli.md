# The command line

`asm198x` assembles retro-CPU source to a flat binary, disassembles one back, or
reformats source in place. One binary, no runtime dependencies, the same
interface on macOS, Linux and Windows.

This page is the reference. `asm198x --help` gives the same information in one
screen.

## Installing

Each release attaches an installer and platform archives to its
[GitHub Release](https://github.com/asm198x/asm198x/releases), and publishes a
Homebrew formula.

```sh
# Homebrew (macOS, Linux)
brew install asm198x/tap/asm198x
```

Homebrew asks you to trust a third-party formula the first time. Approving
`asm198x/tap/asm198x` trusts that one formula; `brew trust --tap asm198x/tap`
would trust everything the tap publishes, now and in future. Prefer the
formula.

```sh
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/asm198x/asm198x/releases/latest/download/asm198x-installer.sh | sh
```

```powershell
# Windows
irm https://github.com/asm198x/asm198x/releases/latest/download/asm198x-installer.ps1 | iex
```

Or download an archive directly: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.

asm198x is **not** published to crates.io, so `cargo install asm198x` will not
find it. Use the installer or one of the archives above.

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

Prints `asm198x <version>`. The version is compiled in from the crate version,
so it names the build you are holding.

Added after v0.0.12. Earlier binaries answer none of the three spellings, so if
`asm198x --version` reports an unknown flag, you are on v0.0.12 or older.

## Options

| Option | Applies to | Meaning |
|---|---|---|
| `-o`, `--output <path>` | asm, fmt | Output path. `asm` defaults to the input with a `.bin` extension; `fmt` writes to stdout |
| `-d`, `--dialect <name>` | all | Source syntax — see *Dialects* |
| `--cpu`, `--target <name>` | all | CPU target where a dialect serves more than one (`z80`, `z80n`); with no `--dialect`, names a chip directly — see *Targets* |
| `-I <dir>` | asm | Add an include-search directory. Repeatable; **order is search order** |
| `--org <addr>` | disasm | Address of the first byte |
| `--message-format <human\|json>` | asm | `human` (default) or a machine-readable result plus diagnostics on stdout |
| `-h`, `--help` | — | This surface, in one screen |

### Output containers

By default `asm` writes a flat binary. These wrap it for a machine's loader:

| Option | Produces | Requires |
|---|---|---|
| `--sna` | Spectrum 48K snapshot | Z80 dialect; `end <addr>` for the entry point; code at or above `$4000`, since below that is ROM |
| `--prg` | C64 program (2-byte load address prepended) | acme |
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
message, a `(file, line, column)` span and a stable code. The human form prints
what it knows — `asm198x: file:line:col: error: message`, dropping the column
where the parse did not record one — and the full record, code included, is on
the `--message-format=json` path.

**stdout carries output; stderr carries everything else.** `disasm` and `fmt`
write their result to stdout, `asm` writes bytes to a file, and the summary line
and diagnostics go to stderr, so a pipeline gets the artifact and nothing else.

`--message-format=json` is the exception: it puts a machine-readable result on
stdout — bytes, symbols and the full diagnostic list — for a build script or an
editor. The human form stays on stderr.
