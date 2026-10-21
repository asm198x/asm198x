# Native unary-plus results

All 135 sources and process streams are retained. Native accepts 126 and refuses
nine incomplete operands. Both the immutable checked #929 executable
and the allocation reader match 54 observations and differ on the same 81.
Exactly 63 are native unary-plus admissions (seven bodies, three active source
contexts and three CPUs); 18 are the existing inactive flat-reader gap #895.
Captured plus tokens and the incomplete-operand controls retain their behavior.

Physical `.byte +1` emits 01 natively and is refused with `expected a value`.
The generated-field example `.res .left(2,{1,2})+1` produces operands `1,+1` and
emits 01 natively; the reader reaches the right delimiter but the shared numeric
parser refuses the unary plus. This explains the three remaining canonical-field
refusals independently of the allocation ordering fix.

A follow-up must add the native operator in the existing shared grammar, gated
to ca65, retain inert captured operators and preserve every predecessor match.
No second arithmetic parser, public API or dependency is required.
