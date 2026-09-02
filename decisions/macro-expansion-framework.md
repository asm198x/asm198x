# Decision: macros are a shared expander with per-dialect grammar, and the formatter never expands

**Status:** Active. Binding for how dialects gain macro support.

**Date:** 2026-08-19.

## The decision

Macro expansion is a **source pre-pass**: definitions are collected and removed,
invocations are replaced by their substituted bodies, and the result goes
through the dialect's ordinary parse. The mechanics live once, in
[`crates/asm198x/src/dialects/macros.rs`](../crates/asm198x/src/dialects/macros.rs);
each dialect supplies its own grammar through the `MacroSyntax` trait.

The seam is drawn where the references agree, and only there.

**Shared, because every reference measured does it the same way:**

- substitution is textual, and happens *before* expression evaluation, so
  `v*2` with `v = 5` assembles as `10`;
- it respects word boundaries, so a parameter `v` leaves the symbol `val` alone;
- it does not reach inside string literals, so `db "v"` emits the letter;
- parameter names match case-sensitively;
- definitions are all collected before anything expands, so a macro may invoke
  one defined later in the file;
- a self-recursive macro is refused with the macro named.

**Per-dialect, through `MacroSyntax`:** the definition header, the closing
keyword, which names are local to an expansion, which lines are declarations
and not code, and what happens when an invocation's argument count does not
match the parameter list.

## Why the grammar is not shared

Because the dialects do not disagree about spelling. They disagree about
meaning.

| | sjasmplus 1.21.0 | PasmoNext v0.1.3 | ca65 V2.18 | lwasm 4.25 | vasm 2.0b | acme 0.97 |
|---|---|---|---|---|---|---|
| header | `MACRO name p1 p2` | `MACRO name, p1` *or* `name MACRO p1` | `.macro name p1, p2` | `name macro` *or* `name macr`, never indented | `name macro` *or* ` macro name` | `!macro name .p1 {` |
| body ends at | `ENDM` | `ENDM` | `.endmacro` | `endm` | `endm` | the matching **`}`** |
| call | `name args` | `name args` | `name args` | `name args` | `name args` | `+name args` |
| parameters | named | named | named | positional `\1` | positional `\1` | named `.p` |
| per-expansion locals | any `.dotted` label | what `LOCAL` declares | what `.local` declares | a `?`/`@` **suffix** | none — `\@` counter | any `.dotted` label |
| a plain label in a body | global, **collides** | global, **collides** | global, **collides** | global, **collides** | global, **collides** | global, **collides** |
| too many arguments | rejected | extras **dropped** | rejected | extras **dropped** | extras **dropped** | **a different macro** |
| too few arguments | rejected | substitutes **empty** | substitutes **empty** | substitutes **empty** | substitutes **empty** | **a different macro** |
| self-recursion | **segfault** (139) | **segfault** (139) | nesting limit | — | — | — |

Six dialects. **Four spellings of a per-expansion local** — a prefix (twice), two
different declarations, and a suffix — plus one dialect that has none and makes
you ask for a counter instead. **Four arity postures**, the last of which is not
a posture at all: acme has no wrong number of arguments, only a name it has
never heard of, because `ldav .v` and `ldav .v, .w` are two macros.

The locals row is the one that settles it. The same source, a macro with
`.spin nop` inside, invoked twice, assembles under sjasmplus and acme and is
rejected by the other four. A house macro system that picked one of those
behaviours would produce wrong bytes — silently, for whichever dialects it
disagreed with — against source its users wrote for a real assembler. That is
the opposite of the identity claim.

So `fit_arguments` has **no default implementation**, and neither does
`locals`. A new dialect must state both, because guessing is exactly the kind
of quiet wrongness the corpus exists to catch.

What *is* shared held up across all six without amendment: substitution stayed
textual, word-bounded, string-safe, and ahead of evaluation every time. Only the
edges of a symbol moved — lwasm counts `?`, `@` and `\` as symbol characters,
which is `is_symbol_char`, not a new mechanism.

### What acme changed, and why it was worth changing

The first five dialects cost a trait method each and left the shared mechanics
alone. acme could not be fitted that way, and two properties of `expand` itself
had to move:

**Collecting a body is the dialect's job.** `collect` now takes the source lines
and an index and hands back a definition, with a default that does exactly what
the code did before — header line, then lines until `is_end`. acme overrides it
to count brace depth at character level, because braces nest inside its bodies
(`!if .v > 3 {` is ordinary), both braces share lines with code, and a `}` inside
a string closes nothing. Delegating rather than parameterising means the five
keyword dialects run the same collector they always did; the risk of the change
sits entirely in the one dialect that needed it.

**A name may carry several definitions.** The table is keyed by name to a *list*,
and `select` picks one. The default takes the last defined, which is what an
overwriting table did. acme picks by argument count.

Both are seams rather than options: nothing chooses between behaviours at run
time, and a dialect that says nothing gets what it had.

This is the v1 scope's *"adopted against real dialect requirements rather than
as a universal macro language"* ([`v1-scope.md`](v1-scope.md)), made structural.

## The formatter must not expand

`asm198x fmt` lays source out. It does not rewrite programs.

Expansion is a source pre-pass, so the obvious wiring — one hook on the shared
parse — makes the formatter inline every invocation and delete every
definition. Over the author's file. This was not hypothetical: sjasmplus shipped
that way from the first macro slice and it was found while adding pasmo's, by
running `fmt` over a probe file and reading the output.

The fix is `z80::Expand`, an explicit mode on the parse entry points. Assembly
paths pass `Expand::Yes`; the `parse_ast` paths that feed the formatter pass
`Expand::No`. The two paths ask for genuinely different parses of the same text,
and the difference is named, not implied.

A regression test in each dialect pins it, and the sjasmplus one asserts both
halves — that formatting gives the macro back, *and* that assembling the same
text still expands — so a future "simplification" that collapses the two parses
fails rather than passes.

## Adoption

- **2026-08-18 — sjasmplus** (#93, #122). `MACRO`/`ENDM`, dot-prefixed locals
  renamed per expansion, arity rejected in both directions. Wired through
  `Z80Syntax::expand_source` so all three entry points inherit it — the first
  attempt hooked `parse` only, which the CLI does not use, so the feature did
  nothing in the actual tool. Expansion frames on `Span` carry `in expansion of
  macro` notes, since an error in generated code otherwise points at an
  invocation and says nothing about why.

- **2026-08-19 — pasmo** (#93). Both header spellings, `LOCAL` declarations, and
  pasmo's no-arity-check posture. The mechanics were extracted to
  `dialects/macros.rs` at this point rather than earlier: one implementation is
  a guess about what generalises, two are evidence. Nine facts recorded in the
  verdict corpus from real pasmo output, and the two `issue-93` gap markers it
  closed were retired with a supersede record — the corpus failed the moment the
  feature worked, which is the marker doing its job.

  **Known gap:** pasmo's formatter still cannot format a file containing macros.
  Its parse is the eager walker, which reads `MACRO` as an instruction and
  rejects it — exactly as it did before macros existed, so nothing regressed,
  but a user who can now assemble macros will reasonably expect to format them.
  sjasmplus formats them because the keyword pipeline keeps unrecognised lines
  verbatim. Pinned by a test so closing it is deliberate.

- **2026-08-19 — the ca65 family** (#93). One grammar in `ca65_flat` serves
  ca65, ca65-816 and ca65-huc6280, because cc65 ships one assembler and the CPU
  is a flag. `.macro`/`.endmacro` with the `.mac` short spellings, `.local`
  declarations, and the reject-too-many/tolerate-too-few posture. Fifteen facts
  recorded against ca65 V2.18; the two `issue-93` markers it closed retired with
  a supersede record.

  The plumbing moved into `dialects/macros.rs` with this adoption — `Expand`,
  the origin map, and the span-replacement helpers — so the flat walk and the
  z80 walk share one vocabulary rather than each growing a copy of the
  `Expand::No` check. The check being the thing worth not re-deriving.

  `FlatWalk` gained an `expand_source` hook defaulting to no-op, so lwasm,
  vasm, rgbasm and asl ride the same walk unchanged until each is probed.

  **Known gap:** as with pasmo, ca65's formatter refuses a file containing
  macros (`unsupported directive .macro`) rather than formatting it. Unchanged
  from before, pinned by a test.

- **2026-08-19 — lwasm and vasm** (#93). The first dialects whose parameters
  have no names: a body refers to `\1`, `\2`, so a macro's arity is decided at
  the call site rather than the definition. Three hooks were added, each for a
  measurement rather than a hypothetical:

  - `argument_names` — build `\1`…`\n` from how many arguments arrived, where a
    named dialect uses the header's list.
  - `is_symbol_char` — lwasm marks a local with a trailing `?` or `@`, and both
    dialects open a parameter with `\`. None of the three is a symbol character
    anywhere else, and substitution cannot see what it cannot tokenise.
  - `expansion_token` — vasm's `\@`, a counter substituted wherever it appears.
    It is not a scoping rule: it can sit mid-name and in several names per body,
    which is why it is a token and not part of `locals`.

  `rename_local` also gained an override, because lwasm's own parser strips the
  `?`/`@` marker and ours does not — appending to `spin?` would bury a character
  our expression parser rejects.

  Thirteen facts recorded. The four `issue-93` markers this closed retired with
  supersede records.

  Two pre-existing divergences surfaced and were left where they belong: vasm's
  backward branch sizing under `-no-opt` (#110, which this is another instance
  of) and duplicate labels going undetected in our vasm dialect (#126). Neither
  is a macro question, and the probes were written to avoid tripping over the
  first.

- **2026-08-19 — acme** (#93), the sixth and last. Brace-delimited bodies with
  arity-dispatched overloads and `+name` calls; `.dotted` locals, which is
  sjasmplus's rule and needed no new code. Twelve facts recorded against acme
  0.97 — including the three shapes a line-oriented collector gets wrong — and
  the last two `issue-93` markers retired. No macro gap markers remain.

  Three unrelated acme divergences surfaced while probing and are filed as #128
  rather than folded in: `!if` rejects a comparison, a zero-page label
  assembles absolute, and `!byte` rejects a string. All three reproduce with no
  macro involved, and the probes were written around them.

  **Known gap:** acme's formatter refuses a file containing macros — its block
  parser reads the body's closing brace as an unbalanced conditional close.
  Unchanged from before, verified by running the old code, and pinned by a test.
  That makes five of six dialects whose formatter refuses what it cannot lay
  out; only sjasmplus round-trips a macro. Closing that is its own piece of work.

- **2026-08-30 — acme macros across `!source`** (#429). ACME 0.97 was probed
  in both directions: a definition in an included file is visible afterward
  in its includer, a definition in the includer is visible inside the included
  file, and a nested include's definition flows all the way back out. Macro
  state is therefore part of ACME's live include environment, alongside
  symbols and zones. A definition later in the includer is not visible inside
  an earlier include, so multi-file ACME registers definitions and expands
  calls in the evaluation walk's exact textual order. The shared expander
  gained a reusable namespace; other dialects keep the short-lived one-file
  default. Expansion frames now retain both the included definition span and
  the invocation span, so textual inclusion does not cost source provenance.
  The pinned 6502Assembly corpus advances to the independent `%` expression
  gap (#455).

- **2026-09-02 — sjasmplus macros are live** (#557). sjasmplus 1.21.0 was
  probed the same way: a definition in an included file is visible afterwards
  in its includer and in later sibling includes, one in the includer is
  visible inside a later include, and a nested include's flows all the way
  out. The rule is textual from the definition forward and nothing else: an
  invocation above the definition, or above the `INCLUDE` that holds it, is
  `Unrecognized instruction`; a definition inside an untaken `IF` defines
  nothing; a second definition of a name is `Duplicate macroname` at its
  header. The per-file pre-pass got the last three wrong as well as the
  cross-file case, because it collected every definition in a file before
  expanding any line of it. sjasmplus therefore drops the pre-pass rewrite
  altogether — single-source and multi-file alike — and registers and
  expands in the evaluation walk, through the namespace #429 added and a
  new `Z80Syntax::expand_live` hook; pasmo keeps the per-file default. The
  pinned SpecNext Invaders corpus advances to temporary labels (`1F`).

## Drift triggers

Stop and re-consult if a change would:

- **Unify the dialects' macro grammar** — a common header parser, a shared
  local-scoping rule, a default `fit_arguments` or `locals`. Six dialects
  produced four local mechanisms and four arity postures; see the table above.
- **Parameterise the collector instead of delegating it** — a `BodyStyle` enum,
  a "terminator" abstraction covering both keywords and braces. The two shapes
  have nothing in common but their purpose: one reads lines, the other counts
  characters and tracks string state. `collect` with a default is what keeps the
  keyword dialects on untouched code.
- **Add a hook to `MacroSyntax` for a case nobody measured.** Every one of them
  exists because a reference did something the others did not. A hook with one
  implementation and no probe behind it is a guess with a trait attached.
- **Route the formatter through the expanding parse**, or collapse `Expand` back
  into a single mode "since assembly always expands anyway". It destroys source.
- **Add macro support to a dialect from its manual** rather than from probes
  against the installed reference. Every row of that table contradicts a
  reasonable reading of the documentation.
- **Reproduce a reference's crash for fidelity.** Both assemblers measured
  segfault on self-recursion (exit 139). Byte-identical output is the goal;
  byte-identical crashing is not.
- **Expand another dialect's macros across an include boundary** without
  probing what its reference does. ACME's namespace is live across `!source`
  and sjasmplus's across `INCLUDE`, each on its own probes; no conclusion was
  inferred for the other dialects.
- **Add a dialect's macros to its single-source parse only.** The CLI assembles
  through the multi-file entry point, so a hook in the wrong place passes every
  library test and does nothing in the tool. It has happened once.

See [`conditional-assembly-framework.md`](conditional-assembly-framework.md)
(the same shared-core, demand-gated-adoption shape), [`syntax-stance.md`](syntax-stance.md)
(fidelity over convergence), and [`v1-scope.md`](v1-scope.md) (macros as a v1.0 bar item).
