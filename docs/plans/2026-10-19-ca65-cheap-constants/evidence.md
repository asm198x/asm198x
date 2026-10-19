# Bind ca65 cheap constants and preserve their tokens

Native ca65 accepts `anchor: / @value=7 / .byte @value`; the old archive refuses
it as an invalid constant name. Cheap definitions now reuse the existing
private anchor key and Scope::Local shape while retaining their authored name.
An ordinary immutable symbol reads its RHS under the previous anchor, then
starts the next cheap namespace. Native controls prove both boundaries.

The existing ca65-only scoped_names expression grammar now preserves @name
(including digit names). The reader binds bare operands, arithmetic and
positional name-query markers with the same private mapping; conditional and
repeat folds receive the current anchor. The existing macro symbol-character
hook includes @, so parameter value cannot rewrite @value into @7. A native
caller-visible label control emits `070010`; the old CLI refused @7.

The fixture/probe retain 36 qualifying native ca65/ld65 V2.18 cases: 28 accepted,
eight refused. Bounded CLI tests compare bytes/status, formatter idempotence,
reassembled bytes and symbol/section debug shapes. Human/JSON refusal of an
unanchored included definition names its file/line and preserves all five
previous outputs and both source files. No public AST, span, API, schema or
trait shape changed; flat readers keep their existing evaluation path.

The previous archive fails only the new cheap-constant cohort. The final real
Lua-enabled local archive passes all 50 required cases, including 120 commands
here, and rejects both corruption controls. Compressed local reports are
preflight without producer-lock receipts; five-host CI remains required.

After a shared-target collision was detected in the following label slice,
workspace packages were cleaned in a separate worktree build target and local
verification repeated: 1,530 Rust tests passed (38 ignored), 65 Node tests
passed, Clippy/fmt/Ruff/docs passed. The archived preflight uses that isolated
Lua-enabled executable. These logs supersede earlier shared-target runs.

Clean producer `0c712059ab12cedd50e490a57bb6ee65fdda2a54` still matches all
564 unchanged upstream/native 4510 bytes, SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
The retained report has complete comparison bodies and fourteen raw streams;
its verifier checks identities/content/completion and rejects a changed byte.
The full ecosystem ledger remains untouched. Rebase preserved identical tested
crate/script/workflow/lockfile trees.

The scope audit held this staged result before merge. The completed #778/#784
correction and its final local verification are in scope-evidence.md; those
results supersede the staged counts and scope deferral above. Reserved names
and attached labels continue as #775/#781.
