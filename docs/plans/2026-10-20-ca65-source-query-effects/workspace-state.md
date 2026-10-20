# Source-query implementation checkpoint

The approved #795 design remains partly implemented. #915 merged conditional
required-name admission at 301e1fe2fc4ad65ac00debaf88d4d2d7da2f9cc3, with
qualified source 0e963625f5dd814e2dc0f135512c34819823b709. The merged tree is
identical to the qualified tree. All 1,639 Rust/Lua tests pass (38 ignored),
along with 115 Node checks, Clippy/fmt and docs/version checks. All 29 required
CI checks and five native archives pass; each archive ran 101 required cohorts,
156 query-admission commands and 1,229 classifier commands, plus both negative
controls. The initial macOS Lua job acquired no runner; the exact capacity
annotations and successful same-commit retry are retained in qualification-915.

The 54-case admission audit now matches 51 cases, fixing twelve wrong
acceptances. Three logical .const controls remain explicitly excluded under
#795. Eighteen supplementary native short-circuit/.ifnconst controls all match,
fixing nine additional wrong acceptances. All 2,194 earlier replay cases retain
every prior match. The classifier fixture now includes all 314 cases, with its
last undefined-name wrong acceptance fixed and no exclusions.

## Retained task workspaces

- `/Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers` is the sole
  task worktree. Reuse it for #795; remove with git worktree remove after the
  issue is complete, after inspecting all tracked, untracked and ignored files
  and positively verifying retained commits and evidence.
- `/private/tmp/asm198x-text-consumers-build` is the reusable task build cache.
  Remove after #795 work finishes and task-owned build/test processes exit.
- `/private/tmp/asm198x-query-admission-producer` is the next frozen baseline:
  source 0e963625f5dd814e2dc0f135512c34819823b709. identity.json records both
  executable hashes and the source tree/lock. asm198x is the replay producer;
  asm198x-lua is the release-feature archive producer. Complete records are
  retained in query-admission-local-qualification-915 and qualification-915.
  Remove after the next corrected producer is qualified and retained.
- `/private/tmp/asm198x-query-admission-local-package` is the 101-cohort
  predecessor archive for the next required-cohort control. Remove after the
  next cohort rejects it and the archive hashes/reports are retained.

The old classifier producer/package, new clean 4510 workspace, five downloaded
native reports and task-only helpers/logs were removed after exact retention
checks. query-admission-cleanup-915/index.json records every verified file,
source retention, removed directory and df readings. No task-owned build, test,
archive qualification or CI monitor remains running. The original checkout
stays at local main 47fc51643eb98ecdd5ef2f64cfc5035bee52997e; no additional
worktrees were created.

## Next implementation

Connect the approved opt-in source parser hooks and live Reader/Projection
context. Consume ordinary names, .const and numeric string arguments at their
native expression positions. Reuse Deferred source classification, the existing
shared scope environment and original bound lookup keys. The conditional
source_reads metadata now demonstrates final admission; extend that seam to
live .const/numeric arguments before short-circuit folding discards names.
Query-only arguments stay non-reading; inactive/unused source stays lazy and
formatting stays syntax-only. The retained 128-case ordering matrix, 54-case
admission matrix, supplementary eighteen controls and earlier source/text
matrices remain the acceptance evidence before #795 can close.
