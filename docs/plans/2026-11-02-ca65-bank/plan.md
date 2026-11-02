# Plan: ca65's `.bank`

**Status:** Proposed 2026-11-02.
**Binding records:** `decisions/layouts-are-data.md` (the bounded ld65
config reader grows by evidence), and 198x's
[assembly source layer decision](https://github.com/stevehill1981/198x/blob/main/decisions/assembly-source-layer-moves-to-lang198x.md)
(Lang198x lowers, Asm198x keeps state and links).

ca65's `.bank(expr)` gives the `bank` attribute of the memory area that
`expr`'s segment runs in. A program uses it to switch in the bank that holds
some code or data, as in a table of banked routines:

```
bank_table:
        .addr   banked_func_1
        .byte   <.bank(banked_func_1)
```

Only ld65 knows the answer, because only the link config says which area a
segment goes in and what bank that area is. So `.bank` is never a constant
while the program is assembled. Today the walk refuses it as "not lowered
yet" (Lang198x's lowering does), and the config reader refuses a `bank`
attribute by name.

## What ca65 and ld65 do

Measured on ca65 V2.19 and ld65 at `8d86ca7`, with a config whose areas
carry `bank = 3`, `bank = $12345`, and no bank at all. `l`, `l1` and `l2`
are labels, `h` a label in the code segment, and `K` a constant.

| Source | Result |
|---|---|
| `.dword .bank(l)` | the bank of `l`'s area, such as `45 23 01 00` |
| `.bank(l+3)`, `.bank(3+l)`, `.bank(l-3)`, `.bank((l))`, `.bank(l+K)` | the same as `.bank(l)` |
| `.bank(*)`, `.bank(*+1)` | the bank of the current segment's area |
| `q = l` then `.bank(q)` | the same as `.bank(l)` |
| `.bank(l2-l1)`, `.bank(h+l)`, `.bank(l-h)`, `.bank(h-h+h)` | ld65: `Argument of .BANK() is too complex` |
| `.bank(-l)`, `.bank(<l)`, `.bank(l*2)`, `.bank(l .bitand $FF00)` | ld65: `Argument of .BANK() is too complex` |
| `.bank(5)`, `.bank(S::m)` (a struct member, a constant) | ld65: `Argument of .BANK() isn't a label attached to a segment` |
| `.bank(l)` with `l` in an area with no `bank` | ld65: `Memory area 'M' is referenced by .BANK(), but has no BANK attribute` |
| `.byte .bank(h)`, `lda #.bank(h)` | ca65: `Range error (Address size 2 does not match fragment size 1)` |
| `.byte <.bank(l), >.bank(l), ^.bank(l)` | `45 23 01` |
| `lda .bank(h)` | absolute: `ad 03 00` |
| `.word .bank(l)`, with bank `$12345` | ld65: `Range error` |
| `.res .bank(h)`, `.if .bank(h)` | ca65: `Constant expression expected` |
| `.bank(undefined)` | ca65: `Symbol 'undefined' is undefined` |
| `.bank h`, `.bank()`, `.bank(h, h)` | ca65 syntax errors |
| `bank = 7, bank = 8` on one area | ld65: `Attribute 'BANK' is already defined` |
| `bank = $1+2` | 3: the config takes an expression |

So the rules are:

- **The argument is one label, plus or minus a constant.** The label may be
  `*`, a label defined later, or a name that stands for a label. A constant
  may be added on either side or taken away on the right. Any other use of
  a label is "too complex", even one that cancels out, such as `h-h+h`. An
  argument with no label at all "isn't a label attached to a segment".
- **The value is the bank of the area the label's segment runs in.** An area
  with no `bank` is an error, not 0.
- **ca65 gives `.bank` the absolute address size**, whatever the bank, so it
  needs `<` to fit a byte. It is never a constant.
- **These are link errors.** ld65 reports them without a line. Asm198x can
  name the line, which is only a better message: the program is refused
  either way.

## Options

1. **A new node in `asm198x-ir`.** Lowering would write `.bank(x)` as, say,
   `Expr::SegmentBank(x)`, and the walk would resolve it after placement.
   This is the most direct. But the IR is shared by every dialect and by
   Lang198x's other front ends. A new variant breaks their exhaustive
   matches, and gives the shared evaluator a node that only ca65's linker
   can answer.
2. **Lowering asks the host.** `Known` gains `bank(value)`, which takes the
   lowered argument and returns the expression to use in its place. Its
   default returns `None`, and lowering then says "not lowered yet", as
   today. The walk returns a name of its own, which no source can spell,
   and keeps the argument beside it. To lowering and to every check, that
   name is an unknown value of absolute size, so `.byte .bank(h)` is a range
   error and `.res .bank(h)` needs a constant, as in ca65. After placement,
   the walk replaces the name with the bank, or reports the error. This is
   how the walk already resolves the `CPU_` constants.
3. **Lowering works out the bank itself.** It cannot: it does not know where
   segments are placed, and a label used before it is defined is common.

**Recommendation: option 2.** It keeps the IR and every other dialect as
they are. The one change to Lang198x is a `Known` method with a default, so
its release stays compatible.

## Settled, if option 2 is agreed

- **The config reader takes `bank`.** A memory area may carry
  `bank = number`, which the reader keeps. A second `bank` on one area is
  ld65's error. The reader takes numbers only, as for `start` and `size`;
  an expression such as `$1+2` is still refused by name, to be grown when a
  real config needs it. `run` stays refused, so a segment's run area is its
  load area.
- **Lowering asks for the bank.** Lang198x lowers `.bank(expr)`, with one
  argument, by asking `Known::bank(&lowered)`. `None` keeps today's "not
  lowered yet". Syntax errors are ca65's, from the parser as today.
- **The walk keeps each `.bank` aside.** It answers with a fresh name, and
  keeps the bound argument, the place it was used, and the scope its names
  resolve in. `known` gives that name `Opaque(Absolute)`. The undefined-name
  check and the symbol outputs skip it, as they skip the `CPU_` constants.
- **After placement, the walk resolves it.** For each `.bank`, it reads the
  argument's tree: exactly one leaf may be a placed label (or `*`, or a name
  bound to one), joined to the rest only by `+`, or by `-` with the label on
  the left. Every other leaf must be a constant. Then it finds the label's
  segment, that segment's area, and the area's `bank`. Each failure is
  ld65's message, at the line that used `.bank`. The name is replaced by
  the bank's number, and the existing range checks apply to it, so
  `.word .bank(l)` with bank `$12345` is ld65's range error.
- **An import counts as what it is linked to.** In a program of one file,
  the walk already links an import to an export of the same name, so
  `.bank` of an import is the bank of what it is linked to. An unresolved
  import is already refused.
- **Verdicts need a config of their own.** The measured lines here use areas
  with banks, so they go in a new fixture, `ca65-bank.json`, with the
  config beside the cases. They are recorded live from ca65 and ld65 by an
  ignored test, and replayed through the walk by a normal one, as
  `ca65-labelled-end.json` is.

## Steps

1. **Asm198x: the config reader.** Read `bank` on a memory area, with its
   error for a second one. Tests for each.
2. **Lang198x: `Known::bank`.** Lower `.bank` through it, with a test host
   that answers it and one that does not. Release.
3. **Asm198x: the walk.** Take the release. Keep each `.bank` aside, resolve
   it after placement, and report each error. Record and replay every line
   in this plan as verdicts in `ca65-bank.json`. Remove `.bank` from any
   list of unsupported functions and update the book.

## Risks

- **A name that no source can spell must stay out of every output.** The
  sidecar, the symbol file and the listing must not show it. The gates
  that compare those outputs would catch it, and a test checks it.
- **ld65 checks the argument's shape in the object file, after ca65 has
  simplified it.** ca65 folds constants, such as `K`, but keeps `h-h+h`
  as written. The walk follows the measured cases. A shape not measured
  here could differ, and gets a measured line before it is changed.
- **The default NES layout has no banks.** `.bank` with it is an error, as
  it is in ld65 with a config that has none.
