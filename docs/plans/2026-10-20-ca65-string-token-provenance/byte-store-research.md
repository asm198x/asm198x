# Byte-store integration evidence

The second frozen prototype executable has SHA-256
`d4d6871adb0c5a48c5ff8cc8578bfeb188aaaaa95c300819e65c578979ee8eec`.
Its source-file hashes are retained in `trial-byte-store-identity.json.gz`.
Production work remains uncommitted while full qualification is prepared.

The retained native comparison gives:

| Audit | Cases | Differences |
| --- | ---: | ---: |
| Complete byte construction | 287 | 0 |
| Native 65816/HuC6280 byte construction | 574 | 0 |
| String consumers, including brace capture | 69 | 0 |
| Macro/repeat/include lifetimes | 36 | 9 existing macro-argument refusals |
| Original format argument audit | 71 | 3 malformed literals (#887), 1 scoped query (#795) |
| Independent quoted token-list admission | 623 | Same 255 existing gaps (#872) |

The predecessor differs in 20 of 69 consumer cases. The lifetime audit has
28 predecessor differences: this prototype resolves 19, retains nine and
introduces none. The nine retained cases pass `.sprintf` as an unbraced macro
argument; the macro argument splitter sees the function's comma before the
string constructor is read. This is an existing argument-admission gap rather
than a byte-value regression. Its accepted native sources and original
predecessor refusals remain in the complete audit; account for them explicitly
in any shipping fixture.

Native captures independently select `.setcpu "65816"` and `.setcpu "huc6280"`
for the flat sweeps. All 1..255 character values, zero/range refusals, raw UTF-8,
byte precision/padding, nested construction and numeric-position refusals are
included. Native compiler/linker hashes and every process outcome are retained.

The first store trial matched all 287 byte cases but leaked a private handle
through the early `.segment` return. The 69-case consumer audit caught it.
Moving text-only materialization before eager directive dispatch resolves that
case. Preserve that first trial only as negative evidence, never as a qualified
producer. The second trial passes all 69 consumer cases.

Numeric `.strlen`/`.strat` calls inside braces must remain token sequences.
Four fresh native controls give counts 4/6 even with nested string construction.
The shared text scanner now exposes that grammar boundary; other dialects keep
its existing default. Strings remain source tokens and `Expr` remains numeric.
