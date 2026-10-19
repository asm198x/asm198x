# Bind scoped exports through existing references

The final audit retains 274 native cases. The qualification cohort fixes 43 disagreements
and contains 222 cases (135 accepted, 87 refused). Eight scoped `.const`
cases remain #795. Forty-four import/global allocation, address-size and
same-file external controls are tracked in #810. Actual preceding/current comparisons
across every raw case show no new regressions. No old fixture is weakened.

The reader scopes expression names but leaves visibility operands bare. Export
operands now use the existing scope environment and lexical reference marker,
with current-scope placeholders and reference events. The existing final
scope_targets lookup resolves them through local definitions or final outward
fallback, just as it resolves expressions. Global visibility checks likewise
name the actual binding for zero-page and collision checks; their broader
allocation behavior and import semantics remain #810.

Exported definitions use the private-to-Equ lowering already used by enum
members. This preserves source width, PC, deferred values, definition ownership,
source queries and duplicate-definition checks. The source-preserving native
node remains unchanged for formatting. No new scope model, statement lexical
field, object-file linker, public shape or dependency is introduced.

The existing fused visibility validation rejects different defined bindings
with the same external spelling. Repeating one binding, including two scoped
exports that both fall back to the same root binding, remains valid. Cheap and
qualified visibility operands are refused as native does. Included error spans,
five old outputs and source contents survive refusal.

The broader attempt to allocate import/global placeholders was rolled back
when new controls exposed same-file external resolution and source address-size
requirements. Those belong together in #810. Retain the raw controls and the
zero-regression comparison; do not silently introduce new import refusals as
part of an export correction.
