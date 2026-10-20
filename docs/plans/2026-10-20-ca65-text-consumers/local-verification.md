# Text-consumer qualification

All 396 native text-consumer cases and 124 native assertion-boundary cases now
match. The preceding producer differs in 56 cases in each audit. Assertion
boundaries remove 48 wrong acceptances and eight wrong refusals. The text audit
also checks actual message payloads and uniquely different file contents, avoiding
exit/image-only false greens. All nine earlier byte/token/lifetime/text audits
lose no preceding native match and introduce no unexplained new admission.

The portable required cohort has 472 cases (264 accepted, 208 refused), with
1,264 CLI commands. It checks exact user payloads, native bytes, four previous
outputs on refusal, canonical/idempotent formatting, official Debug198x parsing
and symbol/section round trips. All 88 native filename requests are also tested
through MemoryLoader on every OS, with 44 additional direct requests on the flat
65816/HuC6280 readers; 48 filesystem-specific cases have additional
Unix CLI round trips. The actual five-host packages execute the 472 portable
cases, including 40 real-file cases with ten portable names. POSIX-only names
are explicitly accounted for rather than assumed to exist on Windows.

The full workspace with Lua passes 1,625 tests (38 ignored), all 106 Node checks
pass, and Clippy over every target, rustfmt, Ruff and generated docs checks pass.
An earlier full run stopped with OS error 28 during test output; its complete
partial log is retained as full-io-failure.log.gz. After removing an inactive
incremental cache, the complete required run passed without disabling checks.

The existing `.out` nonstring diagnostic remains “one string”. Private typed
filename entry points preserve the existing shared physical-filename interfaces
used by RGBASM. Binary data remains valid while the existing UTF-8 text contract
refuses binary values. Cross-reader/unknown handles fail closed and quoted marker
bytes remain literal data. Numeric Expr, source-order semantics and inactive
lexical policy stay unchanged.

The preceding archive passes all 90 old cohorts and fails exactly the new text
cohort. Source/test/native evidence and complete negative package outcomes are
retained here. Clean producer, local archive controls, fresh 4510 replay and all
five actual package reports remain required before merge.
