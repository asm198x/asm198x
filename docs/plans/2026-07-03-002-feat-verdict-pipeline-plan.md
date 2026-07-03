---
title: Verdict Pipeline - Plan
type: feat
date: 2026-07-03
topic: verdict-pipeline
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Verdict Pipeline - Plan

## Goal Capsule

- **Objective:** Make the byte-identical guarantee enforceable and visible without the reference tools: a committed **reference-verdict corpus** that CI and contributors replay against, and a per-release public **conformance ledger** generated from it.
- **Product authority:** Steve Hill. Seeded from the ideation record at `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` (idea 2); scope confirmed 2026-07-03; pressure-tested by document review the same day (nine findings applied; two judgment calls returned to Steve).
- **Open blockers:** None. Ready for planning.

---

## Product Contract

### Summary

A committed, append-only corpus of **reference facts** — (tool, exact version identity, dialect, source text) → assembled bytes or rejection — recorded whenever the reference-arbitrated suites run with tools present, and replayed by CI and contributors with no tools installed. Regressions against arbitrated text fail CI; unarbitrated text is a governed coverage metric with per-PR delta visibility and a per-release ratchet. Each release generates a conformance ledger (per CPU: arbiter identity, verdict counts by kind, arbitration coverage, documented-divergence counts, corpus hash) as the public receipt.

### Problem Frame

Every reference-arbitrated suite is `#[ignore]`d and CI installs no reference tools, so the project's core guarantee — byte-identical against 8+ references across 19+ CPUs — runs only on one machine. That is a bus-factor-one trust chain for *verification* (replay fixes this) though growth/re-arbitration remains single-machine until a tools-installed runner exists; it makes external contributions unprovable in PRs beyond already-arbitrated text; and it leaves the strongest correctness claim invisible to the skeptical adopters the public posture courts. The arbiters themselves (asl, pasmo) are aging community tools whose behaviour deserves capture before it rots. Closed issue #36 shows real-world bugs concentrate at the acceptance boundary — what the references *reject or truncate* — which the positive corpus cannot see and nothing currently records.

The design must survive one structural fact: the conformance audits are generative — they synthesize bytes, render them with **our** disassembler, and hand that text to the reference. Our rendering legitimately changes as the toolchain improves, so verdicts must be keyed on what the *reference* was asked and answered, never on our pipeline's internals.

### Key Decisions

- **The corpus is a memo-table of reference facts, not golden files.** Each verdict records (tool, version identity, dialect/CPU, source-text identity) → (reference output bytes, or rejection with diagnostic). A verdict is true forever — it describes the reference, not us. When our disassembler's rendering changes, previously-arbitrated text stops being consulted and the new text surfaces as unarbitrated coverage to grow, never as corpus invalidation. (Conventional golden files were rejected: the `ASL A` lesson shows rendering churns; snapshots keyed to our output would rot with every improvement.)
- **Sweep verdicts are chunked, and store full reference bytes.** Sweep audits are keyed per mnemonic-group chunk (not one blob per CPU): a rendering change de-arbitrates only the touched chunks, bounding the coverage cliff, and replay mismatches localize to a chunk plus a first-differing byte offset that maps to a case via the deterministic synthesis order. Verdict outcomes for audit suites store the reference's full output bytes, not digests — a mismatch must be diffable with no tool present. (This commits to full-byte storage ahead of the deferred corpus-size arithmetic; the fallback, if that arithmetic later shows full bytes are prohibitive, is digest-plus-on-demand regeneration via the arbiter container — but full bytes stay the default because replay must diff with no tool present, and the container is a growth-time actor, not present at PR replay.) Curriculum verdicts remain digest-only (sources are never copied).
- **Coverage is governed, not just measured.** Steady-state unarbitrated text warns (fail-closed rejected — it would block every rendering change on a maintainer growth run), but two controls compensate: every PR reports its arbitration-coverage **delta** and a coverage-reducing PR requires explicit acknowledgment or attached growth verdicts to merge; and the release ledger **ratchets** — a release whose coverage dropped since the previous one requires an explicit decision-record acknowledgment. The PR acknowledgment is **content-bearing** — it enumerates the de-arbitrated cases as recorded **growth debt** (a grep-able residue), not a bare check-off — and that debt must be cleared by a growth run before the next release, bounding the window a rendering *regression* (the `ASL A` bug class) could hide in while co-located in an acknowledged chunk. This **narrows** the camouflage gap: a regression and a rendering improvement both surface as a visible, acknowledged, enumerated delta rather than silent green.
- **Documented divergences are first-class, with our side held in-repo.** Replay failure applies only to verdicts without a divergence class. A divergence verdict (issue-#36 truncation; the fuzzer's deliberately scoped-out canonicalizations) pairs the corpus's reference-side fact with an **our-side expectation living in code/tests** — never in the corpus, which describes only the reference. Replay checks both halves. Live mode records scoped-out fuzz cases as divergence verdicts rather than plain facts.
- **Version identity is behavioural, not nominal — and one behavioural version may have many binaries.** A verdict's tool version is the fullest self-reported identity including build/revision markers (asl's perpetual "1.42 Beta [Bld N]"), plus a digest of the tool binary. The **behavioural** version identity is primary: replay-selection and the R2 integrity alarm key on it, never on the binary digest. The digest is recorded **unconditionally, as provenance** — multiple binaries of the same behavioural version (the decided arbiter-container's rebuild of asl; a Homebrew vs source build) **corroborate** a verdict rather than fork it into a parallel fact-line. The digest disambiguates only where the self-report genuinely under-identifies *and* the bytes differ. Multi-identity — many binaries behind one behavioural version — is a named v1 design input, not an edge case, precisely because the container guarantees a fresh digest on day one. Nothing captures versions today (`have(bin)` gates on presence only); this is what makes verdicts durable facts and lays the dialect-editions groundwork without committing to it here.
- **Two modes on the existing four layers, not a fifth layer.** The curated/round-trip/audit/fuzz architecture (per `decisions/spec-conformance-and-fuzzing.md`) is untouched. With tools present (live mode) the suites arbitrate and append verdicts; without tools (replay mode — CI, contributors) the same suites consult the corpus. The `#[ignore]` wall becomes a mode switch.
- **Negative conformance is recorded, not authored, in v1.** Verdict records carry an open, extensible divergence tag (initial vocabulary: we-accept/they-reject, they-accept/we-reject, both-accept-different-bytes — only the third has a demonstrated case, so the tag is deliberately not a closed enum). Authoring rejection corpora per dialect is a staged follow-up.
- **The ledger is generated, never hand-written.** Plain git history and checksums are the integrity story in v1; Certificate-Transparency-style signing is a deferred upgrade. The ledger's evolution is governed by a decision record; the corpus format is versioned and additive.
- **Curriculum is pinned, checked out, and in the net.** Curriculum verdicts record the `code198x/code-samples` ref they were arbitrated against; CI checks out that pinned ref (public `code198x/code-samples`; a depth-1 checkout transfers ~15 MB — fork-PR safe) and runs curriculum replay per R5. A digest miss with the pin unchanged is an alarm; a pin bump is expected re-arbitration — but the whole layer re-arbitrates at once, so a pin-bump PR carries a **machine-checkable completeness receipt** (every file under the new pin has a fresh verdict) that CI validates before merge, so a partial re-arbitration cannot land green. The ledger reports the pinned ref's **age** so staleness is visible; bumps track Code198x milestones. The pinned ref is a named ledger input (R10). (Alternative — live-mode-only curriculum in v1 — rejected: it would leave the layer that proves *shipped programs* on one machine.)
- **Hybrid enforcement is the destination; the corpus ships first.** "CI installs no reference tools" is a sequencing choice, not physics — the arbiters are Linux-buildable. Decided: corpus replay guards every PR (speed, fork-safety, preservation, durable facts), and a scheduled **arbiter-container growth job** — live mode in a pinned container, auto-proposing growth PRs — is the named follow-up that removes growth's bus factor. v1 scope is corpus + replay + manual growth; the container job is a decided next increment, not an open question, and supersedes the earlier "nightly runs are optional" stance.

### Requirements

**Corpus content**

- R1. A verdict records: arbiter tool, its behavioural version identity (fullest self-report including build markers) plus a tool-binary digest recorded unconditionally as provenance, dialect/CPU target, source-text identity, and the outcome — full reference output bytes for audit suites (digest for curriculum), or rejection with the tool's diagnostic. A rejection is recordable only when the tool exited deliberately with a diagnostic attributable to the source text; crashes, I/O failures, and other environmental outcomes are non-verdicts and never enter the corpus. Two source texts may share an identity only if the reference's behaviour on them is guaranteed identical — column- and whitespace-sensitive dialects make byte-exact identity the safe default.
- R2. Verdicts are append-only. A re-arbitration that disagrees with a stored verdict (same **behavioural** version identity + text, different outcome — regardless of binary digest) is a first-class alarm; a differing binary digest with identical bytes for the same text corroborates and never alarms. Alarms resolve through an auditable **supersede record** — a new appended entry referencing the disputed verdict, stating the adjudication and why, and marking the loser inert. The supersede states its **adjudication basis**: where the dispute is reproducible, re-arbitration under the pinned identifying build (exact binary digest) settles it; where it is not reproducible, the record carries a maintainer judgment with its rationale. Supersede records **chain** — a supersede may itself be superseded; the latest adjudicated entry for a (behavioural identity, source text) is authoritative, and the full chain stays walkable. History is never edited.
- R3. Negative verdicts carry the open divergence tag (initial vocabulary per Key Decisions).
- R4. Sweep-audit verdicts are keyed per mnemonic-group chunk with full reference bytes; form-audit, differential-probe, and position-dependent round-trip verdicts per case, and fuzzer verdicts per case; curriculum verdicts per file (file path + source digest → output digest, over hunk-symbol-normalized output on the Amiga path) under the pinned ref, with the tree/pin identity as shared context and no source content copied.

**Replay and CI**

- R5. With no reference tools installed, the conformance, differential, and curriculum suites run in replay mode against the corpus — in CI on every PR. CI obtains curriculum sources by checking out the pinned `code198x/code-samples` ref recorded in the corpus. A PR that bumps the pin must carry a completeness receipt CI validates — every file under the new pin has a fresh verdict — before it can merge.
- R6. A replay mismatch against a verdict **without** a divergence tag fails CI, localized to the case (per-case suites) or chunk + first-differing offset mapped to a case (sweeps). Divergence-tagged verdicts replay by checking both halves: reference bytes against the corpus, our bytes against the in-repo expectation.
- R7. Unarbitrated text is reported and counted, not failed. Arbitration coverage is a guidance metric: it drops transiently when renderings change and recovers via growth runs; per-CPU coverage is tracked and published.
- R8. Live mode (tools present) behaves as today plus verdict recording — including recording deliberately scoped-out fuzz cases as divergence verdicts; a dedicated growth run arbitrates all currently-unarbitrated text.
- R12. CI reports the per-PR arbitration-coverage delta; a PR that reduces coverage requires explicit acknowledgment (or attached growth verdicts) to merge. The acknowledgment is content-bearing: it enumerates the de-arbitrated cases as recorded growth debt (grep-able residue), and that debt must clear (via a growth run) before the next release.
- R13. A PR introducing or modifying a CPU/dialect must not merge below its pre-PR arbitration coverage; for a new CPU (zero coverage), a growth run is a merge precondition.

**Ledger**

- R9. Each release generates the conformance ledger from the corpus: per CPU — arbiter tool + behavioural version identity (multiple binary digests behind one behavioural version are provenance, not separate ledger rows), verdict counts by kind (form / sweep-chunk / probe / fuzz / curriculum), arbitration coverage, documented-divergence counts by tag, and the corpus hash; plus the pinned curriculum ref and its age. Published with the release and on the org docs surface. (Replay pass rate is deliberately absent: R6 makes it structurally 100% — a column with no signal.)
- R10. The ledger is deterministic over its enumerated inputs: corpus hash, release tag, and the pinned curriculum-source ref — all named in the ledger itself. Identical inputs produce byte-identical ledger output.
- R14. The release path enforces the coverage ratchet **before the release is tagged** — a required status on the release PR, or a gating step before release-plz's `release` command, never post-merge (by then the tag and the cargo-dist release already exist): a release whose per-CPU coverage dropped **below the last acknowledged baseline** requires an explicit decision-record acknowledgment naming the drop. A drop already acknowledged at PR time (R12) is not re-challenged.

**Contribution**

- R11. A PR's arbitration status (coverage, delta, unarbitrated cases) is visible in CI output; the maintainer growth run is the documented merge precondition for new/changed dialects (R13) until a tools-installed runner exists. Contributor-facing process documentation beyond this is deferred (see Scope Boundaries).

### Key Flows

- F1. Regression caught without tools
  - **Trigger:** A PR changes an encoding path; CI runs replay mode.
  - **Steps:** The suites synthesize/render as always; corpus lookup finds the text arbitrated; our bytes disagree with the stored reference bytes; CI fails naming the CPU, the case (per-case suites) or chunk + offset-mapped case (sweeps), and the arbiter identity.
  - **Outcome:** The guarantee is enforced on every PR, on machines that have never seen a reference assembler.
- F2. Corpus growth
  - **Trigger:** Maintainer runs the growth command with tools installed (after a rendering change, a new CPU, or a coverage-delta acknowledgment debt).
  - **Steps:** Unarbitrated text is arbitrated live; verdicts (with version identities) append; environmental failures are skipped as non-verdicts; the coverage metric recovers; the diff is reviewable in the PR.
  - **Outcome:** Coverage gaps close through normal git workflow; verdicts are permanent.
- F3. The public receipt
  - **Trigger:** Release-plz release PR is open for merge.
  - **Steps:** The ratchet (R14) runs as a blocking check **before the tag exists** — comparing coverage against the last acknowledged baseline; on pass, the release tags and post-merge ledger generation is purely mechanical; publishes with the release artifacts and the docs surface.
  - **Outcome:** A skeptical adopter reads per-CPU verdict counts, coverage, arbiter identities, and documented divergences without cloning anything.

### Acceptance Examples

- AE1. **Covers R5, R6.** Given a deliberate one-byte spec regression on a corpus-arbitrated Z80 form, CI with no tools installed fails, naming the form and the arbiter identity; given the same regression on a sweep CPU, CI names the chunk and the offset-mapped case.
- AE2. **Covers R7, R11, R12.** Given a disassembler rendering change that alters audit text for ten 6502 forms, CI's per-PR output shows the arbitration coverage, the −10 delta, and the newly-unarbitrated cases; the delta requires acknowledgment; with acknowledgment (or attached growth verdicts) the PR merges; coverage recovers after the next growth run.
- AE3. **Covers R2.** Given a growth run where the same tool identity returns different bytes than a stored verdict, the run halts with a corpus-integrity alarm; appending a supersede record adjudicating the dispute unblocks growth with full history preserved.
- AE4. **Covers R3, R6.** Given the issue-#36 out-of-range immediate case, the corpus holds the reference's truncate-and-warn bytes tagged both-accept-different-bytes, the in-repo expectation holds our error behaviour, and replay verifies both halves.
- AE5. **Covers R9, R10.** Given identical enumerated inputs, regenerating the release ledger twice produces identical bytes; the ledger names asl's full build identity for each asl-arbitrated CPU and reports documented-divergence counts.
- AE6. **Covers R1.** Given a growth run where a reference tool crashes on one case, no verdict is recorded for it and the run continues; given a deliberate rejection with a diagnostic, a rejection verdict records with the diagnostic text.
- AE7. **Covers R4, R8.** Given a growth run over a sweep CPU, verdicts append per mnemonic-group chunk with full reference bytes; given the seeded fuzzer, scoped-out canonicalization cases append as divergence-tagged verdicts, not plain facts.
- AE8. **Covers R13.** Given a PR adding a new CPU with zero arbitration coverage, CI reports the status and the merge gate holds until a growth run's verdicts land with the PR.
- AE9. **Covers R14.** Given a release-plz release PR whose per-CPU coverage dropped below the last acknowledged baseline, the pre-tag ratchet check fails until a decision-record acknowledgment naming the drop is added; a drop already acknowledged at PR time (R12) passes without re-challenge, and no tag or release is created while the check is red.

### Scope Boundaries

**Phasing within v1 (sequencing, not scope cuts)**

The confirmed CI-net-first ordering suggests two increments inside v1; nothing here is deferred out of v1.

- **v1a — the enforceable net:** R1–R8 (the corpus of reference facts, replay mode, regression failure with no tools). This alone makes the byte-identical guarantee enforceable on every PR. Curriculum (pinned, in the net) rides v1a — it proves *shipped programs*, and the decision to keep it in the first increment stands.
- **v1b — the receipt and governance:** R9/R10 (generated ledger), R11 (contribution visibility), R12/R13 (coverage-delta and new-CPU gates), R14 (release ratchet). Much of this layer's value compounds once the arbiter-container growth job exists, but the ledger and gates ship in v1.

**Deferred for later**

- Authored per-dialect rejection corpora — schema support ships in v1; the authoring campaign is staged.
- Contributor-facing arbitration-request documentation — written when the first external PR arrives; R11/R13 already give any contributor a working CI signal and a documented gate.
- Cryptographic signing / transparency-log machinery for the ledger — git history + checksums first.
- Corpus compaction/archival policy for inert verdicts (superseded or permanently unconsulted) — the format decision record owns this before the corpus's first birthday.
- Dialect-edition pinning (`--dialect tool@version`) — enabled by R1's version identity, decided separately.
- The arbiter-container growth job (scheduled live mode, auto-proposed growth PRs) — decided as the hybrid's second half per Key Decisions; lands as its own increment after v1, with container contents and pinning specified in its own plan.

**Outside this product's identity**

- The crater-style period-source corpus (magazine listings, scene archives) — cut in ideation for licensing and process weight; this record re-affirms the cut.
- Copying curriculum sources into this repo — Code198x stays canonical; the corpus stores digests only.

### Dependencies / Assumptions

- The full reference-tool set exists on exactly one machine (the maintainer's); growth runs happen there until a tools-installed runner exists — replay fixes verification's bus factor, not growth's.
- The generative-audit structure (synthesize → our disasm → reference) is unchanged; the corpus keys on the text handed to the reference.
- Confirmed 2026-07-03: purpose ordering is CI-net first, public ledger second, contributor enablement derived, preservation as framing; steady-state unarbitrated text warns (with the R12/R14 governance); the corpus lives in this repo, keeping code + verdicts atomic in PRs.
- Planning dependency: R5–R8's mode switch must thread through the existing `#[ignore]` + `have()` architecture without inverting its semantics (`tests/conformance.rs:33-35`); CI's coverage runner already forwards extra args (`--include-ignored` slot exists).
- The recording harness must distinguish deliberate tool rejections from environmental failures — today's `ref_assemble` collapses both into `None` (`tests/conformance.rs:76-88`); R1's non-verdict rule depends on separating them.
- CI must reproduce the sibling-container layout inside the workspace (nested checkout paths) or the curriculum harness gains a corpus-path override: the hardcoded `../../../../Code198x` locator (`tests/curriculum.rs:25-29`) resolves two levels above the repo root and cannot be satisfied by a plain `actions/checkout`, which rejects paths outside `$GITHUB_WORKSPACE`. R5's curriculum checkout is not free.

### Outstanding Questions

**Deferred to planning**

- Where the pinned curriculum ref is declared, and the exact form of the pin-bump completeness receipt. (Cadence is decided — bumps track Code198x milestones.)

- How the R12/R13 arbitration-coverage baseline is obtained in CI — base-ref checkout + second counting run vs a committed coverage stamp — and the fetch-depth / second-checkout implications for `ci.yml`. The base's arbitration coverage is not derivable from the append-only corpus alone: the denominator is the base ref's rendered-text set, which needs the base ref's code, not just the corpus.

- Verdict file format and layout; the chunking function for sweep verdicts (mnemonic-group boundaries).
- How replay mode threads through `#[ignore]`/`have()`; the growth command's UX; growth-run atomicity (interrupted mid-append) and live-vs-replay precedence when both are possible.
- Where the integrity alarm and the supersede workflow surface (test failure vs dedicated tool).
- Ledger rendering and publication mechanics (release asset + docs page); corpus size arithmetic before the first growth run.

### Sources

- `docs/ideation/2026-07-03-asm198x-world-class-ideation.html` — idea 2, three-reviewer merge, escrow-first sequencing per the fresh-context verifier.
- `decisions/spec-conformance-and-fuzzing.md` — the four-layer architecture and disassembler-reuse trick this design must not disturb; the `ASL A` lesson motivating both reference-fact keying and the coverage-delta governance.
- Grounding scout + review verification (2026-07-03): case scales (sweeps 1024–65,536 candidates, one live reassembly per CPU — `tests/conformance.rs:729-746` incl. the tool-dependent localization replay cannot use); `have(bin)` presence-only gating and no version capture; `ref_assemble` collapsing rejection/environmental outcomes (`tests/conformance.rs:76-88`); fixed fuzzer seeds; curriculum ~646 `.asm` files, ~94 MB working tree in the public `code198x/code-samples` repo (depth-1 checkout ~15 MB); the hardcoded `../../../../Code198x` corpus locator (`tests/curriculum.rs:25-29`); CI jobs and the `--include-ignored` slot; release flow (release-plz → tag → cargo-dist).
- Closed issue #36 — the acceptance-boundary divergence grounding the divergence-tag vocabulary (#33 reclassified in review: an internal warning-channel feature, not a reference divergence).
- External prior art: wpt.fyi, Certificate Transparency, reproducible-builds rebuilder networks; asl's rolling "1.42 Beta [Bld N]" versioning as the version-identity forcing case.
