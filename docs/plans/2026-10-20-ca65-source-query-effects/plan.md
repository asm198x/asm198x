# Audit source-query effects before the next #795 design

1. Retain 56 native root/scoped, within-expression, constructor, alias,
   macro/include/unused and inactive controls. Replay the current frozen
   producer and compare accepted bytes and preserved refusal outputs.
2. Trace Ca65Text's read-only number/defined snapshots, text Scope.evaluate,
   project text_line and Reader.source_value/scope_syms reference effects.
   Explain why marking references in a whole-line preprocessing pass would
   reorder earlier .ref queries. Consult the binding scoped-symbol and selected
   reading decisions. Do not change parser/evaluation behavior during this audit.
3. Write a concrete source-aware evaluation proposal using the existing shared
   scope environment, preserving pure adapters and syntax-only formatting.
   Obtain agreement before introducing a shared source-expression/callback seam.
   All current #906/#908/#909/#910 verification and cleanup continue meanwhile.

## Approved implementation

4. Extend probe.py/native.json with missing/forward names, alias classes, nested
   numeric argument ordering, repeats and inactive controls. Replay the frozen
   predecessor before changing the existing parser and reader seams.
5. Add optional source hooks in dialects/mos6502.rs and the checked text numeric
   callback in dialects/text.rs. Connect Reader and Projection through the
   existing scope environment; consume text functions in expression order.
6. Add native-backed integration and candidate archive coverage. Require all
   retained source/text cases and explicit refusal/output/formatter invariants.
7. Freeze and retain the producer, run workspace/lint/docs and local archive
   controls, then require exact-head CI and five actual native archive reports.
   Commit, push, rebase-merge the checked PR and remove completed task caches.
