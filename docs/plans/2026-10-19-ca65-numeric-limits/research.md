# Numeric token evidence

The first 72 controls cover decimal, prefixed/suffixed hex, binary and leading
zeros in assignments, arithmetic cancellation, empty-tag counts and skipped
branches. Native accepts 48 and refuses 24. Decimal's endpoint is 4294967299,
not 0xFFFFFFFF; the existing large valid empty-tag count must remain valid.

The 48 admission controls accept 26 and refuse 22. Oversized tokens fail in
unused macros, zero-repeat captures and skipped conditional source. Quoted
text/comments and source after .end remain accepted. Inactive includes remain
unopened. Correct macro argument syntax has no comma between the macro name
and its first parameter; the initial admission capture retains that syntax
mistake, and the corrected capture replaces it. Never use the initial macro
syntax refusal as evidence that a valid invocation rejects numeric input.

The linked path also refuses native-valid h/H suffixed hexadecimal literals.
That is independently fixable without changing skipped-source handling. Keep
lexical numeric range checks waiting for agreement on the binding decision's
narrow exception; do not work around the decision by parsing inactive meaning.
