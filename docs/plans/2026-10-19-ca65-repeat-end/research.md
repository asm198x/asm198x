# Executed repeat termination

The native ca65 V2.18 captures have 22 cases: 17 accepted images and five
refusals. The clean numeric-admission predecessor differs on eleven accepted
cases; its 4294967295-count execution exceeds the five-second process bound.
The baseline comparison terminates and reaps that command explicitly.

The sweep already stops reading after .end, but its repeat loop continued
running the remaining count, then reported an unclosed repetition. The capture
was in fact complete before execution. Stop after restoring each iteration's
registers/counter, and unwind captures normally. Keep the existing conditional
unclosed-block refusal and the deferred character-input guard. A pending include
can be opened before .end without reading its contents; a missing selected file
must still fail when opened. Captured oversized tokens still fail before any
iteration, even after an authored .end or under a zero count.

Four accepted controls verify formatter idempotence, native bytes and stable
symbol/section shape. Thirteen other accepted captures expose the existing #833
formatter source-loss bug: the authored .end disappears. The zero-count control
proves this changes 08 to an empty image in native reassembly. Those formatter
gaps are retained separately; all seventeen original images still require native
byte equality. The mandatory ca65_repeat_end package cohort runs all 22 original
assemblies and twelve supported formatter commands, 34 commands total.

The initial integration test fails count_1 with
`asm198x: input.s:1:1: error: repetition block is never closed`, left false,
right true. After the narrow loop fix, both focused tests pass, including
five-output preservation and included nested-conditional locations in human
and JSON modes. Each command has a five-second runtime bound.

Local verification: 1577 Rust tests pass (38 ignored); 85 Node tests,
Clippy with Lua/all workspace targets, fmt, Ruff and docs pass.
