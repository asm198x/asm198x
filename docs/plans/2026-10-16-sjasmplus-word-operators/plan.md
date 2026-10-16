# SjASMPlus word operators (#635)

1. Capture fresh native uppercase/lowercase/mixed-case, identifier-boundary, precedence and logical-NOT cases, including data, EQU and conditional expansion. Keep version/hash/argv and whole outputs.
2. Add red fixture replay for both SjASMPlus CPU frontends, then admit only the measured vocabulary through the existing Z80 syntax hook. Keep Pasmo's independent case, NOT and precedence rules. No new dependency, public format or architecture.
3. Verify native bytes and refusals, formatter idempotence and reassembly, includes/macros/repetition, debug/listing surfaces and failed-output preservation. Run workspace, Clippy and generated-source checks, push a focused PR and wait for every exact-head check/workflow before merging.
4. Record any independently observed numeric-width behavior separately; word recognition does not establish whole-expression parity.
