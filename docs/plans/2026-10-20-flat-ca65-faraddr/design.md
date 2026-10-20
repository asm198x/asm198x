# Unsigned far-address data validation (#906)

Status: approved by Steve on 2026-10-20: extend the existing Sized operation.
The 132-case native audit accepts 64 and refuses 68. Native `.faraddr` accepts
0 through 16777215 and rejects negative values, including -1. The existing
`Operation::Sized` three-byte emission accepts signed negatives as well as
unsigned positives, so using it unchanged would still accept invalid source.

## Recommended shape

Extend the existing private sized-data representation with an explicit range
policy: its current signed-or-unsigned behavior remains the default for all
existing consumers; ca65 `.faraddr` selects unsigned. Carry that policy through
AST lowering/operation conversion into the existing sized-data emission path.
Instruction-piece range rules and byte-extraction directives remain unchanged.
This adds no expression type, dependency or public JSON/debug field.

Use the complete expression at layout time so forward labels and arithmetic
receive the same checked 24-bit range. Preserve the authored directive spelling
and canonical formatting. Cover all 132 native boundaries/contexts and the ten
wrong acceptances from the diagnostic sweep, including output preservation.

## Alternative

Add a dedicated private far-address data operation and AST item, with explicit
three-byte unsigned emission. This keeps the existing sized-data shape intact
but duplicates data/layout/formatter dispatch for a range distinction.

A parse-only check is insufficient: a forward label may cross the limit or
become negative only after layout. Encoding a sentinel into expressions would
hide the original value and invent an expression workaround, so it is not
proposed.

The proposed change extends a shared engine/AST convention. The user-provided
AGENTS.md requires agreement for significant departures from conventions and
new architectural patterns; That agreement is recorded above. #904
can continue independently using the same native diagnostic evidence.
