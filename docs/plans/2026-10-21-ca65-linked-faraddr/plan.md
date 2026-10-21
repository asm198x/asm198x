# Configured ca65 far-address ranges (#921)

Start from checked main/#920, 15a2c6dc. Reader candidates #922 and the following
file trial were rejected by generated-field controls; do not import their code.
Reuse the active task worktree and build caches. No extra worktree is needed.

1. Reproduce the original 90 native controls and extend them to 190 cases before
   code: full-width bounds, forward aliases/labels, scopes, `*` at carries and
   24-bit limits, lists, masking, invalid suffix output and lazy/macro/include
   controls. Compare the frozen checked #920 binary; require explicit counts,
   preserved original verdicts and retained tool identities/process streams.
2. Keep each original expression through the existing configured native payload
   and linker. Add private FarAddrs variants beside Words/DWords in ca65/mod.rs
   and link.rs. Update project.rs symbol mapping/source widths, demanded-value
   discovery and resolved placement to three bytes per expression. Reuse the
   configured linker's existing unsigned to_width(value, 0xffffff) at final
   evaluation, then emit three little-endian bytes. Advance PC once by three,
   including `*`; do not evaluate cloned byte extractors at separate PCs.
3. Leave lobytes/hibytes/bankbytes extraction, flat family Operation::Sized,
   scope/type metadata, config/layout, public AST/API and reader behavior intact.
   No dependency or new architecture is needed: this follows existing private
   data-width variants and the current configured range policy. Before editing,
   inspect every existing Words/DWords match, including wildcard metadata paths.
4. Add complete native configured-faraddr evidence and a required package cohort
   with omission/predecessor controls, bytes/JSON/debug-reader/formatter tests,
   refused-output preservation and original file/macro diagnostics. Preserve all
   native matches of checked #920, especially generated fields that rejected
   #922. Explicitly retain unresolved #795/#918/#923 audit differences.
5. Run focused/full Rust/Lua, all-target Clippy, Node, fmt/docs/version/ecosystem,
   clean release, fresh4510, local archive corruption/predecessor checks and all
   five actual native archives. Freeze the shared CLI for the full suite. Use
   CARGO_INCREMENTAL=0 with the existing task caches; do not alter tests/profiles.
6. Push conventional commits, qualify exact source and rebase-merge only after
   all current required checks and actual receipts pass. After the final PR body
   edit, wait for new metadata workflows to complete before merging.
7. Retain evidence and recovery refs, inspect/remove obsolete trial artifacts,
   and verify exited processes. Keep checked #920's producer/archive until a
   checked replacement is retained. Await #924 design agreement before resuming
   source-token reader work.

Correct AGENTS.md's stale descriptive protection sentence using the current main
rules snapshot; preserve its stricter full-check/native/rebase convention.
