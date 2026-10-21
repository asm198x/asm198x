# Linked repeat-header migration checkpoint

Repeat counts and optional counter tokens now use the canonical stream. Count
reads precede counter constructors, generated commas remain grammar delimiters,
and physical register tokens differ from generated identifiers. Existing scope
binding, source-name requirements, token capture, loop substitution, termination
and pending-input mechanisms remain in place.

All 320 scalar observations and all 18 counter-name observations match native.
The 98-case repeat subset/name fixture passes full CLI contracts. The 39-matrix
unchanged-executable replay retains all 6,826 predecessor matches and reaches
6,865 across 7,309 rows. Generated/canonical fields remain 40/40 and 100/100.

Verification passes all 779 library tests and 32 affected source/deferred,
repeat/end, lexical-admission and macro contract tests; Clippy -D warnings and
formatting pass. The initial build identified one unused line variable after
passing the real span into fold_repeat; it was removed before final checks.
Lossless hashed receipts are retained in repeat-checks/.

The full reader remains unmerged. Numeric conditional/name-only headers, file,
flat and syntax adapters and final full/archive qualification are still needed.
Counter-body construction semantics were not inferred or widened from this
header-only migration.
