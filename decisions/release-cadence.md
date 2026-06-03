# Decision: per-CPU release cadence

**Status:** Active. Binding for Asm198x.

**Date:** 2026-06-03.

## The decision

**Cut a release when a CPU's work lands** — not after several CPUs have piled up.
Each release should map to roughly one CPU's worth of change, so its changelog is
focused and a reader can see "this is the release that added the 6809" at a
glance.

Concretely: when a CPU is complete (assembler + disassembler + its conformance
coverage, all green), **merge the open `chore: release` PR**. That tags the
version and publishes the GitHub Release. Then let the next CPU accumulate in the
next release PR.

## Why

The `v0.0.4` release rolled up *everything* since `0.0.3` — 6502 dialects, Z80,
68000, 6809, 65816, the conformance audit — because the release PR sat unmerged
while CPU after CPU landed. The changelog became a wall that maps to no single
unit of work. Releasing per CPU keeps each changelog legible and each release a
meaningful milestone, and it shortens the gap between "shipped" and "released" so
regressions surface against a smaller, attributable change set.

## How it works with the tooling

Nothing in the tooling changes — this is a **merge-cadence** policy, not a config
change:

- release-plz already opens/updates a `chore: release` PR on every push to main
  (a running draft of the next release). Per-CPU cadence just means *merging it
  at CPU boundaries* rather than letting it run.
- Versioning stays **lockstep** (one `[workspace.package]` version). A per-CPU
  release bumps the shared version even though only one CPU changed; the version
  number is a workspace checkpoint, not a per-crate semver claim. In `0.0.x`
  every bump is a patch, so cadence does not affect the number's meaning.
- Conventional-commit scopes (`feat(6809)`, `feat(65816)`, …) already group the
  changelog by CPU, so a per-CPU release's entry is naturally all-one-CPU.

## What counts as a release boundary

A CPU is the unit, but the rule is "a self-contained, green unit of work":

- A new CPU (assembler + disassembler + conformance coverage) → release.
- A cross-cutting unit that stands alone (e.g. the conformance-audit layer, a
  packaging change) → its own release rather than riding the next CPU, when it is
  substantial enough to warrant attribution.
- Don't release a half-finished CPU just to hit the cadence; "green and
  self-contained" beats "frequent."

## Drift triggers

- **"Just let the release PR keep accumulating"** — no; that is exactly what
  produced the `0.0.4` wall. Merge at CPU boundaries.
- **"Hold the release until several CPUs are done so it's a bigger milestone"** —
  no; per-CPU keeps changelogs attributable and regressions cheap to bisect.
- **"Give each CPU its own version line / split the workspace version"** — no;
  versioning stays lockstep (see [`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md)
  for the one-binary/one-version stance). Cadence ≠ per-crate versioning.
