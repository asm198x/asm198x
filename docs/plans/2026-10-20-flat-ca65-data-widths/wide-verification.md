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
