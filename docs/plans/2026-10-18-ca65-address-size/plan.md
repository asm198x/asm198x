# ca65 address-size overrides and colon progress (#751)

1. Capture native ca65/ld65 controls for `z:`, `a:`, `f:`, case/spacing,
   indexed/indirect/immediate forms, ranges, forward/scoped names and malformed
   colons in `probe.py` and `tests/fixtures/ca65-address-size.json`.
2. Reproduce the hang with a bounded subprocess regression. Fix the shared
   scoped-name tokenizer so every accepted token advances; a lone colon errors.
   Keep valid `::` names intact. Retain native ca65 size intent at its existing
   private operand seam; select existing ISA forms and check measured widths.
   Inspect 65816/HuC paths before claiming their override coverage.
3. Verify native bytes/refusals, formatting/address/symbol invariants and CLI
   include diagnostics/output preservation. Require bounded package replay and
   omission controls. Run Rust/Lua, Clippy, fmt, Ruff, docs and Node checks.
4. Retain a clean replay of unchanged 4510 opcodes, then push a PR. Verify all
   exact-head CI and five actual-package reports/negative controls, squash merge
   and clean resources. Continue with #747 and further corpus audit findings.
