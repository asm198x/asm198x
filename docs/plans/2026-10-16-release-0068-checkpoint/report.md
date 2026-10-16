# 0.0.68 checkpoint interrupted

PR #702 merged after all exact-head checks and triggered workflows passed or
were appropriately skipped; the exact snapshot is retained in
[`premerge.json`](premerge.json). The source tree and immutable `asm198x-v0.0.68`
tag are `eecde0674b8be684a3b14fe75301a76f0d24e557`.

A subsequent native comparison found the absent custom segment regression
tracked in #704. Native archive publication run `38903970383` was cancelled;
its host job has no executed steps. The published crate/tag were preserved.
This version is not claimed as a qualified public native release.

The correction, reproduction, full cancelled timeline and replacement patch
checkpoint plan are retained in
[`../2026-10-16-ca65-absent-segments/report.md`](../2026-10-16-ca65-absent-segments/report.md).
The completed public 0.0.67 evidence remains unchanged.
