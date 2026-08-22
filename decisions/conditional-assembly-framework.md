# Decision: conditional assembly is a shared evaluator, adopted per-dialect on demand

**Status:** Active. Binding for how dialects gain conditional-assembly support.

**Date:** 2026-07-05.

## The decision

Conditional assembly (`!if`/`!ifdef`/`!ifndef … { } else { }`, `IF … ELSE …
ENDIF`, and kin) is modelled as a node in the shared AST — [`Item::Conditional`
in `crates/asm198x/src/ast.rs`](../crates/asm198x/src/ast.rs) — and **evaluated**
by a shared, trait-parameterised walk, not a per-dialect preprocessor. Only ACME
uses it today; **other dialects adopt it on demand**, when a real program needs
it, not speculatively.

The reusable core is deliberately drawn at one seam:

- **Shared and done:** `ast::CondEval` (the trait) + `ast::evaluate` (the walk).
  The walk — prune the untaken branch, thread the live/skipped flag so a skipped
  branch defines nothing — is dialect-agnostic. The two dialect-specific parts
  are `eval(head)` (test a condition against the environment) and `lower(node)`
  (lower one live line, updating the `equ`/`=`/`!set` environment a later
  condition tests). ACME implements this as `AcmeEval`; its assembler is
  `ast::evaluate(&mut AcmeEval{…}, …)`.
- **Per-dialect, built with the first consumer:** recognising the dialect's
  conditional *syntax* during parse (into `Item::Conditional`), and rendering it
  back in `emit`. These entangle with each dialect's own line handling
  (comment char, label rules, directives), so a shared "keyword-block parser"
  would be callbacks-for-everything — more indirection than it saves.

## Why this seam, and why on-demand

- **The evaluator is the hard part; it generalised cleanly.** Correct branch
  pruning and environment threading is where the bugs live, and it is identical
  for every dialect. Extracting it once (proven byte-identical on ACME across all
  142 buildable C64 curriculum files) means any future dialect skips that risk.
- **The parse/emit are thin and dialect-shaped; they did not.** Forcing them into
  a shared helper *before a second consumer exists* would be guessing at the
  callback shape — an abstraction for a hypothetical future. The right time to
  build them is against the first real keyword dialect, when its shape is known.
- **No demand outside ACME.** A scan of the curriculum found conditional-assembly
  usage only in the C64/ACME code (the `SCREENSHOT_MODE`/`VIDEO_MODE` guards) —
  none in the 231 Spectrum (Z80), 84 NES, or 193 Amiga files. Real-world Z80/6809
  source *does* use conditionals (a genuine source-compat gap per
  [`syntax-stance.md`](syntax-stance.md)), so this is expected to be needed
  later — but curriculum-first priority means it waits for a concrete driver.

## How a dialect adopts conditionals (the on-demand recipe)

When a real program hits the gap (sjasmplus for the Spectrum is the likely first;
asl-family `IF … ENDIF` and lwasm/rgbasm follow the same shape):

1. **Implement `CondEval`** over an evaluator that owns the dialect's environment
   — `eval(head, line)` parses/tests the dialect's condition syntax; `lower(node)`
   lowers one line (re-parsing from the node's source with the current
   environment, as ACME does, when per-line parsing is environment-dependent).
   ~40 lines.
2. **Recognise the conditional syntax in the dialect's parse**, building
   `Item::Conditional { head, then_body, else_body, inline }`. Keyword styles
   (`IF … ENDIF`) are *simpler* to parse than ACME's braces — line-oriented, no
   brace matching.
3. **Route the dialect's assembler through `ast::evaluate`** (as ACME's `parse`
   does) instead of the plain `ast::lower`.
4. **Add a style branch in `ast::emit`** to render the dialect's delimiters
   (`head … ENDIF`) instead of ACME's braces, for the formatter.

## Dated notes

- **2026-07-07 — sjasmplus is the second `CondEval` consumer** (language-surface
  U8, plan `docs/plans/2026-07-04-001-feat-language-surface-plan.md`). The
  adoption followed the recipe above verbatim: `SjasmEval` over the `DEFINE`
  table + `equ` consts, line-oriented `IF`/`IFDEF`/`IFNDEF`/`ELSE`/`ENDIF`
  parsed into `Item::Conditional`, assembly routed through `ast::evaluate`, and
  the keyword style branch in `ast::emit` built now that its first consumer
  exists (`CondStyle::{Brace, Keyword}` on the node). The driver is real-world
  Z80 source compatibility (the `syntax-stance` gap this record anticipated),
  not curriculum demand. Includes resolve inside the walk, so a guarded
  include in an untaken branch never loads. Further adopters remain
  demand-gated.
- **2026-08-22 — the gate admits external source compatibility as a driver,
  and pasmo is the third adopter.** The demand test above was written as "a
  real program or curriculum unit that needs it", and the curriculum half of
  that is now known to be the wrong instrument. [#93] made the argument for
  macros and it holds here unchanged: *we wrote the curriculum, and it avoids
  what the toolchain does not support*, so a scan of our own samples can never
  signal demand for a feature they were authored around. The driver is
  real-world source compatibility, which is the same driver the sjasmplus
  adoption above already ran on.

  **What this does not do is open the gate for everyone.** ca65, rgbasm and
  vasm each have conditionals in their references and none in us, and they stay
  gated. pasmo goes first as the proving case: a third `CondEval` consumer
  tests whether the recipe is a recipe or a coincidence, and it is the
  narrowest surface of the four — measured against pasmo, not read from its
  manual:

  | form | pasmo |
  |---|---|
  | `IF` / `ELSE` / `ENDIF` | yes, and case-insensitive |
  | `IFDEF`, `IFNDEF` | **no** — pasmo has neither |
  | `ELSEIF` | **no** — an error in a live branch |
  | `ENDC` as a closer | **no** |
  | a label on the `IF` line | **an error**, though a label on a `REPT` line is fine |

  That is four fewer forms than sjasmplus, so the adoption is the recipe with
  the smallest possible surface — which is what makes it a test of the recipe
  rather than of pasmo. Repetition (`REPT` … `ENDM`) rides the same pipeline
  and lands with it; see `docs/plans/2026-08-22-002-feat-repetition-plan.md`
  for the reference survey behind both.

  The next adopter is not pre-approved. It needs its own note here.

## Drift triggers

Stop and re-consult if a change would:

- **Build a generic keyword-block parser or a style-aware emit "for later"**,
  with no keyword dialect actually consuming it. Wait for the first real
  consumer — that is the whole point of the seam.
- **Add conditional support to a dialect with no concrete driver.** Adoption is
  demand-gated, and each adopter is named in a dated note above before its
  code is written. Real-world source compatibility counts as a driver (see the
  2026-08-22 note); *curriculum* absence never counts as evidence against one,
  because the curriculum was authored around what the toolchain supports.
- **Read "pasmo is adopted" as "the rest are next".** ca65, rgbasm and vasm are
  still gated, individually.
- **Re-introduce a per-dialect conditional *preprocessor*** (a second parse that
  evaluates conditionals outside the tree), rather than a `CondEval` over the
  shared `Item::Conditional`. ACME's `process_block` was retired for exactly this
  reason (idea 4).

See [`asm198x-and-shared-isa-spec.md`](../../../decisions/asm198x-and-shared-isa-spec.md)
(the AST layer) and the plan `docs/plans/2026-07-04-005-feat-ir-ast-layer-plan.md`
(U6 / idea 4).

[#93]: https://github.com/asm198x/asm198x/issues/93
