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
(code-samples `5bade0fa`):

| Result | Sources |
|---|---|
| Same bytes | 272 |
| Asm198x accepts, the reference rejects | 15: asm198x#967 (ACME), #971 (PasmoNext) |
| Different bytes | 12: #968 (ACME), #969 (vasm), #970 (Game Boy ROM layout) |
| Both reject | 4: code-samples#183 (two), #216 (NES `tile-map`), and `two-word-parser`, which needs its generated include |

The curriculum side is code-samples#216.

## The table

`verdict_corpus::patterns::BUILDS`: machine, CPU, reference tool, the corpus
variant, and the reference command's form. Measured to match the curriculum's
builds:

- **pasmo** (15 Z80 machines): `pasmo -I <pattern> <source> <out>`.
- **acme** (11 6502 machines): `acme -f plain -I <pattern> -o <out> <source>`.
- **ca65** (NES): `ca65 -I <pattern>`, then `ld65 -C` with
  `nintendo-entertainment-system/assembly/dash/unit-01/nes.cfg`, the layout
  Asm198x's built-in NROM config matches.
- **lwasm** (Dragon 32): `lwasm --6809 --raw -I <pattern>`.
- **vasm** (Amiga): `vasmm68k_mot -Fhunkexe -kick1hunks -nosym -quiet -I <pattern>`.
- **rgbasm** (Game Boy): `rgbasm -I <pattern>`, `rgblink -t`, `rgbfix -v -p 0xFF`.

Each command runs in the source's own folder. That builds both kinds of
`mistakes/` file: those that include from the pattern folder, and those that
include `../sample.inc`.

## Steps

1. **The rule and the table,** in `verdict_corpus`:
   - `patterns::sources(machine)`: every `.asm` and `.s` under
     `patterns/assembly/`, except `verification/` and `build/`.
   - **The check:** each pattern Makefile's `--dialect` must agree with the
     table, and every machine with pattern sources must be in it.
2. **Recording,** in the live curriculum suite (`--ignored`). Each source's
   reference verdict goes into the corpus: a digest of the bytes, or a
   rejection. The key digests the root file *and every file it includes*, so
   a change to a shared `cross-platform/` routine makes the verdict stale
   rather than silently old. Where Asm198x's existing path differs, the verdict
   is a tracked divergence naming its issue.
3. **Replay:** both PasmoNext paths, and every other dialect's existing path,
   replay the pattern verdicts from the source's folder. A divergence must
   still diverge, so a fix shows as a stale marker to remove.
4. **Parity:** machines are enumerated from code-samples, not from the
   corpus, so a machine with sources and no verdicts fails the check.

## Checks

- **The check can fail:** with one machine removed from the table, or one
  Makefile's dialect changed, it must fail.
- **Recording is repeatable:** run twice, the corpus diff is empty.
- **Replay is real:** in CI, the Replay job must reach every pattern verdict,
  as the PasmoNext gate does since asm198x#966.
