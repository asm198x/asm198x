# Text values at ca65 consumers (#892, #897)

Use the already approved private byte-token store. The 396-case native audit
pins correct requests, messages and accepted bytes; 56 current differences cover
computed quotes and comma assertion messages. Preserve canonical syntax-only
formatting, lazy selection, UTF-8 text contracts, source spans and numeric Expr.

1. Add `assertion_probe.py` and retain native source/link acceptance boundaries
   for missing, empty, nonstring, adjacent and trailing message arguments across
   assertion actions and true/false conditions. Build/freeze the clean preceding
   producer and reproduce the complete text and assertion audits before code.
2. Add a private leading-text reader in `ca65/string_tokens.rs`. Resolve a
   compiler handle to UTF-8 directly and return its source tail; authored strings
   keep native raw spelling. Unknown markers fail closed and quoted marker-like
   bytes remain data. Keep strict data-literal validation separate from native
   diagnostic-prefix reading. No string reconstruction into quoted source.
3. In `ca65/mod.rs`, preserve handles through include/incbin/out/message/assert
   dispatch and consume values directly. Parse assertion header separators while
   protecting native quoted/character tokens and keep its complete message tail.
   Use native measured message-operand rules; preserve source-order numeric
   conditions and the existing action semantics.
4. In `ca65_flat.rs`, let filename readers consume private string values before
   parsing the incbin offset/size tail. Update existing linked/flat callers in
   `ca65/mod.rs`, `ca65_816.rs` and `ca65_huc6280.rs`. Formatter classification
   remains unexpanded and canonical; no global shared MOS grammar change.
5. Test all filename payloads through the existing in-memory loader on all hosts.
   Add exact message-JSON and byte/refusal/format/debug tests. Qualify portable
   message cases everywhere; name POSIX-only CLI filename coverage explicitly.
   Add required package evidence and omission controls; prove the preceding
   archive fails and inspect every real host report. Preserve all existing native
   string audits, library/Lua, Clippy/fmt/Ruff/docs and Node checks.
6. Retain clean producer, local archive controls and fresh 4510 replay, push
   conventional commits, inspect all exact-head CI and five platform archives,
   rebase-merge, then remove the owned worktree/build cache and generated files.
