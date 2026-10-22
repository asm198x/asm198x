# The arbiter container

The image that holds the reference assemblers this project measures itself
against, one pinned version of each. Growth runs in it, re-recording the
verdict corpus; CI replays that corpus with no tools installed.

The corpus records what the tools in use now do: one verdict per case,
regenerated when a tool moves
([`decisions/verdicts-track-current-tools.md`](../../decisions/verdicts-track-current-tools.md)).

## What it has to do

Build each tool at its pin, and report the version line listed in
[`identities`](identities). `verify.sh`, installed as `verify-arbiters`, checks
that every listed tool is present and reports its line, using each tool's own
probe (`--version` or `-v`; asl and p2bin print theirs on the second line after
a non-zero exit, so it matches on a marker rather than a line number).

`identities` is the list of versions in use. Each verdict records the version
that answered it, and growth prunes verdicts from any other version, so a pin
and its line in `identities` change together.

## The pins

| tool | pin |
|---|---|
| `asl` and `p2bin` | the per-build tarball, `ASL_BUILD` |
| `acme` | an SVN trunk revision, `ACME_REV`: ACME publishes no release tarballs or tags |
| `ca65` and `ld65` | the cc65 `V2.19` release tarball |
| `lwasm` | the release tarball, from `https://www.lwtools.ca` (**https**: plain http looks like a dead host) |
| `rgbasm`, `rgblink`, `rgbfix` | a release tag |
| `sjasmplus` | a release tag |
| `vasmm68k_mot` | none: upstream serves one always-current `vasm.tar.gz` |
| `pasmo` (PasmoNext) | a commit: the fork has no tags |

**How the source is obtained can change the version line.** cc65's tools put
their build provenance in it: from a git checkout ca65 reports
`V2.19 - Git <sha>`, and from a release tarball `- N/A` (the 2.19 tarball even
reports `V2.18`).

**vasm has no pin available**, so the image builds whatever upstream currently
serves and `verify-arbiters` decides. When vasm releases again, the image stops
building until `identities` is updated, which turns an unannounced upstream
change into a reviewed bump rather than a silent one.

`asl`'s version line carries no architecture (the triple prints on the next
line), so arm64 and amd64 builds answer identically.
`Makefile.def-unknown-linux` is the right generic definition; the
architecture-specific ones do not build in an arm64 container.

## Bumping a tool

1. Change its pin in the [`Dockerfile`](Dockerfile) and its line in
   [`identities`](identities). The pull request builds and verifies the image
   without publishing it.
2. Merge. `.github/workflows/arbiter-image.yml` publishes the image to
   `ghcr.io/asm198x/arbiter`.
3. Run growth (`.github/workflows/grow.yml`, weekly or by hand). It checks the
   image against the checkout's `identities`, re-records every case, prunes the
   old version's verdicts in each group it re-recorded, and opens a pull
   request whose diff is every answer that moved.
4. Review that diff: each moved answer is a bug of ours the old version hid, or
   the reference changing behaviour, to fix or record as a divergence.

## Running it locally

Run against a **copy** of the worktree, so a growth run cannot touch the real
one:

```sh
docker run --rm -v "$copy:/work" -w /work -e CARGO_TARGET_DIR=/tmp/target \
    ghcr.io/asm198x/arbiter:latest bash -c 'verify-arbiters && cargo xtask grow'
```

On macOS, do not let cargo build into the bind mount. Docker Desktop's shared
filesystem cannot back rustc's memory mapping, and the compile dies with
`SIGBUS`, which reads like a toolchain fault and is not one.
`CARGO_TARGET_DIR=/tmp/target` keeps the build on the container's own
filesystem. A Linux runner does not hit this.
