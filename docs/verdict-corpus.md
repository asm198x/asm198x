# The verdict corpus

Asm198x claims its output is byte-identical to the real assembler for each
dialect. Checking that claim means running `acme`, `ca65`, `asl`, `pasmo`,
`sjasmplus`, `vasm`, `lwasm` and `rgbasm` — eight tools most machines do not
have, and CI has none of.

So the claim is **recorded once and replayed forever**. A verdict is one
observation: *this arbiter, at this version, given this source, produced these
bytes*. Nothing in the corpus is written by hand. A golden says what someone
thought should happen; a verdict says what did.

## What this means for a pull request

**You need no reference assembler.** The `Replay` check assembles every recorded
case with your build and compares against what the real tool produced. If it is
green, byte-identity still holds — on your machine, on any machine.

Two checks guard the corpus:

| check | what it means when it fails |
|---|---|
| `Replay` | your change alters output the reference has already ruled on |
| coverage (inside `Replay`) | your change stopped something being arbitrated |

The second is the subtle one. Coverage is the count of spec forms with a
recorded verdict. A change that makes a form unrenderable does not fail replay —
there is simply nothing left to replay — so it would pass while proving less
than the day before. The stamp at
`crates/asm198x/tests/verdicts/coverage.stamp` is what makes that visible, and
a drop shows up in your diff rather than in a log.

If a drop is deliberate, run `cargo xtask coverage --write` and say in the
commit which cases went and why. The stamp is the record of that debt, so it
must not move silently.

A **rise** needs the same command, without the explanation — refresh the stamp
in the change that earned it. The check asks for this because a stamp that lags
is a ratchet that has let go: while it reads the lower number, a regression back
down to that number passes unnoticed.

## Adding to the corpus

If you have the reference tools:

```sh
cargo xtask grow            # arbitrate whatever is not yet recorded
cargo xtask grow 8080       # or narrow it
```

It runs the live suites, refreshes the stamp, and shows you the diff. Recording
is idempotent, so a run that learns nothing changes nothing — safe to run
habitually, which is the only way a corpus stays current.

Review the diff before committing. Every added line is an observation of a real
tool; if one looks wrong, it is the tool or the harness that needs explaining,
not the line that needs editing.

## Known differences

Some differences are real, understood, and not defects. They are recorded as
**divergences** tagged with the issue tracking them, and they self-police: if
one starts matching, replay fails and the marker must go, so the ledger cannot
quietly become a lie.

`cargo xtask ledger` prints what the corpus holds — arbiters and their versions,
verdict counts, coverage, and every tracked divergence.

## Where things live

| path | what |
|---|---|
| `crates/asm198x/tests/verdicts/*.ndjson` | the corpus, one file per CPU |
| `crates/asm198x/tests/verdicts/coverage.stamp` | recorded coverage |
| `crates/asm198x/tests/verdicts/code-samples.pin` | the curriculum revision replayed against |
| `crates/verdict-corpus/` | the record format and its reader |
| `crates/asm198x/tests/verdict_replay.rs` | the tool-free check |
