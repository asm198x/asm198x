# Validate constructors, retain numeric calls in captures

The 255 native programs distinguish constructor parsing from numeric token
capture. All 63 selected empty constructors are native errors: concat, string,
ident, sprintf, left, right and mid, across three readers and direct/include/
macro contexts. The #927 guard matches those errors. Empty numeric calls inside
the list remain three tokens, including `.const()` and `.match()`; their argument
rules are not evaluated by the capture.

Current #927 source matches 191 programs. There are 30 selected flat false
refusals under #928 (empty const/match/xmatch/ismnem/ismnemonic) and 34 known
inactive flat cases under #895. No unmatched case lies outside those issues.
The checked #925 executable was also compared against all 255 sources; no
previous native match was lost. Both immutable executable hashes and every
native/Asm stream are retained.

The full reader needs separate constructor and numeric-capture admission. Do not
apply selected numeric-call arity or name validation to an unevaluated token
list. Unused/inactive source remains governed by the existing lazy-reading and
physical lexical-check decisions. Matrix-local names may repeat across audits;
namespace case IDs when combining them into a single mandatory fixture.
