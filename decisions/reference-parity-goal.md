# Decision: full reference parity comes before any superset

**Status:** Active. Binding for Asm198x (accepted 2026-08-26). Records the goal
the parity issues ([#272](https://github.com/asm198x/asm198x/issues/272)–[#277](https://github.com/asm198x/asm198x/issues/277))
have cited since they were opened, and the terms on which a word may be left
out. Applies the house rule in [`syntax-stance.md`](syntax-stance.md).

**Date:** 2026-08-26.

## The decision

**Every word a reference assembler accepts on the target we claim to match, we
take — before we add anything it does not have.**

Source compatibility is the identity claim: real source for a machine should
assemble unchanged and produce identical bytes. A word we do not take is source
somebody already has that we refuse, and no amount of syntax we invented instead
makes up for it.

## Why this needed writing down

It did not exist. Six parity issues have cited
`decisions/asm198x-reference-parity-goal` since they were opened; there is no
such file, and never was. The goal lived in the issue bodies, in the ledger's
prose, and in working memory — which is exactly the shape of a rule that drifts
without anyone deciding to change it.

## How it is measured

`cargo xtask surface` harvests each installed reference's vocabulary and reports
what it accepts and we do not. Which version of each is being measured is
[`reference-versions.md`](reference-versions.md)'s, and `cargo xtask versions
--check` keeps the prose honest about it.

Two things the number is not. It is **surface, not effort**: a family of dotted
spellings is often one rule, and one line can be a week's work or an afternoon's.
And it excludes **words the reference itself refuses on our target** — those
measure a wider target rather than a gap here, and are declared as
`RefusedByReference` with the rule quoted in the reference's own words.

## What counts as a legitimate deferral

A word may be left out only for a reason that is written down and does not
expire on its own. Three qualify:

1. **It writes a different artifact.** Governed by
   [`multi-artifact-output.md`](multi-artifact-output.md).
2. **It needs a surface we have deliberately not built**, and the entry says
   which surface. Per-dialect conditional adoption is the worked example:
   [`conditional-assembly-framework.md`](conditional-assembly-framework.md)
   holds that a dialect has no conditionals until it adopts them, which is why
   pasmo does not answer sjasmplus's `IFDEF` just because they share a walk.
3. **It asks for behaviour asm198x does not implement**, in which case it is
   refused *by name where it is used*, naming the gap as ours. ACME's `!cpu`
   ([#302](https://github.com/asm198x/asm198x/issues/302)) and lwasm's eight
   refused pragmas are the pattern.

What does not qualify: "it looked hard", "nobody has asked", or a category
marker with no prose behind it. **A deferral with no record is a backlog item
wearing a decision's clothes**, and the four in the register below were exactly
that until this record existed.

## The deferrals as they stand

Recorded here so there is one place to look. A word not listed is *outstanding*,
not deferred — nobody has examined it yet, which is a different thing.

| Words | Dialect | Basis |
|---|---|---|
| `os9`, `mod`, `emod` | lwasm | [`multi-artifact-output.md`](multi-artifact-output.md) — accepted, **not yet implemented** |
| `output` | vasm | the same |
| the `save*` family | sjasmplus | the same |
| `setstr`, `ifstr`, `includestr` | lwasm | string symbols are their own surface, unbuilt |
| `.condes` | ca65 | builds an ld65 constructor table from linker-config features our fixed NROM layout does not declare |
| `\#`, `_NARG`, `\@` | rgbasm | macro-expansion surface, unbuilt |
| eight pragma spellings | lwasm | rule 3 — refused by name; see the `PRAGMAS` table |
| every processor but `6502` | ACME `!cpu`, ca65 `.setcpu`/`.pNN` | rule 3 — [#302](https://github.com/asm198x/asm198x/issues/302) |

Two open questions are **not** deferrals, because nothing has decided them:

- **`dtb`/`dts`** (lwasm) assemble the current date and time. Implementing them
  makes a build's output depend on the clock, which cuts against reproducibility
  everywhere else here. Undecided —
  [#314](https://github.com/asm198x/asm198x/issues/314).
- **`ifp1`/`ifp2`** (lwasm) warn "Not supported IFP1" and take the true branch.
  The branch is a line; the warning has nowhere to go, because `CondEval::eval`
  folds a head from `&self`. Plumbing, not a decision —
  [#315](https://github.com/asm198x/asm198x/issues/315).

The first entry of the register is a third kind again: `multi-artifact-output.md`
is accepted and **unimplemented**, so five words across three dialects are
deferred on a decision nobody has built yet. That is legitimate under rule 1 and
should not be a standing state —
[#316](https://github.com/asm198x/asm198x/issues/316) tracks it.

## Where the count stands

Measured 2026-08-26 against the installed references:

| Reference | Outstanding | Of |
|---|---|---|
| acme 0.97 | 0 | 74 |
| lwasm 4.25 | 10 | 257 |
| sjasmplus 1.21.0 | 78 | 264 |
| vasm 2.0b | 87 | 183 |
| ca65 V2.18 | 113 | 225 |
| rgbasm 1.0.3 | 114 | 306 |

ACME is complete. lwasm's remaining ten are the deferrals and open questions
above and nothing else.

## Drift triggers

Re-read this record when any of these appear:

- **"we could add a flag for that"** — a superset before parity is closed on
  that dialect. Take the reference's spelling first.
- **"the reference is wrong here"** — [`syntax-stance.md`](syntax-stance.md)
  binds: reproduce questionable behaviour rather than out-converging it. Being
  stricter than the tool we claim compatibility with is still a mismatch.
- **"this one is niche"** — niche is not a basis. Either it meets one of the
  three tests above and joins the register, or it stays outstanding.
- **"it is deferred"** — check the register. If it is not here with a basis, it
  is not deferred.
