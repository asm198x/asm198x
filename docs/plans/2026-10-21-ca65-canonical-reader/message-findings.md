# Linked assertion/message migration checkpoint

Assertion condition/action/message fields now share the canonical input.
Conditions retain deferred evaluation; actions remain identifier tokens and
messages are consumed even when the condition succeeds. Output reads one string
and requires termination. Warning/error tails drain through token constructors
without evaluating ordinary skipped names or numeric calls.

All 80 assertion/message controls match native and pass their full CLI contracts.
The complete scalar audit rises from 266 to 293 matches out of 320; its remaining
27 differences are repeat headers. The final unchanged-executable replay retains
every one of 6,793 predecessor matches across 39 matrices / 7,309 rows and reaches
6,826 (including six additional numeric-consumer matches).

Verification passes 779 library tests and all 24 tests in the affected contract
suites (deferred/source contexts, text consumers/call spacing, characters, quoted
admission and macro introspection), Clippy -D warnings and formatting. The first
library run caught `.out 5` losing its established “one string” diagnostic;
the strict-token precheck restores it without remapping nested constructor
errors. Both failed and corrected receipts are retained in message-checks/.

This ca65-only slice follows the full 1,653-test workspace checkpoint in
3d5a819e. Full current workspace and five native archive qualification remain
required before #924 merges. Repeat, conditional/file, flat and syntax adapters
are still incomplete; no merge qualification is claimed here.
