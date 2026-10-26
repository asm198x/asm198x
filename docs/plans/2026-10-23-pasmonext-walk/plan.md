# PasmoNext's walk over Lang198x's front end

Step 6 of Lang198x's
[PasmoNext profile plan](https://github.com/lang198x/z80-asm/blob/main/docs/plans/2026-10-23-pasmonext-profile.md):
the gate in Asm198x. The design is recorded in
[`decisions/pasmonext-walk.md`](../../../decisions/pasmonext-walk.md).

## Done in this pull request

1. **The dependency:** `lang198x-z80-asm` with its `ir` feature, and a
   `[patch.crates-io]` that makes its `asm198x-ir` the workspace crate.
   `crates/asm198x-web` is its own workspace root, so it repeats the patch.
   Checked: `cargo tree -d` shows one `asm198x-ir` in both, also after a
   simulated 0.0.73 release.
2. **The walk:** `dialects/pasmo_walk.rs`, behind `Pasmo::lang198x`, reached
   through the hidden `assemble_pasmo_lang198x` and
   `assemble_pasmo_lang198x_files`. The shared Z80 core stays the default.
3. **The gate test:** `tests/pasmonext_lang198x.rs` replays every committed
   `pasmo` and `pasmonext` verdict through the walk, including the curriculum
   digests when code-samples is checked out (1,229 checked), and the 88
   measured programs; the 3 run with options wait for the step that takes
   them. Differences must be named, with the step that covers them, and the
   list may only shrink.

## Measured at the start

| Corpus | Shared Z80 core | Lang198x and the walk |
|---|---|---|
| `pasmo` verdicts (1,069) | 1,069 | 1,067: two use `LOCAL` |
| Curriculum programs against PasmoNext 0.1.3 (391) | 388 | 391 |
| Measured programs, two-pass (72) | not run | 71: `org 5 junk` waits for a parser rule |
| Measured programs, three-pass (13) | not run | all 13 reported as three-pass mode |

## Next

- Lang198x's 5c steps 6 to 8: reading options, three-pass mode, `--86`.
  - **Deferred, `--86`** (2026-10-26): it waits for 8086 support in Isa198x
    and Asm198x, which is the CPU roadmap's new-encoding-model tier. Neither
    has it yet.
  - **Done, three passes:** a value that moves in pass 2, or `--pass3`,
    runs a third, and a value that moves in it is a phase error. Every
    three-pass program in the fixture now runs; six are named deviations,
    where PasmoNext keeps an earlier pass's bytes or values. A test checks
    the "Switching to 3 pass mode" warning against PasmoNext's.
  - **Done, reading options:** `--bracket` and `--w8080`, as
    `PasmoNextOptions::brackets_only` and `warns_8080`. Seven `--bracket`
    programs measured on PasmoNext join the fixture, and all match. A test
    checks that the 8080 warning is given once a line, where PasmoNext gives
    it on both passes, and follows the rule for the two lines it gets wrong.
  - **Done, the Next's banks:** `??`, `SETBANK` and `SAVEBIN`, with a new
    IR operation, `SaveImage`, for a file of the flat image. 47 programs
    measured on PasmoNext join the fixture; all match but two `SAVEBIN`
    deviations, and a test checks each file saved against PasmoNext's.
  - **Done, messages:** `.ERROR`, `.WARNING`, `MESSAGE` and `HEX`. 46
    programs measured on PasmoNext join the fixture; all match but five
    `HEX` deviations, and a test checks what each message says.
  - **Done, structures:** `STRUCT`/`ENDS`, `RB`, `RW`, `RSSET` and `STACK`,
    through z80-asm's `Definition`. 80 programs measured on PasmoNext join
    the fixture; all match but nine, each a named deviation where PasmoNext
    has an obvious bug.
  - **Done, scopes' options:** `--nocase`, `--equ`/`-E` and `--alocal`,
    through `assemble_pasmo_lang198x_files_with`. The gate now runs the
    measured programs that use them, instead of skipping every program with
    options; 56 join the fixture, and all match. Four `$`-in-name programs
    join too, three of them named deviations.
  - **Done, scopes' directives:** `LOCAL`, `PROC`, `ENDP` and `PUBLIC`,
    through `lang198x-z80-asm`'s `Scope`. Both `LOCAL` verdicts now match,
    so the named list is empty; 56 scope programs measured on PasmoNext join
    the fixture, and all match.
- Record the measured programs as verdicts (asm198x#963), so the old path's
  differences are tracked too.
- **Done, the walk is the default** (2026-10-26) for `pasmo`, `pasmonext`,
  `z80` and `z80n`, through the library, the CLI and the web shell. Steve
  agreed to switch once the named lists were empty.
  - The CLI takes PasmoNext's own `--equ` (and `-E`), `--nocase`,
    `--alocal`, `--bracket`, `--w8080` and `--pass3`. `--equ` is read as
    PasmoNext reads it, and a refused text is reported on its line of
    `<command-line --equ>`.
  - Diagnostics keep the frozen contract: expansion frames with the
    `MACRO` line, the line each include was written on, the operand's
    column, and the single-source entry's pointer to the multi-file one.
  - Over the 618 Spectrum curriculum sources, the old and new paths give
    the same bytes, symbols, debug lines and cycles for every program that
    assembles. Twelve file tables differ, because the walk, like PasmoNext,
    reads an `INCLUDE` in an untaken branch.
  - Tests the old path held that PasmoNext contradicts were measured and
    corrected, among them `ld hl,(1+2)*3`, `mul d,e`, `5 MOD 2`, an untaken
    `INCLUDE`, `DM`, and the line an error in a macro body names.
- **Done, `fmt`** (2026-10-26): `pasmo` and `pasmonext` format with
  Lang198x's `format`, which never adds, removes or reorders lines. The old
  formatter put a label on a line of its own and added colons; this one moves
  only the label, statement and comment columns. PasmoNext 0.1.3 assembled
  the 618 Spectrum curriculum sources before and after formatting (1,018
  files changed) to the same bytes and symbol files for all 614 it
  assembles, and gave the same verdict on all 1,230 verdict sources.
  `tests/fmt_pasmonext_gate.rs` keeps the check, through the walk.
- Done, one path (2026-10-26). The pasmo dialect has no choice of front end
  left: assembly always reads Lang198x's tree and runs the walk. The hidden
  `assemble_pasmo_lang198x*` entry points are gone, and `PasmoNextOptions`
  is public beside `assemble_pasmo_files_with`. The gate's verdict replay
  duplicated `verdict_replay.rs`, which now runs the same path, so it went
  too; the measured programs stay, as `tests/pasmonext_measured.rs`.
- Still on the shared Z80 core: `convert`'s pasmo-to-sjasmplus AST, through
  `PasmoSyntax`. It goes when the converter reads Lang198x's tree, which
  waits for SjASMPlus's profile, since the converter writes SjASMPlus. The
  shared Z80 core itself stays until SjASMPlus moves.
