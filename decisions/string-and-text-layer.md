# Decision: strings are a source pre-pass, not a second type in the expression language

**Status:** Active. Binding for how dialects gain string functions, string
symbols and token-list predicates.

**Date:** 2026-08-27.

## The decision

An expression evaluates to an `i64`. It always has, and it continues to.

The reference assemblers' string features — `.concat`, `STRFMT`, `equs`,
`setstr`, `.ident`, `.match` and their kin — are a **source pre-pass**, in the
shape [`macro-expansion-framework.md`](macro-expansion-framework.md) already
settled for macros: string symbols are collected, string functions are folded
to the text they produce, and the result goes through the dialect's ordinary
parse. Mechanics live once; each dialect supplies its grammar through a trait.

The pass runs **in source order**, over a constants environment folded as it
goes — the same environment `.if`, `.res` and the direct/extended size
decisions already read.

## What it covers

51 words, across four dialects — the largest single block of outstanding
reference vocabulary.

| | words |
|---|---|
| return **text** | ca65 `.concat` `.sprintf` `.string` `.left` `.mid` `.right` `.ident` `.define` `.undefine` `.delmac`; rgbasm `STRCAT` `STRFMT` `STRSUB` `STRSLICE` `STRUPR` `STRLWR` `STRRPL` `STRCHAR` `REVCHAR` `EQUS` `REDEF` `PURGE`; lwasm `setstr` `includestr`; sjasmplus `defarray` |
| return a **number** over text or token lists | ca65 `.match` `.xmatch` `.tcount` `.paramcount` `.blank` `.const` `.definedmacro` `.ismnem`; rgbasm `STRCMP` `STRFIND` `STRIN` `STRLEN` `BYTELEN` `CHARLEN` `CHARVAL` `STRBYTE` `ISCONST` `DEF`; lwasm `ifstr` |

The second row is why the pass answers both halves: once text is resolved
before parsing, a numeric predicate's argument is an ordinary literal by the
time the expression parser sees it.

## Why not a string type in `Expr`

It would be closer to how rgbasm models this internally, and it was rejected
for three reasons.

- **It makes the expression language two-typed.** Every dialect's evaluation
  and folding, the engine's byte emission, and every exhaustive match over
  `Expr` would need a type-error story for "a string where a number was
  wanted". There is no such story today and no other feature wants one.
- **It does not answer the token-list functions.** ca65's `.match({a},{b})`
  compares *token sequences*, which are neither number nor string. A second
  type would need a third beside it; a text pass sees tokens as what they are.
- **The references treat these as text.** Probed: `DEF s EQUS "N"` with `N` a
  constant, then `db s`, emits `N`'s *value* — the name was substituted into
  the source and then evaluated, which is a text pass and not a value. ca65's
  are pseudo functions written for macro bodies. Modelling them as values would
  describe our design rather than theirs.

## The one thing it cannot do

**A string function applied to a value only the layout knows.** The pass runs
before addresses are assigned, so it can fold a constant and not a label's
address.

Measured, because the two references differ:

- **ca65 refuses this itself.** `.sprintf("%d", L)` on a label is `Constant
  expression expected`, as is any constant defined *below* the line. Nothing is
  lost.
- **rgbasm allows it** for a label defined above: `STRFMT("%d", lbl)` formats
  the address. A constant or label defined *below* is `Expected constant
  expression` there too, so only this one case is out of reach.

That case is refused by name, saying why, rather than answered wrongly. If it
ever matters, the answer is a second pass fed by the layout — not a type.

## Drift triggers

Re-read this before acting on any of these:

- "we should make `Expr` generic over its value type"
- "the string functions would be easier with a `Value` enum"
- "just evaluate the string function during layout"
- adding a string feature to a dialect **without** a `TextSyntax` grammar entry,
  which is how the mechanics start growing per-dialect special cases
- a string function that reads `*` or a label address and appears to work —
  it cannot, and a passing test means the pass has been given layout state it
  should not have
