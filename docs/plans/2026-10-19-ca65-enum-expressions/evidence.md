# Enum expression and cursor audit (#796)

The two native batches retain 98 cases: 94 completed (39 accepted, 55 refused)
and four bounded timeouts. The explicit scoped-binding predecessor disagrees
on 44 completed outcomes. collect_completed.py accounts for every case and
excludes only the observed timeouts. Raw reports retain native tool identities,
source/config and process streams. Direct self-name use timed out in native;
subprocess.run killed/reaped each after ten seconds. No byte or refusal outcome
is assigned to those observations.

Native enum members keep expressions. A new explicit initializer resets the
base expression and offset; default members use that base plus the running
offset. This is visible with forward references: RED=GREEN+1, BLUE, GREEN=4,
WHITE yields 5,6,4,5. A later local shadows an outer constant; explicit root
references retain the root value. Queries run in source order, and currently
unresolved aliases cannot be used as source-time allocation counts.

Named enums open a namespace; anonymous enums declare in their current scope.
Nested records/enums in these tested enum/record positions refuse. Native
reports .sizeof(enum) and .sizeof(enum_member) as unknown; the numeric cursor
is not a record size. The existing reader instead publishes its cursor as size
and folds member expressions against an unqualified numeric table.

The pinned upstream enum.c is implementation precedent for a base expression
plus an offset and for named versus anonymous scopes. Installed ca65/ld65 V2.18
captures remain the authority; the newer pinned source is not substituted for
those observations.

A separate sixteen-case scope/enum matrix exposes #800: with an already-defined
outer RED, an unused local RED=RED+1 or RED=RED-RED is accepted by native, while
using the local value refuses. Ordinary scoped aliases already refuse the two
accepted controls; the eager enum reader instead accepts the two used controls
with wrong bytes. A rule that simply rejects every unresolved enum definition
would introduce a new refusal of native valid source. Establish native use and
size-analysis behavior before routing enum expressions through the existing
deferred queue. Do not generalize this into skipping unused undefined names or
cycles: the other completed controls refuse those.

The implementation now retains member expressions in a private native event
and lowers it through the existing Equ projection. The same source-width,
queue, PC-placement, final-binding and debug paths handle ordinary and enum
constants. The enum reader keeps a base expression plus an offset, captures
query results once, and distinguishes named from anonymous scopes. Closure
publishes no invented size property. No shared AST or public contract changed.

The final fixture combines all 94 completed outcomes with 42 stream/token
controls: 136 cases, 64 accepted and 72 refused. The actual preceding #804
executable disagrees on 65; the corrected executable matches all 136. The
fixture builder retains every completed case, with the four native timeouts
still outside expectations. No existing expectation was removed or weakened.

The source-preserving formatter retains enum member text without evaluating
macro parameters or repeat counters. Dynamic headers use private placeholders
only for balanced formatting scopes. Mnemonics such as LDA are identifiers in
an enum parser; register tokens remain reserved. The first token-context check
caught four mnemonic refusals, and the expanded controls pin the correction.
An initial formatter run reported an invalid generated record name; retaining
the unexpanded source fixes that without executing the counter.

Formatter idempotence, reassembly and stable debug sections/symbols hold for
every accepted case. A final numeric use and a root-file constancy query both
report the included enum definition and preserve five outputs plus both sources
in human/JSON modes. The accepted unused-cycle control has no fabricated debug
value; ordinary known constants retain their real values.

An actual local Lua archive passes all 59 required cohorts, including 328
commands in this cohort, and both corruption controls. The preceding actual
archive fails only this cohort; its other 58 cohorts pass. Local archive reports
remain artifact-only; clean producer and five-host qualification are separate
requirements before merge.

Twenty-seven bounded performance observations cover 500/2,000/8,000 default
members and forward chains. At 8,000, defaults take 0.306–0.351 seconds versus
0.213–0.297 in the predecessor. Current forward chains take 0.499–1.884 seconds
while other verification runs; that range includes an outlier. These local
observations are not a runtime guarantee. Default expressions do not grow into
a chain of previous members, and no namespace is copied per member.

Final isolated checks pass 1,553 Rust tests (38 ignored), 74 Node tests,
Clippy/all workspace targets/features with warnings denied, fmt, Ruff and
generated docs. Full logs, process records and actual archive controls are
retained alongside the native/predecessor evidence.
