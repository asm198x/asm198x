# Unsigned wide-data correction (#908)

Both frontends now use existing unsigned Sized data: width two, big-endian for
`.dbyt`; width four, little-endian for `.dword`. No engine or instruction range
changes are made. Native audit: 108 cases, 56 accepted, 52 rejected. The fix
corrects forty wrong acceptances, including delayed forward arithmetic and
included negative values. The four remaining native acceptances are safely
refused by unchanged `.if` admission under existing #895. Full facts/replays
and explicit four-case refusal evidence remain retained.

The permanent fixture covers all 104 supported cases (52 accepted, 52 rejected)
and names the four excluded #895 audit cases explicitly. CLI and required
archive tests check native bytes/refusals, error file/line, four preserved outputs,
formatter idempotence, file/JSON bytes and debug symbol/section round trips.
The new required archive cohort uses 260 commands and cannot be omitted.

The 254-case broader replay improves exactly the 28 known dbyt/dword boundary
cases and loses no preceding match. Remaining ten addr boundaries and twelve
per-element counter cases remain separately tracked under #909 and #910.
The initial full-matrix test reported `unsupported directive .if` for one of
#895's known refusals; the source fix was unchanged when the fixture scope was
made explicit. The failed and final focused logs are retained.

The complete workspace passes 1,634 Rust/Lua tests (38 ignored), and all 112
Node checks pass. Clippy initially reported `unused import: Piece` in each flat
frontend after dword stopped using instruction pieces. Removing only those two
imports leaves all-target Clippy, rustfmt, generated docs/version checks and all
316 supported focused CLI cases green. Both Clippy logs are retained.

The initial frozen producer is 100c0d7774e9255290891b6e2085f61e10580e8e.
The final import-only corrected producer is 8041d4f52e9f03726cbd3e4f0897c7b8fef0c409,
CLI SHA-256 7c1d2665d1f8211a7d65676b1816e305ae64c33541b66bfa80725ca4ba0591e2.
Its binary differs, so the final producer was independently replayed against
80 diagnostic, 132 far-address and 108 wide-data native cases (the four #895
refusals stay explicit) and rebuilt into a new archive. All 97 final archive
cohorts pass, including the new 260-command cohort. Wrong bytes fail exactly
three controls and corrupt archives fail SHA-256. The preceding archive fails
only the new wide-data cohort.

Fresh release-mode 4510 replay from the initial source matches all 564 bytes
and fourteen streams, rejects changed bytes and removes disposable work.
The subsequent 148-case addr audit reproduces 37 separate 65816 safe refusals
under #909; every HuC6280 and strict-word control agrees. Exact-head CI and five
actual native archive reports remain required before merge.
