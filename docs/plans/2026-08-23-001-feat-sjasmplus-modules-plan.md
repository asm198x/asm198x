> Planning document. Do not treat status claims here as current unless they match `../../CLAUDE.md`, `../../README.md`, and the current test/CLI surface.

---
title: sjasmplus Modules - Plan
type: feat
date: 2026-08-23
topic: sjasmplus-modules
artifact_contract: ce-unified-plan/v1
artifact_readiness: in-progress
product_contract_source: probe-survey
execution: code
---

# sjasmplus Modules - Plan

## Goal Capsule

- **Objective:** `MODULE`/`ENDMODULE` name scoping in sjasmplus — the last
  untouched item on [#93].
- **Product authority:** Steve Hill.
- **Open blockers:** none. One divergence was recorded below (the
  unclosed-at-EOF advisory) and has since been closed — see *The one
  divergence*.

---

## Product Contract

### Problem Frame

[#93]'s § *Scope* lists three items: macros, repetition, and **modules /
namespaces**. The first two have landed. This is the third, and it is
single-dialect: sjasmplus is the only dialect in the set whose reference has
modules, so none of the shared machinery built for the other two applies. There
is no cursor to extend and no `CondEval` hook to add.

It also sits in a different layer. Macros and repetition are *source
transformations* — they happen before anything is lowered. Modules are a
**symbol-naming rule**, so the work lands in label resolution alongside the
existing leading-`.` local scoping, which is the same shape one level down.

The demand argument is [#93]'s, unchanged: the curriculum does not use modules
because we wrote the curriculum. External sjasmplus source does — a module is
the normal way a Spectrum project keeps two libraries from colliding, and
without it such a file does not assemble.

### What the reference actually does

Probed against SjASMPlus v1.21.0, 2026-08-23. Every row is a probe, not a
reading of the manual.

| Aspect | Rule | Probe |
|---|---|---|
| Definition | the open modules' dotted prefix is prepended to the label | m1, m5 |
| Reference | **two** candidates: `<prefix>.<name>`, then the bare name | m2, m13 |
| No walk-up | intermediate levels are **not** tried | m8 |
| `@` escape | on a definition *or* a reference, means the bare global name | m4, m9, m15 |
| Nesting | prefixes concatenate with `.` | m5 |
| Locals | `.loc` qualifies under its global first; the module prefix wraps the result (`foo.glob.loc`) | m6, m25 |
| `EQU` | qualified like any other label | m11 |
| Macros | **not** module-scoped — macro names stay global | m22, m23 |
| `DEFINE` | **not** module-scoped | m24 |
| Includes | a module spans an include boundary | m17 |
| Expansion site | a macro expanded inside a module defines qualified labels | m18 |
| Reopening | the same name may be reopened; definitions accumulate | m12 |
| Spelling | `MODULE` / `ENDMODULE` / `ENDMOD`, case-insensitive | m7, m21 |
| Unnamed | `MODULE` with no name is an error, and opens no scope | m10 |
| Unclosed | a warning at EOF; assembly **succeeds** | m19 |
| Stray close | `ENDMODULE` without `MODULE` is an error | m20 |

The two rows that decide the design are **the two-candidate reference** and
**no walk-up**. m8 is the discriminator: inside `foo.baz`, a plain reference to
a label defined in `foo` fails, and the reference names exactly one candidate —
`foo.baz.outer`. So resolution is not a scope chain. It is the fully-qualified
name, then the global name, and nothing between.

### Non-Goals

- Modules in any other dialect. No other reference in the set has them, and
  `syntax-stance.md` forbids inventing a house spelling.
- Module-scoped macros or `DEFINE`s. The reference does not scope either
  (m22–m24), so neither do we.
- `MODULE` inside a macro body defining a scope that outlives the expansion.
  Not probed, not claimed.

---

## Design

### The problem the two-candidate rule creates

`Expr::Sym` holds one name. A module reference is a *choice between two names*,
and which one is right depends on what ends up defined — including by a forward
definition the walker has not reached yet. Neither eager rewriting nor a
symbol-table lookup at lowering time can answer it.

Three options were considered:

1. **A new `Expr` variant** carrying both candidates. Rejected: a shape change
   to the shared IR, paid by every dialect, for one dialect's feature.
2. **A pre-pass collecting label definitions**, then resolving at lowering.
   Rejected: to be correct the pre-pass would have to run conditionals and macro
   expansion, which is the whole lowering pipeline. It is not a pre-pass.
3. **Qualify eagerly, then repair.** Chosen.

### Qualify eagerly, then repair

The walker emits the qualified name and records the fallback:

- a reference under prefix `foo.` becomes `Expr::Sym("foo.bar")`, and the
  walker records the alias `foo.bar → bar`;
- after the walk, the complete statement stream is known, so the set of
  *defined* names is known;
- for each alias whose qualified name is undefined **and** whose bare name is
  defined, every occurrence is rewritten back to the bare name.

The decision needs only the *set* of definitions, never their values, so a
forward reference is no harder than a backward one. The `and whose bare name is
defined` half is deliberate: when neither exists, the reference keeps its
qualified spelling, so the error names the same candidate the reference names.

The rewrite reuses the traversal that already exists. `qualify_locals` walks
every `Expr` in an `Operation` to prefix leading-`.` symbols; that traversal
becomes a general `map_syms`, and `qualify_locals` is expressed on top of it
unchanged. One traversal, two callers — rather than a second twelve-arm copy.

### Where each rule lands

| Rule | Site |
|---|---|
| `MODULE`/`ENDMODULE`/`ENDMOD` grammar | `sjasmplus.rs` `DIRECTIVES` + the walker's directive arm |
| module stack, prefix | the z80 walker, gated on a new `Z80Syntax::scopes_modules()` |
| definition qualification | `resolve_label`, after the existing local rule |
| reference qualification + alias recording | `lower_line`, after `qualify_locals` |
| `@` in a label | `split_label`, gated on `scopes_modules()` |
| `@` in an expression | the expression tokenizer, same gate |
| the repair pass | after the walk, before the statements are returned |

`current_global` stays **unprefixed**, so locals compose as m25 requires: the
local rule produces `glob.loc` and the module prefix wraps the whole to
`foo.glob.loc`.

### The one divergence

The reference *warns* on a module left unclosed at EOF and assembles anyway
(m19). `Dialect::parse` returns `Result<Vec<Statement>, AsmError>` — there is no
warning channel from a flat dialect's parse, and adding one is a trait-signature
change across every dialect for one advisory.

So: **accept it silently.** Bytes match the reference, which is what the
identity claim is about; the advisory text does not. Erroring instead would
reject source real sjasmplus takes, which is the worse failure. If a parse-time
warning channel ever lands for another reason, this is a caller for it.

**Closed 2026-08-23.** That channel landed with #99 — `parse_warned`, defaulted
so no other dialect changed — and this was one of the three advisories waiting
on it. The warning is now raised: one per program, naming the innermost open
module by its full dotted path, as the reference does.

---

## Verification

- Probe-pinned unit tests for every row of the table above.
- Differential arbitration against SjASMPlus v1.21.0 for the byte-producing
  cases, recorded in the verdict corpus.
- `directive_surface.rs` covers the two new directive declarations; the
  generated docs page is regenerated with `cargo xtask docs`.
- The existing local-label tests are the regression guard for `map_syms`
  standing in for `qualify_locals`'s traversal.
