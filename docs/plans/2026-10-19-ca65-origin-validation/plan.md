# Validate native source-time origins (#815)

1. Re-consult syntax, source scopes, native AST, conditionals/passes, placement
   and diagnostic decisions. Retain native ca65/ld65 cases for 24-bit bounds,
   forward/known constants and aliases, absolute/relative PCs, label differences,
   scopes, lazy expressions, includes/macros/repeats and source-order effects.
   Compare an explicit preceding #812/#813 executable before choosing a fix.
2. Write a concrete design using the existing selected-source/projection and
   constant-analysis seams. Do not add a naive early numeric fold that rejects
   native-valid label differences or loses an alias's definition PC. Keep source
   time availability separate from final outward lookup and linked CPU bases.
3. Correct the private reader/projection/validation path and add
   `tests/ca65_origin_validation.rs` plus its retained native fixture. Assert
   bytes/status, range endpoints, formatter/debug invariants, bounded alias
   behavior, included spans and preservation of five old outputs on refusal.
4. Add a required package cohort and omission control in the two existing
   package scripts. Verify isolated Rust/Lua, Clippy, fmt, Node, Ruff/docs and
   preceding/current archives with both corruption controls. Push verified work
   and retain clean unchanged 4510 producer replay.
5. Wait for #816, inspect every exact-head workflow/check and five actual package
   reports/receipts with ten controls. Squash, verify closure/retention, clean the
   finished workspace/cache and continue the next confirmed issue.

The #814 logical-debug-section proposal is awaiting design agreement. Its
metadata representation is independent of this source-validation audit.

## First slice

Land the independent seven-case origin-range correction first, using the
existing Kind::Org error path and a required ca65_origin_bounds package cohort.
Keep #815 open for the broader source-time validation described above.
