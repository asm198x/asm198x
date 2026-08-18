# Decision: what v1.0 promises — the CLI and the formats, not the library

**Status:** Active. Binding for Asm198x. Sets the v1.0 bar and the scope of the
stability promise the version number makes.

**Date:** 2026-08-18.

## The decision

**v1.0 promises the `asm198x` CLI's behaviour and the Debug198x format. It does
not promise the Rust library API, the core contract, or the ISA spec crates.**

Those stay where their own records put them: public draft, on their own freeze
triggers, carrying the workspace version number but no semver claim. The version
stays lockstep across the workspace per
[`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md);
what changes at 1.0 is what the number *means*, and it means it for two surfaces
only.

## Why the library is out

Not caution — mechanics. Nothing is published:
[`release-plz.toml`](../release-plz.toml) sets `publish = false` at the
workspace, and the umbrella
[`rung1-wiring.md`](../../../decisions/rung1-wiring.md) holds that position with a
drift trigger (`isa` is too generic a name to claim; revisit only with a rename
and a new decision). Emu198x, the only external consumer, pins git revs.

So a semver promise on the library crates would be a promise with no delivery
mechanism — nobody can depend on `isa` or `contract.rs` *by version* to begin
with. The packaging record already says the shared version is "a workspace
checkpoint, not a per-crate semver claim." 1.0 restates that deliberately rather
than letting the number quietly start claiming more than it can deliver.

`isa` has a further reason to stay out: if the extraction trigger ever fires it
is re-cut into per-CPU crates (`isa-6502`, `isa-z80`, …) and moved to a neutral
home. Freezing an API on a crate that a fired trigger would dismantle is the
worst available timing. The trigger has **not** fired and does not gate 1.0 — see
[`rung1-wiring.md`](../../../decisions/rung1-wiring.md) § *Escalation trigger* — but
the shape of the deferred work argues against promising anything about `isa` now.

## What this scope dissolves

Read the freeze records literally and "1.0 needs the contracts frozen" smuggles
in two unbuilt surfaces:

- the core contract freezes at **first surface contact, and the surface is MCP**
  ([`core-contract-freeze.md`](core-contract-freeze.md)) — MCP is Layer 3 and
  unstarted;
- **R4/R5** (the public `Dialect` trait and spec-query) hold draft past MCP until
  a **second** surface — LSP or WASM — has also consumed them.

Under this scope neither is a 1.0 blocker, because neither is in the promise.
They keep their existing triggers and freeze when a surface exercises
them, which is what those records were protecting in the first place.

**Only Debug198x must freeze for 1.0**, because it is a wire format read across a
repo boundary by another project — a real external promise with a real consumer.
Its own checklist governs, per [`debug198x-format.md`](debug198x-format.md).

## The v1.0 bar

Six items. Two are substantial; four are small.

1. **Macros.** The README's identity claim is that *real-world source for a
   machine assembles unchanged*, and there is no macro support at all today —
   the largest gap between the promise and the product. Scoped as the codex
   roadmap scopes it: adopted against **real dialect requirements**, not as a
   universal macro language. Repetition with it; modules only where a dialect
   demands. This is the deliberate stage 2 recorded in the language-surface
   plan's Scope Boundaries.

2. **The verdict corpus, v1a (#61).** Byte-identical output against reference
   tools is the product's central claim and it is verified only on the
   maintainer's machine — the reference-arbitrated suites are `#[ignore]`d, CI
   installs no reference tools, and nothing runs `--ignored`. A 1.0 that promises
   byte-identical output needs CI that can prove it, and outside contributors
   currently cannot prove anything in a PR.

3. **Freeze Debug198x.** Leg 3 of the banked fixture's validation — the Emu198x
   paging cross-check — is the last open checklist item.

4. **Fix the open accuracy bugs** (#66 8080 `ds` fill, #67 sjasmplus conditional
   fringe). They are counter-examples to the exact claim 1.0 makes.

5. **Move the CLI to subcommands.** `asm198x asm` / `asm198x disasm`, already
   decided in the packaging record for the Stage-3 checkpoint and deferred there
   "so the CLI isn't churned mid-build." Under this scope the CLI *is* the
   promise, so the churn lands before the freeze, never after.

6. **Documentation and distribution for a tool people install.**
   - Real installers. `installers = []` today, so the CLI ships as raw archives;
     a shell installer and a Homebrew tap are cargo-dist configuration.
   - A CLI reference. `--help` is dense and complete, and it is the only CLI
     documentation that exists.
   - Per-dialect directive matrices, **generated from the conformance corpus** —
     not hand-authored. There are 21 dialect front-ends and **one** dialect
     reference document (`dialects/6502.md` in the org docs repo, itself carrying
     a caveat that its gap notes may be stale). Source-compatibility is the
     product's identity, so per-dialect fidelity is what a user most needs
     written down and it is the least written down — but hand-writing 21 pages is
     the naive fix the docs-site plan
     (`docs/plans/2026-07-04-004-feat-docs-site-plan.md`) exists to prevent: *"a
     second source of truth already stale at 19 CPUs and losing harder at 30."*
     That plan already wires directive matrices as a **generated slot** fed by the
     conformance corpus, which is item 2 above. The corpus pays off twice — it
     makes the guarantee provable *and* it is the source the dialect
     documentation generates from. This item is downstream of item 2, not
     parallel work.
   - The docs-site plan's **v1 core** — R1 instruction references generated from
     `isa` with provenance links into the umbrella `reference/` library, R2 every
     sample assembled by the real binary in CI, R3 mdBook plus the House198x Vale
     lint promoted to a CI gate — in scope while it stays cheap. It is unblocked
     today per [`roadmap-sequencing.md`](roadmap-sequencing.md), and it is where
     the generated matrices land. Its later slots stay out.

## Explicitly outside v1.0

Cycle analyzer (blocked on the unowned timing/flags data seam), dialect
converter, LSP, WASM playground, MCP, ARM2, the `isa198x` extraction, a
standalone linker and object format — and the docs site's post-v1 slots
(diagnostic explain-pages, the conformance ledger, cycle columns), each gated on
a source that does not exist yet. None are promises the README makes. Folding any
of them in converts a 1.0 bar into a 2.0 bar.

## Order

Bugs, then the cheap freeze, then the two substantial items, then the surface:

1. #66, #67
2. Debug198x freeze (leg 3)
3. Verdict corpus v1a
4. Macros
5. CLI subcommands, installers, the CLI reference
6. Docs-site v1 core, carrying the dialect matrices generated in step 3
7. Cut 1.0

## Drift triggers

Re-consult this record before:

- *"1.0 should freeze the library API too / consumers need a stable Rust
  surface."* → Not while `publish = false` stands. A published crate is a
  different decision (a rename plus a new record, per `rung1-wiring.md`); until
  then the promise has no delivery mechanism. Revisit **here** if publishing
  changes.
- *"We can't ship 1.0 until the core contract is frozen."* → The core contract is
  not in the promise. Its trigger is MCP and it keeps it.
- *"Build MCP so the contract can freeze for 1.0."* → No. Build MCP when a
  surface is wanted, not to satisfy a freeze that 1.0 does not require.
- *"Let's do the `isa198x` extraction before 1.0 so the story is clean."* → The
  trigger has not fired and extraction is decoupled from the roster by design.
  See `rung1-wiring.md`.
- *"Add \<feature\> to the 1.0 bar, it's nearly done."* → The bar is what makes
  the README's existing claims true. A feature that adds a *new* claim is 1.1.
- *"Ship 1.0 with the reference suites still `#[ignore]`d; they pass locally."* →
  That is the bus-factor-one trust chain #61 exists to remove, on the one claim
  the product is built around.
- *"Just hand-write the dialect pages, it's faster than wiring generation."* →
  No. That is the drift the docs-site plan was written to prevent, and it
  compounds with every CPU added. Dialect documentation generates from the
  conformance corpus. Editorial prose that duplicates generated data is what to
  avoid; a **CLI reference** is genuinely editorial and is hand-written on
  purpose.
