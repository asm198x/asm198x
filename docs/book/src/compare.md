# Compared with what you have

You already have an assembler that works. This page is about what changes if you
put `asm198x` in front of it, and — more usefully — what does not.

## What does not change

**Your syntax.** Every dialect here is somebody else's assembler. You keep the
source you have and name the dialect it is written in; there is no house dialect
to convert to. [Dialects](reference/dialects.md) lists them all.

**Your bytes.** The point of matching a reference assembler is that the output
is the same. Where it is knowingly not, that is written down — see
[Where we differ](divergences.md).

**Your build.** `asm198x` reads a file and writes a file. It has no opinion
about your Makefile.

## What we measured against

This is the part worth checking rather than believing. Every recorded verdict
carries the reference tool's own version self-report, so the corpus can say
exactly which build of which assembler produced the bytes we compare with:

<!-- generated: xtask compare --markdown -->
| Reference tool | The version we measured against | Instruction sets | Verdicts |
|---|---|---|---|
| `acme` | This is ACME, release 0.97 ("Zem"), 28 June 2020 | 1 | 486 |
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 14 | 3057 |
| `ca65` | ca65 V2.18 - N/A | 3 | 602 |
| `lwasm` | lwasm from lwtools 4.25 | 1 | 621 |
| `pasmo` | PasmoNext v0.1.3 (PC) (C) 2004-2005 Julian Albo | 1 | 999 |
| `rgbasm` | rgbasm v1.0.3 | 2 | 545 |
| `sjasmplus` | SjASMPlus Z80 Cross-Assembler v1.21.0 (https://github.com/z00m128/sjasmplus) | 2 | 254 |
| `vasmm68k_mot` | vasm 2.0b (c) in 2002-2025 Volker Barthelmann<br>vasm 2.0f (c) in 2002-2026 Volker Barthelmann | 1 | 1276 |
<!-- /generated -->

The version column says **what we measured against**, not what is current. That
distinction is the reason the table can be trusted: a reference tool shipping a
new release does not make this page wrong, because it is a record of an
observation rather than a claim about the world. [Why asm198x](why.md) has the
totals and how the replay works.

## What arrives with it

Stated as what this tool does, because that is what we can vouch for:

| | |
|---|---|
| One binary | Every dialect above, one install, the same interface on macOS, Linux and Windows |
| `fmt` | Canonical layout for every dialect, idempotent, and the bytes do not change |
| `disasm` | Every dialect, in that dialect's own syntax, and it reassembles |
| `--prg`, `--sna`, `--exe` | The loadable file comes out of the assembler |
| `--message-format=json` | Structured results and diagnostics for editors and build scripts |
| `--debug`, `--sym`, `--listing` | A Debug198x sidecar, a symbol table, an address/bytes/source listing |
| Multi-file | `-I` search paths, with each dialect's own resolution rules — see [Projects in more than one file](guide/multi-file.md) |

## What this page does not claim

It says nothing about what any other assembler can or cannot do.

That is deliberate. A feature comparison is a claim about software we do not
control and do not track, and the moment one of those tools ships, a table like
that becomes wrong without anyone touching it — which is exactly the failure the
rest of this documentation is built to avoid. Writing "X has no formatter" from
memory would be worse still.

So the comparison offered here is the one we can stand behind: these are the
tools we measured against, this is the version of each, this is what we produce,
and [here is every place we knowingly differ](divergences.md). What your current
toolchain does, you already know better than we do.
