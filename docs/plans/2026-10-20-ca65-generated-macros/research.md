# Generated macro definition and invocation labels

The retained native audit contains 118 cases: 65 accepted and 53 refused.
Clean predecessor `9b48a599d997e883505d60ce47ed65d76c05ff03` has 74 differences.
Generated heads are structurally classified before text folding, while the
linked macro-header/call split recognizes only static labels. The generated
label therefore survives into ordinary parsing instead of becoming a selected
macro head. Static instruction names on definition heads also bypass the native
reserved-token admission check. Those are the separate #862/#870 symptoms.

Use the existing balanced generated-label classifier to find the macro word
without evaluating an inactive head. For a selected definition or invocation,
fold only its label, apply the existing native reserved-head rule, and bind it
through the existing selected binding path. Bind a call head before reading its
expanded body. Preserve definition-body capture and invocation-argument timing,
source AST and the shared macro grammar. Existing #881 registration makes actual
names visible to constancy queries on both raw and canonically separated labels.

The fresh red-first test fails on `6502_mark_True_definition` with
`unsupported directive .macro`. The focused fix passes the full 118-case test,
59 explicit official debug-reader label address/kind assertions, canonical
round trips, include/macro diagnostic frames and preservation of all prior
outputs and source files. The complete original-source comparison has zero
remaining native differences, and all 65 accepted native images match.

Formatting preserves all 118 native outcomes and 65 images. Twenty-four invalid
static register heads become ordinary label lines that the assembler already
accepts under #853; every formatted false acceptance is proved against the
frozen predecessor. They are not counted as repaired by this slice. Both former
#862 raw-assembly gaps in the existing generated-format cohort are restored;
eight unrelated tracked raw gaps remain.

The earlier trials and failed boundary evidence remain at
[the retained resume](https://github.com/asm198x/asm198x/tree/733ca4bb/docs/plans/2026-10-19-ca65-generated-macros).
The independent spacing, identifier grammar, cheap-anchor and generated-name
constancy fixes qualified before this resume. The shipping branch imports this
slice alone; it does not import the prototype's earlier trial ancestry.
