# Asm198x producer lock control

The native macOS ARM64 candidate passes all 16 existing qualification cases with
a verified producer source-lock receipt. Missing receipt, wrong archive identity
and wrong source commit controls each stop before any runtime case executes.
The existing incorrect-output and corrupt-archive controls also reject.

[Local verification](local-verification.json) and the
[actual producer receipt](producer-receipt.json) identify tested code commit
`91961d9a618d518aaa8946c6d3c1ca55d6afdae3`, lock and artifact hashes. The lock
was independently reread from that commit's Git blob and the archive digest was
recomputed after qualification. The build used the existing cargo-dist 0.32.0
and unchanged Asm198x 0.0.66 sources/fixtures; no release was published.

The guard compares Cargo.lock with the checkout commit before and after the
actual packaging command. Only a successful command, unchanged lock and valid
native archive can write its receipt. Candidate qualification independently
checks the receipt's source, lock, target, version, run/attempt and archive plus
extracted executable hashes. The pre-host gate requires the receipt. Published
reconciliation validates available receipts against the release tag's resolved
commit and source lock; old releases without receipts remain `artifact-only`.
HTTP errors other than an absent historical receipt cannot silently downgrade.

All 35 Node candidate/source-lock/setup tests pass. They include changed, missing
and empty locks before/after a bounded packager, failed commands, missing/changed
receipt identities, wrong run/attempt, template drift, and public lookup success,
404, 403 and 500 controls. Full workflow regeneration and diff checks pass.

The real build initially exposed that a native producer manifest lists only its
selected archive. Producer validation now requires that target; candidate plan
validation still requires all five. A regression covers both scopes. Windows CI
also exposed a CRLF-sensitive assertion in the new workflow test; its input is
now normalized. These findings did not require changes to tools, versions or
fixtures. Fresh five-host CI results are retained with
[PR #684](https://github.com/asm198x/asm198x/pull/684) before review readiness.

The receipt is unsigned workflow evidence of build-time lock identity. It does
not establish a cryptographic signer or each executable's compiled dependency/
feature graph. This is the Asm-only first slice of
[umbrella #137](https://github.com/stevehill1981/198x/issues/137); rollout to other
producers remains separate. Release publication itself is not exercised by a PR.

Task-created compile caches were removed after confirming they had no open
files. Existing target caches and unrelated work were preserved. Compact durable
records are retained here; task-owned disposable packaging output is removed
before completion.
