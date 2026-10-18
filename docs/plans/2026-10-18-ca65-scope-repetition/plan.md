# Native procedure closers and repeat tokens (#759, #760, #764)

1. Capture unnamed/named procedure closers, nested/repeated scopes, immutable
   definitions, labels, scope headers, qualified names, substrings, quoted text,
   nested counters, and zero-iteration controls against ca65/ld65. Retain .set
   and := findings separately for #761/#762.
2. Reproduce mismatches with the bounded CLI native/formatter harness. Treat an
   unnamed `.proc` as the native deprecated `.scope` form for closer matching.
   Substitute active repeat counters as whole tokens at the selected-line read,
   before parsing definitions and operands. Reuse the established macro token
   substitution mechanism if it preserves text correctly; retain original AST
   source for formatting. Do not pre-expand untaken branches or the whole file.
3. Require package cohort and omission controls. Align with the scoped-constant head for the full suite, then rebase onto its
   squash and verify all tested source/config/fixture trees are identical before
   the clean 4510 replay and exact-head CI.
   Run the Rust/Lua suite, Clippy, fmt, Ruff, docs and Node checks.
4. Push a PR, inspect every exact-head workflow and all five actual package
   reports and negative controls, squash merge, clean finished worktrees, then
   continue the measured compatibility findings.

Native probes require preserving UTF-8 and cheap/numeric token boundaries in
substitution, and stopping an outer counter at include-file boundaries. Nested
counter headers are parsed after outer substitution, so numeric counter names
refuse. Existing unnamed-procedure behavior is native `.scope` closer matching.

Keep #765 (native refusal of a repeated included constant) as a distinct
include-capture investigation with retained native evidence; do not guess its
binding mechanism in this token/closer slice.
