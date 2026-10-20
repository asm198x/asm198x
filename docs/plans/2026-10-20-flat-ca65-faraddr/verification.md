# Unsigned far-address verification

The approved DataRange policy remains private. Existing ACME and SjASMPlus
Sized constructors select SignedOrUnsigned; both flat ca65 `.faraddr` readers
select Unsigned and preserve the complete expression until emission. Explicit
low/high/bank extraction remains byte extraction. Instruction pieces are unchanged.

All 132 native cases match: 64 accepted, 68 rejected. The preceding clean
producer wrongly accepts 66 cases. All 80 diagnostic cases now match their
native outcomes and expected source/macro context as well.

Permanent CLI tests retain all 212 cases, check the four output files survive
errors, and require file/JSON bytes, official Debug198x reading, symbol/section
round trips and idempotence for accepted source. Archive cohorts require 140
and 324 actual commands respectively and cannot be omitted from qualification.

The first new test run shared a scratch name across concurrent tests, producing
`left: Number(3), right: Number(6)` and `No such file or directory`. Distinct
suite names fix that test isolation bug; both tests pass (212 cases). No source
behavior was changed in response to those test harness failures.

The complete Rust/Lua workspace passes 1,633 tests (38 ignored).
All 111 Node checks, all-target Clippy, rustfmt, Ruff and generated docs pass.
The first full run stopped with `spawn CLI: No such file or directory` during
an overlapping rebuild of the test executable. The subsequent complete run
kept the executable stable and passed; both logs are retained.

Frozen clean source e45db33642a42c4a10e4002b61f050c62ddb21bd has CLI SHA-256
bb458d654a38dd17af6101f41a60120445a262e58fba709509713f6b833de536.
All 96 archive cohorts pass, including 140 diagnostic and 324 far-address
commands. The preceding archive fails exactly these two cohorts. Wrong bytes
fail the three byte/determinism/structured-success controls; a corrupt archive
fails SHA-256. Fresh 4510 replay matches all 564 bytes and fourteen streams,
rejects changed bytes and leaves no disposable work directory.

The subsequent 254-case data-width audit retains every preceding match and
improves 20 far-address cases. Its remaining fifty differences reproduce on
the preceding producer and stay separate from these two fixes. Exact-head CI
and five native archive reports remain required before merge.
