# Bind labels when ca65 macro definitions are read (#782)

The structural range shield, selected definition detector and captured header
parser now peel a lexical label through the same private linked-ca65 helper.
Selected definition labels bind through the existing block label path. The
shared/flat MacroSyntax grammar and formatter path remain unchanged.

There are 27 native ca65/ld65 cases: 23 accepted, four refused. The predecessor
fails 21 accepted cases; two unlabeled/dead controls already matched. Tests
compare native bytes/status, formatter idempotence/reassembly and debug shapes.
An uncalled definition and a delayed call prove the label's position is the
definition address. Included macro-body failures preserve all five outputs and
retain the actual invocation and definition frames in human/JSON diagnostics.

Isolated verification passes 1,541 Rust tests (38 ignored), 69 Node tests,
Clippy/all features, fmt, Ruff and generated docs. A real Lua-enabled archive
passes all 54 required cohorts and both corruption controls; the preceding
archive fails only the new cohort, which runs 96 CLI commands. These are local
combined checks on the staged #781 parent. Preserve the tested source tree when
rebasing, audit the clean unchanged 4510 producer and require all exact-head
checks plus five actual platform reports before merge.

The next native audit retains wrong-byte/scoped-fold examples for #785 and
nonpositive struct/union allocator refusals for #786. They remain separate
from macro-definition label binding. No public shape or dependency changed.

The rebase onto merged #789 preserved all four tested source subtrees
(crates, scripts, workflows and Cargo.lock). Generated docs pass again. Clean
producer `06979fb89a43c0181f7fa093a50880943f6c4cf1` matches all 564 unchanged
4510 bytes; all seven processes and fourteen streams are retained, and the
changed-byte guard fails as required. The qualified source is retained on the
task branch. Five-host exact-head qualification remains the merge gate.
