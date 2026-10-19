# Numeric source admission (#788, following #831)

1. Re-consult parse-affecting, conditional, macro and diagnostic decisions.
   Retain native boundary/admission/order cases and the clean #831 predecessor.
   Measure formatter and optional-feature baselines before choosing controls;
   keep unrelated pre-existing syntax/formatter failures separately visible.
2. Add a private character-level numeric validator in `ca65_flat.rs`, sharing
   `parse_number`'s measured bounds. Consume native digit/hex-suffix boundaries,
   skip identifiers, quoted text and comments, and perform no expression/name,
   directive, text-environment or include operations. No AST/schema/dependency
   change. Pin lone prefixes, trailing letters, leading zeros and token lists.
3. In `ca65/project.rs`, validate selected character lines before text work,
   untaken source without meaning, and entire original macro/repeat captures
   before execution (including zero counts). Preserve literal .end stopping.
   Existing AST spans determine physical delimiter lines; borrow the original
   root on the single-file Sweep and read included sources through SourceMap.
   Index source lines once and never substitute a missing location with zero.
   In `ca65/mod.rs`, supply that root source to the private constructor only.
4. Validate discarded .else/closer source as well as node bodies. Determine
   a block's physical end from its last child and its existing close marker;
   .elseif's nested conditional shares its parent's closer. Preserve every
   original source/macro span. Captured token sources retain the existing
   input priority, text snapshots and deferred-include order.
5. Extend the existing macro grammar's read/capture seam only if needed for
   the flat ca65 CPU paths: default no-op for other dialects, ca65 numeric
   validation before a body can be discarded. Do not add a source prepass or
   enable unsupported flat directives. Preserve formatter parsing separately.
6. Add `tests/ca65_numeric_admission.rs`, a retained native fixture and a
   mandatory package cohort/omission control. Reproduce red with the clean
   predecessor; prove native acceptance, bytes, source positions and five-output
   preservation in both modes. Keep inactive includes unopened and unknown
   inactive names/directives unparsed. Check supported formatter round trips.
7. Freeze source/fixtures/harness for Rust/Lua, Clippy, fmt, Node, Ruff/docs,
   actual preceding/current packages and both corruption controls. Push source,
   retain a clean unchanged 4510 replay, inspect all exact-head workflows/checks
   and five actual host reports/receipts plus ten controls. Merge only the
   qualified head, close #788 only when its numeric admission scope is covered,
   retain commits and remove finished task worktrees/caches.

The user approved the narrow lexical exception on 2026-10-19. This implements
that agreed boundary using the existing sweep, source map and macro seams;
it does not change the AST or introduce a second semantic reader. #829/#830
cover separate .end execution/label gaps. No new dependencies are planned.
