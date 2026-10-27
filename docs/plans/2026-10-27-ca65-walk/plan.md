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

### 6d. Functions, visibility and the rest

In five parts, each measured first:

1. ca65's functions of tokens and symbols, the set the old reader supports:
   `.defined`, `.referenced`, `.const`, `.strlen`, `.strat`, `.tcount`,
   `.blank`, `.match`, `.xmatch`, `.definedmacro` and `.ismnemonic`.
2. `.import`, `.export`, `.global`, their zero-page forms and
   `.forceimport`.
3. Messages and assertions: `.out`, `.warning`, `.error`, `.fatal` and
   `.assert`.
4. `.incbin`.
5. What a link gives besides its image, in two parts:
   1. the debug record by segment, which the sidecar, the symbol table and
      the listings are made from, the memory map, and the budgets checked
      against them;
   2. warnings.

The named list should be empty at the end.

## The audit before step 7

[`audit.md`](audit.md) runs the old reader's whole test suite through the
walk and sorts what it still lacks: the source-language gaps, the outputs
besides bytes, diagnostics, linker configs, the old fixtures pinned to ca65
V2.18, and ca65's 13 CPU modes. It proposes 6d.5 (outputs), 6e (the
language gaps), 6f (fixtures on V2.19) and then step 7.

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
| 6c, scopes | `lang198x-ca65` 0.0.4 reports `.scope`, `.proc` and their ends, and lowers `.sizeof` through `Known::size_of`. The walk keeps the scopes, their sizes and each label's size. |
| 6c, segments | `lang198x-ca65` 0.0.6 reports `.segment`, its short forms, `.pushseg` and `.popseg`. The walk keeps each segment's length and address size, and opens an engine `Section` at each change, placed at the end from `ca65_layout`. A hidden `assemble_ca65_nes_lang198x` links as the curriculum does. |
| 6d.1, functions | `lang198x-ca65` 0.0.7 makes the functions of tokens in its expander and lowers `.defined`, `.referenced` and `.const` through `Known`. The walk answers them, and counts each name lowering reads as a use. |
| 6d.2, imports and exports | `lang198x-ca65` 0.0.8 reports what a line declares to a linker. The walk keeps each declaration and checks, in one program, what ca65 and ld65 check between them. |
| 6d.3, messages and assertions | `lang198x-ca65` 0.0.9 lowers `.out`, `.warning`, `.error`, `.fatal` and `.assert` to the engine's `Diagnose` and `Assert`, which the walk passes on. |
| 6d.4, files | `lang198x-ca65` 0.0.10 reports `.incbin`. Hidden `assemble_ca65_lang198x_files` and `assemble_ca65_nes_lang198x_files` read `.include` and `.incbin` through a loader, as the old reader's multi-file entries do. |
| 6d.5.1, the link's record | The engine keeps each section's debug records apart, and the walk names them by segment, as the old reader does. Hidden `_debug` and `_with_config` entry points give the record, the memory map, no origin, and check budgets against the link. |

The scope rules, measured on ca65 V2.19 and recorded as 57 probes in
`tests/differential.rs`:

- A name written without a scope binds at the end of the program to the
  innermost scope around the reference that defines it. Until then, a
  constant from further out is known only by its address size: zero-page
  addressing still applies, but `.ifconst` is false and `.res` refuses it.
- A qualified name's first scope is found from the innermost scope
  outwards. One not opened yet is taken to be in the current scope.
- A cheap local belongs to its scope as well as to the last label.
- `.sizeof` must be known on its line. A label's size is the bytes its line
  made. A closed scope's size is its bytes, and a scope is looked up before
  a label.
- An unnamed `.proc` is an unnamed `.scope`.

The old reader is frozen (#940). Twelve of the new probes are ones it gets
wrong, so they are recorded as gaps against that issue.

`tests/ca65_lang198x.rs` replays the 552 `ca65` verdicts, and all of them
match, so the list of named differences is empty. It also checks that the
walk's warnings and errors say what the old reader's do, since the
verdicts compare bytes only.

It also builds the 51 NES curriculum programs, and all 51 ROMs match the
ones ca65 V2.19 and ld65 built. That half runs where the curriculum is
checked out: CI's replay job, or anywhere `ASM198X_CODE_SAMPLES` is set.

Segments follow ca65 and ld65, measured on V2.19 and recorded as 14 probes:
a program starts in `CODE`; a segment carries on where it left off; a label
takes its segment's address size, the zero page in `ZEROPAGE`; ca65's own
segments keep their address sizes; names are case-sensitive; and only a
segment that holds bytes needs an area in the link.

The seven cases Lang198x once reported as unsupported now match.
`lang198x-ca65` 0.0.5 hands on the tokens it makes rather than text (its
plan `docs/plans/2026-10-27-made-tokens.md`): a name `.ident` makes stays a
name even when it is spelt like a register, and a made string keeps a byte
above 127. It also joins a mid-line `.endrepeat`'s line to the next line,
as ca65 does.

The functions follow ca65 V2.19, recorded as 43 probes. `.defined` and
`.referenced` do not count as a use of the name they ask about, but `.const`
and a plain name do, in the order ca65 reads the line, so
`.byte .ref(v), v, .ref(v)` gives `00 01 01`. The functions of tokens stay
tokens inside another function's token list, since ca65 does not run them
there. `.match` and `.xmatch` compare tokens as ca65's `TokCmp` does: each
register is a token of its own, so `.match(a,b)` is 0. A line's label is
known on its own line, so `l: lda l` under `.org $10` is zero page, which
the walk got wrong before. The old reader gets 12 of the probes wrong, and
they are gaps against #940.

Imports and exports follow ca65 and ld65 under a flat link, recorded as 56
probes. A program here is one translation unit, so an import that is used
is never resolved, and the engine refuses it as an undefined name. An unused
import is fine. An export must be defined by the end, in its own scope or
one further out, and not by `.set`. A `.global` is an export if the program
defines the name, and an import if it does not. Declaring a name again must
agree on its address size, an import may not be exported or defined, and
`.forceimport` is always unresolved. An export counts as a use, an import
does not. ca65 also warns of a zero-page export of an absolute label; the
walk has no warnings yet, and step 7 needs them. The old reader gets 4 of
the probes wrong, and they are gaps against #940.

Messages and assertions follow ca65 V2.19, recorded as 30 probes, all of
which the old reader already matches. `.warning`, `.error` and `.fatal`
read one string and ignore the rest of the line; `.out` takes exactly one.
An assertion is checked once the program is linked, so it may name a label
further on, and `error` and `lderror` refuse a program where `warning` and
`ldwarning` only warn.

Files are read as the old reader reads them, which its multi-file probes
measured: a request is looked for beside the file that asks, then beside
each file that included it, out to the root; `.incbin`'s offset and size
are constants known on the line, a negative size means the rest of the
file, and a window past the data is refused. A macro or scope crosses a
file boundary, as ca65's text does. The live multi-file probes now run each
`ca65-nes` case through the walk too, and every one it reaches links the
ROM ld65 links. Running them found a walk bug, fixed here: `.align` pads
within the segment, not to the absolute address.

They also found what the walk does not reach yet, which the verdicts never
covered, named in `WALK_NOT_YET` in `tests/differential.rs`:

- `.autoimport`;
- `.list`, `.listbytes`, `.pagelen`, `.debuginfo`, `.dbg` and `.fileopt`;
- `.struct`, `.union`, `.enum` and `.tag`;
- `.setcpu`, `.p02`, `.smart`, `.pushcpu` and `.popcpu`;
- `.reloc`;
- a difference of two labels in one segment as a constant, so
  `.ifconst LB-LA` is true.

The old reader supports each of them, so step 7 needs them first.

Replaying the verdicts in 6b also found three places where `lang198x-ca65`
gave a line a different meaning from ca65, fixed there: `*` in a data list,
a label on a conditional's line, and `.sprintf`'s `%c` above 127, which it
wrote as two bytes. 0.0.3 refused that `%c`; 0.0.5 makes the byte.

A linked program's debug record follows the old reader's, which the
Debug198x fixtures pin, and `tests/ca65_lang198x.rs` checks the walk gives
the same record, memory map and image for the fixtures, a set of small
programs and, where the curriculum is checked out, every NES program. The
walk writes the fixtures' sidecars exactly. The record works as follows:

- A segment's number is its place in the layout, and only a segment the
  program put something in is in the record.
- Constants come first, by name, then labels as the program made them.
  Unnamed labels are left out.
- Every statement that makes bytes in the file has a line, including `.res`
  with a value and `.align`. The iNES header has none, and neither does a
  segment with no bytes in the file.
- A macro's lines are the line that called it, as ca65's own debug
  information gives them. A repeat's lines are its own.
- `.org` gives a section an address of its own. The record gives it a
  section of its own, numbered after the layout's, with the address and the
  place in the file its bytes went. A later `.org` that carries on from it
  stays in it.
- `cycles()` and `free()` budgets are checked against the link, so a
  routine is counted within its own segment and an area is the layout's.

ca65 counts addresses in 24 bits, so a linked section may sit above $FFFF.

Running the old reader's tests through the walk found three more language
gaps, for 6e:

- `.org` stays in force across a change of segment, until `.reloc`, and
  counts the bytes every segment makes: in ca65 V2.19, `.org $2000`, then
  a byte in `RODATA`, then `.word *` back in `CODE` writes $2001. The walk
  starts each segment afresh.
- `.reloc` is not lowered.
- A label spelt like a register, such as `a:`, is refused.

## After this step

Step 7, the switch: the walk becomes the default for the main reader, `fmt`
moves onto Lang198x's tree, and the old reader is deleted, as for PasmoNext.
