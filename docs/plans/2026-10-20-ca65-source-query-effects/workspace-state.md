# Source-query implementation checkpoint

#916 merged the live conditional source context at
f6bf23efc1b52949066836cb47531fb9aef935fd. Qualified source is
5408b4790fa88f22c127201f70f0d7e57ecde3cd; the merged tree is identical.
All 1,641 Rust/Lua tests pass (38 ignored), plus 115 Node checks,
Clippy/fmt/Ruff and docs/version checks. All 29 required CI checks pass.
Five actual native archives each pass 102 required cohorts, 725 conditional
commands and both negative controls. The fresh 4510 build matches all 564
reference bytes and verifies fourteen streams. The predecessor archive fails
only the new conditional cohort. The checked merge required one API retry;
the unchanged head/base and final positive merge/tree receipts are retained.

The final conditional audit has 200 cases, with 179 native accepts and 21
refusals. All 197 supported cases match, fixing 72 of the predecessor's 75
differences. Three ordinary-label sizeof cases remain explicitly under #798.
All 2,266 prior cases retain every previous native match. #795 remains open
for data-list operands, repeat counts and other numeric/string consumers.
The checked live context and pure defaults are now in place; this is not a
claim that all source queries across the language have been completed.

## Retained task workspaces

- `/Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers` remains the
  sole task worktree for #795. Reuse it for data-list integration, then remove
  with git worktree remove when the issue is complete, after inspecting all
  tracked, untracked and ignored content and verifying retained source/evidence.
- `/private/tmp/asm198x-text-consumers-build` is the reusable task build cache.
  Remove after #795 finishes and all task-owned build/test processes exit.
- `/private/tmp/asm198x-conditional-context-final-producer` is the new frozen
  baseline: source 5408b4790fa88f22c127201f70f0d7e57ecde3cd. identity.json records
  the qualified source tree, lock and executable hash. Its replay-summary.json
  covers 2,466 retained native cases. The supplementary eighteen-case input is
  now durable at conditional-context-prior-short-circuit-native.json rather
  than depending on the retired predecessor directory. Remove after the next
  corrected producer is qualified and its complete evidence is retained.
- `/private/tmp/asm198x-conditional-context-final-local-package` is the
  102-cohort predecessor archive for the next required-cohort control. Remove
  after the next cohort rejects it and hashes/reports are durably retained.

Seven completed directories were removed after inspection and exact retention
checks: the previous producer/package, two superseded preflight producers, the
finished clean 4510 workspace, five native report downloads and the task-created
Ruff cache. Task-only helpers/logs were also removed. The complete proof and df
readings are in conditional-context-cleanup-916/index.json. Final local and
five-host evidence is in conditional-context-local-qualification-916 and
qualification-916; incomplete/failed preflight remains explicitly separate.
No task-owned build, test, archive-control or CI monitor remains running.
The original checkout remains clean at local main
47fc51643eb98ecdd5ef2f64cfc5035bee52997e. No extra worktrees were created.

## Next implementation

Extend the approved live source context to data-list operands at their read
positions. Preserve preceding name queries, string token ownership, native
source ordering and original bound keys for final admission. Delegate scoping
and source classification to the existing reader/projection machinery and keep
formatting syntax-only. The retained root/scoped order, string consumer,
admission and conditional matrices are the next acceptance baseline. Keep
#798 label/procedure-size work explicit rather than supplying layout values to
source-time expressions.
