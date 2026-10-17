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
