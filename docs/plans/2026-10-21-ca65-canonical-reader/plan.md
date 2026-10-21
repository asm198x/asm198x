# Canonical ca65 reader implementation

The user selected the full reader redesign. Start from checked #925 main
`ee522abd40a0bfa7a61b5581679679690717ccff`; retain the shared numeric grammar,
existing scope environment and shared TextSyntax/StringTokens mechanics.
Neither rejected #922 reader implementation is a donor for source changes.

1. Centralize the existing native token classifier from ca65_flat.rs in
   ca65/reader.rs and route current token-list consumers through it. Keep its
   exact lexical behavior, authored slices, opaque handles and register kinds.
   Add the module in ca65/mod.rs and only the visibility needed by its callers.
   Verify ca65-family library/native controls and compare frozen #925 outcomes.
2. Build canonical ordered source consumption on those tokens, retaining
   physical/generated origin, capture/source spans and expansion bounds. Delegate
   numeric precedence/operator parsing to mos6502.rs through one token-source
   seam; preserve strict pure expression entry points for other dialects.
3. Migrate ca65/mod.rs directive fields and ca65/project.rs SourceContext,
   conditional/repeat/header/data/file consumers to the same reader. Expand
   generated fields at their consumed position. Preserve pending-include order,
   source-time requirements and shared scope binding. Avoid whole-line effects.
4. Apply the same token consumption to nested text arguments through the
   existing text.rs/TextSyntax contract and ca65_flat.rs grammar. Keep numeric
   Expr values as i64 expressions and computed byte strings in StringTokens.
   Migrate ca65_816.rs and ca65_huc6280.rs consumers through shared seams.
5. Verify physical register/generated identifier admission (#918/#923), lazy
   lexical validation, captured macro/repeat input, syntax-only formatting and
   logical/physical debug invariants. Reconsult binding decisions per slice.
6. Require all old failure-mode controls in full-reader-design.md: the checked
   historical baseline, original 40 generated-field, 320 scalar, 200 file, 18
   repeat-name, expanded 100 reader and 190 far-address controls. No checked
   predecessor match may be lost. Add mandatory package cohorts and explicit
   known-gap dispositions; remove dispositions only against native evidence.
7. Qualify the final source with full Rust/Lua, Clippy, Node, fmt/docs/version/
   ecosystem/ledger checks, clean release, fresh4510 and all five native archives
   with corruption and predecessor controls. Use checked PR/rebase workflow.

Each implementation commit must build and pass checks appropriate to its seam.
Keep the single existing task worktree and reuse its caches. The frozen #925
producer/archive replace #920 as baseline. Preserve evidence and remove task
artifacts/processes before finishing. Public AST/API breaks and new dependencies
require separate approval; this plan does not introduce either.
