# Decision: a source may produce several artifacts — the library describes them, the CLI writes them

**Status:** Active. Binding for Asm198x (accepted 2026-08-24). Implements the
multi-output case [`assemble-io-model.md`](assemble-io-model.md) already
specifies, within the additive-change rules of
[`core-contract-freeze.md`](core-contract-freeze.md).

**Date:** 2026-08-24.

## The decision

1. **`AssemblyResult` gains a described artifact list.** Each entry carries the
   name as the source wrote it, the format, and the bytes.
2. **The library never writes a file.** `assemble_*` describes; the `asm198x`
   binary writes. Nothing in a source string causes a library call to touch the
   filesystem.
3. **`bytes` keeps its meaning** — the assembled machine code. Artifacts are the
   containers built *from* it, not replacements for it.
4. **Artifact paths are honoured, resolved by the host, and reported.** Each
   write is a diagnostic, as each `SHELLEXEC` is.

## Why this is due now

`assemble-io-model.md` § "Format selection" already settles the behaviour:

> **Several directives** of different formats — *allowed*; that is intentional
> multi-output (sjasmplus emits a `.sna` and a `.tap` from one source).

`AssemblyResult` has one `bytes` and one `origin`, and `main.rs` writes exactly
one file (`std::fs::write(&out_path, &result.bytes)`). The record mandates
multi-output and the type cannot express it. This is a gap between two binding
records, not a new feature: `SAVETAP` is named in-scope for Asm198x by the
umbrella [`tape-framing-vs-mastering.md`](../../../decisions/tape-framing-vs-mastering.md),
which calls stub parity "entailed, not optional".

## Why the library must not write

`assemble_*` is the public library surface, and Emu198x assembles in-process. A
library call that creates files because of a string in its input is a surprise
no consumer can defend against — it would make "assemble this untrusted source
to see if it parses" a filesystem operation. Describing artifacts keeps the
decision with the caller, and the CLI is the caller that wants to write.

## Why `bytes` stays the machine code

The reference draws the same line and says so in its own help text:

```
--raw=<filename>   Machine code saved also to <filename>
```

`SAVETAP` does not replace `--raw`; it joins it. So the code and the containers
are separate outputs in sjasmplus too, and keeping `bytes` as the code means
every existing consumer keeps working and keeps getting the same thing.

The alternatives were rejected: making `bytes` the first artifact is
order-dependent, and emptying it when artifacts exist breaks consumers silently.

## Paths, and the host's say

A `SAVE` target is a **source-controlled filesystem write**, the same shape of
concern as `SHELLEXEC` and answered the same way — honour the reference, report
what happened, give the host a control rather than a veto:

- Paths resolve relative to the output directory.
- `--outprefix` mirrors sjasmplus's own flag ("Prefix for save/output/..
  filenames in directives"), which is the reference conceding the host gets a
  say without changing the language.
- Absolute paths and `..` are **allowed and reported**, not refused. Refusing
  them would fail source sjasmplus accepts, which is the out-converging
  `match-the-reference` warns against. `--outprefix` and a `--no-save` are the
  controls for a caller that wants one.
- Every write is emitted as a diagnostic, so a JSON consumer sees the full set
  of side effects without scraping a log.

## Contract cost

None. `AssemblyResult` is `#[non_exhaustive]` and `core-contract-freeze.md` is
explicit that "additive fields never bump it", so `CONTRACT_VERSION` stays at 1
and older payloads keep loading.

## The formats are the family's, not this repository's

**Amended on acceptance, 2026-08-24.** A container is not an Asm198x private
matter, and the implementation must not treat it as one. Every format in scope
here is handled by more than one sibling: Asm198x **writes** it, Emu198x
**loads** it, Build198x **masters** media containing it, and Cat198x
**identifies** it. Four readings of one byte layout.

So a container's layout is a fact under
[`shared-hardware-reference-canon.md`](../../../decisions/shared-hardware-reference-canon.md)
like any other: it goes to `reference/`/`syntheses/` first, and the serialiser
here cites upward. It is not derived from an emulator's loader, and it is not
worked out at the keyboard from a file that happened to load.

That upstream already exists for the first two Spectrum targets.
`syntheses/zx-spectrum/tape-loading-format.md` documents the TAP file format
(§4) and the TZX file format (§5) alongside the physical encoding they frame,
so `SAVETAP` has something to cite on the day it is written.

**The formats graduate to Format198x.** The family already has the org for
exactly this — `format198x/format198x`, a workspace of independent,
dependency-free crates that read and write the on-disk formats of 1970s–1990s
computers, released separately and usable by any Rust tool or emulator rather
than only by us. `format-commodore-amiga-adf` is the first, and the README
already names the next candidates and the rule they arrive by: *"More formats
(C64 D64, Spectrum TAP, and others) graduate here from their originating
projects as they earn a standalone consumer."*

So the sequence is settled, not open. Write the serialiser here, citing the
primary library; when a second consumer appears — Emu198x's loader reading a
layout Asm198x writes is the obvious one — the format graduates to a
`format198x-sinclair-zx-spectrum-tap` crate and this workspace depends on it.

That has a consequence for how the first one is written, and it binds:
**write it graduation-ready.** Format198x's conventions are
`format-{manufacturer}-{system}-{format}` naming, `core`/`std` only,
deterministic bytes for the same inputs, and a typed error rather than a panic
on malformed input. A serialiser written to those constraints from the start
graduates by moving; one written against `AssemblyResult` and `AsmError`
graduates by being rewritten. Keep the layout code free of this crate's types
and behind a seam the artifact list calls into.

What follows from the amendment now, and binds:

- A format's byte layout is cited to `reference/`/`syntheses/` in the
  serialiser's doc comments, or the fact is added there first.
- A gap in the primary library is filled there, not worked around here.
- No container layout is transcribed from an emulator, ours or anyone's.
- Layout code stays free of `AssemblyResult`, `AsmError` and the engine's
  types, so a format graduates to Format198x by moving rather than by being
  rewritten.

## Open, deliberately

Whether an artifact also carries a load address or entry point. Some containers
need one and some do not. Start with name/format/bytes and add on the first
format that proves it necessary, rather than guessing the shape now.

## Drift triggers

Re-read this record when any of these appear:

- a `std::fs::write` reachable from an `assemble_*` entry point
- "the save directive can just set the output path"
- `bytes` being repurposed to hold a container
- refusing a `SAVE` path for safety rather than reporting it
- a proposal to bump `CONTRACT_VERSION` for an additive field
