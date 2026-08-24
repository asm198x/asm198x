# Comparison operators across the references

Probed 2026-08-24 against the installed references, to settle
[#229](https://github.com/asm198x/asm198x/issues/229). Two things vary that a
single shared implementation would have got wrong.

## The table

| dialect | true is | `=` | `==` | `<>` | `!=` | `<` `>` | `<=` `>=` |
|---|---|---|---|---|---|---|---|
| vasm | `$FF` | ✓ | | ✓ | | ✓ | ✓ |
| sjasmplus | `$FF` | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ |
| pasmo | `$FF` | ✓ | | | | ✓ | |
| ca65 | `1` | ✓ | | ✓ | | ✓ | ✓ |
| acme | `1` | ✓ | | ✓ | | ✓ | ✓ |
| rgbasm | `1` | | ✓ | | ✓ | ✓ | ✓ |
| lwasm | `1` | ✗ | | ✓ | | ✓ | ✗ |

`✗` is a form the reference **refuses**, checked rather than assumed:
sjasmplus rejects `<>`, and lwasm rejects both `=` and `<=`/`>=`.

## True is not always 1

vasm, sjasmplus and pasmo answer `$FF`; ca65, acme, rgbasm and lwasm answer
`1`. `dc.b 2=2` is `$FF` in vasm and `.byte 2=2` is `$01` in ca65, from the
same source shape.

That is why comparison cannot simply be a `BinOp` whose evaluation returns
`1`. It is expressed instead as the comparison wrapped in a negation for the
`$FF` dialects, so the tree carries the dialect's answer and evaluation stays
dialect-agnostic.

## `<` is positional, not ambiguous

This was the open question in #229, and the answer makes the work far smaller
than it looked. In the 6502-family dialects `<` is the low-byte prefix, and it
is *also* the less-than operator — told apart by where it sits:

```
ca65    .byte <$1234    →  34      (prefix: low byte)
ca65    .byte 2<3       →  01      (infix: less than)
acme    !byte <$1234    →  34
acme    !byte 2<3       →  01
```

Both meanings, in both dialects, in ordinary expressions. So no dialect has to
choose, and acme's separate `infix_relation` in its condition evaluator is not
a workaround for an ambiguity — the expression parser can carry both, because
a prefix operator and an infix operator never occupy the same position.

## What this changes about the earlier reading

#229 was filed saying the design question was "how `<` disambiguates per
dialect". There is no ambiguity to resolve. The real per-dialect facts are the
accepted spellings and the value of true, both of which are data rather than
design.
