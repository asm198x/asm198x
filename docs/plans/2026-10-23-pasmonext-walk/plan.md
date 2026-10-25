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

- Lang198x's 5c steps 5 to 8: the Next's banks, reading options,
  three-pass mode, `--86`.
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
- Once the named lists are empty: make the walk the default for `pasmo` and
  `pasmonext`, and delete the old PasmoNext layer.
