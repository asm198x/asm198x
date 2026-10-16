# Bind release packages to the producer source lock

Implement umbrella #137 in Asm198x first. User approved the proposed first-producer
scope on 2026-10-16. No release, version, dependency or fixture update is included.

1. Reproduce the missing guarantee in the current wrapper: candidate archives are
   qualified but no packaging receipt proves the producer lock matched HEAD.
   Add stdlib Node controls using a temporary Git repository and a bounded fake
   packager; changed/missing/empty locks and failed packaging must reject.
2. Add `scripts/build-locked-package.cjs`: compare the on-disk lock with the
   committed checkout before/after the actual packaging command, then retain a
   receipt with commit, lock hash, target, GitHub job/run/attempt and archive plus
   extracted executable hashes. Refuse stale receipt output and zero artifacts.
   This receipt is unsigned workflow evidence, not a per-binary feature graph.
3. Reuse archive identity checking from `test-candidate-package.cjs`. Candidate
   qualification requires a matching receipt before its runtime checks. Publish
   the receipt alongside archives; public reconciliation checks it when present.
   Older releases keep artifact-only scope unless explicitly requiring receipts.
4. Extend the checked generator's exact local-build transformation to invoke the
   guard and upload its receipt. Keep strict existing publication dependencies.
   Apply the same guard to native PR packages; require downloaded producer
   receipts for the reusable pre-host gate. Update the narrow generator decision
   and tests together; full regeneration must remain reproducible.
5. Run Node controls, setup tests and full regeneration. Exercise the current
   native candidate where practical; use five-host PR CI for actual packaging,
   corruption checks and receipt retention. Inspect passing reports rather than
   inferring success from workflow status. Commit/push and open a focused PR.

Scratch/build output: existing ignored `target/` is reused; task-owned bounded
records under `target/producer-source-locks/`. Track and clean test/generation
scratch in system temporary folders via finally blocks; no new worktrees,
background services or dependency installs are planned. Check disk before builds.
