# Decision: symbol visibility is a check, and a reference's own refusal is a category

**Status:** Active. Binding for Asm198x (accepted 2026-08-24). Reads with
[`assemble-io-model.md`](assemble-io-model.md), whose principle 3 is why there
is no object file to be visible *to*, and with
[`multi-artifact-output.md`](multi-artifact-output.md), which still owns the
words that write a second file.

**Date:** 2026-08-24.

## The decision

**1. Visibility words are implemented, not ignored.** `export`/`global`/`xdef`
and `import`/`extern`/`xref` reduce, in a fused assemble+link over one
translation unit, to a single rule: *a name a visibility word declares must be
defined in the program.* That is a check, and the dialects that have one get it.

**2. `Category::Ignored` is wrong wherever the word can fail.** It means
"accepted and discarded". ca65 refuses `.export nope`; vasm refuses `xdef nope`.
Discarding those accepts source the reference rejects, which is the divergence
this project exists to avoid — in the direction that is hardest to notice,
because nothing complains.

**3. A word the reference itself refuses gets its own category.**
`Category::RefusedByReference(rule)` carries the reference's rule as its
payload. These words are **covered**, not outstanding: assembling them would be
the divergence, so `xtask surface` counts them as ours.

**4. Words that write a file are not visibility words.** sjasmplus's `EXPORT`
appends `foo: EQU 0x00000000` to an export file. It belongs with `SAVEBIN`,
under `multi-artifact-output.md`, and is not covered here.

**5. Linker-table words stay `KnownUnsupported`.** ca65's `.condes`,
`.constructor`, `.destructor` and `.interruptor` build an ld65 table from
linker-config features our fixed NROM layout does not declare. Those are real
gaps and are counted as such.

**Amended 2026-08-24, after doing it.** vasm's `comm` and `weak` were named
here too, on the assumption that reserving common storage and marking a symbol
weak must show in the output. They do not: in binary output both emit nothing
and vasm asks only that the name be defined, so they are ordinary members of
rule 1. `comm name,size` needed one correction of its own — only the first
operand is a name, and checking the size as one would refuse every correct
`comm`.

## What the references actually do

Probed 2026-08-24 in the output mode Asm198x produces — flat binary, one
translation unit, assembly and linking fused.

| Tool | Words | What it does |
|---|---|---|
| ca65 2.18 + ld65 | `.export .exportzp .import .importzp .global .globalzp` | `.export nope` → ca65 itself: `Exported symbol 'nope' was never defined`. `.import`, and `.global` for a name defined nowhere, → ld65: `Unresolved external` when something references it |
| vasm 2.0b | `xdef xref public global export import` | `xdef nope` → `warning 87: missing definition for symbol <nope>` **and** `error 3007: undefined symbol`. `xref other` buys nothing: `dc.l other` is the same `error 3007` with it or without |
| rgbasm 1.0.3 | `EXPORT` | requires nothing by itself — `EXPORT nope` links to a ROM. Only a *reference* fails, at rgblink |
| lwasm 4.25 | `export extdep extern external import` | refused: `Only supported for object target (EXPORT)` |
| ca65 | `.forceimport` | **unsatisfiable**: defining the name is `already an import`, not defining it is an unresolved external at ld65 *even unreferenced* |
| vasm 2.0b | `xref import nref` | **was unsatisfiable**: `error 86: external symbol <foo> must not be defined` when defined, `error 3007` when not. **2.0f accepts them with the name defined** — see below |
| sjasmplus 1.21.0 | `EXPORT` | writes an export file. Not visibility |
| acme 0.97 | — | has none |

Five tools, four different answers, and one word in the wrong family. There is
no shared rule to lift here, which is why the answer is per-dialect data rather
than a shared implementation.

## Why a category rather than a comment

`KnownUnsupported` and `RefusedByReference` are one line apart in every dispatch
and read almost alike. The difference is who is at fault, and it is the only
part of this a user sees:

```
KnownUnsupported     `os9` is a real directive here and asm198x does not
                     implement it yet — the source is valid and the gap is ours
RefusedByReference   `export` is only supported for an object target, and
                     asm198x emits a binary — lwasm refuses it there too, so
                     this is not a gap in asm198x
```

Telling a reader with valid source that their source is invalid was the failure
`KnownUnsupported` was introduced to fix. Telling a reader that *we* are behind
when the reference refuses their source just as hard is the same failure
pointed the other way: it sends them to wait for a feature that is never
coming, and it inflates the outstanding-word count with words nothing will ever
be done about.

The alternative considered was teaching `xtask surface`'s harvest to classify
lwasm's answer as off-target, alongside the 6309-in-6809-mode case it already
knows. That fixes the count and fixes nothing for the user — the diagnostic
would still claim a gap. It also puts a fact about a dialect in the measuring
tool rather than next to the word.

## The obligation this creates

`Category::RefusedByReference` is the only claim in the declared surface that is
about **the reference** rather than about us. Every other category is checkable
by reading our code; this one is not, and it is the claim most likely to be
wrong, because the manual describes these as ordinary directives.

So it is arbitrated against the tool:
`conformance::lwasm_refuses_its_object_target_words_for_a_binary` runs lwasm on
each of the five words, with an operand and without, and fails if lwasm accepts
one. If lwtools changes its mind, the word stops being a refusal and becomes a
gap, and the test is what says so.

## What actually landed

Three commits, one per dialect family, and the shape held: every word fell into
rule 1, rule 3, or `Ignored`, and the count of each was not what the manuals
suggested.

| | words | where |
|---|---|---|
| must be defined | 7 vasm, 2 ca65 (`.export`, `.exportzp`) | a statement folded against the finished symbol table, so a name defined below the directive counts |
| must not be defined | 2 ca65 (`.import`, `.importzp`) | checked where labels are placed, so the refusal points at the definition as ca65's does |
| no check | 2 ca65 (`.global`, `.globalzp`), 1 ca65 switch (`.autoimport`), 2 vasm (`local`, `idnt`), 1 rgbasm (`EXPORT`) | `Ignored` |
| refused | 5 lwasm, 3 vasm, 1 ca65 | `RefusedByReference` |

Two behaviours were richer than "a check" and are implemented as themselves:
`.export name := expr` defines the name it exports, and the `zp` spellings warn
for a label outside the zero page — but never for a constant.

`RefusedByReference` gained two more dialects the same day it was introduced,
which answers the objection to adding it: it was not a category for one word in
one tool.

## When the reference changes underneath a refusal

`vasm` 2.0f accepts `xref`, `import` and `nref` in binary output when the name
is defined, and answers `error 3007: undefined symbol` when it is not — the same
rule as the seven words beside them. So the three moved out of
`RefusedByReference` and into the ordinary visibility check.

The category was not wrong. It recorded what 2.0b did, and 2.0b refused them from
both sides at once. What changed is the reference, which this
project must expect: a refusal is a fact about a *version*, and it carries that
version's number for exactly this reason.

The category still has users — `lwasm`'s five and `ca65`'s `.forceimport` — so
nothing here is weakened by one tool moving on.

Found because the arbiter container adopted 2.0f (`ops/arbiter`), the first time
a reference version moved under this project deliberately rather than because
somebody upgraded a machine. It will not be the last, and the answer is the same
each time: probe the new version, match what it does now, and say which version
the record describes.

## Consequences

- The refused and implemented words leave the outstanding count together: 512
  words outside our surface, down to 485.
- A dialect that wants `RefusedByReference` declares the reference's rule beside
  the word. `directives::refused_by_reference` renders it, so the wording cannot
  drift across the fourteen dispatch arms.
- The category is dispatched in every dialect, including the twelve that declare
  no such word. An arm nothing reaches is cheaper than a word that slips through
  to be refused as a typo.
