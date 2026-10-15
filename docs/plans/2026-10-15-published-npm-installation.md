# Correct the published browser-package installation path (#667)

The README leads with the all-architecture build name, but only the Z80 package
is published. Reproduce public-registry availability, then correct the examples
without publishing a new bundle or changing the browser architecture decision.

1. Update `crates/asm198x-web/README.md` to install/import the pinned published
   `@asm198x/z80` release, describe its actual dialects, and distinguish local
   all-architecture builds. Make the shown listing/project examples work with
   that package and document initialization for a Node verification run.
2. In an empty task directory, install the documented exact version from the
   public registry, run the documented import/assembly plus structured failure,
   and retain package integrity, version, dialects and command results here.
   Run generated-doc checks and review the diff; no new project dependency.
3. Commit/push a PR closing #667, wait for completed CI, squash-merge the tested
   head and clean task resources. Then implement #668 in a separate change using
   the existing cargo-dist release plan and reusable package jobs.
