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

## Published setup verification

After public 0.0.66 exists, refresh `.github/workflows/setup-action.yml` to install
and smoke-test 0.0.66 from PATH. Its historical observational probe deliberately
asserts version 0.0.65, so retain that probe and install its exact baseline in a
separate step before collecting it. Preserve the old probe and retained evidence
unchanged. Use the existing setup action and smoke script for both identities;
update the existing setup-node action to v7, already used by the package workflow.
The resulting PR must execute the 0.0.66 setup smoke on all four action-supported
targets. Linux ARM64 remains archive-qualified, outside the action's four-target
contract. This extends verification, without dropping the historical controls.

Retained qualification files use scoped LF Git attributes because their manifest
hashes exact bytes. Setup CI must execute the fixed-version evidence verifier
on every supported host; its historical case contract is independent of future
changes to the live candidate gate.
