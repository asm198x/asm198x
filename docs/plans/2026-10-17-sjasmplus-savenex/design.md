# Bounded SAVENEX output (#739)

## Recommendation

Implement the NEX V1.2 OPEN / CORE / AUTO / CLOSE path used by unchanged
SpecNext Invaders, with native defaults, expressions, warnings and command-order
checks. Keep other SAVENEX subcommands and explicit V1.3 output as diagnosed
unsupported features. Broader SAVENEX coverage remains in #275. No dependency
is needed.

The alternative is the whole SAVENEX surface now: BANK/CFG, loading screens,
palettes, copper, V1.3 checksums and appended input files. That adds unrelated
input and image-format work before the measured project can advance. The
bounded path gives a complete native output for the concrete target first.

## Existing seams

- Parse an internal typed command in `crates/asm198x/src/dialects/sjasmplus.rs`.
  Carry its expressions through the existing operation/AST qualification and
  lowering paths in `engine.rs` / `ast.rs`; do not evaluate forward addresses
  while parsing or add a second assembler driver.
- Execute commands in source order in the engine's emit pass, against the
  current `DeviceMemory`. Require ZXSPECTRUMNEXT at each reached command.
  Untaken conditional branches remain inert.
- Keep one optional in-memory NEX session. OPEN sets its name, entry PC,
  stack pointer and entry bank; CORE updates the required-core bytes; AUTO
  appends snapshots of selected nonzero 16KiB banks in native order; CLOSE
  finalizes the header and describes an artifact. Finalize at end-of-source
  too, as native does. Permit sequential outputs; refuse a second live OPEN.
- The byte-layout helper belongs in `crates/asm198x/src/nex.rs`, using only
  standard-library types and a typed error, free of engine/contract types.
  It cites the reference library and can graduate to Format198x by moving.
  No family NEX consumer was identified to require graduation in this slice.
- Raw `AssemblyResult.bytes` remains assembled code. NEX is a separate
  `Artifact` with `ArtifactFormat::Nex`, serialized as `"nex"`. Reuse the CLI's
  existing artifact path resolution, --outprefix, --no-save, JSON/human notes
  and collision/preservation controls. Do not route it through primary-output
  `RequestedOutput` or reinterpret -o as the NEX destination.
- Preserve the CLI's existing all-output preflight and refusal behavior rather
  than copying native's partial-file writes on failure. The library writes no
  files. No new filesystem access is required for this bounded command set.

## API approval included in this proposal

`ArtifactFormat` is public and currently lacks `#[non_exhaustive]`. Adding Nex
requires external exhaustive Rust matches to handle the new variant. Consumers
that deserialize the format enum must update before consuming requested NEX
artifacts. Existing artifact values and all existing result fields stay the
same. This is an explicit API extension requiring agreement under AGENTS.md;
it must not be passed off as having no consumer impact. The existing contract's
additive format mechanism retains version 1; no existing payload is rewritten.

## Measured behavior

`native/report.json` and content-addressed compressed images retain 53 real
SjASMPlus 1.21.0 executions. Every output was decompressed and its digest and
length independently verified. The probe records source, argv, complete streams,
status, output hash/length and all output bytes, including native partial files
on refusal.

- Default OPEN: PC 0 (or a previously established END start), SP 0xFFFE,
  entry bank 0, NEX V1.2, required core 2.0.28. Header-only output is 512 bytes.
- Empty AUTO stores no banks. AUTO scans native NEX bank order, not numeric
  bank order, and advances its cursor even over zero banks. A second full AUTO
  refuses. Explicit from/to bounds use that same order.
- AUTO snapshots memory when reached: later assembly writes do not alter an
  already stored bank. Native bank order is 5,2,0,1,3,4,6,...,111.
- OPEN addresses truncate to 16 bits with the native warning behavior.
  CORE out-of-range values warn and truncate to bytes; they are not refused.
  Invalid entry banks, argument counts and command order refuse.
- Storing banks 48..111 without CFG's 2MB flag refuses at finalization. CFG is
  outside this first slice; we must not silently produce an unloadable image.
- Forward OPEN expressions, implicit EOF close and multiple sequential output
  files are accepted. Explicit V1.3 OPEN is a measured native acceptance but a
  deliberate unsupported boundary of this V1.2 slice, not an assertion of
  complete SAVENEX parity.

## Evidence and acceptance

Before layout implementation, add primary writer provenance/facts under the
umbrella reference library, citing pinned SjASMPlus v1.21.0 `sjasm/io_nex.cpp`.
The existing local SpecNext wiki mirror documents the V1.2 layout as a community
reference; do not relabel it manufacturer documentation. Follow the binding
multi-artifact, native-I/O and layout decisions.

Extend the probes for all bank-order boundaries, partial AUTO ranges, scope and
forward expressions, default END interaction, duplicate targets, path controls,
missing/extra operands, inactive branches and an error after a completed save.
Tests compare complete reconstructed native images, header fields and bank data;
formatter round trips preserve raw bytes, artifacts and symbols. CLI checks cover
human/JSON include spans, no-save/outprefix, collision preflight and retention
of all previous outputs on failure. Mandatory packaged-executable replay includes
wrong-byte/corrupt-archive controls on all five hosts.

Finish with a clean-producer replay of pinned SpecNext Invaders. Require the
same complete 33,280-byte NEX hash if the target succeeds; otherwise retain its
next concrete boundary with complete streams. Never bypass another unsupported
operation to claim whole-project parity. Run appropriate workspace/Lua tests,
Clippy, formatting, docs, Ruff and Node checks, then exact-head PR workflows and
actual host receipts before squash merge. A public release is a separate step.
