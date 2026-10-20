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

The full Rust/Lua workspace passes 1,635 tests (38 ignored). All-target Clippy
and rustfmt pass again after the full run. The clean producer is
ebb4477f08489b01025d351cb4a1f31d130b773f, CLI SHA-256
7d234fe1abc3766cad625f20ec56e3d0d908d753a0e93dfaba41ceb509b4683e.
All 98 local archive cohorts pass, including the new 469-command cohort. Wrong
bytes fail exactly the three expected controls and corrupt archives fail SHA-256.
Fresh release-mode 4510 replay matches all 564 bytes and fourteen streams,
rejects changed bytes and leaves no disposable work.
The subsequent 304-case counter audit isolates the remaining #910 bug: 165
wrong-byte outputs and 33 wrong acceptances, with no unrelated safe refusals.
Exact-head CI and five actual archive reports remain required before merge.
