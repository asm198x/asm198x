> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: Docs Adoption Narrative - Plan
type: docs
date: 2026-08-20
topic: docs-adoption-narrative
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: reading-the-book
execution: prose
---

# Docs Adoption Narrative - Plan

## Goal Capsule

- **Objective:** Split the documentation by what a reader is doing. The site takes the deciding path — why adopt, what it gives you, why to trust it, what it costs to move — and the book takes the using path. Then fill the pages neither surface has. Reference and per-dialect fidelity are committed elsewhere and are not reopened.
- **Product authority:** Steve Hill, who placed the split: most of this belongs on the website, not the book. Prompted by reading the four hand-written book pages on 2026-08-20 and finding they document the command line and the opcode tables with nothing in between.
- **Open blockers:** None; all four questions raised on 2026-08-20 are settled in the record. One item comes first regardless: the site's parity figures are already published and already able to rot, so they are step 2 rather than a page in the queue. The diagnostics claim, originally first, was corrected on 2026-08-20 in `88584ad`.

---

## What is already decided, and not reopened here

This plan sits downstream of three records. It adds pages; it does not restate their scope or argue with it.

| Record | Covers | Status |
|---|---|---|
| [`decisions/v1-scope.md`](../../decisions/v1-scope.md) item 6 | Installers, a CLI reference, per-dialect directive matrices generated from the conformance corpus, the docs-site v1 core (R1/R2/R3) | On the 1.0 bar, ordered as step 6 |
| [`2026-07-04-004-feat-docs-site-plan.md`](2026-07-04-004-feat-docs-site-plan.md) | R1 instruction references, R2 samples assembled in CI, R3 mdBook + Vale | R1/R2 landed; the generated-slot machinery exists |
| [`2026-08-18-001-feat-declared-directive-surface-plan.md`](2026-08-18-001-feat-declared-directive-surface-plan.md) | The declared surface the dialect matrices generate from | Implementation-ready |
| [`decisions/one-documentation-surface.md`](../../decisions/one-documentation-surface.md) | One surface: source stays in this repo, the site renders it, mdBook withdrawn | Active, 2026-08-20 |

Explicitly out of v1 per `v1-scope.md`, and therefore out of this plan: diagnostic explain-pages, the conformance ledger, cycle columns.

**The 24 dialect pages are not in this plan.** They are generated, they are on the bar, and they are downstream of the verdict corpus. Hand-writing them is the failure the docs-site plan exists to prevent.

---

## Product Contract

### Problem Frame

There are two surfaces and the material is on the wrong ones.

**The site is one page.** `src/pages/index.astro` is the whole of asm198x.github.io. It does positioning well at hero depth — source-compatible with the dialects you already use, validated byte-for-byte, a front-door card per dialect with concrete parity figures. Then it stops. A reader the hero convinces has nowhere to go but the reference manual.

**The book has depth and no reason to start reading it.** 420 lines of hand-written prose — introduction, first program, CLI reference, dialect overview — under 21 generated instruction pages. It never mentions `fmt`, `disasm`, the debug sidecar, JSON diagnostics or the output containers, which are the five capabilities that answer "why switch". A reference manual is the wrong place for that argument regardless: nobody undecided reads one.

The split that follows:

- **Site — deciding.** Why this over what you have, what it gives you, why to trust it, what adopting costs, how to install.
- **Book — using.** Task workflows, reference, generated material, for someone who has already decided.

The book's remaining gaps are then genuinely about *using*: include paths appear once as a row in an options table, macros nowhere, multi-file projects nowhere, and the ca65 and vasm linked paths are referenced twice and explained nowhere.

### The drift the site has already reintroduced

The landing page carries hard figures — "all 32 NES units match ca65 + ld65", "32/32 hunk-exe parity with vasmm68k_mot", "the whole 80-unit C64 curriculum". Their source is asm198x's `README.md` and `decisions/why-not-llvm.md`, hand-copied into an `.astro` file in a different repo.

This is the failure the docs-site plan already recorded when the CLI reference drifted: *"R1's premise is that generated data cannot drift, and that only holds if the generator and the page fail the same build; across a repo boundary they do not."* Moving more narrative to the site walks further into it.

It is fixable rather than fatal, because the site build already reaches across. `.github/workflows/pages.yml` checks out `asm198x/asm198x` to build the book, at the released tag rather than main, and the dependency points site → code so it needs no credentials. Generated fragments can travel the same path. **The rule for the site is the same as for the book: a number a reader could check must be generated by asm198x, never authored on the site.**

### The JSON example is unchecked

`cli.md` now shows a real `--message-format=json` payload, because an integrator needs to see the shape and prose cannot supply it. It is hand-written and nothing verifies it.

`crates/asm198x/tests/book_samples.rs` assembles every `<!-- sample: ... -->` block and asserts refusals with their message, so source samples cannot rot. It has no mode for an *output* block. Growing one — a marker that runs the binary on a named sample and compares the emitted JSON — closes the last hand-authored, drift-capable thing on the page. Small, and it belongs with the workflow pages, since an editor-integration page will want the same treatment.

### Settled 2026-08-20: the diagnostic code claim

`cli.md` told the reader diagnostics carry "a stable code". `Code` in `crates/asm198x/src/contract.rs` has one variant, `AssemblyError`, so every diagnostic returned the same value and nothing could be keyed on it. The mechanism is right — `#[non_exhaustive]`, codes assigned as sites are classified, never renumbered — but a reader who knows rustc reads "stable code" as a taxonomy and finds one value.

Resolved by describing what is there rather than withdrawing the mention, since the additive contract is real and an integrator should know about it. The page now states plainly that every diagnostic carries `AssemblyError` today and that codes arrive additively. It also gained the JSON payload and four field notes taken from `span.rs` that were not documented anywhere: `file` is always `0` while v1 is single-file, `col: 0` means line-granular, `expansion_frames` is innermost-first and empty until macros land, and `fix` may carry a concrete `replacement`.

### Settled 2026-08-20: the quickstart

The site carries a quickstart. The apparent conflict with R2 — every sample assembled by the real binary, in CI that lives in this repo — came from conflating who owns the page with who owns the sample.

**One surface dissolves it.** Under [`one-documentation-surface.md`](../../decisions/one-documentation-surface.md) the quickstart's source lives in `docs/` like every other page, so `book_samples.rs` assembles its sample on every `cargo test` with no fragment mechanism, no cross-repo extraction, and no second source of truth. The earlier draft of this plan designed a generated-fragment pipe to carry one sample across the split; the split is gone and so is the pipe.

The reasoning that survives: the quickstart is the highest-stakes sample on the estate. A stale book page irritates someone already invested; a broken quickstart loses a new user in their first minute. It needs R2's guarantee more than a reference page does, which is why an unverifiable copy was never the answer.

The released-tag rule still applies and still helps. A reader following a quickstart has just installed the release, and `pages.yml` already selects the newest tag carrying the docs rather than building from `main`.

### Settled 2026-08-20: worked examples, and what they cover

*A first program* widens, and the same treatment applies to `fmt` and `disasm`.

**What this is not.** The existing suites are the safety net and stay it: `fmt.rs` carries 106 tests, and `conformance.rs` round-trips the disassembler by reassembling its output with the *reference* assembler. Samples are not where correctness is established, and the plan should not imply otherwise.

**What it does add**, in order of value:

1. **`fmt` and `disasm` have no worked example anywhere.** Both are thoroughly tested and never shown. A reader cannot see what the formatter does to their source, or what a disassembly listing looks like, without running it. This also exposes a hole in the structure below: there was a *Formatting* page and no disassembly page.
2. **The book demonstrates 2 dialects out of 24.** acme and pasmo. Every other front-end is documented only as a table row.
3. **Whole programs exercise interactions the sweeps do not** — labels, directives, origin and layout resolving against each other, rather than one instruction at a time.

**Where the samples go.** Not one page with 24 programs. *A first program* stays curated at the five front doors the site already names — acme/C64, ca65/NES, pasmo/Spectrum, vasm/Amiga, lwasm/CoCo. The remaining dialects get a worked program on their own generated dialect page, beside the matrix: a hand-written sample next to a generated block is the pattern `dialects.md` already uses. That keeps each example where someone looking for that dialect will land.

**The checker has to grow three modes**, and they are one piece of work with the unchecked JSON block above:

| Mode | Asserts |
|---|---|
| `fmt` | formatting the sample is idempotent, and the result reassembles to the same bytes |
| `disasm` | disassembling the sample's output round-trips |
| `output` | the emitted text or JSON shown on the page is what the binary emits |

Without those, a widened sample set is more unverified prose, not less.

---

### Settled 2026-08-20: version, changelog and release notes

Every documentation surface states which version it describes, and links to what changed.

**Three artefacts, three jobs.** They are not duplicates and all three are wanted:

| Artefact | Answers | Source |
|---|---|---|
| Version | "which binary does this page describe?" | The tag `pages.yml` already selects; the crate version for the book |
| Release notes (GitHub) | "what is in the version I am about to install?" | The release body — changelog entry plus cargo-dist install commands |
| `crates/asm198x/CHANGELOG.md` | "what changed across versions, should I upgrade?" | release-plz, full history |

**Nothing is typed.** The site workflow already computes the version it is publishing — `steps.release.outputs.ref`, e.g. `asm198x-v0.0.14` — and discards it. Once the site builds every page from the checkout, that value is available to all of them. The crate version is also available to `cargo xtask docs`, so a generated block guarded by `docs --check` covers any page that wants it inline.

**The label has to be honest.** `pages.yml` selects the newest tag that *carries the book*, not the newest release, and it falls back to main when no tag qualifies. Those genuinely differ — every release before the book landed lacked `docs/book/book.toml`, which the workflow calls a standing condition rather than a one-off. So the surface reads "documenting 0.0.14", never "latest", and renders the fallback state plainly when it fires. That converts a `::notice::` nobody reads into something visible on the page.

**Reference pages need this most.** A stale landing page irritates; a stale reference page misleads, because nothing on it says which binary it describes. Every page carries the version, and the reference pages are the reason.

**Install commands stay unpinned.** The release body carries version-pinned URLs (`.../download/asm198x-v0.0.14/...`) because it documents one release. `/install` and `cli.md` keep `releases/latest/download/...`, so the displayed version and the install command answer different questions and neither goes stale.

---

---

## Positioning

The site's lede is already right. What it lacks is anything beneath it.

The argument, stated as what the user gets: **keep your source and your syntax; gain the tooling that never existed for it.** Every dialect asm198x reads is somebody else's assembler — ACME, ca65, pasmo, sjasmplus, lwasm, vasm, rgbasm. Adoption costs nothing because there is nothing to port. What arrives with it did not exist for those tools:

| Capability | Verified in | Why it matters to someone choosing |
|---|---|---|
| One binary, 20 CPUs, 24 dialects | dialect table, instruction index | One install for every machine you target, not one toolchain per machine |
| `fmt` — canonical layout, idempotent, reassembles identically | `cli.md` | Retro assemblers have no formatter; this makes source reviewable in a PR |
| `disasm` in the same binary, round-trip verified | `cli.md`, introduction | Read a binary you did not build without a second tool |
| Debug198x sidecar consumed by Emu198x | `cli.md`, frozen at v1 | Source-level debugging in an emulator |
| `--message-format=json` | `cli.md` | Editors and build scripts get structured results |
| `--prg`, `--sna`, `--exe` | `cli.md` | The loader format comes out of the assembler, not a packaging step |
| No install ceremony | `cli.md` | One binary and a brew formula, against building a suite from source |

The sceptic's question — *why trust a new assembler with a project that currently builds?* — has a real answer that reads as self-praise in the book today:

- Differential suites assemble the same source with the real tools and compare bytes.
- The verdict corpus records what those tools produced, keyed on their own version strings, and CI replays it on machines with none of them installed.
- Known differences are tracked divergences with join ids (`Outcome::Divergence` in `crates/verdict-corpus`), and the build fails if one silently starts matching again.

The last point is the unusual one and it belongs on the site: a published list of where we differ from the tool you use today is a stronger argument than a claim of parity, because a sceptic can read it instead of believing it.

### Comparison, and the two kinds of claim

`/compare` is in scope (decided 2026-08-20). The verification cost it looked like carrying is mostly absorbed already, because the corpus records what it measured against.

The page has to keep two kinds of claim apart, and lean on the first:

**Byte-level behaviour — generated, version-stamped, cannot rot.** Every verdict carries an `Arbiter { tool, identity, digest }`, where `identity` is the reference tool's own version self-report. Eight tools are recorded across `crates/asm198x/tests/verdicts/*.ndjson` today:

```
acme          This is ACME, release 0.97 ("Zem"), 28 June 2020
asl           Macro Assembler 1.42 Beta [Bld 309]
ca65          ca65 V2.18 - N/A
lwasm         lwasm from lwtools 4.25
pasmo         PasmoNext v0.1.3 (PC) (C) 2004-2005 Julian Albo
rgbasm        rgbasm v1.0.3
sjasmplus     SjASMPlus Z80 Cross-Assembler v1.21.0
vasmm68k_mot  vasm 2.0b (c) in 2002-2025 Volker Barthelmann
```

The "measured against" table generates from that. It stays true when a reference tool ships, because it states the version we measured, not the version that exists.

**Feature claims — hand-written, and the standing liability.** "asl has no formatter" is not something the corpus knows; it is a claim about software we do not control, and it needs checking against that tool's current documentation. Keep these few, frame them as what asm198x provides rather than what others lack, and never write one from memory.

---

## Proposed structure

One surface, all source in this repo's `docs/`, rendered by the site. Additions marked **new**; everything else exists or is already planned.

```
/                          landing — exists; parity figures become generated
/why                       new — the capability argument and the trust story
/compare                   new — against the toolchain you have now; version table generated
/install                   new — lifted out of cli.md, which is a reference
/quickstart                new — install, one program, what to read next
/migrate                   new — moving an existing ACME/pasmo/vasm project across
/releases                  new — the documented version, links to GitHub releases and CHANGELOG.md
/divergences               new — generated from the verdict corpus

/guide/multi-file          new — -I, include search order, project layout
/guide/macros              new
/guide/linking             new — the ca65 and vasm linked paths, currently unexplained
/guide/containers          new — --prg/--sna/--exe, lifted from the options table
/guide/debugging           new — the Debug198x sidecar with Emu198x, end to end
/guide/integration         new — --message-format=json for editors and build scripts
/guide/formatting          new — fmt in a workflow, not as a flag
/guide/reading-a-binary    new — disasm, worked; no page exists today
/guide/first-program       exists — widen to the five front doors

/reference/cli             exists
/reference/dialects        exists — overview
/reference/dialects/<n>    planned, generated (not this plan)
/reference/instructions    exists, generated, 21 pages

(every page)               the version it documents
```

**The 24 dialect pages are not in this plan.** They are generated, on the 1.0 bar, and downstream of the verdict corpus. Hand-writing them is the failure the docs-site plan exists to prevent.

The introduction shrinks to orientation and stops trying to sell, because `/why` now does that.

### What generates and what does not

- **Landing-page parity figures** — must move out of the `.astro` file and become generated. Highest-risk item here: they are checkable, wrong-able, and already published.
- **Tracked divergences** — generatable today; `Outcome::Divergence { divergence, hex }` carries a join id, so the list and its in-repo half are already structured.
- **Dialect matrices** — generatable once `2026-08-18-001` lands. Not this plan.
- **The `/compare` version table** — generatable today from `Arbiter.identity` in the verdict corpus.
- **The documented version** — from `steps.release.outputs.ref`, or `cargo xtask docs` for an inline block.
- **Diagnostic codes** — not generatable into anything useful while one variant exists.
- **Everything under `/guide/`** — hand-written task narratives with no derivable source. R2 keeps their assembly samples honest today, and since step 9 their `fmt`, `disasm` and output blocks are checked too.

---

## Sequencing

1. ~~Settle the diagnostics claim and correct `cli.md`.~~ Done 2026-08-20.
2. **Render the docs from the site, and withdraw mdBook.** The enabling step: Astro builds every page from the existing `_asm198x/` checkout, and the three things mdBook provided are replaced — the `create-missing = false` dead-link gate, search across the reference, and nav from `SUMMARY.md`.
   - *Done 2026-08-21, assembler half.* `cargo xtask docs` generates `docs/book/nav.json` from SUMMARY.md; `--check` fails on a listed chapter with no file, a page listed nowhere, or a stale nav. The mdBook CI step and `book.toml` are gone. The site does not parse SUMMARY.md — the nav is generated here and read there, for the reason the decision record gives.
   - *Done 2026-08-21, site half.* Astro renders all 25 pages from the checkout, the sidebar draws the generated nav, and `/docs/` is a contents page built from the same nav. mdBook is gone from both workflows. Needed a hand-authored release (v0.0.18) to ship: release-plz opens a PR only when a file inside a package directory changes, and `nav.json` lives in `docs/book/`.
   - *Outstanding:* **search**. Deferred 2026-08-21 — wanted, but below the remaining content pages in priority.
3. **Landing-page parity figures, the documented version, and `/releases`.** All three are the same shape of work — a checkable value moving out of hand-written markup into something generated here and consumed there. The parity figures come first within this step, since they are already published and already able to rot.
   - *Done 2026-08-21:* the parity figures (`cargo xtask parity`, `crates/asm198x/tests/verdicts/parity.json`, gated in CI) and the documented version. Taken out of order, ahead of step 2, because the published figures were wrong — the page claimed 80 C64 units, 32 NES and 20 Spectrum against a corpus holding 138, 51 and 161.
   - *Done 2026-08-21:* `/releases`, rendered from `crates/asm198x/CHANGELOG.md` at the released tag, with the documented version marked. **Step 3 complete.**
4. **`/quickstart` and `/install`.** A quickstart without install instructions is incomplete, so they land together: the quickstart carries the one-liner, `/install` carries the platforms, archives and troubleshooting lifted out of `cli.md`.
5. **`/why`.** Highest leverage per page, no new machinery, and it is what the sibling sites link to.
6. **`/migrate`.** The entry point for the user the tool is aimed at.
7. **`/divergences`**, generated from the verdict corpus. Makes the trust argument checkable rather than asserted.
8. **Shrink the introduction** to orientation, once `/why` carries the pitch.
   - *Done 2026-08-21.* It was making the pitch and the trust argument both, so
     a reader met the differential suites, the corpus and the round trips twice
     before reaching a command. It points instead: four doors by what the reader
     came for. The note on how the book is built stays — it is orientation, not
     argument.
   - *Also done, and not in the step:* `/why`'s evidence figures are generated
     (`xtask evidence --markdown`). Two of them had already drifted — 5,637
     verdicts against a corpus holding 5,625 live ones, and "nine differences"
     conflating six tracked differences with their nine recorded cases. Taken
     now for step 3's reason: a page whose whole argument is *you can check
     rather than believe* cannot be quietly wrong about its own evidence.
9. **Extend `book_samples.rs`** with `fmt`, `disasm` and `output` modes. Everything below depends on it, and it retires the unchecked JSON block on `cli.md`.
   - *Done 2026-08-21.* Four modes rather than three: `output` (the human
     diagnostic, on stderr), `json`, `fmt` and `disasm`. A sample is named after
     the file it is written as, and a block directly below it claims what the
     binary prints. Each runs in the sample's own directory, so a path in the
     output is the name the page shows.
   - *A mode may carry arguments* (`disasm --org 0xc000`), so a page documenting
     a flag can show what the flag does.
   - *`json` compares the parsed document*, not the text, so `cli.md` shows the
     payload indented while the binary emits one line.
   - *The JSON block was wrong twice over:* column 13 where the assembler
     reports 15, and no source sample at all — a payload with nothing to be the
     output of. Both fixed.
   - *`fmt` and `disasm` gained the worked examples their reference sections
     described and never showed.* `fmt` moves a label onto its own line, which
     "labels at column 0, operations indented" does not say.
   - *A mode nothing uses is a checker nobody is checked by,* so the suite fails
     if any of the four is unexercised.
10. **The `/guide/` pages** — multi-file and linking first, since those are referenced today and explained nowhere, then formatting and reading-a-binary, which are the first worked examples `fmt` and `disasm` have ever had.
    - *Multi-file done 2026-08-21.* The fact a reader needs was worse than
      undocumented, it was unstatable: a relative include anchors differently in
      each dialect family, and the facts lived in `pub(crate)` consts.
      `asm198x::includes::resolution()` is the accessor a generator reads —
      derived from each dialect's `WalkSemantics` const where there is one,
      stated and test-held for acme and the Z80 pair where there is not.
    - *Linking done 2026-08-21.* The ca65 NROM link and vasm's hunk executable,
      with the relocatable-hunk consequence that changes how you write code
      (PC-relative within a section only), and the distinction from `--prg` and
      `--sna`, which wrap rather than build.
    - *Deliberately no counts on the linking page.* The decision log's "all 32
      buildable NES units" has moved on — the corpus records 51 NES sources
      today. Restating a figure by hand on the day two of them were fixed would
      have been the joke writing itself; the page points at `/why`, which counts.
    - *Formatting done 2026-08-21.* What `fmt` changes, what it deliberately
      does not touch, the two properties that make it safe on a project you have
      not read (idempotent, byte-identical), and the two-step in-place edit and
      CI check — because it writes to stdout and never rewrites in place.
    - *Reading a binary done 2026-08-21.* `--org` shown as two disassemblies of
      one file, so the moved `BNE` target makes the relative-displacement point
      by itself; the data fallback as the tell that a boundary is wrong rather
      than as a failure; and the reassembles-to-the-same-bytes round trip.
    - *Step 10 complete.* Both pages' listings are checked output blocks, so
      every line of disassembly and formatting shown is what the binary printed.
    - *It found `/why` wrong twice more.* The capability table claimed `fmt`
      covered seven CPU families and `disasm` two; both cover every dialect.
      Understating drift is the expensive kind — an overstated claim is
      corrected by a reader who tries it. Fixed and made a test.
11. **Widen `/guide/first-program`** to the five front doors, with `fmt` and `disasm` examples alongside.
    - *Done 2026-08-21.* NES (ca65), Amiga (vasm) and CoCo (lwasm) join the C64
      and Spectrum programs, all five assembled by CI. It also settles the
      forward reference the quickstart already made to "the same five front
      doors".
    - *Each carries the thing its machine makes you know*, since a program that
      only assembles teaches nothing: the NES pair of `bit $2002` waits that
      code works without on some emulators and not on hardware; the Amiga
      program not taking the machine over, so the OS repaints whatever it writes;
      the 6809's `sta ,x+` as the C64 loop with the bookkeeping folded into the
      addressing mode; and the CoCo origin as a decision about the target rather
      than about the assembler.
    - *`fmt` and `disasm` are linked rather than repeated*, since steps 9 and 10
      gave both their own worked pages.
12. **`/compare`.** The generated version table is available today; schedule it against how much hand-written feature comparison it needs.
    - *Done 2026-08-21, and the answer to "how much hand-written comparison" is
      none.* The page says outright that it makes no claim about what any other
      assembler can do, and why: a feature table is a claim about software we do
      not control and do not track, so it goes wrong on their release schedule
      without anyone touching it — the failure the rest of these pages exist to
      avoid. Writing one from memory would be worse.
    - *What it offers instead* is the comparison we can stand behind: the tools
      we measured against with each one's own version self-report
      (`xtask compare --markdown`, from `Arbiter.identity`), what we produce, and
      every place we knowingly differ.
    - *The version column says "what we measured against", not "current".* That
      is what lets the table stay true when a reference tool ships — it records
      an observation rather than a claim about the world.
    - *A cross-check fell out of it:* the per-tool verdict counts sum to 5,625,
      which is the figure `/why` reports from a different query over the same
      corpus.

**Sequencing complete.** Steps 1–12 are done. Search remains deferred (see
Outstanding Questions), and the landing page's own parity figures moved to
generated in step 3.

Per-dialect worked programs are not sequenced here. They land on the generated dialect pages when those exist, which is the v1 bar's step 6.

Steps 3–12 are independent of that bar and can run beside it. None block on the declared directive surface.

## Outstanding Questions

**Search — done 2026-08-22, as a generated index.** The decision record required
it — "21 generated instruction pages today, 45 once the dialect matrices land.
Reference without search is worse than no reference" — and it was the one thing
mdBook provided with no replacement designed.

**The generated index, not Pagefind**, and the reference's own shape decided it
rather than the dependency question alone. Every mnemonic on the twenty-one CPU
pages is already a heading with its description underneath, so indexing headings
indexes every mnemonic for free, with the text that explains it. The prose pages'
headings are the questions they answer, so someone hunting the include rules
matches "Where a relative include is looked for" — which full-text ranking would
bury under every page that says "include". It needs no build dependency, and it
is 1,177 entries, 16K over the wire, against 400K of markdown most of which is
instruction tables nobody searches by opcode byte.

Pagefind indexes built output and could sit alongside this later if full text is
wanted; nothing here forecloses it.

`cargo xtask docs` writes `docs/book/search.json` and `--check` fails when a
heading has been added without the index catching up. The site reads it exactly
as it reads the nav.
