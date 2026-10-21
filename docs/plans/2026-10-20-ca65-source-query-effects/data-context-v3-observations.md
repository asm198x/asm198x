# Ordered data queries on checked external bindings

The frozen checked #810 producer differs on 186 of 515 native data cases.
The candidate matches all 515: 446 accepts and 69 refusals. All previous native
matches across 26 matrices / 4,136 cases survive. The original query matrix
improves from 35 to 56 matches, the extended matrix from 62 to 128, and the
visibility matrix from 266 to all 274. Exact before/after results retain the
executable hashes; these comparisons are development evidence, not release
qualification.

The root cause is eager evaluation of a complete data tail against read-only
numeric snapshots before the expression reader visits earlier name queries.
For VALUE=2, the list .ref(VALUE),.const(VALUE),.ref(VALUE) must emit 000101:
constancy creates an ordinary read between the first and last reference query.
Numeric string arguments and queries inside one expression have the same
ordering requirement. Token-list predicates retain unevaluated source tokens.

The existing selected reader now admits data operands through a borrowed live
context. It delegates ordinary reads, query-only names and constancy to the
reader and retained projection/source graph at their expression positions.
The context owns read requirements, not a second namespace. Selected token
streams are parsed before following queries; qualified generated identifiers
remain components of the complete name. Syntax-only formatting and lazy
inactive/unused source remain on their existing paths.

The checked external-binding implementation stays intact: declaration keys,
source graph, source-time widths, byte refusal checks and explicit exported
value binding are preserved. Both fixture harnesses and mandatory cohorts are
retained. The previously failing scoped import/definition control now refuses
for the actual external binding, while all eight valid old scoped data-constancy
cases accept. The original 1,646-case external fixture is its historical
subset; the 515-case data fixture and full visibility replay cover the added
outcomes. No prior test or archive gate is weakened.

The package gate requires all 104 cohorts, including all 515 data cases and
1,853 data command records per native archive. Rust fixtures retain official
debug-reader checks, native bytes/JSON, symbol/section round trips, formatting
idempotence, refusal provenance and preservation of prior output files.

Focused Rust tests, all-target Lua Clippy, formatting, 115 Node/setup tests and
docs/version/ecosystem checks pass. The full Lua workspace suite passes 1,643 tests, with 38 ignored.
No checked PR or native archive qualification is claimed yet. #795's separate
50-case numeric-consumer audit still has 30 active differences; #918 retains
visibility register-token provenance. The approved source-aware design governs
the next numeric-consumer slice after this data slice is checked.
