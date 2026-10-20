# Next literal-boundary slice

The 354-case native audit has 225 acceptances and 129 refusals. The retained
byte-store predecessor differs in 78 cases: 56 selected malformed single-string
operands (#887), four numeric .asciiz acceptances, and 18 inactive/captured lexical
shape controls (#835). All inactive controls remain in the complete audit; the
binding numeric-only exception does not authorize fixing them in this slice.

The selected malformed operands include adjacent/separated strings, addition,
escaped-looking quotes and adjacent empty strings, in direct/labelled/selected
source, active macro bodies and string constructor arguments. Plain, empty,
one/two/trailing backslashes, UTF-8, punctuation, numeric byte lists, comments
and computed quote/backslash values are controls. Native .asciiz permits comma
separated strings with one final NUL but refuses a numeric list member.

After the byte-store merge, qualify 336 cases (225 accepted and 111 refused),
with the 18 inactive/captured cases explicitly under #835. Validate one complete
physical literal in StringTokens::data/bytes, resolve computed handles first,
and preserve full token-list spelling for #872. Make .asciiz require string
values while .byte keeps numeric items. This needs no Expr/schema/dependency or
inactive lexical-policy change. Verify exact native images/refusals, formatter
and debug round trips, old producer failure, all mandatory packages and controls.
