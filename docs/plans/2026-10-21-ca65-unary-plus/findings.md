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

The implemented fix adds an explicit private unary_plus option, enabled only for
the three ca65 readers. Inspection found RGBDS also uses the logical switch, so
that switch could not provide the initially proposed ca65-only guard. Every
other dialect explicitly retains false. A regression test proves logical
operators alone do not admit unary plus and binary addition is unchanged.

All 117 qualified native cases now pass full CLI contracts; the only remaining
18 observations are the existing inactive flat gap #895. The full workspace
passes 1,653 Rust/Lua/doc tests with 38 existing ignored; Clippy -D warnings and
formatting pass. The 39-matrix / 7,309-row unchanged-executable replay retains
all 6,790 predecessor matches and reaches 6,793, including 100/100 canonical
fields. The 35 constructor-boundary contract controls also pass in this suite.
Hashed lossless receipts are in checks/. #931 stays open until #924 is merged.
