# The arbiter container

Stage one of [#279](https://github.com/asm198x/asm198x/issues/279): a container
that reproduces every arbiter identity the verdict corpus is keyed on. It does.

Growth does not run here yet — that is the next stage, and the first thing it
must prove is that a rebuilt binary **corroborates** an existing verdict rather
than alarming on it.

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

All eight identities reproduce. `docker run --rm asm198x-arbiter:bld309` says so.

| tool | share | pin |
|---|---|---|
| `asl` 1.42 Beta [Bld 309] | 45% | `asl-current-142-bld309.tar.gz` — upstream publishes one tarball per build |
| `pasmo` PasmoNext v0.1.3 | 14% | commit `60957e69` — the fork has no tags |
| `ca65` V2.18 (cc65 V2.19) | 9% | the release **tarball** for `V2.19`, not a clone — see below |
| `rgbasm` v1.0.3 | 8% | tag `v1.0.3` |
| `lwasm` lwtools 4.25 | 8% | `https://www.lwtools.ca/...` — **https**; an `http://` attempt looks like a dead host |
| `acme` 0.97 "Zem" | 6% | SVN trunk at **revision 266**, from the Homebrew formula that built the local copy |
| `sjasmplus` v1.21.0 | 4% | tag `v1.21.0` |
| `vasmm68k_mot` vasm **2.0f** | 6% | none available — adopted, see below |

## Two things the identity strings taught

**How the source is obtained can change the identity.** `ca65` puts its build
provenance in its version line: from a git checkout it reports
`V2.18 - Git 5552824`, and from a release tarball `V2.18 - N/A`, which is what
the corpus holds. A clone and a tarball of the same tag are not interchangeable
here.

**`vasm` has no pin and was adopted rather than faked.** Upstream serves a
single `vasm.tar.gz`, always current, with no versioned archives — and it had
already moved from the 2.0b the corpus first recorded to 2.0f. Pinning to a
version nobody can fetch would be a pin in name only, unreproducible for the
next person as well. So the image builds current and `verify-arbiters` decides:
when upstream releases again the check fails and the image will not build,
turning a silent drift into a decision. The 436 verdicts recorded under 2.0b
are superseded and re-arbitrated under 2.0f separately.

## Also worth knowing

`asl`'s identity carries no architecture — the triple prints on the line *after*
the one the corpus keys on — so an arm64 and an amd64 build are interchangeable
for keying. `Makefile.def-unknown-linux` is the right generic definition; the
arch-specific ones do not build under an arm64 container.

## Stage two: does a rebuilt binary corroborate?

Run against a **copy** of the worktree, so a growth run cannot touch the real
corpus:

```sh
docker run --rm -v "$copy:/work" -w /work -e CARGO_TARGET_DIR=/tmp/target \
    asm198x-arbiter:bld309 bash -c 'verify-arbiters && cargo xtask grow'
```

5,844 records appended, and the split is the answer:

| | records |
|---|---|
| sharing an existing key — **corroboration** | **5,508** |
| new key, `vasm 2.0f` | 251 |
| new key, `lwasm` | 85 |
| alarms | **0** |

**The digest-as-provenance design works.** A binary built on Debian/arm64,
reporting the same behavioural identity as one built by Homebrew on
macOS/arm64, was recognised as the same fact rather than forking it. That is
KTD4's first real test and it had never had a second machine to run on.

The 251 `vasm` records are correct: a new identity is a new key, not a conflict.
The 85 `lwasm` records are 6809 **fuzz** cases — growth the fuzzer produces that
no ordinary pull request ever commits, because in any given change it looks
incidental. A dedicated growth run is exactly where they belong.

## What adopting vasm 2.0f costs

Not free. `vasm_refuses_its_import_side_words_for_a_binary` fails under 2.0f:

```
3 import-side probe(s) disagree:
  `xref` with the name defined: Bytes([1])
  `import` with the name defined: Bytes([1])
  `nref` with the name defined: Bytes([1])
```

Under 2.0b these were refused when emitting a binary, and the directive surface
declares them `RefusedByReference` on that basis. 2.0f accepts them. The house
rule is to match the reference rather than out-converge it, so the declaration
is now wrong and needs updating with the version — alongside re-arbitrating the
436 verdicts keyed to 2.0b.

## Running it on macOS

Do not let cargo build into the bind mount. Docker Desktop's shared filesystem
cannot back rustc's memory mapping and the compile dies with `SIGBUS`, which
reads like a toolchain fault and is not one. `CARGO_TARGET_DIR=/tmp/target`
keeps the build on the container's own filesystem. A Linux runner does not hit
this.
