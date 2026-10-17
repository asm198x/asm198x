# Native NEX V1.2 output (#739)

The user approved the ArtifactFormat::Nex extension. The implementation follows
the recorded bounded OPEN/CORE/AUTO/CLOSE design, with no new dependency.
The user separately approved tracking the absent --no-save control in #741;
this slice does not add or claim that flag.

The initial red log records the existing UnsupportedFeature refusal on the first
OPEN. The extended fixture contains 70 actual SjASMPlus 1.21.0 executions,
42 accepted and 28 refused, retaining every source, stream, status and complete
output bytes. Explicit V1.3 OPEN is one measured native acceptance deliberately
kept as UnsupportedFeature by the agreed V1.2 scope. The original 53-case
research capture and compressed images remain intact alongside it.

Primary writer facts were added first to the umbrella reference library and
merged in stevehill1981/198x#182, commit
2f50de4ff77f136a1edf65f8412fe35377553bf4. The serializer cites that source;
its typed error and byte-layout helper depend only on the standard library,
not assembler/contract types. No emulator-loader code was used as layout truth.

SAVENEX lowers to an internal typed command in the existing AST/operation stream.
Its expressions use normal symbol and local/module qualification. Commands run
in the emit pass against current Next device pages; AUTO snapshots nonzero banks
in native NEX order, advancing over zero banks too. CLOSE and EOF finalize the
header. END's resolved start address supplies OPEN's default entry PC. CORE
and 16-bit address truncation retain native warnings. Wrong devices, invalid
banks/order/arguments and extended RAM without CFG refuse. Sequential saves,
including repeated destination names, retain native ordered writes.

The library describes artifacts and never writes a path. Raw bytes remain the
machine-code emission log. Human/JSON CLI writes use the existing artifact
path and outprefix rules and report successful writes as Note entries.
The CLI preflight lacked checks between source artifacts and input/primary image
paths; this slice adds those checks through the existing file-identity helper.
The tests cover included inputs, root input and hard-link aliases, preserving
all six previous image/artifact/sidecar files after refusal. Existing artifact
versus sidecar checks are retained. Predictable failures are preflighted; the
existing multi-file write path is not claimed atomic against later I/O failures.

Differential tests compare complete NEX images and raw bytes on both CPU entry
points, warnings for accepted cases, and byte/artifact/symbol-preserving,
idempotent formatter output. CLI tests cover both CPUs and human/JSON, outprefix,
include error spans, source/image collisions, and all previous outputs. A separate
library test proves a requested absolute path is described without being written.
The mandatory package case executes all 70 sources and verifies file and JSON
artifact bytes; V1.3 remains an explicit refusal. Removing the NEX case makes
the qualification contract fail (38 required cases).

The declaration invariant caught the invalid subcommand diagnostic being
misclassified as an unknown directive; that diagnostic now names the expected
commands. The test remains unchanged and passes. All implemented subcommands
are declared; the generated vocabulary docs were regenerated.

Local workspace verification completed with 1,506 passed and 38 explicitly
ignored. All-target/Lua Clippy with warnings denied, formatting, generated docs,
Ruff and 53 Node package/workflow checks passed. A separate local 70-case CLI
replay matched all mandatory package expectations for files and JSON artifact
bytes, including the omitted empty-artifact field. Logs are retained.

## Clean whole-project control

Producer `3d916010e0b6bcb9a30de4287815577de360c382` was built from a clean
checkout by the explicit ecosystem runner against unchanged SpecNext pin
`e3211ea8ad2e83aca4d07d70bd8e09b7b51a9a1d`. SAVENEX is no longer refused
by the parser. The engine reaches a separate error at `src/sprite.asm:34`:
`undefined symbol MAX_PLAYERS` in a derived EQU, before MAX_PLAYERS is defined
in later included player.asm. Whole-project NEX equivalence is not claimed.
Native still emits 33,280 bytes with SHA-256
`5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f`.
The full native image and ten raw process streams are retained; every stream
was compared against its process record and the image hash/length recomputed.
No upstream build script ran. This focused result does not replace the
complete-cohort compatibility ledger.

A fresh debug build from that same clean commit independently reproduces the
boundary: `TOTAL EQU MAX_PLAYERS*3` before the later `MAX_PLAYERS EQU 2` emits
06 natively and refuses here on line 1. Defining MAX_PLAYERS first emits 06
in both tools; direct `db MAX_PLAYERS` before its later definition emits 02
in both. The six complete executions, tool identities and build log are retained.
