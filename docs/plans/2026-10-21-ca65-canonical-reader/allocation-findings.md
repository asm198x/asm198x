# Linked allocation migration checkpoint

.res/.align/.org now consume complete canonical streams. Counts and bounds are
validated before fills, fills are read even for zero reservation/no padding,
and placement keeps its existing Expr-based lowering. Scope binding, spans,
logical debug sections and physical offsets use their existing mechanisms.
Record-member parsing and syntax-only entry points retain their previous paths.

The 160 allocation controls are selected by grammar from the full 320 scalar
inventory; all 160 match. The other 160 are explicitly out of this fixture's
scope, not labeled as failures. The full scalar audit rises from 218 to 266
matches. Every generated-field control still matches (40/40).

The full workspace passes 1,650 Rust/Lua/doc tests with 38 existing ignored.
Clippy -D warnings, Rust formatting and Ruff checks pass. The reusable replay
checks all 39 matrices / 7,309 rows against losslessly retained predecessor
results, preserves every one of its 6,727 native matches and reaches 6,790.
The unchanged-executable hash guard passes. All receipts, including the rejected
fixture-count assertion and initial Python lint findings, are retained.

Fresh audits identify two required follow-ups rather than qualifying the reader
for merge. The 135 unary-plus controls show 63 arithmetic admission gaps plus
18 existing inactive flat gaps. The 35 constructor-lookahead controls show 12
false refusals and three false acceptances. Their exact inputs, native processes,
observations and plans live in the sibling unary-plus and constructor-lookahead
plan folders. Correct both before completing #924; preserve the existing 39
matrices and these new controls. No main merge or native archive qualification
is claimed by this checkpoint.
