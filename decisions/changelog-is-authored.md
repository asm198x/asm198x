# Decision: the changelog is authored, not generated

**Status:** Active. Binding for Asm198x.

**Date:** 2026-08-22.

## The decision

**`crates/asm198x/CHANGELOG.md` is written for a reader. release-plz produces a
draft of it, not the finished article.**

Before merging a release PR, rewrite its new entry as what changed and why it
matters, grouped under *Added* / *Fixed* / *Changed*, each entry linking the
pull requests behind it. `cargo xtask changelog` fails while the newest entry
still carries an `### Other` section, so this is a gate rather than a habit.

Two supporting rules:

- **Squash-merge subjects carry a conventional-commit prefix**, so the draft
  starts closer to the finished text.
- **Edit the entry immediately before merging.** release-plz regenerates it on
  every push to `main`, so an early edit is liable to be overwritten by an
  unrelated merge.

## Why this is not a failure to automate

Everything else in this repository's documentation is generated, and the rule
is deliberate: a figure a reader could check must come from the thing it
describes. The changelog is the exception, for two reasons that are worth
keeping apart.

**The first is fixable and still costs a pass.** A squash-merge subject with no
conventional-commit prefix lands under `### Other` as its raw subject. Every
merge here is a squash, so this is the normal case rather than the edge one. The
prefix rule above closes it — but a well-written commit subject is still a
statement to another developer, and a release note is a statement to someone
deciding whether to upgrade. Those are different documents.

**The second cannot be fixed here.** release-plz associates a commit with a
package by path, and the book lives at `docs/`, outside every crate. So a change
to the documentation is invisible to it. That is not a bug: it follows from
[`one-documentation-surface.md`](one-documentation-surface.md) putting the book
in this repository but outside the package, which was decided for better
reasons than changelog convenience.

The consequence is concrete. **v0.0.23 was mostly four new guide pages, and the
generated draft mentioned none of them** — it listed the two pull requests that
happened to touch a package directory, under `Other`, as raw subjects.

**And release notes are a judgement no tool can derive.** Which of eleven merged
changes mattered to a reader, and which two sentences explain a fix, is not
recoverable from commit metadata. `/releases` publishes this file, so the
question is not "did the tool list everything" but "will someone deciding
whether to upgrade learn what they need".

## What good looks like

From v0.0.23, an entry that says what a reader gets rather than what was merged:

```markdown
### Fixed

- *Why asm198x* understated two capabilities by most of their coverage. It said
  `fmt` handled seven CPU families and not the 6502, and that `disasm` read
  6502 and Z80. Both cover **every dialect**. A test now runs each operation for
  every name `--dialect` accepts, so the claim fails the build if it stops being
  true. ([#171](https://github.com/asm198x/asm198x/pull/171))
```

Name the specific wrong thing. "Documentation fixes" tells a reader nothing they
can act on; "it said seven CPU families and it is every dialect" tells them
whether they were put off by a claim that was not true.

## Drift triggers

- **"release-plz generated it, so it is fine"** — it generated a draft from
  commit subjects, and it cannot see `docs/` at all. Read the entry before
  merging.
- **"The commit messages are good, so the changelog is good"** — a commit
  message addresses a developer reading a diff; a release note addresses someone
  deciding whether to upgrade.
- **"Fix it by configuring release-plz to include `docs/`"** — there is no
  package for those commits to belong to, and moving the book inside the crate
  to solve a changelog problem would trade a settled decision for a small one.
- **"Edit the changelog early so it is ready"** — release-plz rewrites the entry
  on every push to `main`. Edit last.
- **"Rewrite the old `Other` entries too"** — no. Several releases predate this
  and carry them; inventing a record now of what someone meant then is worse
  than a thin entry. The gate judges the newest entry only.

## See also

- [`release-cadence.md`](release-cadence.md) — when to merge the release PR.
- [`one-documentation-surface.md`](one-documentation-surface.md) — why the book
  is in this repository and outside the package.
