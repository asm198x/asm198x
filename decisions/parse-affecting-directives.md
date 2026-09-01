# Decision: a chosen branch is read eagerly, but understood lazily

**Status:** Active. Binding for Asm198x (accepted 2026-09-01, #485).
Extends
[`conditionals-in-multipass-dialects.md`](conditionals-in-multipass-dialects.md)
in the direction it already points; changes nothing that record measured.

**Date:** 2026-09-01.

## The bind

The conditionals record settled *value* state: ca65 folds each condition in a
single sequential sweep during projection, against the `=` constants gathered
so far, and an untaken branch defines nothing. That machinery is positional
and it works.

`.define` (#481) is different in kind: it is a **text macro** — it changes how
later lines *tokenise*, not what their expressions are worth. Tokenising
happens at parse, and the shared cursor is deliberately structural: it groups
both branches of a conditional into the tree before the projection selects
one. So a `.define` inside a deferred branch has nowhere to act — by the time
the branch is chosen, every line in it has already been read under the old
text environment. #480's explicit diagnostic is the correct holding position,
and no amount of environment-threading at parse time fixes an ordering
problem. The walk's existing line-state hook (`WalkSemantics`, which threads
`.define` across *includes*) runs at parse for the same reason it cannot help
here: an include is resolved where it stands; a branch is not.

## The decision

**Structure is read eagerly; meaning is read lazily.** The cursor keeps doing
exactly what its own documentation says — grouping heads, bodies, and closers,
never interpreting — and a conditional's bodies stop being *tokenised* at
parse. Every node already carries its verbatim source text; when the
projection sweep selects a branch, that branch's lines are read **then**,
under the text environment in force at that point in the sweep. Concretely:

1. The projection sweep carries the text environment (`.define`/`.undefine`
   state) beside the `=` constants it already gathers, in the same source
   order.
2. A selected branch's body is tokenised at selection, under that
   environment; definitions made inside it flow to the rest of the sweep.
3. An untaken branch is never tokenised at all — which is the measured ca65
   posture ("a definition inside an untaken branch is invisible afterwards"),
   now by construction rather than by suppression.
4. The formatter is untouched: it renders both branches from their verbatim
   source, exactly as today. Nothing rewrites source; the tree survives; this
   is a different *reader*, not a preprocessor — the same distinction
   `conditional-assembly-framework.md`'s drift trigger protects.
5. Errors inside a chosen branch surface at selection, carrying the node's
   own span — same file, same line as today.

The same evaluation-ordered home serves the rest of the family as it is
implemented: ca65's `.delmacro`/`.exitmacro`/`.set` mutate macro and binding
state in source order, and belong in the sweep beside the text environment.

## What this deliberately excludes

- **vasm's pass conditionals** (`if1`/`if2`/`ifp1`/`ifmacrod`/`ifmacrond`,
  #274). Those are *condition semantics* — what is true in which pass — not
  tokenisation order. They need their own probes against vasm before anything
  is decided; nothing here presumes their answer.
- **Repetition bodies with iteration-varying text state.** A `.repeat` whose
  body `.define`s differently per iteration is the same mechanism applied per
  iteration; it rides this shape, but its edge cases are probed with #481's
  implementation, not asserted here.

## Costs, named

A chosen branch's lines are read twice — once structurally at parse, once
meaningfully at selection. That is bounded by the source's own size, and it
buys the property the reference itself has: ca65 is a single sequential
reader, and a split pipeline can only imitate one by deferring the half that
depends on order.

## Acceptance

- #481's six behaviour points implement on this shape, probe-pinned against
  ca65 V2.18, and #480's diagnostic retires.
- The formatter's byte-identical and idempotent round trips hold unchanged.
- The declared-vs-dispatched surface invariant holds — no dialect's spelling
  surface moves.

## Drift triggers

- **"Thread the text environment through the parse."** The environment's
  value at a branch is unknowable at parse; that is the bind, not a fix.
- **"Expand `.define`s in a source pre-pass."** That is the preprocessor the
  framework's drift trigger forbids; the formatter must keep seeing what the
  author wrote.
- **"Tokenise untaken branches anyway, just ignore the definitions."**
  Reading an untaken branch under a wrong environment manufactures parse
  errors ca65 never raises.
- **"if1/if2 are the same problem."** They are pass semantics; probe them.

## Related decisions

- [`conditionals-in-multipass-dialects.md`](conditionals-in-multipass-dialects.md) — the sweep this rides
- [`conditional-assembly-framework.md`](conditional-assembly-framework.md) — the tree-not-preprocessor contract
- [`macro-expansion-framework.md`](macro-expansion-framework.md) — the expansion machinery `.define` borrows from
- [`syntax-stance.md`](syntax-stance.md)
