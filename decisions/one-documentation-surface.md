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
- **The released-tag rule.** The site documents the release, not `main`, per
  `pages.yml`. Surfacing the version is a separate item; the rule is unchanged.
- **What the docs contain.** This record settles where pages are rendered from,
  not what they say. Content is
  [`docs/plans/2026-08-20-001-docs-adoption-narrative-plan.md`](../docs/plans/2026-08-20-001-docs-adoption-narrative-plan.md).

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

## Log

| Date | Event |
|------|-------|
| 2026-08-20 | Captured. Raised by Steve while reviewing the documentation plan — "it's starting to feel like the book is defunct". Three options weighed: keep the split, dissolve the renderer and keep source ownership, or move pages into the site repo. Chose the second. The third was rejected as the drift already documented in the docs-site plan and already live in the landing page's parity figures. |
