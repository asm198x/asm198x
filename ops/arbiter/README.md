# The arbiter container

Work in progress. Building this surfaced a finding that changes the shape of
[#279](https://github.com/asm198x/asm198x/issues/279), recorded here so the next
attempt starts from it rather than rediscovering it.

## What this has to do

Reproduce the eight *behavioural identities* in [`identities`](identities) — the
version self-reports the verdict corpus is keyed on. A different binary is fine
and expected; the corpus records a digest as provenance so a second build
corroborates rather than forks. A different identity string is not fine, and
would key every verdict recorded here separately from all 6,908 already held.

`verify.sh` checks, using the same probe rules as the harness's own table
(`--version` vs `-v`, asl and p2bin printing theirs on the second line after a
non-zero exit, matched on a marker rather than a line number).

## Where the pins stand

| tool | share of corpus | pinnable today |
|---|---|---|
| `asl` 1.42 Beta [Bld 309] | 45% | **yes** — upstream publishes a tarball per build number; verified to fetch, build and install |
| `pasmo` PasmoNext v0.1.3 | 14% | repository reachable, but carries no tags — a commit SHA is the only pin |
| `ca65` V2.18 (cc65 V2.19) | 9% | **yes** — tag. Note the self-report lags the release |
| `rgbasm` v1.0.3 | 8% | **yes** — tag |
| `lwasm` lwtools 4.25 | 8% | **no** — `lwtools.ca` did not respond (40s, twice) |
| `acme` 0.97 "Zem" | 6% | **no** — the GitHub mirror has no tags; Debian ships `0.97~svn20211115`, a later snapshot |
| `vasmm68k_mot` vasm 2.0b | 6% | **no** — upstream serves `vasm.tar.gz`, always the current release, with no versioned archive |
| `sjasmplus` v1.21.0 | 4% | **yes** — tag |

Four of eight pin cleanly. **Roughly 28% of the corpus has no stable upstream
URL today**, and `pasmo`'s 14% pins only to a commit.

## What that means

Fetching from upstream at build time cannot give a reproducible arbiter. A tool
whose source moves, or whose host is down, either fails the build or — worse —
succeeds with a different version and reports a different identity.

The container therefore needs the exact source archives held somewhere durable,
which is a decision about storing artefacts rather than a technical one. Until
that is settled this image is not reproducible, and growth must not run in it:
`verify.sh` exists to make that failure loud rather than silent.

## Also worth knowing

`asl`'s identity carries no architecture — the triple prints on the line *after*
the one the corpus keys on — so an arm64 and an amd64 build are interchangeable
for keying. `Makefile.def-unknown-linux` is the right generic definition; the
arch-specific ones do not build under an arm64 container.
