# Native size observations before implementation

The first 100 completed controls contain 83 native acceptances and 17 refusals.
Raw source/config/includes, process results and explicit preceding CLI identity
are retained. Labels record the logical counter delta of their own line (zero
for a standalone label); scopes/procedures take the first retained physical
segment span. Empty scopes/procedures have no size attribute. Scope-only labels
remain separate from named scope sizes, and aliases do not inherit sizes.

Pinned cc65 main.c defines the label before HandlePseudo and records GetPC()-PC
when the line leaves the segment unchanged. symtab.c closes spans at scope exit,
defining a size only when a retained span exists and using the first span.
Counter-mode changes can therefore affect label sizes without affecting scope
spans. Non-first segment data may matter when the initial span is empty.

The boundary controls independently prove an address bug for labels on .org and
.reloc: the current implementation binds them after changing the counter,
contrary to native. Fix that bug before adding missing size attributes. This
commit retains research only; #798 remains open and no size implementation is
claimed. Source-time .org constant validation and unsigned counter underflow
also need explicit boundaries in the eventual size design.
