# Preserve published-source acquisition failures (#708)

The audit reproduced HTTP 403 before native archive qualification in run
38913220501 at the 0.0.67 tag. Current main e726270 still uses the same anonymous
commit lookup and writes acquisition metadata only after acquisition succeeds.
The response body/headers from the hosted failure are absent; do not assert a
rate-limit cause. Release-candidate qualification remains mandatory.

Approach agreement is pending the user's answer to the 16 October question:
use the existing read-only workflow token only for api.github.com, retain safe
HTTP diagnostics and a structured failed-acquisition record, without weakening
archive validation or adding broad retries. No implementation before agreement.

1. Reproduce current behaviour with an injected fetch returning a source-lookup
   403. Retain request credential-presence (never token values), error and
   whether a structured acquisition outcome exists in
   `docs/plans/2026-10-16-published-source-acquisition/baseline.json`.
2. After agreement, edit `scripts/fetch-published-package.cjs` to constrain token
   use to the GitHub API and retain safe acquisition status/failure data. Keep
   errors nonzero; no provenance fallback when a receipt exists.
3. Update `.github/workflows/test-linux-packages.yml` to supply the existing
   token at the acquisition step and retain its result even if that step fails.
   Check whether the release wrapper owns this file before editing.
4. Add rejection/auth-scope/acquisition controls to
   `scripts/source-lock.test.cjs`, including no credentials on archive/raw hosts,
   failed API request, bounded diagnostics, and preserved historical 404 scope.
   Prove the new regression fails before the fix, then run scoped Node tests,
   generation checks, prose and diff checks.
5. Push a focused PR, then dispatch the existing published-version workflow on
   its branch using pinned 0.0.67. Retain actual native outcomes and any failure.
   No merge or release is implied. Remove task workspace/scratch after verified
   remote retention, preserving every completed commit and evidence file.

Workspace: `/private/tmp/asm198x-published-source-acquisition`.
Lane: Asm198x package-acquisition reliability; no emulator or ISA changes.
