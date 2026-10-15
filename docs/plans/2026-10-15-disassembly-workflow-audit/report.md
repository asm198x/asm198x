# Ordinary disassembly workflow audit

Public CLI 0.0.66 repeats 13 ignored-option violations and one wider-origin
limitation. Nine operation/rejection controls pass. Two complete runs produce
46 observations: 18 passing controls, 26 option violations and two high-origin
rejections. The audit also confirms two stale CPU-coverage claims in help/docs.

Findings:

- [#677](https://github.com/asm198x/asm198x/issues/677): ordinary `disasm` exits 0
  while ignoring `-o`/`--output` and 11 assembly-only options. Its stdout/stderr
  equal the valid baseline listing and its files remain unchanged. In particular,
  a seeded output stays stale after a successful command. The option table scopes
  these requests to other modes; the bounded fix is validation, preserving
  recovery's separately supported output path.
- [#678](https://github.com/asm198x/asm198x/issues/678): `--org 0x10000` is refused
  for 68000 before reaching the existing u32-origin decoder. The locked API
  renders the position-dependent `60 02` branch as `bra.s $8004` at `$8000` and
  `bra.s $10004` at `$10000`; the released CLI agrees at the lower origin and
  rejects the higher one. This is separate from 16-bit recovery and banked
  assembly placement. The first RTS-only API fixture could not observe an
  origin; the retained control uses a branch so truncation cannot pass.
- [#679](https://github.com/asm198x/asm198x/issues/679): the CLI reference still
  says disassembly is 6502 “otherwise Z80”, and help lists seven formatter
  families. Actual 68000 disassembly and ACME formatting/reassembly pass. This
  concerns the descriptions, rather than adding decoder/formatter capability.

The archive is the official macOS ARM64 0.0.66 release, source
`35a07893a57cbb42c3a341cc293fc4b713af99f4`. Archive SHA-256:
`2a38257f7a87745cf29931d6d9b97ed62b1dd20397393678087b283b9cb5d1f6`.
The existing candidate-package helper verified its checksum, executable target,
version and 16 archive qualification cases before this audit. `identity.json`
separates the binary source from the runner checkout containing #675's metadata
fix. Raw observations record both streams, exits, arguments and every file's
bytes before/after each process. No input or seeded source changed.

Positive controls cover 6502/Z80/68000 listings, valid assembly, invalid CPU and
sidecar rejection, recovery to a new file, refusal of an existing recovery output,
and ACME formatting. Recovery and formatting outputs reassemble to `A9 05 60`.
All processes are bounded to 30 seconds. The API control was compiled against
already-built `isa198x-disasm` using the release-identical Cargo.lock; it adds no
project dependency. Its Rust source, compiler identity and output are retained.
A deliberately failing executable is rejected by the first positive control,
without producing an audit report. A rehashed empty-observation record is also
rejected by the retained-evidence verifier.

This is a local macOS ARM64 ordinary-disassembly slice. It does not qualify all
hosts, decode spaces, container inputs or full round-trip recovery. #641 and #504
remain their wider trackers. The recommended first fix is #677 because successful
commands can leave callers using stale output. #675 is fixed separately in #676;
its package wording ships through the next normal release.

Rerun with Python 3.13+ and the verified executable:

```sh
python3 docs/plans/2026-10-15-disassembly-workflow-audit/probe.py /absolute/path/to/asm198x /tmp/disassembly-observations.json
python3 docs/plans/2026-10-15-disassembly-workflow-audit/verify.py
```

The API control source is embedded in `origin-api-control.json`; compile it with
Rust 2024 against the existing locked `libisa198x_disasm` rlib and its dependency
search directory. The report/manifest preserve the exact observations rather
than presenting audit-process completion as a product qualification pass.
