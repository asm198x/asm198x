# Decision: work lands through a pull request, and merges are squashes

**Status:** Active. Binding for Asm198x (accepted 2026-08-24). Supplies the
premise [`changelog-is-authored.md`](changelog-is-authored.md) already relies on.

**Date:** 2026-08-24.

## The decision

1. **Work goes through a pull request** — every change that is not a trivial
   revert, including documentation and decision records.
2. **Pull requests are squash-merged.** One commit per pull request on `main`,
   carrying the pull request's title.
3. **The squash subject carries a conventional-commit prefix**, so release-plz
   files it under *Added* or *Fixed* rather than under `Other`.

"Where possible" is the escape hatch and it is narrow: a change that cannot wait
for CI because CI itself is broken, or a revert of something already on `main`.
Absence of branch protection is not a reason — `main` has none here, so the
convention is the only thing holding.

## Why squash rather than merge commits

`changelog-is-authored.md` says "every merge here is a squash" and builds on it:
a squash subject with a conventional-commit prefix starts the changelog draft
close to finished, and the gate that fails on an `### Other` section assumes
each pull request contributes one line rather than thirty.

That assumption was tested by accident. A 33-commit branch was merged with a
merge commit, and every one of its subjects landed under `### Other` as raw
text. The changelog is authored either way, so nothing was lost — but the draft
started from noise instead of from a summary, which is the cost the rule exists
to avoid.

Merge commits also make `main`'s history a poor place to read what happened. One
squash per pull request means `git log main --oneline` is a list of changes; a
merge per pull request means it is a list of changes interleaved with the
working steps that produced them.

## Why a pull request even with no protection

`main` is unprotected in this repository, so a direct push succeeds. That makes
the convention easier to break than to follow, which is the reason to write it
down rather than the reason to relax it.

Three things only a pull request gives:

- **CI before `main`, not after.** A stale generated page or a failing reference
  suite is caught on the branch. Pushed straight to `main`, it is caught after
  the fact and the fix is a second commit on a broken tree.
- **A reviewable unit.** The pull request body is where the reasoning that does
  not belong in a decision record goes — what was probed, what was rejected.
- **One squash subject**, which is what rule 2 above needs to exist.

## Drift triggers

- **"`main` isn't protected, so this is fine"** — that is the reason for the
  rule, not an exemption from it.
- **"It's only a documentation change"** — documentation has a CI gate here
  (`xtask docs --check`), and it has failed on a branch this week.
- **"The branch is long, a merge commit preserves the detail"** — the detail is
  preserved on the branch and in the pull request; `main` wants the summary.
- **"Squashing loses the individual commit messages"** — it does, and they stay
  readable on the pull request. A commit that needs its own line on `main`
  needs its own pull request.
