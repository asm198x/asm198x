# Source-query implementation checkpoint

The approved #795 design is partly implemented. #914 merged the checked text
numeric callback and native source alias classification after all 29 exact-head
checks and five native archive qualifications passed. All 1,638 Rust/Lua tests
pass (38 ignored), along with 115 Node checks and Clippy/fmt/Ruff/docs checks.
The 314-case classifier audit fixes 161 wrong-byte results; 313 supported cases
are permanent. One existing undefined-query wrong acceptance remains explicit.
The new 54-case query-admission audit pins fifteen wrong acceptances for the
next context slice. Live .const and numeric text argument ordering remain open.

## Retained task workspaces

- `/Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers` is the sole
  task worktree. It remains for #795 implementation and checked merge. Remove
  it with git worktree remove when #795 is merged, after inspecting all tracked,
  untracked and ignored content and verifying retained commits/evidence.
- `/private/tmp/asm198x-text-consumers-build` is the active shared build cache.
  Reuse it for #795, then remove it after all task-owned build/test processes exit.
- `/private/tmp/asm198x-source-constancy-final-producer` is the next frozen audit
  baseline: source 499a58a29015f79a0b3897572db1e1ba9dc816c8, CLI SHA-256
  1d5c767f35ac32e5acdb3543db9a06cc320802d194bf99b83c72f70a333afc2a.
  Native results and identity are durable in source-constancy-local-qualification-914.
  Remove after the next corrected context producer is qualified and retained.
- `/private/tmp/asm198x-source-constancy-final-local-package` is the 100-cohort
  predecessor archive for the next required-cohort control. Remove after the
  next cohort proves that archive fails and its retained hashes/reports are verified.

Superseded producers, packages, native report downloads and completed clean
4510 workspaces were removed after positive retention checks. Their indexes
are in the qualification and cleanup folders. No task-owned build, test or CI
monitor remains running. The original asm198x checkout stays at its original
local main; no additional worktrees were created.

## Next implementation

Connect opt-in source hooks to the existing expression parser and live Reader /
SourceConstants environment. Reuse Deferred source classifications and original
bound lookup keys; do not duplicate namespaces or consult final layout addresses.
Read ordinary names, query-only arguments and numeric text arguments in native
order. Preserve unresolved query dependencies and spans until existing final
binding can validate them, including references discarded by constant folding.
Keep inactive/unused source lazy and formatting syntax-only. The approved design,
128-case order matrix, 54-case admission matrix and prior text/source controls
remain the acceptance evidence before #795 can close.
