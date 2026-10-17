# DISPLAY formatting (#735)

The old handler treated `/H` as an expression, split quoted commas, printed
negative numbers as 64-bit hexadecimal, and substituted unknown expressions
as source text. The retained initial red log shows the original `/H` refusal
and `-1` printing sixteen hex digits instead of native's eight.

The bounded probe captured 132 SjASMPlus 1.21.0 executions: 118 accepted and
14 refused. The probe retains the exact command; every record retains source, status, raw diagnostic text
and emitted bytes; the fixture pins the native binary version and SHA-256.
The tool is invoked with only error/display messages, so successful stderr is
the native DISPLAY payload plus its `> ` prefix and final newline.

The implementation follows native [dirDISPLAY](https://github.com/z00m128/sjasmplus/blob/v1.21.0/sjasm/directives.cpp#L1603).
H prints unsigned 32-bit hexadecimal with at least four digits; D prints unsigned
decimal; A prints both; B prints the low byte in two four-bit groups; C prints
the low byte as printable ASCII or an escaped hexadecimal byte. Options are
case-insensitive. Each numeric expression resets the mode to H; strings do not.
L and T are accepted legacy options that leave the mode alone. Empty output
emits no note. Single quotes allow doubled apostrophes without backslash
interpretation; double quotes use the existing native escape decoder. Output
stops at the first NUL, after all operands have still been validated.

DISPLAY now runs through the existing live SjasmEval walker with its location
counter and three-pass forward resolver. It reuses operation-level local/module
qualification and bare-global fallback. A global label on the DISPLAY line
becomes the local anchor before evaluation. No engine operation, AST, public
API, dependency or diagnostic schema changes. Notes still use Diagnose/Note
and preserve the established human/JSON representation.

The native differential checks both CPU entry points, exact bytes and note
text, idempotent formatting and preserved symbols/text/bytes after formatting.
Controls cover all numeric formats, mode reset, signed widths, quoted commas,
escapes and apostrophes, UTF-8, NUL, forward/global/local/module/PC expressions,
macros and inactive branches. Refusals cover invalid options, malformed operands,
undefined symbols and division by zero. CLI controls check included line 5 in
human/JSON errors and preserve the source plus all five prior output artifacts.
The mandatory package replay checks every fixture against the actual executable,
including JSON note text and retained binary output on refusal. Removing that
case makes the qualification contract fail.

A forward divisor has a provisional zero before its definition is reached.
The retained forward-red log shows that early division wrongly refused native
accepted `10/N` with a later `N equ 2`. DISPLAY now defers provisional evaluation
errors when a forward read requires more passes, and always propagates final
errors. A later `N equ 0` still refuses, as does a permanently undefined value.

## Clean whole-project control

Producer `4f58a38f9b8d8d802ee395c8b3b7988625362ed6` was built clean by the
explicit ecosystem runner against unchanged SpecNext pin
`e3211ea8ad2e83aca4d07d70bd8e09b7b51a9a1d`. DISPLAY no longer blocks the
project. The next boundary is `src/main.asm:289`, `SAVENEX OPEN`; this directive
is explicitly reported as unsupported. No whole-project byte match is claimed.
Native still emits 33,280 bytes, SHA-256
`5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f`.
The report retains that complete image and all ten raw process streams; each
stream was checked against its process record, and the image hash/length were
recomputed. No upstream build script ran. This focused one-target capture does
not replace the complete-cohort ecosystem ledger.

Local validation: 1,503 workspace tests passed, 38 explicitly ignored. The last
temporary-label guard and 132-case fixture also passed the focused suite.
All-target/Lua Clippy with warnings denied, formatting, docs, Ruff and 52 Node
checks passed. Logs are retained alongside this report.
