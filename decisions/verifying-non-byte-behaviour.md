# Decision: compare everything the run produced, not just the output file

**Status:** Active. Binding for Asm198x (accepted 2026-08-24). Extends
[`spec-conformance-and-fuzzing.md`](spec-conformance-and-fuzzing.md), which owns
how correctness is checked.

**Date:** 2026-08-24.

**Accepted as the verification story for
[`multi-artifact-output.md`](multi-artifact-output.md).** That record went
Active the same day and commits the assembler to writing files a source names.
A harness that compares one output file cannot see any of them, so this is what
makes that work checkable rather than merely written.

## The decision

1. **The differential compares every file a run produced**, not the primary
   output alone: listings, symbol dumps, map and export files, and every file a
   `SAVE`-family directive wrote.
2. **Source-derived console output is compared; tool-phrased chrome is not.**
3. **Our own diagnostics are pinned by unit test**, not by diffing them against
   a reference's wording.
4. A directive that changes nothing observable needs no new machinery: the
   existing byte comparison showing no difference *is* its verification.

## Why this is smaller than it looked

The first reading of this gap was that a whole class of directives —
`SHELLEXEC`, `LUA`, `DISPLAY`, `.list`, `EXPORT`, the `SAVE` family — sits
outside a correctness model built on byte diffs. Most of it does not. It sits
outside the *harness*, which only ever compares one output file.

Every reference already emits the evidence:

```
ca65        -l name              Create a listing file if assembly was ok
lwasm       -l, --list[=FILE]    Generate list [to FILE]
            -m, --map[=FILE]     Generate map [to FILE]
acme        -l, --symbollist FILE
sjasmplus   --lst  --sym  --exp=<filename>  Save exports to <filename>
```

sjasmplus ships `--exp` *so that `EXPORT` is observable*. The seventeen-word
symbol-visibility cluster is therefore byte-verifiable, not a judgement call:
assemble both sides, diff the exports file. `SHELLEXEC` comes back inside the
net the same way — a probe whose command has an observable effect
(`SHELLEXEC "echo x > marker"`) is verified by the directory comparison.

So the correctness model does not change shape. It is the same byte-for-byte
evidence, applied to more of what the run already produces.

## The console line

What remains after the widening is output that reaches no file. It splits:

- **Source-derived content is ours to match.** `DISPLAY "loaded ", N` prints
  text the *source* determined. A divergence there is a real parity failure.
- **Tool-phrased chrome is not.** `Pass 1 complete (0 errors)` and
  `Executing <...>` are the reference's own wording. It changes between
  versions and matching it buys no source compatibility, only a brittle corpus.

Our own equivalents — the `SHELLEXEC` execution notice, the `SAVE` write
notice — are chrome by the same definition, so they are not diffed against a
reference. They are still behaviour we must not lose silently, and an ordinary
unit test pins them. That is a regression question, not a parity question, and
conflating the two is what made this look like a hole in the differential.

## Corpus shape

Verdict outcomes today are `bytes`, `digest` and `divergence` — all singular,
because a run had one output. A run that writes four files needs a new outcome
carrying a digest per named artifact, so the replay-without-tools CI job keeps
working with no reference installed.

Existing records are untouched and keep replaying: this is an added outcome
kind, not a change to the three that exist.

## Drift triggers

Re-read this record when any of these appear:

- "there is no way to test this, it does not affect the bytes"
- a probe that assembles with a listing or symbol flag and compares only the ROM
- a test asserting a reference's exact message text
- a new verdict outcome that assumes a single output file
- a `SAVE` or `SHELLEXEC` probe whose effect nothing observes
