# Preserve published-source acquisition failures (#708)

The audit reproduced HTTP 403 before native archive qualification in run
38913220501 at the 0.0.67 tag. Current main e726270 still uses the same anonymous
commit lookup and writes acquisition metadata only after acquisition succeeds.
The response body/headers from the hosted failure are absent; do not assert a
rate-limit cause. Release-candidate qualification remains mandatory.

The user approved using the existing read-only workflow token only for
api.github.com, retaining safe HTTP diagnostics and a structured failed-
acquisition record, without weakening archive validation or adding broad retries.

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

## Local verification and hosted comparison

Five regressions fail before the implementation; 44 scoped package/setup/source-
lock tests pass after it. Pinned cargo-dist 0.32.0 generation and prose checks
pass. The authenticated API refuses redirects; public asset/raw requests receive
no token. HTTP failure diagnostics retain only bounded approved headers.

Run 38916453960 uses the patched acquisition with old 0.0.67: all five hosts
acquire successfully, then reject four later-added Pasmo/ca65 regression cases.
Those failures remain version-specific evidence, not a green qualification.
Run 38916705512 separately reconciles 0.0.69 against the current gate. Retain
both outcomes; do not change old artefacts or relax the newer gate for 0.0.67.

## Outcome

Run 38916705512 succeeds on all five native hosts with the exact 0.0.69
publication: each acquisition is retained as acquired, all 28 qualification
cases pass, and wrong-byte/corrupt-archive controls reject. The 0.0.67 run stays
failed with its four later-added regression failures. No archive, receipt or
source check was relaxed. This validates Asm package acquisition/qualification;
it does not qualify the remaining Windows Build/Emu journey.

Run `node docs/plans/2026-10-16-published-source-acquisition/verify-evidence.cjs`
to check retained hashes, both five-host outcomes and a rejecting missing-case
control. Firmware, credentials and downloaded executables are absent. Native
tested code commit: 3f36f70eba002868202ea222dd2f24a49ccf6734. Subsequent changes
retain documentation/evidence only.

All local reproduction directories and generation scratch were removed by their
bounded runners. Remove this linked worktree after remote retention verification.
No merge, release, installed-tool change or new CI workflow was performed.
