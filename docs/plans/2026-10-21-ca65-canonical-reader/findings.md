# Canonical-reader admission matrix (#795, #924)

The 100 native controls accept 88 programs and refuse 12. Both checked #920
and the isolated #921 release producer match 91. The nine differences are
res_trailing, res_suffix_const and res_selector_count, each in direct, included
and invoked-macro source. Inactive and unused controls preserve lazy behavior.
These extend the existing generated-field and source-order issues; #921
introduces no difference in this matrix.

The portable probe reproduces all 100 source/configuration contexts and include
files before comparison. Native tool hashes and both process streams are
retained. No native process timed out. Expected results come from native ca65
and ld65; they are not inferred from the current implementation.

The agreed approach is in full-reader-design.md. Before replacing the reader,
retain these controls together with the original 40 generated-field controls,
320 scalar controls, 200 file controls, 18 repeat-name controls and all checked
#920 matrices. Migrate source tokens and consumers above the shared numeric
grammar, preserving source provenance, scope bindings, lazy capture/include
order, opaque strings, syntax-only formatting and logical debug addresses.
