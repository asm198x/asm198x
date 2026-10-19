# Generated macro-head qualification

The complete 120-case native audit contains 67 accepted and 53 refused inputs.
All 96 targeted raw/formatted outcomes and all 67 images match native ca65.
The 24 original raw static register/prefix macro-head false acceptances remain
explicit #853 exclusions. Each has a native refusal and a positive acceptance
in the frozen predecessor; tests account for every audit case exactly once.
This slice resolves 50 prior differences in generated heads and instruction-name
admission. Both former #862 gaps in the existing 46-case formatter cohort pass,
leaving eight unrelated raw gaps and two #762 formatted alias gaps tracked.

The initial pending producer added register/prefix checks using the current CPU.
The follow-up 398-case audit caught two new valid refusals before merge: S/SP
call labels captured under 6502 remain identifiers after switching to 4510.
The PR became draft. The corrected slice checks instruction names and preserves
existing register/prefix behavior for #853's fuller token-origin work. Both
captured CPU inputs are required positive controls. All 398 outcomes retain the
same 204 existing differences as the preceding main producer; none is introduced
by this corrected slice. Native evidence is linked in research.md and #848/#853.

The red-first base test reports unsupported .macro on a generated definition;
the extended red-first test catches the captured-S refusal. The corrected tests
preserve native bytes, 61 explicit official-reader label addresses/kinds, canonical
idempotence and symbol/section shape, include/macro diagnostics, all old output
files and authored source. The initial pending actual archive fails exactly the
extended macro cohort. The preceding main archive fails both changed cohorts.
The corrected and clean archives pass all 85 required cohorts, including 297
commands in the 96-case macro cohort and 184 in the 46-case formatter cohort.
Wrong bytes fail exactly three byte-contract cases; archive corruption fails
checksum validation. Complete reports and both controls are retained.

Full local validation passes 1,608 Rust tests (38 ignored), 100 Node tests,
Clippy/fmt/Ruff and generated docs. Clean producer
c478532a4f4fee2fd608a594f8e0f33a125b01a4, tree
c1ba5b03656cc28fbde9762c54469930b057fa08, has CLI SHA-256
4e34c5dcdd830a0769c39727fcd47aa279d1d35e1dfaea06e2eeb84ca9a2842a,
identical to the frozen native-check binary. Shipping source, tests, scripts
and embedded native facts match the verified producer byte for byte.

The fresh clean 4510 replay preserves all 564 native/upstream bytes, seven
processes and fourteen stdout/stderr streams; its verifier rejects a changed
byte. The disposable build workspace was removed automatically. Exact-head CI
and five actual platform reports/receipts with all ten corruption controls must
qualify this corrected shipping head before rebase merge. Older-head CI cannot
qualify it. Complete local evidence is retained as deterministic gzip files.
