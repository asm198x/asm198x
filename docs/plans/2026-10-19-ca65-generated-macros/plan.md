# Bind generated labels at macro definition and invocation heads (#862)

The selected ca65 reader recognises macro definitions and labelled calls before
live text folding. Both use a static-label-only split; generated labels hide the
macro word. Reuse the balanced syntax helper to locate the authored prefix, and
bind selected generated heads through the existing projection seam. Keep macro
bodies opaque and capture-time text state unchanged; never evaluate an inactive
head or add a second macro expander.

1. Retain native definition/call controls for static/generated/concatenated and
   spaced heads, parameters, empty bodies, captures, include/macro frames,
   nested/inactive/repeated source, cheap/anonymous labels and CPU-dependent
   register/mnemonic token rules. Separate any unrelated false admissions.
2. Add red-first Rust controls using native bytes, official address labels,
   canonical formatter outcomes and human/JSON diagnostics with output/source
   preservation. Confirm definition labels bind when defined and call labels
   bind before the body, including .defined/.referenced tests.
3. Extend only ca65's private macro-head recognition/selected binding in
   `dialects/ca65/{mod,project}.rs`, leaving shared macro grammar and source AST
   unchanged. Preserve original text for formatter and capture semantics.
4. Add a required package cohort and omission guard. Run focused/full Rust+Lua,
   Node, native formatting, Clippy/fmt/Ruff/docs, prior/current actual archives,
   corruption controls, clean producer and complete native 4510 replay.
5. Ship isolated task commits on current main after #869, inspect exact-head CI,
   five actual package reports/receipts and all ten controls, rebase merge and
   clean or reuse owned workspaces after verifying retained source and evidence.

## Resume after independent prerequisites

The initial trial was withdrawn and retained. #873 qualified horizontal text-call
spacing, #877 qualified identifier-string validation, and #880 qualifies native
cheap-token/anchor rules. Resume on those seams, without reintroducing trial
harness changes or trusting body bytes as proof of a bound label.

Extend the native matrix with invalid string names and root/anchored cheap heads.
Rebuild the fixture and assert real mark/row2 addresses for every concatenated,
sprintf and spaced head, including after actual canonical formatting. Apply only
the two ca65 source-file hunks from the retained patch, then re-run all controls
against the fresh predecessor. Keep shared macro grammar, body capture and argument
timing unchanged. Ship isolated task commits on main after #880, with the full
native/Rust/Node/archive/4510/five-platform qualification used for each prerequisite.
