# ACME #602/#603: already fixed, freshly verified

Both open reports are resolved in published Asm198x 0.0.67. The implementation
landed in [#605](https://github.com/asm198x/asm198x/pull/605), commit
`478ab19d9a26a3e20cc5f2d53547ee4fc7eb04ba`, on 10 October 2026. No new parser
change is needed. Recommending these reports as the next fixes without checking
that history was a tracker-reconciliation error.

Fresh observations compare the downloaded official macOS ARM64 0.0.67 archive
with independently installed ACME 0.97. Both exact issue inputs pass:

| Report | Native and released result |
| --- | --- |
| [#602](https://github.com/asm198x/asm198x/issues/602), `!cpu 6502` before the first origin | CBM PRG bytes `01 10 A9 01` |
| [#603](https://github.com/asm198x/asm198x/issues/603), `!if * > $0fff` after one NOP | Raw bytes `EA EA` |

Seven positive sources each pass raw/PRG comparison in human/JSON modes: **28
byte comparisons** against fresh native output. They include a 65C02-only `stz`
to prove CPU selection takes effect, a false branch, a counter that advances
between conditions, a logical PC inside `!pseudopc`, and multiplication with a
high-byte prefix. **Seven formatter round trips** retain native bytes. **Four
rejection controls** confirm that instructions and PC-dependent conditions still
need an origin in both diagnostic modes and preserve an existing output.
Native ACME independently rejects both invalid sources.

The former #602 cause was an empty-byte CPU-selector marker reaching the
engine's origin check. #605 retires that marker after updating the evaluator's
CPU state, so selection emits nothing while later code still needs an origin.
The former #603 cause was conditional folding without the live logical PC;
#605 passes that counter into expression evaluation and distinguishes infix
comparison from prefix byte extraction. Existing regressions
`cpu_selection_before_origin_does_not_emit_data` and
`conditions_use_the_live_logical_program_counter` remain in
`crates/asm198x/src/dialects/acme/mod.rs`. They were inspected here; no fresh local
Rust suite run is claimed for this evidence-only change.

`identity.json` records archive/executable hashes and the verified unsigned
producer-lock receipt. The release source is
`46cedb445a01f8fefd9d2a96d63961414036fcf4`; the probe checkout is
`735fc96b3dada2e20dafb9860114657b8c7f1f17`. Archive identity agrees with the
[retained five-host release evidence](../2026-10-16-release-0067-checkpoint/report.md).
`observations.json` records host/Python/native executable identity, all sources,
argv, exits, streams and output bytes. This is fresh local macOS ARM64 evidence,
not new native-reference execution on five hosts, installer qualification or a
new release. Producer receipts remain unsigned; compiled dependency/features
provenance and signing identity remain unknown.

Run `probe.py /path/to/published/asm198x /path/to/acme /new/observations.json`
with Python 3.13+ to repeat the probes. `node verify.cjs` verifies retained
identities, exact case coverage, comparisons, controls and file hashes. A
substituted `/usr/bin/false` assembler was rejected before a report could be
written. A rehashed empty observation set is also rejected by the verifier.
Ruff checks and formatting pass.

Close #602/#603 with #605 and this fresh evidence. Keep #640 open. Existing
correctness issues #609, #629, #631, #634, #635 and #696 still require verification
before treating their tracker state as evidence of a current defect. #618 owns
actual upstream build execution. Earlier dated reports remain unchanged.
