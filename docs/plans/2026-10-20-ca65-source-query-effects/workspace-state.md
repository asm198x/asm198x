# Source-query implementation checkpoint

The full #795 source-aware context design is approved. The checked text numeric
callback is implemented and verified: 1,637 Rust/Lua tests pass (38 ignored),
114 Node checks pass, and all-target Clippy/rustfmt/Ruff pass. The live reader
and parser integration is unfinished; #795 remains open. The main 128-case
native matrix and nine alias controls are retained with predecessor results.

The preceding #910 fix is merged as #913 after all 29 checks and all five
native archive qualifications passed. Its actual five-host reports and merge
identity are retained in the counter plan's qualification-913 folder.

## Retained task workspaces

- `/Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers` is the sole
  task worktree. It remains for #795 implementation, regression verification
  and checked merge. Remove it with git worktree remove once that work is
  merged and all tracked, untracked and ignored content is inspected.
- `/private/tmp/asm198x-text-consumers-build` is the active shared build cache.
  Reuse it for #795, then remove it after all task-owned build/test processes
  exit. It contains no durable unfinished source or unique evidence.
- `/private/tmp/asm198x-flat-counter-producer` retains the frozen #795 audit
  predecessor (source 0a67ffa43e7eadd15103c84ec0795b7ba8a60739, SHA-256
  9dd103083dba8a4ecd5148ad619d317b8ff3f965a371be9c4db6370aff10e8bc).
  Retain until the source-aware replacement is qualified and its before/after
  replay is durable, then verify retained evidence before removing it.
- `/private/tmp/asm198x-flat-counter-local-package` retains the 99-cohort
  predecessor archive for the next package omission control. Remove it after
  #795's new required cohort proves the old archive fails and that report is
  retained. Its existing qualification and controls are already durable.

The original asm198x checkout remains on its original local main commit. No
additional worktrees were created. Completed native report downloads and the
older producer/package/audit directories were removed after positive retention
checks; their indexes are in completed-slice-cleanup and the qualification
folders. No task-owned background build, test or CI monitor remains running.

## Remaining implementation

Connect an opt-in live reader/parser context through SourceConstants and the
existing scope environment. Consume ordinary reads, name-only queries and
numeric text arguments in native order. Use Deferred source classifications
for aliases without consulting final addresses. Preserve syntax-only formatting
and lazy inactive/unused source. Require the retained native matrices, preceding
source/text controls, permanent CLI/archive coverage, full verification and all
five actual host archives before closing #795.
