# CPU-specific address extraction verification (#909)

The 65816 declaration now separates `.addr` from `.word`. Only `.addr` lowers to
existing low/high byte extraction; HuC6280 `.addr`, word ranges and unsigned
wide-data behavior are unchanged. No engine, expression or public schema is added.

All 148 native cases agree (107 accepted, 41 refused), correcting the 37 measured
65816 safe refusals. Permanent CLI tests cover all cases with native file/JSON
bytes, four preserved outputs, first-error file/line, official debug reading,
symbol/section round trips and idempotence. The required archive cohort uses
469 commands and cannot be omitted.

The broader 254-case replay improves exactly ten 65816 addr boundaries and
loses no preceding match. Only the twelve separately tracked per-element
counter cases remain under #910. All four focused data cohorts pass, all 113
Node checks pass, and all-target Clippy, rustfmt and generated docs/version
checks pass. The generated 65816 surface and book search index are regenerated;
the initial stale-doc failure and successful generation/check logs are retained.
Full workspace, frozen artifact and five-host CI evidence remain required.
