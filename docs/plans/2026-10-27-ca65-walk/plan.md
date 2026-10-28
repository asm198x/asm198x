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
| 6d.5.2, warnings | The walk warns, as ca65 V2.19 does, of a name exported to the zero page with an absolute address size, and, as ld65 does, of a `bss` or `zp` segment that holds a byte other than zero. |

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

Running the old reader's tests through the walk found two more language
gaps, for 6e:

- `.org` stays in force across a change of segment, until `.reloc`, and
  counts the bytes every segment makes: in ca65 V2.19, `.org $2000`, then
  a byte in `RODATA`, then `.word *` back in `CODE` writes $2001 (done in
  6e).
- `.reloc` is not lowered (done in 6e).

The same run found a label spelt like a register, `a:`, which one of the
old reader's tests accepts. ca65 V2.19 refuses it ("Expected
‘end-of-line’ but found ‘a:’"), and so does the walk.

Warnings follow ca65 V2.19 and ld65, measured. A name exported to the
zero page warns if its address size is absolute, constant or label alike:
`K = $1234` warns where `K = 7` does not, which the old reader gets wrong.
The warning is at whichever of the declaration and the definition comes
second. ld65 warns once for each `bss` or `zp` segment that holds a byte
other than zero, at the first line that wrote one; a reservation writes
nothing, so it never warns, whatever the layout fills with. ca65 gives an
equate of a name defined later its warning at the equate's own line, which
waits for the forward equate in 6e.

### 6e, the language gaps

Each gap is measured on ca65 V2.19 first and recorded as a probe.

**Equates of names defined further on.** ca65 works out an equate when it
is used, so `other=value` before `value=$34` is fine, and chains resolve in
whatever order they need. The engine now lets a dialect defer such an
equate to the end of its first pass (`defers_equates`), and refuses one
that reads itself, directly or not, as ca65 does: "Circular reference in
definition of symbol". The walk keeps each equate that read a name nothing
was known of, and works it out again when it is used. So `lda other` is
absolute before `value` is defined and zero page after, as in ca65. Inside
a scope, the name stays the scope's own until the end, so it stays
absolute. 13 probes; the old reader gets the scoped one wrong, a gap
against #940.

ca65 refuses a circle of equates at once, with one exception: an equate in
a scope that reads its own name while a name further out is defined, such
as `red=red+1` under a top-level `red`, is refused only where it is used,
even behind a lazy `&&` or `||`. The engine lets a dialect leave such an
equate without a value (`leaves_equate_unresolved`), so only a use of it
fails. Six probes record it.

**`.org` and `.reloc`.** In ca65 V2.19, `.org` stays in force across a
change of segment, counting every segment's bytes, until `.reloc`
(`lang198x-ca65` 0.0.11 lowers it). In a linked program the walk opens a
section at each, with the address `.org` gave or where the segment has
got to; a section with nothing in it is left out of the debug record. A
flat program closes each `PseudoPc` block `.org` opened.

**Lines a macro makes.** They belong to the outermost call, which
`lang198x-ca65` 0.0.11 now names (`Expander::macro_call`), so a call with
no label, which the walk never sees, is right too.

**Cheap locals and equates.** ca65 reads an assignment's value before its
name starts a new cheap local scope, and a label before the rest of its
line; the walk now does both.

**Settings that make no bytes.** `lang198x-ca65` 0.0.12 checks the CPU
commands, the switches (`.smart`, `.autoimport`, `.list`, `.debuginfo`),
`.listbytes`, `.pagelen`, `.fileopt` and the hand-written forms of `.dbg`
as ca65 V2.19's `pseudo.c` does, and then they have no effect. Only the
6502 profile exists, so `.setcpu "6502"` and `.p02` change nothing, and
another CPU is not lowered yet; that is stage 4. The walk keeps ca65's
stack of eight CPUs: it refuses a ninth `.pushcpu` and a `.popcpu` with
nothing kept, and warns "CPU stack is not empty" at the last line read.
`.condes`, `.constructor`, `.destructor` and `.interruptor` export their
name, with the type and priority checked; the tables ld65 builds from them
are stage 5. 18 probes; the old reader gets `.condes` wrong, a gap against
#940.

**Distances between labels.** ca65 V2.19 keeps a label as an offset into
its segment, so two labels in one segment are a known distance apart once
both are defined. That holds with another segment's lines between them,
across `.align`, and across `.org` and `.reloc`, whose bytes the segment
still counts. `.res r-l`, `.if r-l` and `.ifconst r-l` then read a
constant, and so do `r*2-l*2` and `r+r-l-l`; any other operation, such as
`r&$FF-l&$FF`, waits for the link. The distance keeps its labels' address
size, so `lda r-l` is absolute. Labels in two segments are not a known
distance apart, nor is a label under `.org` from one outside it, nor plain
names read from inside a scope, which stay the scope's own until it
closes. `lang198x-ca65` 0.0.13 adds `Value::Placed` and `Value::Measured`
for this, and the walk numbers each segment and says how far into it each
label is. 19 probes; the old reader gets 12 wrong, gaps against #940.

**Records.** `lang198x-ca65` 0.0.14 lowers `.struct`, `.union`, `.enum`
and `.tag` as ca65 V2.19's `struct.c` and `enum.c` read them, and the walk
counts the offsets. A named record is a scope whose members are constants
counted from zero; a struct's members follow one another, and a union's all
start at the same place, which makes it as big as its biggest member. A
record with no name inside another carries on from where that one has got
to, and puts its members in the same scope; a named one starts again at
zero, and the record around it makes room for it. `.org` in a struct moves
the next member but not the size. An enum's member is the last value
written plus a count, worked out when it is used, so it may be a name
defined further on. An enum has no size. `.tag` in a struct takes its count
with no comma; in the program it makes room for the record, as `.res`
does. A macro's lines inside a record are members. 44 probes; the old
reader gets 7 wrong, gaps against #940. With records done, every
multi-file ca65 probe runs through the walk.

**Data that must fit its room.** ca65 refuses a one-byte value it learns
only at the link, such as `.byte lab` or `lda #lab` for a label in a
segment, if its address size is bigger than a byte, even when the value
would fit. It checks once it has read the program, so a name defined
further on is judged by what it turns out to be. `lang198x-ca65` 0.0.15
sizes expressions as ca65's `StudyExpr` does, read from its listings: in
names and numbers added, taken away, negated or multiplied by a number,
only the names count, so `zz + $100` is on the zero page; `&` takes the
smaller side, so `lda lab & $FF` is on the zero page too, where the walk
used to make it absolute; `<`, `>`, `^`, comparisons and the logical
operators make one byte; anything else counts its numbers as well. The
walk checks each `.byte` value and immediate operand at the end. 31
probes; the old reader gets 3 wrong, gaps against #940.

**Imports in a scope.** An `.import` in a scope makes the name the
scope's own, so it hides a name of the same spelling further out, as a
definition would, even for a use read before the `.import`. The import is
not defined, so `.defined` and `.ifdef` say no, and its size is the one
declared, so `.importzp` is on the zero page. The walk read the outer name
instead and accepted programs ca65 refuses when it cannot find the import.
12 probes.
**Every error.** The owner chose to report every error, as ca65 does,
rather than stop at the first. A failure now carries the errors after the
first in `AsmError.further` (a dated note in
[`core-contract-freeze.md`](../../../decisions/core-contract-freeze.md)),
which the CLI renders one by one and its JSON failure output lists. The walk
reports an error and carries on with the next line, as ca65 V2.19 does. A
line the parser could not read is abandoned, so it gets one error. Then, as
in ca65, it checks the names at the end only if it found no error on the
way, and one-byte data only if the names are sound. ca65 lists undefined
names in its symbol table's order, and the walk in the order they were
used.

The engine carries on too, for a dialect that says it should
(`Dialect::reports_every_error`, which only the walk does): when a statement
fails in the second pass, the error is kept, the statement is put back as
room of the size the first pass gave it, so every later address stays put,
and the pass runs again. So every `.error` and every value out of range is
reported, as in ca65, and `.fatal`, now its own `DiagSeverity::Fatal`, stops
at once. It gives up after 100 errors.

What still differs from ca65:

- ca65 refuses a constant out of range, such as `.byte 300`, as it reads the
  line, so a syntax error elsewhere does not hide it; the walk leaves it to
  the engine, which does not run once the walk has found an error. Lowering
  could refuse it at the line instead, with ca65's message, "Range error (300
  not in [0..255])".
- An error inside `.repeat` is at the line inside it; ca65 names the line of
  `.endrepeat`.
- `lang198x-ca65` lowers `.fatal` as `.error`, and the walk turns it into a
  fatal one; lowering can say so itself once an `asm198x-ir` release carries
  the new severity.

## After this step

Step 7, the switch: the walk becomes the default for the main reader, `fmt`
moves onto Lang198x's tree, and the old reader is deleted, as for PasmoNext.
