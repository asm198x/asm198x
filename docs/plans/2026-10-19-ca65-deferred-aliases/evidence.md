# Retain deferred immutable aliases (#791)

The linked ca65 projection retains unresolved expressions with their original
spans and definition positions. Only affected dependants are visited when a
numeric value becomes known; the existing reader sees those new constants.
Final scope binding includes all immutable definitions. Remaining expressions
resolve after label/definition-PC placement through the shared evaluator and
an iterative dependency queue. Unused undefined and cyclic aliases refuse.
Private cached operand hints retain short byte-extractor/zero-page forms.
Scoped res/align folding borrows the live reader instead of copying names.
No shared AST, contract, dependency or public shape changed.

The final fixture has 63 completed native cases: 48 accepted, 15 refused,
including a 500-link forward chain. The explicit predecessor disagrees on 48.
Tests compare status/bytes, formatter idempotence/reassembly and debug shape.
An included unresolved definition reports its own file/line and preserves all
five outputs plus both source files in human and JSON modes.

Isolated checks pass 1,545 Rust tests (38 ignored), 71 Node tests, Clippy/all
features, fmt, Ruff and generated docs. A real Lua-enabled local archive passes
all 56 required cohorts and both corruption controls. The preceding archive
fails only the new alias cohort (207 commands). Local receipts are artifact-only;
clean-producer replay and five-host exact-head qualification remain required.

Performance.json retains three observations at each of 500/2,000/8,000
definitions. At 8,000, independent constants take 0.244–0.245 seconds versus
0.185–0.188 in the predecessor; forward chains take 0.302 seconds. Observed
scaling is close to linear. No complete namespace is copied per instruction.

The initial 62-case comparison config used optional=yes, which this bounded
config reader does not support. Its native/predecessor reports are retained
as *unsupported-config* evidence, and were not used to validate the fix. Both
sides were rerun with the supported config before interpretation. The final
fixture uses that supported config and the predecessor red is regenerated.

The separate #788 numeric-token follow-up retains 64 native probes (20
accepted, 44 refused). Decimal 4294967299 is accepted but 4294967300 and
hex/binary values above 0xFFFFFFFF are refused. This is not a generic 32-bit
count limit. Scoped binding, source-time queries and remaining scoped folds
continue under #785; this commit does not claim to close that issue.

Clean producer `36a3fc5f3e539c0ca129b7943c2d0c3e2c69c352` matches all
564 unchanged native 4510 bytes. All seven processes and fourteen streams are
retained; the changed-byte guard fails as required.
