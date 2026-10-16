# Prepare Asm198x 0.0.67 for published producer reconciliation

1. Use existing release PR #680 and its release-plz version/lock/documentation
   changes. Verify the release contains producer fix #684 and inspect the other
   included fixes before describing them.
2. Author only the new 0.0.67 section of `crates/asm198x/CHANGELOG.md`, including
   the unsigned producer-receipt boundary. Preserve older release notes and the
   generated version choice. No dependency or runtime changes.
3. Run `cargo xtask changelog`, the existing source-lock/package regression
   tests and diff checks. Push the small release-note commit to the existing
   release branch and refresh #680's description with the actual scope.
4. Inspect native candidate and normal CI checks on the final head. Seek release
   approval only after the concrete PR is reviewable; merging #143 did not
   authorize publication. After an approved release, reconcile downloaded public
   archives and receipts before claiming published producer proof under umbrella
   #137. Broader dependency/features provenance remains unknown.

Workspace: existing `Asm198x/asm198x` checkout, existing release branch.
No temporary worktree, clone or build cache is planned; use the existing target
directory. Do not modify sibling release machinery to force a version bump.
