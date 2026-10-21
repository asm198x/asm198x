# Active audit workspace

Checked main remains 15a2c6dc3492a88f77495e4fa162189e43870c82 (#920).
PR #922 is draft and rejected: scalar b69d510f and the subsequent uncommitted
file trial regress generated fields that pass on checked main. Recovery patch,
native matrices and qualification receipts are retained on
`audit/ca65-scalar-context-922`. Source refs remain
`fix/ca65-ordered-scalar-queries` and `fix/ca65-ordered-file-queries`.

The user selected the full ca65 reader redesign for #795/#924. The accepted
approach and old failure modes are in the generated-fields full-reader-design.
Issues #918/#923 track token provenance. Independent #921 continues first on
`fix/ca65-configured-faraddr-range` from checked main, with native baseline and
plan at 3177d4f0a95a41586209d2fb4be2d082d227f5a2.

The sole reused linked worktree is
`/Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers`.
The original checkout remains untouched. Rejected trial evidence is retained
before cleanup; cleanup-922 records inventories and completion. Two disposable
caches, `/private/tmp/asm198x-scalar-build` and
`/private/tmp/asm198x-scalar-release-build`, remain for #921 and can be removed
after its checked replacement is retained. The good #920 frozen producer and
local package remain in `/private/tmp/asm198x-data-context-v3-producer` and
`/private/tmp/asm198x-data-context-v3-local-package` until a checked successor
replaces their baseline role. Shared native sources are not task-owned cleanup.
