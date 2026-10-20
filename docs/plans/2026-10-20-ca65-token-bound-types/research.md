# Native selector-bound types

The dedicated 100-case audit accepts 33 inputs and refuses 67. Frozen clean
producer c478532a4f4fee2fd608a594f8e0f33a125b01a4 has 22 differences: 21
native-invalid string-valued bounds are accepted and one existing scoped numeric
lookup is refused. Keep numeric_scope explicit with #795's scoped text-function
controls; do not change lookup or source-order effects in this type-admission fix.

Scope::number accepts both Arg::Bare and Arg::Text, evaluating raw() in either
case. The latter removes the quotes, so "2" becomes numeric 2 and "VALUE" reads
a numeric constant. Native selector positions require a numeric expression.
The same false acceptance follows .string/.concat/.sprintf output, text defines,
macro arguments, captured defines, selected repeats and includes. A private
ca65 helper should reject Arg::Text before calling the unchanged Scope::number
at .left/.right's count and both .mid bound positions.

Native numeric, constant, character and large/negative controls remain governed
by the existing numeric evaluator and selector behavior. Parenthesized quoted
expressions are already refused. Untaken source, unused macros and zero repeats
remain lazy. Numeric expression and string/token-list representations, other
text functions and other dialects are unchanged.

The wider [623-case audit](https://github.com/asm198x/asm198x/tree/cd6dd586/docs/plans/2026-10-20-ca65-quoted-token-lists)
positively records seven quoted-bound false acceptances under #884. Its other
255 native-valid admission refusals stay with #872 and its follow-up grammar
work. The correction must remove exactly those seven wrong acceptances without
introducing any new mismatch in that complete matrix.
