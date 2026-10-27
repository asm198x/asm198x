# ca65's walk over Lang198x's front end

Step 6 of Lang198x's
[ca65 front-end plan](https://github.com/lang198x/lang198x/blob/main/docs/plans/ca65-front-end.md):
lowering and the walk, with the gate in Asm198x. It follows
[PasmoNext's walk](../2026-10-23-pasmonext-walk/plan.md), which did the same
for the `pasmo` dialect.

## Where things stand

`lang198x-ca65` 0.0.2 reads ca65 V2.19 source and runs all of its
preprocessing: macros, `.define`, `.repeat`, the `.if` family, token
functions and includes. Its `expand::Expander` hands each finished line to a
`Host` that the caller supplies. The host answers `.if` conditions and
supplies the text of included files. The crate has no lowering yet.

Asm198x's main ca65 reader (`dialects/ca65/`, `ca65_flat.rs`) is an engine of
its own. It parses, sweeps conditionals and repeats, places segments and
links the NES image itself, using the shared engine only for `lay_out`.
`decisions/sections-in-the-shared-engine.md` calls this "a ca65 engine" and
holds that there should be none: a dialect with sections feeds them to the
shared engine.

## Settled

- **Lowering lives in `lang198x-ca65`,** behind its optional `ir` feature, as
  in `z80-asm`. It gives each line the meaning ca65 gives it. Names stay
  names, and a `Known` trait answers what the line needs at once, such as
  whether a name is known to be on the zero page.
- **The walk lives in Asm198x** (`dialects/ca65_walk.rs`). It keeps what
  ca65 ties to its symbols: passes, the address, scopes and `.proc`,
  segments, and imports and exports. It implements the expander's `Host`.
- **The walk feeds the shared engine.** Each segment becomes an
  `Operation::Section`, placed from the existing bounded layout
  (`ca65_layout`, the NES default or a `-C` config), so the engine lays out
  and serialises the image. The old reader's engine stays the default until
  step 7, and then goes.
- **The gate is the recorded verdicts.** The walk must give ca65's verdict
  for every `ca65` record in `tests/verdicts/6502.ndjson` (352: 151 form,
  100 fuzz, 90 probes, 11 of them divergences) and every `ca65-nes`
  curriculum digest (51). The 90 `ok` and 11 `gap` cases in
  `tests/differential.rs` count too.
- **Differences are named.** A gate test lists each difference with the
  step that covers it, and the list may only shrink. Where Lang198x follows
  ca65's manual over ca65 V2.19 (listed below), the gate expects the
  manual's result, and the verdict is recorded as a divergence.
- **The 65816 and HuC6280 readers wait for stage 4.** This step is the 6502
  main reader only.

## Steps

Each step is one pull request that passes format, Clippy, tests and the
coverage floor in its repository.

### 6a. Lowering (`lang198x-ca65`)

`lower::line` and `lower::expression` behind the `ir` feature, with the
dependency on `asm198x-ir` through crates.io. It covers instructions, with
ca65's address-size rules (zero page when the value is known to fit, `z:`,
`a:` and `f:` to force one), labels, cheap locals and unnamed labels as
names, assignments, and the data and space commands (`.byte`, `.word`,
`.addr`, `.dbyt`, `.res`, `.align`, `.org`). Scope, segment and visibility
commands are reported to the caller, as `z80-asm` reports `PROC`, not
lowered. Each rule is a measured case.

### 6b. The walk behind a switch (Asm198x)

`dialects/ca65_walk.rs`, reached through hidden
`assemble_ca65_lang198x` and `assemble_ca65_lang198x_files` entry points,
with flat programs first: one segment, no imports. The gate test,
`tests/ca65_lang198x.rs`, replays the verdicts above and records where the
walk stands, with every difference named.

### 6c. Scopes and segments

`.proc`, `.scope` and their name lookups, then `.segment`, `.pushseg` and
`.popseg` as sections placed by the layout, so the NES curriculum digests
can pass.

### 6d. Visibility and the rest

`.import`, `.export`, `.global` and their zero-page forms, `.incbin`,
messages and assertions, and the debug capture the CLI's `--debug` writes.
The named list should be empty here.

## Where Lang198x follows the manual over ca65 V2.19

These are recorded in
[lang198x/ca65's preprocessing plan](https://github.com/lang198x/ca65/blob/main/docs/plans/2026-10-27-preprocessing.md).
Each one needs a probe whose verdict is a divergence:

- `.exitmacro` inside a repeat inside a macro leaves at once.
- `.end` inside a repeat ends the program, where ca65 fails an internal
  check.
- `.sprintf` follows C for `%x` of a negative number, `%#x` of 0, and the
  width of `%c` and `%s`.
- `.mid` takes a start above 100 as given.

These are refused or reported as unsupported, where ca65 accepts them:

- a `.endrepeat` after other tokens on its line, in a body that runs;
- a string with a quote or a line break made by a token function, and
  `.ident` of a register's name;
- text after the file name on an `.include` line.

## Where the walk stands

| Step | State |
|---|---|
| 6a, lowering | Done in `lang198x-ca65` 0.0.3: expressions, instructions with ca65's address sizes, labels, assignments, data and space commands. |
| 6b, the walk | `dialects/ca65_walk.rs`, behind the hidden `assemble_ca65_lang198x`. It reads the program once, keeps what ca65 knows of each name, names cheap locals and unnamed labels, and makes `.org` the engine's `PseudoPc`. |

`tests/ca65_lang198x.rs` replays the 352 `ca65` verdicts. At the start of
6b, 338 match. The 14 others are named in the test:

- 5 need scopes and `::` (6c);
- 2 need `.ref`, `.const` and `.tcount` (6d);
- 7 are ones Lang198x reports as unsupported: `.ident` of a name that
  reads as a register (5), a label before `.endrepeat`, and a byte above
  127 made by `.sprintf`. Each waits for a decision on how to hold such
  text.

Replaying the verdicts also found three places where `lang198x-ca65` gave a
line a different meaning from ca65, fixed there: `*` in a data list, a
label on a conditional's line, and `.sprintf`'s `%c` above 127.

The 51 NES curriculum digests wait for segments (6c).

## After this step

Step 7, the switch: the walk becomes the default for the main reader, `fmt`
moves onto Lang198x's tree, and the old reader is deleted, as for PasmoNext.
