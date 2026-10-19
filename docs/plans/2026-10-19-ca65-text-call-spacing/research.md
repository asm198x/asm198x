# Text-function whitespace boundaries

The 73 native controls accept 61 inputs and refuse 12. The clean scope producer
1cacfa5d42d42522e4e2e036cb95653d81845f41 (CLI SHA-256
99a47d49e1730577c0c3a5294724c626c3aa29036dd6f11d1f0b9621e01f2d48)
has 35 differences. The shared text scanner requires an adjacent parenthesis in
both fold_once and has_call. Native admits spaces/tabs; nested folding must use
the same boundary in both functions. An unrecognised spaced .tcount/.blank even
succeeds with zero output bytes instead of native 03/01, so status-only checks
would miss data loss. Some numeric functions already work through another path;
keep them as controls rather than infer one scanner explains every function.

Four .left controls expose a separate token-list admission difference, including
adjacent syntax: native .left(2,"abcd") returns the quoted token unchanged and
emits all four ASCII bytes 61 62 63 64. Asm198x refuses the unbraced string token
list. This is not character slicing or argument order. Keep it explicit and
separate from whitespace. The remaining 31 differences are the spacing target.

This prerequisite follows the withdrawn #862 trial. Do not reuse its production
patch until this independently qualified lexical boundary is available. Spaced
generated heads must get actual symbol/address assertions, not merely successful
body bytes that do not reference the head.

The private TextSyntax seam now supplies the measured ca65 horizontal-whitespace
posture, with other dialects retaining their defaults. A single call_open helper
is used by fold_once and has_call. Nested calls therefore use the same boundary;
quotes/comments remain untouched. All 31 spacing differences are resolved. The
four unbraced .left controls remain explicit #872 failures, not waived matches.

The Rust matrix asserts real mark/row2 address labels through the official debug
reader, accepted native bytes, rejected-output preservation and canonical
idempotence/symbol-section round trips. Include/macro diagnostics check human and
JSON spans/frames and preserve five existing outputs and all source text.
All 73 native formatter outcomes and 61 accepted images match. The exact two-line
#871 report also reproduces native 03 01 versus the preceding CLI's empty image.

All 96 Node tests and Ruff checks pass. The actual archive passes all 81 cohorts,
including 244 new commands, and both corruption controls fail. Its executable is
identical to the frozen native-check prototype (SHA-256
1fdcbf84dace5acad9ee6d11f5677a83906937cfbe3cc846ca2be9903646905d).
The preceding archive fails exactly the new cohort. One focused run hit a CLI
replacement race from a concurrent cargo build: spawn CLI: Os { code: 2, kind:
NotFound, message: "No such file or directory" }. Retain that failure; external
checks now use a frozen executable. Final full/focused Rust, lints, clean producer
and replay qualification remain required before merge.

Final full Rust passes 1,601 tests (38 ignored). The sequential focused rerun,
Clippy, rustfmt and generated docs pass; all 96 Node tests and Ruff checks pass.
The focused build-output race is superseded by this clean rerun. Source is ready
for a clean producer and complete replay before shipping.

Final clean producer: `551ec01e5c4f511f4d8758b7124dc8f8c7269b6a`, tree
`6211df2d3b933d8c814b292607d5d98d7af91dfd`, CLI SHA-256
`1fdcbf84dace5acad9ee6d11f5677a83906937cfbe3cc846ca2be9903646905d`.
The actual archive passes all 81 cohorts and 244 spacing commands. Changed
bytes fail exactly three reference controls; corrupt archives fail SHA validation.
A clean 4510 replay matches all 564 native bytes across seven processes and
fourteen retained streams, with a changed-byte guard. Disposable audit work was
removed. Shipping code and harness match the clean producer exactly.

A subsequent 40-case name audit found existing adjacent/spaced .ident argument
validation and missing cheap-local-anchor differences. Those are separate
semantic scopes; this token-boundary slice preserves their adjacent-form posture.
They need retained dedicated issues and native name/anchor probes before the
macro-head work resumes. No source from the withdrawn macro trial is shipped.
