# Canonical ca65 reader implementation

The user selected the full reader redesign. The current v2 branch starts from
checked #929 main `71ee7240a8518c2deadd4340d9e10d0dd6bd33d9`; retain the shared numeric grammar,
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
Keep the single existing task worktree and reuse its caches. The frozen #929
producer/archive replace #925 as baseline. Preserve evidence and remove task
artifacts/processes before finishing. Public AST/API breaks and new dependencies
require separate approval; this plan does not introduce either.

## Current implementation baseline

The original preparation history is retained on fix/ca65-canonical-reader at
dccdc1b40cf2b9d432efff046eca2fe8eaeb85b9. The v2 branch starts from checked
#929 main 71ee7240a8518c2deadd4340d9e10d0dd6bd33d9 and preserves its numeric
empty-vector syntax and diagnostic fixes. All five native archives and final
metadata checks passed before that merge; complete recovery evidence and verified
112-root/615-file cleanup are retained on audit/ca65-empty-calls-929 at
bb01746d76a1f4990abebcf01d60f0aac20fa492.

The successor /private/tmp/asm198x-empty-producer holds 39 matrices, 7,309 case
rows and 6,700 native matches. These include duplicate contexts and known gaps,
not a completion percentage. Replay each matrix with its recorded comparator
and explicit dialect handling; mixed-reader matrices cannot use one global
flat/configured switch. Keep every prior outcome during preparation and every
prior native match during semantic migration. Retain accepted and refused
controls together, and namespace matrix-local names in combined inventories.

## Checked successor preparation

The v2 branch retains the preparation history on checked #929 without rewriting
any pushed branch. The one numeric-parser conflict was resolved by combining
the borrowed peek/advance interface with #929's empty-vector call grammar.
All 777 library tests passed at that conflict checkpoint. The owned token and
provenance preparations are then replayed, and the 722-case #927 fixture and
all 39 successor matrices must retain every outcome before v2 is pushed.

Current baseline: /private/tmp/asm198x-empty-producer, immutable executable
SHA e851007448fb971890c98258fe7b5429ed890a7e3f19e3715dd1dbe788b0006e,
39 matrices / 7,309 rows / 6,700 native matches. Numeric/brace/empty capture
sources are retained on audit/ca65-captured-numbers-928 at
f89cb4c0a02752f44a787d8312b836ae195ca5bc. The empty-capture extension confirms
all 63 selected constructor errors, 30 flat numeric-capture gaps (#928) and 34
inactive gaps (#895); 75 observations improve over checked #925 with no lost
matches. Keep numeric captured calls unevaluated, namespace matrix-local case
names, and require new controls for generated selector brace tokens.

The first semantic migration now moves complete linked data operand lists to
owned canonical input, including captured queries and token constructors. The
stream-controls plan and findings retain its native evidence and regression
gates. The remaining linked scalar/header/file consumers and flat/syntax
adapters still need migration. This partial implementation does not close #924
and is not ready to merge independently of the approved full-reader design.

## Checked linked consumer milestones

The v2 branch now has checked owned-stream adapters for linked data, allocation,
assertion/message fields, repeat count/counter headers, numeric conditions and
include/incbin fields. The native name/selector lookahead correction (#932) and
explicit ca65 unary-plus grammar option (#931) are checked and pushed. Issues
remain open until the owning full reader is qualified and merged.

The current replay preserves all predecessor matches and has 6,934 native
matches across the retained 39 matrices / 7,309 rows. Scalar, file, numeric-
consumer, canonical, generated-field and repeat-name audits are respectively
320/320, 200/200, 50/50, 100/100, 40/40 and 18/18. These overlapping observations
are acceptance evidence, not a completion percentage. Each slice's findings,
source hashes, full receipts and verified cleanup are retained alongside this
plan. The latest full workspace checkpoint has 1,653 passing Rust/Lua/doc tests
and 38 existing ignored; later ca65 slices run the affected library/contracts
and Clippy/fmt, and a full current run remains mandatory before merge.

Remaining work includes other name/head/statement adapters, flat ca65-family
readers and canonical syntax-only consumption. Preserve macro/repeat capture,
input priority and all contract invariants; then add mandatory native package
cohorts, run the complete current checks and five actual native archives, and
use checked PR/rebase merge. The branch is not yet merge-qualified.
