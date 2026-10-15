# Protect consumed inputs from primary-image writes (#670)

The candidate gate exposed a remaining source-loss path: #651/#654 protect
sidecars, but `preflight_outputs` only checks primary-image writability and
never compares that image with consumed inputs. A fresh current-tree archive
returned success and changed a hard-linked source into byte EA.

1. Add a regression in `crates/asm198x/tests/cli_output_safety.rs`, using the
   existing four-driver fixtures. Require valid controls, then reject primary
   output aliases to root/include/INCBIN inputs in human/JSON modes before any
   source or seeded sidecar changes. Cover exact/parent/hard-link/symlink/case
   names where the host supports them. Observe failure on unchanged code.
2. Add the existing file-identity guard for the primary image in
   `crates/asm198x/src/main.rs::preflight_outputs`. Keep source-level SAVE
   artifact semantics unchanged; no new dependency or diagnostic shape.
3. Correct the historical release audit report's source-fixed attribution for
   this primary-output case and refresh its retention manifest. Keep original
   observations untouched. The case belongs to #670, not the sidecar fix.
4. Verify the regression, workspace tests, strict Clippy, formatting and docs;
   land a separate green PR closing #670. Then rebuild candidate archives and
   require all #668 cases and corruption controls to pass before gating release
   publication. Preserve and clean this task workspace after remote retention.

Evidence: the unchanged CLI accepted `-o program.s`, overwrote the root source,
and wrote the seeded symbol sidecar; the new regression failed with exit 0 and
that success summary. The root source is read before `RecordingLoader`, so the
primary guard must check `input` explicitly as well as the loader's consumed
paths. After that guard, the four-driver alias matrix passes; JSON failures
remain structured and the original inputs/sidecars remain byte-identical.

Local verification: 1403 workspace tests passed, 0 failed and
33 ignored. Strict Clippy, formatting, generated docs and retained-audit
evidence verification passed. The source-level SAVE behavior is unchanged.
