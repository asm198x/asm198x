# Decision: a `:` line is several statements, and neither the debug format nor the formatter grows a special case

**Status:** Active. Binding for the Z80 keyword-conditional parse.

**Date:** 2026-08-23.

## The decision

**A statement carries its own line and column; a `:` line produces several.**
The parse used to index its lines and take the number from the position, which
held while a line was a statement. It no longer is, so the number travels with
the text.

The two questions [#98] said had to be answered first are answered by
**not changing anything**:

- **The Debug198x format is untouched.** A colon line produces one `LineSpan`
  per statement, all naming that line.
- **The formatter expands.** No AST flag, no shared-a-line bit.

Both are argued below, because both looked like they would cost more.

## Why the debug format does not grow a column

[#98] read the risk as *"two statements on one line collapse into one record"*.
They do not. Each statement produces its own span, and the spans differ in
offset and length:

```text
 ld a,1 : ld b,2      line 1 offset 0 length 2
                      line 1 offset 2 length 2
 nop                  line 2 offset 4 length 1
```

Nothing is lost; the granularity is the line, which is what the format is keyed
on. A debugger stepping through a colon line stops twice and shows the same
line twice — coarse, and correct.

A column field would sharpen that, and
[`debug198x-format.md`](debug198x-format.md) permits it: the format froze on
2026-08-18 with additive changes still free. So this is a decision to wait, not
a wall. **Waiting is right because the sharpening has a consumer cost and no
consumer.** The field would mean nothing until the Emu198x importer read it,
which is a cross-repo change, and no fixture, example, or curriculum file uses
the form yet. Add it when someone is stepping through colon-separated source
and finds the line granularity wanting.

## Why the formatter expands rather than preserves

[#98] framed this as a choice with a cost either way: expanding rewrites source
people wrote deliberately, preserving means the AST carries a "these shared a
line" flag through emit.

There is already a precedent, and it settles it. The formatter **already**
splits a label off the operation that shared its line:

```text
lbl: ld a,1 : ld b,2        lbl:
 djnz lbl              →            ld a,1
                                    ld b,2
                                    djnz lbl
```

One statement per line is what this formatter means by canonical layout, and it
has meant that since before colons parsed at all. Preserving colon lines would
make `:` the one construct that survives canonicalisation — a special case
earning nothing. The result is idempotent and assembles to the same bytes.

## What a colon is not

A `:` separates statements *except* when it terminates a label, and the two are
told apart positionally: the colon closing a label is the first in its
statement with nothing but an identifier before it.

| source | reading |
|---|---|
| `lbl: ld a,1 : ld b,2` | label, then two statements |
| `.l: ld a,1` | a local label, same rule |
| `gl:: ld a,1` | `::` closes a label as one token (sjasmplus's export form) |
| `db ":" : db 1` | the first colon is inside a string |
| `db ':' : db 1` | the first colon is a character literal |
| `ld a,1 ; a:b` | the comment is found first and never split |

## Scope

Gated on `Z80Syntax::splits_on_colon`, off by default. sjasmplus has the form;
pasmo does not, and splitting on a character it treats as ordinary would invent
a dialect — the posture `syntax-stance.md` sets.

## Drift triggers

- *"Add the column to `LineSpan` while we're here, it's additive."* → Additive
  is free, but a field no consumer reads is a promise to keep it. Wait for the
  consumer.
- *"Make `fmt` preserve colon lines, they were written that way."* → So was
  `lbl: nop`, and the formatter has always split that. One statement per line
  is what canonical means here.
- *"Derive the line number from the position, it's simpler."* → That is the
  assumption this record exists to remove. It was true while a line was a
  statement.

[#98]: https://github.com/asm198x/asm198x/issues/98
