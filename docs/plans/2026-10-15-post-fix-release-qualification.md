# Qualify the first release after the workflow fixes (#668)

Use the existing 0.0.66 release PR after #672 and #673. Keep the established
release-plz/cargo-dist pipeline and lockstep versioning. The release-cadence and
changelog-is-authored decisions bind this self-contained cross-cutting release.

1. Confirm the release branch includes #672, the CLI fixes and generated version
   documentation/standalone browser lockfile. Rewrite only the new 0.0.66 entry
   in `crates/asm198x/CHANGELOG.md` for readers after release-plz finishes.
2. Run generated docs/version/changelog checks, full release workflow generation
   comparison and existing Node gate tests. Push the authored entry/plan to the
   existing release branch. Inspect every check and workflow at the exact head;
   squash-merge #647 only when all are completed acceptably.
3. Observe actual release candidate qualification on all five native targets,
   inspect every JSON report and both corruption controls, then confirm hosting
   followed successful qualification. Retain the release tag/source identity.
4. Dispatch the same package workflow with published-version=0.0.66. Download
   public archives/checksums/manifest rather than rebuilding. Inspect all five
   public reports and controls. Require candidate/public archive and executable
   hashes, versions and targets to match exactly.
5. Retain bounded candidate/public evidence, checksums and verification commands
   in a follow-up report PR. Update #668 and umbrella #72/#108 with the observed
   result; keep broader #641/#640 open for their remaining scope. Clean task
   worktrees/caches only after exact remote retention and final evidence checks.
