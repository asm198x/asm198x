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
