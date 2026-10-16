# Release candidates must qualify before hosting

**Status:** Active. Accepted 2026-10-15 for #668; the checked generation wrapper
was explicitly approved in the task conversation.

Keep cargo-dist 0.32.0 and its existing reusable package job. Qualification runs
at the global-artifact stage after archives/installers exist, on all five native
host targets. Required checks must succeed before GitHub hosting/publication.
Failed, missing, skipped or cancelled qualification cannot count as success.

The pinned generator allows skipped custom global jobs to reach hosting and
runs custom global jobs alongside its own global build. The checked wrapper
`scripts/generate-release-workflow.cjs` preserves the complete generated workflow
and requires qualification to wait for global artifacts and hosting to require
qualification success. The approved producer-lock extension wraps local packaging
with `scripts/build-locked-package.cjs`, installs the same Node version used by
qualification, and uploads the resulting per-target receipt. All transformations
match exact generator anchors and reject upstream template drift. Regenerate and check
through that wrapper, not direct `dist generate`.

`allow-dirty = ["ci"]` delegates the built-in CI-file comparison to the wrapper.
It does not waive regeneration verification: every candidate package job checks
the full freshly generated/transformed workflow in a disposable copy, and tests
exercise publication under unsuccessful and absent qualification results. The
wrapper fails if the upstream template no longer matches its exact edits. Only
the disposable generation copy removes `allow-dirty` so the generator will run.

This is a narrow override of the former direct-`dist generate` maintenance
instruction in `Cargo.toml`, needed because the upstream guard accepts skipped
required checks. It does not replace the release system or loosen branch/merge
rules. Revisit when cargo-dist supports a strict required-job guard directly;
remove the transformation only after the same failure-state tests pass.

Candidate reports identify checkout commit, native host, plan version and archive/
executable hashes. A successful candidate is distinct from a downloaded public
release. Public qualification must be rerun against published artifacts, and old
failures remain tied to their audited versions. The historical observational
audit is retained unchanged; it is not the promotion gate.

## Producer source-lock receipts

The first Asm198x implementation of umbrella qualification #137 was approved on
2026-10-16. Both native PR packages and release producer jobs compare Cargo.lock
with the checked-out commit before and after the actual cargo-dist build. A
failed command or a changed, empty or missing lock cannot produce a receipt.
The lock is checked out as LF bytes on every host so the comparison with the Git
blob remains meaningful on Windows.

Each unsigned `asm198x-<target>-source-lock.json` names the source commit, committed/
before/after lock digests, target, version, producer run/attempt/job and archive/
executable digests. It travels with the producer artifacts and is included in the
release asset upload. Required pre-host candidate qualification rechecks those
identities against the checkout and actual archive, including the run/attempt;
missing or stale receipts cannot fall back to artifact-only qualification.

Published reconciliation validates an available receipt against the public tag's
resolved commit, source lock and downloaded archive. Historical releases without
receipts retain explicit `artifact-only` scope. A bad available receipt is a
failure, not a reason to discard it and retry without provenance. Passing receipts
are recorded as `verified-producer-lock`; they do not establish a per-binary
compiled dependency/features graph or a cryptographic signer identity.

Published-source acquisition uses the workflow's existing read-only token only
for the fixed GitHub API lookup, without authenticated redirects or credentials
on public archive/raw hosts. Acquisition records are written before fetching,
including unsuccessful outcomes and bounded status/rate-limit/request-ID
diagnostics, so failure can be retained before qualification starts. A successful
acquisition is not qualification; source, receipt and archive checks remain
mandatory. Response bodies and credentials are not acquisition evidence.
