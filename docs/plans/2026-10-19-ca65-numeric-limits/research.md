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

## Token boundaries

A further 72 native boundary controls accept 24 and refuse 48. In skipped
source, 0xFF, 123G, $FFFFFFFFG, %102 and 0FFhG are accepted, while
123ABC, 0ff_4294967300, lone numeric prefixes and oversized numeric prefixes
are lexical failures. Identifiers containing large decimal suffixes remain
identifiers. Quoted strings and character constants are distinct: native
rejects a multi-character single-quoted constant even in skipped source;
that literal syntax is separate from numeric range correction.

The primary cc65 V2.18 scanner source is
https://github.com/cc65/cc65/blob/V2.18/src/ca65/scanner.c
(blob 361a817c1383dc0b0aee9c5fa8fcb78f3e66501a, SHA-256
eea593534c8211287437e6acf9042e74eb5714c08ebe255e16ae5a74096844f8).
Its numeric scan stops before G or x; digit-start tokens collect hexadecimal
digits and consume an optional h/H suffix before choosing decimal or hex.
Prefix hex/bin scanners stop at the first non-digit of their radix. This is
why validating the existing coarse token as a full expression would over-reject
skipped source. Leading decimal zeros are skipped before the bounded buffer;
range detection examines the previous partial value before multiplying.

## Capture and stopping order

Seventeen additional controls accept six and refuse eleven. Native `.end`
stops before oversized tokens both on its own line and on later lines. A
selected conditional still reports an unclosed conditional if it reaches `.end`.
Macro and repeat capture validates the whole captured body before execution:
an oversized token after `.end` inside a zero/one-repeat or unused macro is
rejected. An executed `.end` inside a repeat, however, succeeds and ignores
the source after the closer; the preceding executable's unclosed-repeat refusal
is a separate confirmed gap, retained in repeat-end-predecessor.json.
Oversized tokens on macro/repeat/conditional closer lines are also read.

These results rule out an eager whole-file numeric prepass and a check attached
only to selected expression parsing. Keep the active numeric conversion and
the source-admission boundaries independently testable; preserve `.end` token
stopping and captured-source validation rather than inferring them from final
expression values.
