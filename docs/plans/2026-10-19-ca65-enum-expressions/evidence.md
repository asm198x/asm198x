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

No implementation changes are included in this research commit. The concrete
representation must preserve the accepted unused controls as well as correcting
used member binding, defaults, anonymous scope, queries and unknown size.
