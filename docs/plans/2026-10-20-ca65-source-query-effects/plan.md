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
