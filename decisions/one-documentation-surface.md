# Decision: one documentation surface — the site renders it, the assembler's repo owns it

**Status:** Active. Binding for all asm198x documentation: the pages under
`docs/book/`, the generators in `xtask/`, the CI gates in `.github/workflows/ci.yml`,
and the site build in `asm198x/asm198x.github.io`.

**Date:** 2026-08-20.

**Supersedes:** the **mdBook** half of R3 in
[`docs/plans/2026-07-04-004-feat-docs-site-plan.md`](../docs/plans/2026-07-04-004-feat-docs-site-plan.md).
That plan's premise — a page and its generator must fail the same build — is
preserved unchanged. Only the renderer is withdrawn.

## The decision

There is **one documentation surface**: the site.

1. **Source stays in this repo.** Every documentation page continues to live in
   `docs/`, beside the code it describes, where `cargo xtask docs --check`,
   `crates/asm198x/tests/book_samples.rs` and the Vale gate already run against it.
2. **The site renders all of it.** `asm198x.github.io` builds the pages from its
   existing checkout of this repo, at the released tag, and publishes them as part
   of the site rather than as a separate artefact behind `/docs/`.
3. **mdBook is withdrawn.**
4. **No documentation page's source moves into the site repo.**

## Why this shape

**The renderer was never what made the book work.** The book was moved into this
repo because documentation maintained across a repo boundary drifts: the CLI
reference was wrong in two ways for months, and the plan recorded why — *"that
only holds if the generator and the page fail the same build; across a repo
boundary they do not."* That property comes from where the source lives. mdBook
was incidental to it.

**mdBook was earning little.** No page uses mdBook-specific syntax — no
includes, no playpen, no preprocessors. The content is plain markdown plus a
`SUMMARY.md`.

**The split charged the reader for our convenience.** Two build systems, two
designs, two navigations, and a seam a reader crosses at `/docs/` — none of which
answers a question they have.

**The tempting simplification is rejected.** Moving the pages into the site repo
would collapse the split too, and it is the failure this repo has already been
bitten by. It is live right now: the landing page's parity figures
("32/32 hunk-exe parity", "all 32 NES units") are hand-copied from this repo's
`README.md` and `decisions/why-not-llvm.md` into an `.astro` file, checked by
nothing.

## What must be replaced, not dropped

mdBook currently provides three things the site build has to carry:

| Provided by mdBook | Why it stays |
|---|---|
| `create-missing = false` | A chapter with no file fails CI instead of shipping a dead link. This is a current gate; losing it is a regression, not a deferral. |
| Search | 21 generated instruction pages today, 45 once the dialect matrices land. Reference without search is worse than no reference. |
| Nav from `SUMMARY.md` | Ordering and hierarchy are authored, not inferred from the filesystem. |

## What this does not change

- **Source ownership, R1 and R2.** Generated blocks, `docs --check`, and every
  sample assembled by the real binary all run exactly as they do now.
- **The Vale CI gate.** It runs on the source tree, not the renderer.
- **The released-tag rule.** *Amended 2026-08-21 — see "Publishing cadence"
  below.* The site documents the released binary, not whatever `main` happens
  to hold. The rule stands; what changed is how it is satisfied.
- **What the docs contain.** This record settles where pages are rendered from,
  not what they say. Content is
  [`docs/plans/2026-08-20-001-docs-adoption-narrative-plan.md`](../docs/plans/2026-08-20-001-docs-adoption-narrative-plan.md).

## Publishing cadence (amended 2026-08-21)

**Publish the newest documentation that still describes the released binary.**

`pages.yml` builds from `main` when `crates/`, `xtask/` and the manifests are
unchanged since the newest release tag, and from the tag otherwise. The version
label keeps naming the release either way, because the release is what those
pages describe.

### Why this was not the original rule

Pinning to the tag is one way to satisfy "the published pages describe the
binary people have installed", and it was the obvious one. It is stricter than
the rule requires, and the difference cost something immediately: a
documentation-only change touches no package directory, so release-plz opens no
release PR, so the pages could not reach a reader without a release cut by hand.
Two were, in one day — v0.0.18 for the mdBook withdrawal, v0.0.20 for the
quickstart — and steps 5 to 12 of the documentation plan are almost entirely
prose.

The alternatives were worse. Moving `docs/` inside a package would make every
typo cut a release, which `release-cadence.md` argues against directly.
Splitting prose from generated pages would put two revisions behind one site,
which is the drift this record exists to prevent. Cutting releases by hand
forever means the one nobody cuts is the one that mattered.

### What the condition protects

Generated pages — the instruction reference, the dialect table — are derived
from the code. Publishing those from `main` while the code has moved ahead would
describe a binary nobody can install. The condition makes that impossible rather
than unlikely: when the code is unchanged, `main` and the tag produce byte
identical generated pages, so there is nothing to be wrong about. When it has
moved, the tag wins until the next release.

`xtask` is in the condition because it generates those pages, and the manifests
because they carry the version.

### The other half: the site has to be told

`pages.yml` has always listened for `repository_dispatch: docs-changed`, and
nothing ever sent it. The only automatic path was a weekly cron, so v0.0.20's
quickstart would have waited up to a week to appear. `asm198x` now fires that
dispatch when `docs/` changes on `main`.

That needs a token with write access to the site repository, which
`GITHUB_TOKEN` is not. Until one exists the dispatch step skips and the cron —
now daily — is the backstop.

## Drift triggers

Re-consult this record before:

- *"Move the docs into the site repo so Astro can import them directly."* → No.
  Source stays here. Rendering reaches across; ownership does not.
- *"Add an mdBook preprocessor for X."* → mdBook is withdrawn. If X is worth
  having, it belongs in `cargo xtask docs`.
- *"Ship the Astro docs now and add the dead-link check later."* → `create-missing
  = false` is a gate that exists today. Shipping without it is a regression.
- *"It is one version string / one parity figure — the site can just hold it."* →
  That is the drift this record and the docs-site plan both exist to prevent. If a
  reader could check it, it is generated here and consumed there.
- *"Keep mdBook for the reference and use Astro for the rest."* → That is the
  split, restated. It was rejected for the seam it puts in front of the reader.
- *"The site should just build from `main`."* → Only while the code is unchanged
  since the release. A generated page built from `main` after a code change
  documents a binary nobody has.
- *"This docs change needs a release cut for it."* → It does not, unless code
  changed too. That was the workaround, not the rule.

## Log

| Date | Event |
|------|-------|
| 2026-08-21 | Amended. The released-tag rule was satisfied by pinning to the tag, which meant documentation-only changes could not reach a reader without a release cut by hand — done twice in one day. Narrowed to "the newest revision that describes the released binary", and connected the `docs-changed` dispatch the site had always listened for and nothing had ever sent. |
| 2026-08-20 | Captured. Raised by Steve while reviewing the documentation plan — "it's starting to feel like the book is defunct". Three options weighed: keep the split, dissolve the renderer and keep source ownership, or move pages into the site repo. Chose the second. The third was rejected as the drift already documented in the docs-site plan and already live in the landing page's parity figures. |
