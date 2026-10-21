# Resume ordered data queries after checked #810

The user approved completing #810 before resuming #917. Preserve the rejected
source branch and audit refs. Start a new task branch at the checked merged
#919 tree; do not force-push the old draft or merge main into it.

1. Reconsult the source-query design and binding scope/text decisions. Write
   this plan into the existing source-query plans folder before source edits.
   Reapply only the implementation from ce3955b3 (ordered data reads) and
   48e2428d (qualified generated names), using a three-way patch over their
   combined source change. Retain the rejected run receipts on the existing
   audit refs; do not copy stale outcomes into the new qualification. The preceding 68e9b6a4/221c7087 commits retain old qualification and
   cleanup; their evidence already survives on the audit ref.
2. Resolve overlaps in ca65/project.rs, ca65_flat.rs and ca65/mod.rs by keeping
   #810 declaration allocation, source-expression graph, external classification,
   width validation and explicit export binding. Apply live data reads through
   the previously approved callback seam. Preserve the pure text admission guard
   wherever live ordered evaluation has not replaced it. Keep the external
   fixture and mandatory cohort; no prior test weakening or gate waiver.
3. Capture the 515-case data audit against the frozen qualified #810 producer,
   then against the candidate. Require every native case to match. Replay all
   26 frozen matrices / 4,136 cases with no lost prior native match. Inspect the
   eight formerly refused scoped data-constancy cases explicitly. Existing
   numeric-consumer and #918 lexical cases remain separately inventoried.
4. Run focused source-query, external, visibility, formatting and string tests,
   all-target Lua Clippy, formatting, 115 Node/setup tests, docs/version/ecosystem
   checks, then the complete Lua workspace suite with a frozen CLI. Record actual
   results and failure output. Stop and reduce scope after repeated failed fixes.
5. Push a small conventional source commit and successor checked PR referencing
   draft #917. Build an isolated clean committed-source release producer; run a
   fresh 4510 audit and all retained native comparisons. Qualify all 104 mandatory
   cohorts. Require wrong-byte and checksum controls to fail, and #810's actual
   predecessor archive to fail only the new ca65_data_context cohort.
6. Read all 29 exact-head CI checks and five workflows, then inspect all five
   actual native archives' reports, source-lock receipts, data bytes/JSON,
   format round trips and refusal streams. Rebase-merge only after every gate
   passes. Mark #917 superseded only with the concrete successor linked.
7. Retain final evidence and inspect/remove finished task-owned caches and
   archives. Reuse the existing sole task worktree and shared debug cache. Keep
   the current qualified producer until its checked replacement is retained.
   Continue #795's retained 50-case numeric-consumer audit afterwards.
