# Warn once for nonzero BSS data (#807)

All 272 native controls match in bytes, acceptance and initialization-warning
counts: 256 accepted and 16 refused. The actual preceding #801 executable has
52 warning disagreements while retaining native bytes. Raw commands, versions,
source/config/includes, process streams and statuses are retained, including
the first 168-case observation and eight nonzero-default-fill controls.

Native initialized content does not always warn. Pinned cc65
`e11fb5c39371046ebe25485f984f644c5a0d65d3`, `src/ld65/segments.c`'s
GetNonBSSSection scans nonzero literal and expression fragments, ignoring
reservations. `src/ld65/config.c` asks once per configured BSS segment. This
explains zero-data/BRK/explicit-zero-fill silence, nonzero data/instruction/fill
warnings, file/non-file equivalence, and one versus two affected segments.
The missing warning is not a reason to restore omitted bytes.

The private placed statement preserves absent versus explicit reservation/
alignment fill. Omitted expressions still validate before their actual bytes
are inspected; raw and fill contents need no new allocation. The warning uses
the existing advisory shape and the first contributing source file/line, once
per segment. Formatter reassembly preserves bytes, counts and debug shape.

The first human-mode test reported no warning even though JSON already carried
one. The ca65 CLI branch had no warning-rendering loop. It now uses the existing
vasm/flat display pattern, with included source paths, both with and without
debug artifacts. The initial provenance assertion expected `nonzero.inc:2:`;
the existing warning contract prints `nonzero.inc: line 2:` and is unchanged.
The initial compiler check caught a moved Kind; the flag is captured before
resolution consumes it. Logs retain these intermediate failures.

Final checks pass 1,557 Rust tests (38 ignored), 76 Node tests, Clippy with Lua
and warnings denied, fmt, Ruff and generated docs. The final expanded focused
test passes separately, including both human/JSON modes, included provenance,
all-zero/no-warning controls, multiple segments and failure preservation of
five prior outputs and source contents.

The actual local Lua archive passes all 61 required cohorts, including 1,040
commands here. Both corruption controls fail for their intended reasons. The
preceding actual archive fails only this cohort while all other 60 pass.
An earlier package run caught a fixture update during qualification:
`272 !== 264`; it performed zero commands in this cohort and was discarded.
The final audit ran against unchanged fixture/harness content and passes.
Local archives remain artifact-only; clean producer replay and five-host
qualification remain required before merge.

Clean unchanged producer `a0b78c0ac5bb759867131886c6ad61325308ddae`
replays pinned cc65-4510-opcodes `e11fb5c39371046ebe25485f984f644c5a0d65d3`.
All 564 bytes match native SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
Seven process records and fourteen streams verify; the changed-byte guard
rejects corruption. The producer is retained on the task branch.
