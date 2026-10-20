# Token-list selector bound-type qualification

The native 100-case audit finds 21 string-valued bounds wrongly accepted by the
preceding producer. The 99-case required cohort now matches all native outcomes
and all 32 accepted images, raw and canonically formatted. The one existing
scoped numeric-lookup control remains explicitly under #795, with complete audit
coverage accounting. The wider 623-case comparison removes exactly seven wrong
acceptances and introduces no new difference; its 255 existing native-valid
admission refusals remain under #872 and its follow-up grammar work.

The ca65-only selector helper rejects Arg::Text at the three bound reads, then
uses the unchanged Scope::number for numeric expressions. Numeric/constant/char
and large/negative controls preserve native behavior. Literal, nested string,
text-define, captured macro/argument, selected/repeated/include controls pass.
Untaken branches, unused macros and zero repeats remain lazy.

The red-first test catches successful assembly of .left("2",{"abcd"}) where
native refuses it. Focused tests preserve complete bytes, official debug parsing,
constant kind/value, formatter idempotence/symbol-section shape, human/JSON
include/macro locations and five previous outputs plus authored source.
The preceding actual main archive fails exactly the new bound-type cohort.
Fixed and clean archives pass all 86 required cohorts; the new cohort executes
195 commands. Wrong bytes fail exactly three byte-contract cohorts; archive
corruption fails checksum validation. Complete actual reports are retained.

All 1,610 Rust tests pass (38 ignored), plus 101 Node tests,
Clippy/fmt/Ruff/generated docs and native formatting. Clean producer
3518aaa314a7b6632b545c1f75d047cf14fddc86, tree
229c682c29194835c62c10c431daca88a97b5a2b, has CLI SHA-256
303e98c5e4a9886a3eb7f20010e96a0744c67038155bea394e12c675b7af1dcb,
identical to the frozen native-check binary. Shipping source, tests, scripts and
embedded native facts match the verified producer byte for byte.

A fresh clean 4510 replay retains all 564 native/upstream bytes, all seven
processes and fourteen stdout/stderr streams. Its verifier rejects a changed
byte. The audit removed its disposable build workspace automatically. Complete
logs and process reports are retained as deterministic gzip files. Exact-head CI
and all five actual platform reports, producer locks and ten corruption controls
must qualify this shipping head before checked rebase merge.
