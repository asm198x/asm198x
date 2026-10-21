# Source-query and external-binding checkpoint

#916 is merged at f6bf23efc1b52949066836cb47531fb9aef935fd. Its qualified source
is 5408b4790fa88f22c127201f70f0d7e57ecde3cd. That remains the last qualified
source-query baseline. The original checkout is untouched.

## Draft #917: deliberately unmerged

Source branch fix/ca65-ordered-data-queries is retained remotely at
48e2428d565e3c7a010ae50963198b47b76e9650 (including ce3955b3). The 515 data
controls match native and fix 186 differences; the release replay retains every
match across 2,466 old cases. Fresh 4510 verification passes. The candidate is
not qualified: the full suite and four actual uploaded native archive reports
fail ca65_scoped_exports/import_queries_before_definition. A scoped imported
symbol followed by a local definition is wrongly accepted after the previous
scoped-constancy refusal is removed. The remaining native job was cancelled.
No failed check was waived and no old fixture was weakened.

A narrow guard trial restored that refusal but lost nine previous matches in
the 274-case import/export matrix. It was preserved and rolled back, not pushed
as source. Follow the user-supplied rollback rule; do not repeat partial guard
fixes. The approved next step is #810's complete coupled contract, then return
to #917. The concrete design is scoped-imports-before-data-design.md; the user
said “Keep going” on 2026-10-21 after the recommendation and scope question.
Start #810 from merged origin/main, not the rejected source tree.

Audit, failed qualification, rejected patch and design are retained remotely
on audit/ca65-data-context-917. Complete local/native reports are in
 data-context-local-preflight-917 (rejected), development trials in
 data-context-trials, and focused successful checks in data-context-verified.
The next-consumer audit is numeric-consumers-{probe.py,native.json,after.json}:
50 native accepts, 30 active differences, 20 passing lazy controls. It remains
part of #795: repeat counts, reserve/fill, align/fill, assertions, numeric output
strings, incbin bounds and include names. #798 ordinary sizeof stays separate.

## Cleanup and retained workspaces

Nine finished task directories and 39 temporary files were inspected, retained
with hashes, checked against remote source refs and removed. Every task process
exited. data-context-cleanup-917 contains the positive inspection, deletion and
df receipts (about 301 MiB more available at that observation). No new worktree
was created. Do not remove any unknown sibling directories.

Keep these exact workspaces while #810/#795 continues:

- /Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers: sole task
  worktree; reuse for #810, then #917/#795. Remove with git worktree remove only
  after all unique tracked/untracked/ignored work and commits are retained.
- /private/tmp/asm198x-text-consumers-build: reusable task build cache. The debug
  CLI was last compiled during the rejected import trial; it is not a frozen
  baseline. Rebuild on the new #810 branch before using it. Remove at task end.
- /private/tmp/asm198x-conditional-context-final-producer: qualified #916 frozen
  binary, identity and 23-dataset replay summary (2,466 cases). Keep until the
  next checked replacement is qualified and all evidence is durably retained.
- /private/tmp/asm198x-conditional-context-final-local-package: qualified #916
  102-cohort predecessor archive. Keep for #810's omission control, then remove
  after a checked replacement and positive retention checks.
- /private/tmp/asm198x-data-context-v2-producer: rejected #917 source 48e2428d
  release binary (SHA e68edf1e9d4dd73044df9ff21d56e422dacca306e155f33e874a332140c8d28f),
  identity and 515-case comparison. Keep only as the frozen unmerged data
  predecessor until #810 is checked and #917 is rebased/requalified. Its
  metadata/outcomes are durably retained; it is not a successful candidate.

No task-owned build/test/archive/monitor process remains running. The two
superseded native candidate workflows and CI runs are stopped. Temporary
helpers from this slice were retained and removed; recreate from retained
sources when needed. Shared CI/rebase helpers still pre-existed in /private/tmp.
