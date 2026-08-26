# Decision: a version claim names what we ran, and the corpus says what that was

**Status:** Active. Binding for Asm198x (accepted 2026-08-26). Gives
[`spec-conformance-and-fuzzing.md`](spec-conformance-and-fuzzing.md)'s
differential method a provenance rule, and
[`reference-parity-goal.md`](reference-parity-goal.md) a stable way to name the
tools it measures against.

**Date:** 2026-08-26.

## The decision

**A version claim in prose or a comment names a version the verdict corpus has
recorded, or it is written down here with the reason it stands.**

`cargo xtask versions --check` enforces it. It reads the `identity` string off
every verdict — the line the tool itself printed when it answered — and refuses
a claim that matches nothing.

## Why this needed a rule

The audit that produced it found **40 claims naming a version this project has
never run**, across five references. Two families were wrong, not just out of date:

- **`ca65 2.19` in seven places.** No ca65 binary here has ever reported 2.19.
  The number is the *cc65 suite* release, which ships `ca65 V2.18` — so the
  claim named the box and the corpus named the tool. Corrected to `V2.18`,
  which is what the ledger prints and what a reader can check.
- **`pasmo 0.5.5` in seven places.** The binary is **PasmoNext v0.1.3**, a fork
  of Julian Albo's Pasmo. Naming upstream for a fork is the worst kind of
  provenance error: it is checkable, it looks right, and a difference between
  the two would be invisible until it bit. Corrected to the identity.

Neither was caught by anything, because nothing else in the build reads prose.
A comment saying "measured against X" is evidence to every future reader, and
it was rotting.

## The versions arbitrated against

Read from the corpus, 2026-08-26:

| Reference | Identity |
|---|---|
| acme | 0.97 |
| ca65 | V2.18 (cc65 suite 2.19) |
| lwasm | from lwtools 4.25 |
| pasmo | PasmoNext v0.1.3 |
| rgbasm | v1.0.3 |
| sjasmplus | v1.21.0 |
| vasm | 2.0b locally, 2.0f in the arbiter |
| asl | Macro Assembler 1.42 Beta [Bld 309] |

`cargo xtask versions` prints this from the corpus rather than from here, so the
list above is a reader's copy and the corpus is the authority.

## Claims that name a superseded version, and stand

These were true when they were made and the tool has moved on since. They are
**not re-measured**: the behaviour each describes has not been checked against
the version now installed, and re-checking one is the work of confirming it, not
of editing it. Rewriting the number without re-running the probe would turn a
true record of an old measurement into a false claim about a new one.

| tool | version | why it stands |
|---|---|---|
| lwasm | 4.19 | macro parameters and byte-string expectations, measured before the 4.24 and 4.25 upgrades |
| lwasm | 4.24 | include-path order, `includebin` negative offsets, conditional vocabulary |
| rgbasm | 1.0 | the `DEF` keyword's required form |
| rgbasm | 1.0.1 | include anchoring at the process directory |
| vasm | 1.9 | conditional and repetition vocabulary, before the 2.0 line |

Clearing a row means re-running its probe against the installed version and
either confirming the behaviour — in which case the claim's version moves — or
finding it changed, which is a bug report.

## Drift triggers

- **"just bump the version in the comment"** — no. The number is a record of
  what answered, not a label. Re-run the probe or add a row above.
- **"the tool's own version string is ugly"** — use it anyway. `ca65 V2.18 - N/A`
  and `PasmoNext v0.1.3 (PC)` are what the reader can reproduce; a tidied
  version is one nobody can check.
- **a new reference is installed** — its first verdict teaches the corpus the
  identity, and the check starts covering it. Nothing to configure.
