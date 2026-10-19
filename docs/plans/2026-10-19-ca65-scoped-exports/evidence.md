# Resolve exports inside their named scopes (#803)

The 222-case qualification fixture matches native status and all accepted bytes:
135 accepted, 87 refused. Its explicit preceding #807 executable disagrees on
43. The full 274-case research matrix retains every source, config, native
process and both executable comparisons; it has no new regressions. The final
hint correction also fixes one external-width control outside qualification.
Eight scoped `.const` cases remain #795; the 44 import/global controls belong
to #810 (43 still disagree). No existing fixture is changed or weakened.

Visibility operands bypassed ordinary scope binding. The reader now records
exports using the existing lexical reference key and source-reference events;
final scope_targets resolves the same key used by numeric expressions. Local,
forward, outer and nested fallback work. Exported definitions use the existing
Equ/deferred path, retaining definition PC, source width, ownership and spans.
The formatter retains the original native source spelling. Different actual
bindings sharing an external spelling refuse, while repeated declarations and
multiple fallback exports of the same binding remain accepted.

The broader import/global placeholder attempt was rolled back after explicit
same-file exports exposed a separate external-resolution/address-size problem.
That complete set is tracked in #810; no new failure is left for its future fix.
Scoped constancy still hits the existing explicit #795 guard; separate native
`.defined`/`.referenced` controls prove the export declaration's source effects.

The first actual archive caught an existing-cohort regression:
`globalzp_forward` emitted `AD0000` instead of native `A500`.
The source-time byte declaration had been stored under a lexical reference
marker while root operands used the plain declaration key. Normalize that key
for the existing size-hint set, leaving final value binding separate. The
existing anchor fixture/expectation is unchanged and now passes with this one.
Both the initial failing Rust/archive records and final passing logs are kept.

Final checks pass 1,559 Rust tests (38 ignored), 77 Node tests, Clippy with Lua
and warnings denied, fmt, Ruff and generated docs. Native fixture cases also
pin formatter idempotence, reassembly and stable debug symbols/sections. An
undefined export inside an include reports its own line in human/JSON modes
and preserves five prior outputs and both source files.

The actual Lua archive passes all 62 required cohorts, including 627 commands
here. Both corruption controls fail for their intended reasons. The actual
preceding archive fails only this cohort while its other 61 pass. These local
archives are artifact-only; clean producer replay and five-host qualification
remain required before merge.
