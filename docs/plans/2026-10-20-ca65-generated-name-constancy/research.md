# Selected generated labels missing from constancy names

The 32 native controls accept 27 inputs and refuse five. Clean predecessor
12af1dae8d81d8be968f717f5a2b3fb5870b5690 (CLI SHA-256
e43eb5c6a1cab293f97d50cdf802aca055e8da840c1905437582d7edd7c4a1f6)
has 22 differences. Thirteen follow an actual selected generated definition;
its address binding exists, but the text-function constancy name set only
pre-collects static source spellings. Both bind_head_label and project_selected
update source_known without adding the selected name to text_names.

Nine controls concern existing #795's broader source-ordered query semantics:
four scoped queries, four forward generated/include definitions and one skipped
static declaration falsely considered present. Do not pre-expand future source
or change that query model to fix already-bound names. Keep those nine cases
explicitly accounted against the complete retained audit and visible under #795.

The isolated registration fix belongs at both selected binding sites, alongside
the existing source_known insertion. It adds no namespace, AST field, layout
lookup or new evaluation pass. Native missing/skipped-generated/unused-macro
controls remain errors; actual selected macro/repeat/include definitions become
known. The withdrawn macro attempt stays withdrawn until this prerequisite has
its own original/formatted byte, address, constancy and full package qualification.
