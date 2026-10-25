# Verdicts for the pattern library

Code198x's pattern library has 303 assembly sources on 30 machines, and none
has a recorded verdict. For 26 of those machines it is all the assembly there
is. The curriculum rule in `verdict_corpus::curriculum` counts only game units
(`<machine>/assembly/<game>/unit-*/`), and the parity check counts only machines
that already have verdicts, so the gap could not show. Steve chose (2026-10-24)
to describe each machine's build in a table that a test checks against the
pattern Makefiles.

## Measured first

Every source was assembled from its own folder, with its pattern folder as an
include path, on its machine's reference tool and on Asm198x 0.0.72's CLI
(code-samples `5bade0fa`). A first pass used the tools installed on a
development machine. The recorded verdicts come from the versions
`ops/arbiter/identities` pins, which differ in one place: vasm 2.0f builds the
two Amiga `input-to-sound` sources as Asm198x does, where vasm 2.0b did not.

| Result | Sources |
|---|---|
| Same bytes | 275 |
| Asm198x accepts, the reference rejects | 15: asm198x#967 (ACME), #971 (PasmoNext) |
| Different bytes | 10: #968 (ACME), #969 (vasm), #970 (Game Boy ROM layout) |
| Both reject | 3: code-samples#183 (two) and #216 (NES `tile-map`) |

`two-word-parser` matches once its generated include is built. The
curriculum side is code-samples#216.

## The pin moves

The curriculum pin, `f11698d0` (2026-08-27), predates the pattern library:
the checkout there has no pattern sources. Recording them moves the pin to
`5bade0fa` (2026-10-23), so every curriculum verdict is re-recorded there too,
and the old program digests are pruned as growth prunes them.

## The table

`verdict_corpus::patterns::BUILDS`: machine, CPU, reference tool, the corpus
variant, and the reference command's form. Measured to match the curriculum's
builds:

- **pasmo** (15 Z80 machines): `pasmo -I <pattern> <source> <out>`. PasmoNext
  accepts Z80N instructions by default, so its Asm198x counterpart is
  `--dialect pasmonext` (variant `pasmonext`), though the Makefiles say
  `pasmo`.
- **acme** (11 6502 machines): `acme -f plain -I <pattern> -o <out> <source>`.
- **ca65** (NES): `ca65 -I <pattern>`, then `ld65 -C` with
  `nintendo-entertainment-system/assembly/dash/unit-01/nes.cfg`, the layout
  Asm198x's built-in NROM config matches.
- **lwasm** (Dragon 32): `lwasm --6809 --raw -I <pattern>`.
- **vasm** (Amiga): `vasmm68k_mot -Fhunkexe -kick1hunks -nosym -quiet -I <pattern>`.
- **rgbasm** (Game Boy): `rgbasm -I <pattern>`, `rgblink -t`, `rgbfix -v -p 0xFF`.

Each command runs in the source's own folder, with the pattern folder given
relative to it (`.`, or `..` from `mistakes/` and `verification/`), so a
reference's message names the same paths on every machine. That builds both
kinds of `mistakes/` file: those that include from the pattern folder, and
those that include `../sample.inc`.

One source, `two-word-parser`, includes a file its Makefile generates.
`patterns::GENERATED` names the generator; recording and replay run it into a
scratch folder and add that folder to the include path.

## Steps

1. **The rule and the table,** in `verdict_corpus`:
   - `patterns::sources(machine)`: every `.asm` under `patterns/assembly/`,
     except in `build/` folders, which builds write. `mistakes/` and
     `verification/` programs are included.
   - **The check:** each pattern Makefile's `--dialect` must agree with the
     table, and every machine with pattern sources must be in it.
2. **Recording,** in the live curriculum suite (`--ignored`). Each source's
   reference verdict goes into the corpus: a digest of the bytes, or a
   rejection. The key digests the root file *and every file it includes*, so
   a change to a shared `cross-platform/` routine makes the verdict stale
   rather than silently old. The closure is the set of files Asm198x's
   include walk loads, recorded by a loader wrapper. A verdict records only
   what the reference did (a digest, or a rejection). Where Asm198x's existing
   path differs, the source is listed in `patterns::KNOWN` with its issue, and
   the divergences page lists it.
3. **Replay:** the existing path replays through the CLI, as the Makefiles
   run it, because the Game Boy ROM is finished there (`--gb-rom`). The
   Lang198x path replays Z80 pattern verdicts in the PasmoNext gate. A listed
   difference must still differ, so a fix shows as an entry to remove.
4. **Parity:** machines are enumerated from code-samples, not from the
   corpus, so a machine with sources and no verdicts fails the check. Pattern
   counts go in their own section, so the site's per-machine curriculum
   figures do not change. Parity checks a pattern source by path; whether its
   verdict still describes its text is the replay's check, since only
   assembling a source finds its include closure.

## Checks

- **The check can fail:** with one machine removed from the table, or one
  Makefile's dialect changed, it must fail.
- **Recording is repeatable:** run twice, the corpus diff is empty.
- **Replay is real:** in CI, the Replay job must reach every pattern verdict,
  and every pattern source must have one, as the PasmoNext gate does since
  asm198x#966.

## Recording

Growth records at the pin, in the arbiter image, so moving the pin and the
first pattern verdicts land through a Growth run dispatched on the branch.

The pattern suite was first recorded on an Apple Silicon Mac, where the
arbiter image is amd64 and its rustc crashes under emulation. The suites ran
natively, with wrapper scripts on `PATH` that `docker exec` each tool in a
long-running arbiter container, `/Users` and the temporary folders mounted at
the same paths. Docker Desktop's file sharing made that unreliable in two
ways, so the suite now guards against both:

- After the host deletes a file, a tool in the container can fail to create
  one at the same path: 29 of 30 rewrites of one output failed. Each source
  now builds in a fresh folder.
- A tool that cannot write its output is not a rejection. Its complaint
  ("Cannot open output file", "Error creating object file") is an error the
  run reports, never a verdict.

A tool can also hang in the container past `kill`, and only restarting
Docker clears it. So locally recorded verdicts are a cross-check, and the
Growth run is the record.
