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
and changes only these two relationships: qualification waits for global
artifacts, and hosting requires qualification success. Regenerate and check
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
