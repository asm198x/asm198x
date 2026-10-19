# Bind mode-directive labels before changing the counter (#812)

1. Re-consult syntax, native AST, placement and diagnostic decisions. Retain
   bounded native before/after controls for .org/.reloc, offsets, global/cheap/
   anonymous/scoped labels, aliases/PC, includes/macros/repeats and failures.
   Compare an explicit preceding scoped-export executable.
2. Correct the existing order in `ca65/mod.rs`: define the label under the old
   PC/mode before applying the directive, as native main.c does. Keep definition
   placement positions and subsequent statements under their existing counters.
   No new counter model or public shape; .sizeof remains #798 and unrelated
   .org source-constant restrictions stay a separate issue.
3. Add `tests/ca65_org_labels.rs` and `tests/fixtures/ca65-org-labels.json`, native
   bytes/status, formatter/debug invariants and included refusal preservation.
   Add a required package cohort and omission control to the two existing scripts.
4. Verify isolated Rust/Lua, Clippy, fmt, Node, Ruff/docs, actual archive and both
   corruption controls. Prove the preceding archive fails only the new cohort,
   push verified commits and retain clean unchanged 4510 producer replay.
5. Wait for #803, inspect every exact-head workflow/check and five actual package
   reports/receipts plus ten controls, squash and verify closure/retention. Clean
   the finished workspace/cache and resume the retained #798 size audit.

The concrete matrix also exposes #813's data-operand PC reuse. This slice owns
both counter corrections: `ca65/link.rs` advances data PCs per width and the
existing `demanded_values` walk uses the same positions. Include the sixteen
lazy-cycle controls so a byte-only correction cannot conceal a new refusal.
