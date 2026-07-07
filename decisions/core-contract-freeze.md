# Decision: the core contract ships public-draft and freezes at first surface contact

**Status:** Active. Binding for how the `asm198x` core contract
(`AssemblyResult`, `Diagnostic`, and kin in `crates/asm198x/src/contract.rs`)
governs its stability promise. Implements R7 of the core-contract plan
(`docs/plans/2026-07-03-003-feat-core-contract-plan.md`).

**Date:** 2026-07-06.

## The decision

The contract's public types are **public draft from day one, frozen only at real
surface contact.** They ship now — `#[non_exhaustive]`, versioned, additive,
skip-unknown — so consumers can build against them, but the semver-stability
*promise* stays **draft** until a real surface has exercised them. Real surface
contact falsifies API designs; the irreversible promise waits for it.

This borrows the **debug198x design pattern** — a versioned, serde-based,
additive/skip-unknown contract that ships as public draft and freezes at first
consumption. (debug198x's own freeze is likewise still pending its first
consumer.)

## The additive mechanism (landed, U1/U2/U5)

The shape is safe to extend without a break, so "draft" costs consumers nothing:

- **`#[non_exhaustive]`** on `AssemblyResult`, `Diagnostic`, `Severity`, `Code`,
  `Fix`, `DiagnosticEnvelope`, and the `span` types — new fields/variants are
  additive, and external code constructs through the provided constructors.
- **`#[serde(default)]`** on every optional field + serde's default
  skip-unknown — a newer producer's payload still loads on an older consumer,
  and an older producer's payload still loads on a newer consumer.
- **`CONTRACT_VERSION`** (currently `1`) stamped on every `AssemblyResult` and
  defaulted when absent. Bumped **only** for a breaking shape change — additive
  fields never bump it. A consumer branches on `AssemblyResult::version` for a
  major shape change; there is only version 1 today.

So the freeze is a *promise*, not a mechanism change: the types already behave
additively. Freezing means committing to not making a **breaking** change without
a version bump + migration.

## The freeze trigger — MCP, the first surface

The v1 core (**R1** structured result, **R2** diagnostics, **R3** JSON) freezes
when the **first surface, MCP**, has consumed it — not at sibling-crate
consumption. Crates in the family (debug198x, Forge198x) **may build against the
draft** before then; the promise is about *external, cross-surface* stability,
which only a real surface exercises.

The freeze at the trigger is gated on a **bounded review** — a deliberate pass
over the shape against the consuming surface, recorded here, not an automatic
flip the moment MCP imports a type. The review asks: did surface contact reveal a
field that is wrong, missing, or misnamed? Fix it while still draft; only then
freeze.

## Split freeze — the highest-risk APIs hold draft past the trigger

Two carve-outs stay draft **past** MCP, because MCP alone is too thin a check for
them (a secondary trigger, per R7):

- **R4 (public `Dialect` trait) + R5 (spec-query)** ship *with* MCP — the freeze
  trigger — but hold their promise draft until a **second surface** (LSP or WASM)
  has also consumed them. These are the most integrator-facing, highest-risk
  APIs; freezing them against a single surface with only a paper cross-surface
  check is the exact failure draft-then-freeze exists to prevent.
- **The symbol/section slice of `AssemblyResult`** (the `symbols` exposure,
  co-designed with debug198x's KTD4 model) stays draft **past MCP** until a resumed
  debug198x has actually **read an `AssemblyResult`**. The slice was designed with a
  paused sibling; freezing it before that sibling exercises it would freeze
  against an unexercised model. (Mirrors the R4/R5 split-freeze reasoning.)

## Dated notes (additive changes while draft)

- **2026-07-07 — multi-file source model (language-surface U1).** Two additive
  fields, both `#[serde(default)]` and skipped when empty/absent so existing
  payloads stay byte-identical: `AssemblyResult.files` (the FileId→path table,
  index = `FileId`, entry 0 = the root input) and `Span.path` (a resolved-path
  string on serialized spans, so a JSON consumer of the bare diagnostic-array
  *failure* output can resolve a file without the success-only table). No
  version bump — additive per the mechanism above. Plan:
  `docs/plans/2026-07-04-001-feat-language-surface-plan.md` (KTD2).

## Drift triggers

Re-consult this record if a change would:

- **Freeze the contract at sibling-crate consumption** (debug198x building against
  it) rather than at the MCP surface. Family crates use the draft; the promise is
  cross-surface.
- **Make a breaking change to a contract type without bumping `CONTRACT_VERSION`
  and providing a migration.** Additive-only is the whole posture; a breaking
  change is a versioned event, not a silent one.
- **Freeze R4 (`Dialect`) or R5 (spec-query) at MCP** rather than holding them for
  a second surface — or **freeze the symbol/section slice** before a resumed
  debug198x has read an `AssemblyResult`.
- **Flip the freeze the instant a surface imports a type**, skipping the bounded
  review that is the point of shipping draft first.

See the core-contract plan (`docs/plans/2026-07-03-003-feat-core-contract-plan.md`
R7, KTD4, KTD7) and the sequencing decision
([`roadmap-sequencing.md`](roadmap-sequencing.md) § freeze-at-first-consumer).
