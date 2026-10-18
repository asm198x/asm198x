# ca65 4510 PHD alias (#748)

1. Retain native ca65/ld65 source, versions, hashes and processes with `probe.py`
   and `tests/fixtures/ca65-4510-phd.json`; reproduce the current alias refusal.
2. Map PHD to existing PHW forms only for ca65 4510 in `ca65/mod.rs` and reflect
   the spelling in `ca65_flat.rs`. Keep 65816 PHD unchanged. Test exact native
   bytes, selectors, reflection, formatter symbols and CLI output preservation.
   Existing word-width mismatch #749 stays separate.
3. Require the captures in actual-package qualification and omission controls.
   Run workspace/Lua tests, Clippy, fmt, Ruff, docs and Node checks.
4. Commit the clean producer; replay pinned unchanged 4510 source and compare
   complete output to native and upstream reference, retaining all raw streams.
5. Push PR, verify exact-head CI and all five actual-package reports and
   corruption controls, squash merge, retain evidence and remove task resources.
