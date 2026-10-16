# Audit the existing dialect-conversion workflow (#641)

Use a clean source build at the recorded origin/main revision, independently
installed Pasmo/SjASMPlus controls, bounded subprocesses and fresh directories.
This slice examines the existing pasmo/pasmonext-to-sjasmplus route, not new
pairs or converter architecture. The source build's version string is not proof
of a published-release identity; retain source, executable and reference hashes.

1. Add `docs/plans/2026-10-16-conversion-workflow-audit/probe.py`. Exercise valid
   stdout/file conversions, repetition and macro structure, comments, includes,
   invalid syntax/pairs/options and previous-output preservation. Record aliases
   as observations; do not infer that explicitly requested in-place conversion
   violates a new-file guarantee the converter does not publish.
2. Run each case twice in fresh task-owned directories with 30-second process
   limits. Capture arguments, exit status, both streams and every file's bytes
   before/after. Positive controls must compare source/converted binaries with
   fresh reference-tool output. Distinguish supported successes, safe rejections,
   documented limitations and demonstrated contract violations.
3. Confirm the probe rejects a failing executable and empty retained evidence.
   Retain observations, identity, integrity hashes and a concise report. Search
   existing issues and verify relevant implementation/decisions before filing
   bounded defects. Include exact reproducers, effects and acceptance limits.
4. Run Ruff and generated docs, push a report-only PR, and wait for exact-head
   checks/workflows before squash merge. Link findings/evidence from #641.
   Inspect and remove the task worktree, build/scratch files and processes after
   verifying remote retention. Production fixes belong to subsequent pieces.

## Outcome and verification

- Forty observations (20 cases, twice) are retained. There are 16 successful
  conversions, 12 preserved-file rejection controls, six explicit alias/in-place
  observations, four include limitations and two native-valid nested-macro
  defects. Native bytes verify successful migration rather than trusting the
  converter's own note.
- Filed #686 for inaccurate include claims/actionability and #687 for ordinary
  Pasmo macro/repetition compatibility. Recommend #687 first. The original
  converter v1 plan defers multi-file fidelity; no new implementation is added.
- Probe reference controls stopped on an invalid initial `m` macro fixture;
  the retained run uses independently valid `setv`. A failing executable stops
  at the first positive control without producing a report. A rehashed empty
  result set fails the retained verifier with `missing/empty observations`.
- Ruff lint/format, retained-evidence verification, generated docs and diff
  checks pass. The evidence manifest hashes the scripts, observations, controls
  and report. A scoped `.gitattributes` LF rule preserves those exact bytes on
  Windows checkouts, following the existing retained-evidence convention.
- Binary source, executable, compiler, Cargo.lock, host and both independent
  reference identities are retained in observations.json. The source is clean
  c96f2b8, not a newly published 0.0.66 release. Native elapsed-time strings vary;
  repeated CLI outcomes, files and reference bytes agree.
