# Verification

Clean producer `15aa3877fd3707f0fde242365c0ec76f88c60394` builds the unchanged
`howprice/specnext-invaders@e3211ea8ad2e83aca4d07d70bd8e09b7b51a9a1d` game
byte-identically to SjASMPlus 1.21.0. Both complete NEX files are 33,280 bytes,
SHA-256 `5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f`.
All 14 displayed addresses/sizes agree. The assembler additionally warns that
`-129` is truncated to a byte; this warning does not affect the image.

`specnext-equivalent/` retains full image hex and all ten raw process streams.
Run `python3 verify_replay.py specnext-equivalent
15aa3877fd3707f0fde242365c0ec76f88c60394` from this folder to verify the bytes,
digests, source pins and streams. The same verifier rejects the retained
initial unequal replay. The focused target does not replace the full cohort
in `ecosystem/results/current.json`.

The fixture retains 86 native executions: 81 accepted and 5 refused. It covers
forward chains, native three-pass cycles, arithmetic/widths, PC/local/module
bindings, generated structure data, binary inclusions and included constants.
Both CPU paths and formatter round trips match. CLI human/JSON refusal tests
preserve include locations and previous primary/sidecar files. Packaged
qualification requires all 86 cases through both CPU dispatches (172 runs per
host); removing that required replay fails qualification.

Local checks pass: 1,508 Rust tests with Lua (38 ignored), Clippy with warnings
denied, Rust formatting, generated documentation, Ruff and all 54 Node tests.
The initial undefined-constant test and generated-data PC test both failed
before their fixes; their full logs are retained here.
