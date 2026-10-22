# Decision: verdicts track the current reference tools

**Status:** Active. Binding for Asm198x. Supersedes
[`reference-versions.md`](reference-versions.md), and the identity-keying,
digest, corroboration and supersede parts of the verdict design
(`docs/plans/2026-07-03-002-feat-verdict-pipeline-plan.md`,
`ops/arbiter/README.md`).

**Date:** 2026-10-22.

## The decision

**The verdict corpus records what the reference tools in use now do: one
verdict per case, regenerated when a tool moves.**

- **One current version per tool**, pinned in `ops/arbiter/Dockerfile` and
  listed in `ops/arbiter/identities`. That file is the only list of versions in
  use; the image verifies itself against it.
- **One verdict per case.** A case is a suite, a case label, a tool, a
  dialect and the source text. Recording it again replaces its verdict.
- **A version bump regenerates the corpus.** Bump the pin, publish the image,
  run growth. Growth refuses to start unless every installed tool reports
  exactly the line `ops/arbiter/identities` lists. Every case is re-recorded,
  and each flush notes in a manifest what it recorded; an unfiltered run then
  prunes only what that manifest shows was superseded (old-version verdicts in
  a group the run re-recorded, and curriculum verdicts for a program's previous
  digest). A suite the run did not reach loses nothing. The pull request's diff
  is the review: each moved answer is either a bug of ours the old version hid,
  or the reference changing behaviour.
- **Files are canonical.** Sorted by case and written whole, so the same facts
  always produce the same bytes. `cargo xtask corpus --check` enforces it in CI.
  Every rewrite takes a lock, because live tests run in parallel and several
  flush the same file.
- **History is git's.** Nothing in the file records what it used to say.

Each verdict still carries its tool's version self-report, so a reader can see
what answered and growth can tell which verdicts are stale. It no longer keys
anything.

## What this removes

| Removed | Why it existed | Why it goes |
|---|---|---|
| Version string in the key | Two versions could disagree, and both answers were kept | Only the current answer matters, so there is only one |
| Binary digest on every verdict | A rebuilt binary agreeing with the record was counted as corroboration | It answered "which exact build said this?", which nobody asks |
| Supersede records and alarms | An append-only file could not edit a wrong or stale fact | The file is rewritten; git keeps the old version |
| `cargo xtask versions --check` | Prose and comments naming versions drifted from what was run | Tests and docs no longer pin versions; the identities file and the generated ledger say what is in use |
| Version assertions in live tests | Each test checked it was running against the version it was written for | The arbiter image decides the version; a test that pins one breaks every bump |

`xtask supersede` becomes two narrower commands. `close-divergence <tag>` turns
a tracked divergence that now matches back into an ordinary verdict, keeping the
reference bytes it already holds. `drop --cpu --suite` removes verdicts a
listing change stranded.

## Why

The provenance layer answered *which exact build produced this fact?* The
question that matters is *do we match the current tools?*, and the layer made
answering it expensive:

- **Every upgrade forked the corpus.** A new version string keyed every verdict
  separately, so moving vasm from 2.0b to 2.0f left 1,251 cases recorded under
  both, and replay checked both. A version whose behaviour changed would have
  failed replay on its predecessor's verdicts until someone wrote supersede
  records.
- **Most of the corpus was duplication.** Of 17,052 verdict lines, 8,257 were
  distinct cases. The rest were corroborating copies from a second build, or the
  same case under an older version. Migrating changed no answer: after dropping
  the 80 retired records and preferring the version in use, no case had two
  answers.
- **It was not what found bugs.** The defects found in October 2026 came from
  running real third-party code, the reference assemblers' own test suites, and
  fuzzing. None came from provenance.

Record-and-replay itself is unchanged and is the part worth keeping: recorded
once from the real tools, replayed in CI with none installed, so byte identity
is a claim anyone can check in a pull request.

## How a bump goes

1. Change the pin in `ops/arbiter/Dockerfile` and its line in
   `ops/arbiter/identities`. The image build verifies the identity before
   anything uses it.
2. Merge. The image publishes, and the next growth run (or a manual one)
   re-records every case and prunes what it did not reach.
3. Review the growth pull request's diff, and fix or track each moved answer.

Between steps 1 and 3, the corpus holds verdicts from the old version.
`corpus --check` notes them without failing, and the ledger shows the old
version beside the new.

## What this does not decide

- **Which versions to track.** The default is the latest release of each tool;
  where a project has no releases (cc65 since 2.19, ACME, PasmoNext), a pinned
  upstream commit.
- **The ecosystem corpus.** Whole upstream projects keep their own pins and
  provenance (`ecosystem-source-corpus.md`); that is about which source is
  admitted, not which tool answered.

## Drift triggers

- **"Keep the old version's verdicts too, in case."** No. Git has them.
- **"This test only passes on version N, so assert it."** Make the test pass on
  the current tool, or record the difference as a divergence.
- **"Append the new verdict rather than rewrite the file."** That is how one case
  came to have two answers.
- **"Record which binary produced it."** Nothing reads it.

## Log

- **2026-10-22** — Recorded. Steve judged the provenance machinery excessive for
  what the project needs now and chose to simplify before bumping the reference
  tools to their latest versions. The migration collapsed 17,052 lines and 80
  supersede records to 8,257 verdicts, matching an independent computation
  exactly, with coverage unchanged.
