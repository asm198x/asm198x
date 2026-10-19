# Generated labels on macro definition and invocation heads (#862)

The 102 native controls accept 63 inputs and refuse 39. Clean producer
1cacfa5d42d42522e4e2e036cb95653d81845f41 (CLI SHA-256
99a47d49e1730577c0c3a5294724c626c3aa29036dd6f11d1f0b9621e01f2d48)
has 74 differences: 48 native-valid sources refused and 26 invalid ones accepted.
The selected reader classifies macro definitions/calls before live text folding;
`macro_header` and call-site separation currently strip only static label heads.

Two static LDA macro-definition heads (6502/4510) are falsely accepted, beyond
ordinary register/prefix-label differences already tracked in #853. Native
rejects them. Both static and generated call/definition admission must preserve
native head-token rules rather than manufacture new false acceptances.

Labels bind at the definition or before a call body, not when the macro emits its
first byte. Native phase cases distinguish .defined/.const and address output.
The matrix includes spaced and nested text functions, formal arguments, empty
bodies, anonymous/cheap heads, skipped malformed text, zero repeats, iteration
names, scope/include/macro contexts, text arguments and numeric lexical errors.
Native refuses the nested-definition captures in this matrix; no support is
inferred from skipped or unused text. Existing capture/expansion seams remain.

The first implementation is withdrawn, not qualified. Two boundary failures
exposed the same underlying assumption: native .ident accepts whitespace before
its opening parenthesis, but the text folder does not consume it. An authored
colon gap also survived folding. The selected macro-header recogniser could then
strip an unevaluated head and register a definition without binding its label;
the formatter's separate label line failed ordinary assembly.

Retain the production patch, candidate test/fixture and all failing logs here.
Production and harness were restored to the last good clean scope producer.
The next slice must probe/fix the text-function token boundary independently,
then return to macro heads with explicit address-symbol assertions for every
spaced/concatenated/sprintf head. Do not treat successful bytes from a body that
never references its head label as evidence that the label was bound.
