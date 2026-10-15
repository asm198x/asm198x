# Qualify candidate release archives before publication (#668)

The generated release host job currently does not depend on the custom Linux
package check. Existing source tests and an observational old-release audit
cannot qualify candidate archives. Keep cargo-dist and the existing reusable
package workflow, but make publication depend on a required native-host check.

1. Inspect cargo-dist 0.32.0's generated dependency graph. Select its supported
   pre-host extension point, regenerate `.github/workflows/asm198x-release.yml`,
   and prove a failed/skipped qualification cannot reach publication. Keep the
   generated file reproducible from `Cargo.toml` rather than hand-patching it.
2. Extend `.github/workflows/test-linux-packages.yml` to execute the five native
   host/architecture targets using the candidate plan/artifacts. Preserve Linux
   Homebrew verification. PR runs exercise freshly built packages; release runs
   consume those same release-plan archives before public hosting.
3. Add bounded dependency-free scripts under `scripts/` to verify archive hashes,
   exact package/version/target identity and the extracted executable, then check
   independent project bytes, structured success/failure, CPU validation and
   previous-output/source preservation (including hard links). No missing/empty
   case set may qualify. Retain machine-readable per-host records.
4. Prove controls reject corrupted archives and wrong output. Exercise the gate
   against official 0.0.65 to reproduce a red result, then current candidate
   archives for a green result on all five hosts. Run script/unit tests and
   generated-CI/docs checks; inspect hosted artifacts and all CI conclusions.
5. Land through a green squash PR. Reconcile post-fix release verification using
   the existing release PR/workflow when a release containing the fixes is ready;
   distinguish candidate qualification from published-artifact qualification.
   Update #668 and umbrella #72/#108 with exact evidence, clean owned workspaces.
6. Continue with the next bounded workflow audit after these fixes, keeping new
   findings separate from implementation and avoiding duplicate issues.

## Local verification

The checked generation wrapper was approved explicitly. The decision and
`scripts/generate-release-workflow.cjs` document the two exact transformations
and the complete regeneration comparison. There are no new dependencies.

The initial current-source archive failed the primary-output hard-link case.
The root cause was a guard limited to sidecars; #670 / #671 now protects primary
images too. The retained old-release report identifies its actual binary source
commit separately from the checkout that executed the gate.

The post-fix macOS ARM64 cargo-dist archive passes all 16 required cases. Its
wrong-byte control fails reference bytes, determinism and structured success;
its archive-corruption control fails SHA-256 verification before extraction.
The public 0.0.65 negative control fails seven cases. Reports and the full plan
are retained in the adjacent directory. These are local observations, not yet
proof of all-five-host qualification or public-release readiness.

All 19 Node tests pass, including real shell execution of reusable-plan JSON
writing, exact native matrix coverage, and publication guards under success,
failure, cancellation, skipping and absent results. Full workflow regeneration
comparison passes. Native CI reports and post-publication reconciliation remain
required before closing #668. Each workflow job is bounded to 30 minutes, each
CLI subprocess to 30 seconds and each fixture download to 30 seconds; reports
are retained by Actions for 90 days.
