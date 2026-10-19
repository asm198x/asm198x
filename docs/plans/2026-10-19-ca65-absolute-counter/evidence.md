# Preserve the native absolute cursor across segment switches (#822)

All 63 completed native controls match: 62 accepted and one refused. The actual
preceding clean suffix-head executable differs on 40. The matrix includes
origins above/below/equal to the linked base, modes and mode labels, CODE/RODATA/
BSS/ZEROPAGE switches, all four data widths, aliases, reservations/alignment,
push/pop, include/macro/repeat streams and absolute/relative bit-branch modes.
The absolute bit-branch uses a fixed target so an incorrect PC cannot cancel
against the same incorrect PC in its operand. No native timeouts are expected
refusals; tool hashes, sources/configs and complete process outcomes are kept.

Native segment.c GetPC/NewFragment uses global AbsPC by default, independently
of ActiveSeg->PC. The old linked path used one bias against each statement's
physical linked address. That agrees within a segment but changes the reported
PC as physical segment addresses change. Replace the bias/bool with one
optional absolute cursor, read before mode directives for labels/definition
positions, and advance it after emission by the same resolved size used by
physical placement. Org sets it after its existing 24-bit range check; reloc
clears it globally. Physical segment offsets, alignment and image layout stay
on the existing path. Native optional OrgPerSeg is not the default.

Both new integration tests failed before the fix; the log retains the exact
wrong bytes. Final checks pass 1,567 Rust tests (38 ignored), 81 Node tests,
Clippy with Lua/all targets/warnings denied, fmt, Ruff and generated docs.
Accepted controls preserve formatter idempotence, reassembly bytes and debug
shape. Included human/JSON refusal retains line 2/path and all five existing
outputs and both source files. The global cursor also supplies deferred alias
definition positions, data operand PCs and instruction-relative addresses.

The actual local Lua archive passes all 66 required cohorts, including 249
commands in this cohort. Wrong-byte and corrupt-archive controls refuse as
intended. The actual preceding archive fails only this cohort; its other
65 pass. The mandatory-cohort omission control passes.

This corrects byte values before the approved #814 logical-debug-section work.
Debug coordinates remain physical here; #814 is still open. Source-time origin
constancy remains #815, and #788's approved lexical exception remains separate.
Clean unchanged producer 2eaad70d2a68b38e363529150eca79c45f31ff8a replays
pinned cc65-4510-opcodes e11fb5c39371046ebe25485f984f644c5a0d65d3.
All 564 bytes match native SHA-256
8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec.
Seven processes and fourteen streams verify; changed-byte corruption is
rejected. The source tree stayed clean and unchanged during replay.
All five actual host package reports/receipts with ten controls remain
required before merge.
