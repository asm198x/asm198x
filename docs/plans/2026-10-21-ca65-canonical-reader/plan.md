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

## Next implementation baseline

The classifier move, borrowed numeric grammar and owned token preparation are
pushed through 172f9da8d42a30b699a25a42d125eba130bb9e35. They do not implement
the full reader or close its compatibility issues. Before the next behavior
change, move those preparations onto checked #929 after it merges, retaining
its empty-vector syntax and diagnostic fixes rather than copying the old
numeric parser wholesale. #929 source is 9b0daf2efaf8f413792a0c7cee24f2ec9717590d;
its five-host qualification was still pending when this note was written.

The frozen successor at /private/tmp/asm198x-empty-producer has 38 matrices,
7,054 case rows and 6,509 native matches. These are regression controls with
duplicate contexts and known gaps, not a completion percentage. Replay each
matrix using its recorded comparator and explicit dialect handling; the three
new mixed-reader matrices cannot use one global flat/configured switch. Keep
all prior matches, and include #928/#930's valid and invalid counterparts in
the full-reader migration's mandatory fixture inventory. Retain source/evidence
before replacing the predecessor or deleting task diagnostics.
