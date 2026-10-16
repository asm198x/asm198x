# Complete native checkpoint after the three fixes

[Native run 38945646806](https://github.com/asm198x/asm198x/actions/runs/38945646806) built main revision `c85d1ddf0608013a52b3a3d072e68892d658e255`, after #716, #717 and #714 were fixed and the upload correction #728 merged through #729. The downloaded ZIP contains all 84 files: the complete ten-target report, 80 process logs, both generated tables and the 646-case live Linux shift replay. Its API digest and byte length match `capture-artifact.zip`; `artifact-inventory.json` records every member's actual length and hash.

The complete result remains six final-artifact matches, two Asm rejections and two native-control failures. The first Asm failures moved to #722 (4510 `ORA (indirect),z`) and #725 (SjASMPlus numeric separators at `src/audio.asm:11`). The native controls still report `ca65: Invalid CPU: '6502dtv'` and `ca65: Invalid CPU: '65ce02'` (#718); no Asm build ran for them. The capture command therefore exited 1 with:

```text
xtask ecosystem: audit contains non-equivalent or unavailable targets; inspect report.json
```

Complete-result validation, table rendering, required staging and upload all succeeded after that expected failed-control result. Both downloaded tables matched local regeneration byte for byte. `captured-native-report.json` and captured table copies preserve the raw upload. `native-report.json` changes only the two reviewed issue links to #722/#725; process results, source identities, artifact bytes and original capture date remain unchanged. The triaged report is the ledger's `ecosystem/results/current.json`.

The independent frozen verifier checks all ten outcomes and native processes/artifact comparisons, raw-versus-triaged equality, producer/source pins, complete-upload identities and required process-log inventory, and every accepted/refused Linux shift case against the retained native fixture. All 646 Linux cases match the independently captured macOS fixture. Its nine mutation controls reject missing reports, promoted failed controls, wrong final bytes, missing shift cases/native steps, substituted producers, failed staging, missing log inventory and false native matches.

```sh
node docs/plans/2026-10-16-next-project-boundaries/github-checkpoint/verify.cjs
node docs/plans/2026-10-16-next-project-boundaries/github-checkpoint/verify-controls.cjs
```

The earlier incomplete run is retained separately in `../incomplete-capture/` and was not promoted. Package qualification is separate from these pinned-project outcomes; no general compatibility percentage follows from either cohort. These retained hashes and unsigned source-lock receipts identify observed artifacts, rather than establishing a signer or complete compiled dependency graph.
