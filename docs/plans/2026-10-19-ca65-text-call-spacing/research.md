# Text-function whitespace boundaries

The 73 native controls accept 61 inputs and refuse 12. The clean scope producer
1cacfa5d42d42522e4e2e036cb95653d81845f41 (CLI SHA-256
99a47d49e1730577c0c3a5294724c626c3aa29036dd6f11d1f0b9621e01f2d48)
has 35 differences. The shared text scanner requires an adjacent parenthesis in
both fold_once and has_call. Native admits spaces/tabs; nested folding must use
the same boundary in both functions. An unrecognised spaced .tcount/.blank even
succeeds with zero output bytes instead of native 03/01, so status-only checks
would miss data loss. Some numeric functions already work through another path;
keep them as controls rather than infer one scanner explains every function.

Four .left controls expose a separate token-list admission difference, including
adjacent syntax: native .left(2,"abcd") returns the quoted token unchanged and
emits all four ASCII bytes 61 62 63 64. Asm198x refuses the unbraced string token
list. This is not character slicing or argument order. Keep it explicit and
separate from whitespace. The remaining 31 differences are the spacing target.

This prerequisite follows the withdrawn #862 trial. Do not reuse its production
patch until this independently qualified lexical boundary is available. Spaced
generated heads must get actual symbol/address assertions, not merely successful
body bytes that do not reference the head.
