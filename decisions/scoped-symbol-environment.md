# Decision: scopes belong to the shared engine, not to the dialects that need them

**Status:** Active. Binding for Asm198x (accepted 2026-09-01, #484).
Applies the extraction test of
[`llvm-alike-legacy-assembly-infrastructure.md`](llvm-alike-legacy-assembly-infrastructure.md)
to the symbol model, the way
[`sections-in-the-shared-engine.md`](sections-in-the-shared-engine.md) applied
it to placement.

**Date:** 2026-09-01.

## The decision

1. **The engine's symbol environment becomes scope-aware.** A binding lives in
   a scope; a reference resolves through the scope path in force where it
   stands. A dialect with no scope concept is a program with exactly one root
   scope, and sees nothing change.
2. **Scope *events* stay dialect-spelled; resolution is shared.** A dialect
   declares where a scope opens, closes, or re-anchors — ACME's `!zone`,
   ca65's `.proc`/`.scope` and `@cheap` locals, the Z80 family's dot-locals
   and `MODULE`s, a global label re-anchoring later locals — and the engine
   owns what those events mean for lookup. Each dialect's declared policy is
   probe-pinned to its reference, exactly as every other behaviour is: one
   mechanism, per-dialect rules, never one dialect's rule generalised to all.
3. **Bindings may be compound.** A symbol's value is no longer only an `i64`:
   a structure binds a total size plus named member offsets
   (`SpriteAttributes` and `SpriteAttributes.vpat`), resolved to plain values
   at reference time so the expression evaluator is untouched.
4. **Lexical state stops accreting onto `Statement`.** `xor_mask`,
   `instruction_set`, and `extension_set` are scope-carried state flattened
   per-statement because there was nowhere else to put them — each annotated
   "set by the walk, which is where the lexical scope is known". New lexical
   features bind in the environment; migrating the existing three fields is
   staged, not a precondition.

## The evidence — the extraction bar, met four times

Scope handling exists in four independent implementations today:

| Where | Mechanism |
|---|---|
| `vasm.rs` | `qualify_stmt`/`qualify_opnd`/`qualify_expr` — name-mangling recursion over the tree |
| `acme.rs` | `qualify_name` — zones |
| `ca65.rs` | `current_global` + `outer_global` save/restore |
| `z80.rs` | `scopes_locals`/`scopes_modules` — already generalised, for one family |

Underneath them, the engine's table is a flat `BTreeMap<String, i64>`, and the
cost of that shows in documented refusals: `Ca65Text` has no scope stack and
prescans the whole file "deliberately blind to scope", refusing to fold a
scoped constant because folding the wrong symbol would be a wrong answer.
Refusing is the honest half-measure; the environment is the answer.

## The consumers waiting on it

- **#477** — sjasmplus `STRUCT`/`ENDS`, plus rgbasm `union`/`sizeof` in the
  same backlog: two dialects needing compound bindings (rule 3).
- The ca65 `.proc`/`.scope` folding gap named above.
- sjasmplus `defarray`/`undefine`/`ifused`/`ifnused` (#275) and ca65 `.set`
  rebinding semantics, which want scope-aware definition tracking.
- **Later, not here:** the address-space qualifier
  (`decisions/roadmap-sequencing.md` seam 3) — a binding will eventually carry
  *which* space qualifies it (Z8001 segments and 65816 banks began this; an
  8051 bring-up, #488, exercises it fully). This record's shape must not
  preclude it; its design arrives with its first full consumer.

## What does not change

- **The reference-parity surface.** No dialect gains a scoping spelling its
  reference lacks; the declared-vs-dispatched invariant holds.
- **The expression evaluator.** Resolution happens before evaluation; `eval`
  still sees names and values.
- **The statement model.** One operation per statement stays as it is — and
  per #489's guard, this work must not bake that assumption in deeper.
- **Macro-expansion locals.** A macro call's per-expansion scoping is the
  expander's, probe-pinned there; this record governs source-level scopes.

## Migration, on the sections precedent

Extraction is by re-landing, behaviour pinned: at least two of the four
hand-rolled implementations move onto the shared environment with the full
differential and replay suites proving byte-identity — the Z80 family's
(already shaped like the destination) and ca65's cheap-locals are the natural
first two. #477 then implements *on* the seam. vasm's tree-mangling and ACME's
zones follow as their own slices; a hand-roll that has not moved yet is debt
recorded here, not a second architecture.

## Drift triggers

- **"Just mangle the qualified name in the dialect."** That is the fourth
  hand-roll's epitaph; the fifth lands on the engine.
- **"Add the field to `Statement` — the walk knows the scope."** The walk
  knowing is the problem statement, not the design.
- **"Flatten struct members to dotted names at parse."** Members resolve
  through the environment so scoping and diagnostics see them; the dotted
  *spelling* is dialect surface.
- **"sjasmplus rescopes on a global label, so everyone must."** Per-dialect
  policy over one mechanism; probe each reference.

## Related decisions

- [`sections-in-the-shared-engine.md`](sections-in-the-shared-engine.md) — the precedent this follows
- [`llvm-alike-legacy-assembly-infrastructure.md`](llvm-alike-legacy-assembly-infrastructure.md) — the extraction test this passes
- [`conditionals-in-multipass-dialects.md`](conditionals-in-multipass-dialects.md) — the projection sweep the environment rides in ca65's case
- [`syntax-stance.md`](syntax-stance.md) — per-dialect fidelity of the scope events
