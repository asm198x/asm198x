# Source-aware ca65 expression reading (#795)

Status: source-aware context approved on 2026-10-20. The full recommended
design, including scoped constancy, is authorized. The retained audit precedes
parser/evaluation changes.

## Measured boundary

The 56-case audit accepts 54 programs natively and refuses two nested-macro
controls. The current frozen producer differs on 24: eighteen wrong-byte
outputs and six explicit scoped-constancy refusals. Direct/include/macro and
unused/inactive controls are retained with exact tool identities and streams.

`VALUE=2 / .byte .ref(VALUE),.const(VALUE),.ref(VALUE)` emits 000101 natively
and 000100 here. Numeric sprintf arguments and nested constancy have the same
missing effect. Within one expression, `.ref(VALUE)+.const(VALUE)` must answer
the earlier query before the later read. Conversely `VALUE+.ref(VALUE)` already
matches and must not regress. A known label-difference alias is constant to
native `.ifconst`; its current classification is false. Scoped constancy needs
native reference creation: querying an outer VALUE can create an unresolved
current-scope reference and change subsequent defined/referenced answers.

## Root cause in the current seams

- ca65_flat::Ca65Text owns read-only numeric/defined snapshots and a depth
  counter. Its evaluate method folds numbers without Reader reference effects;
  constancy expressly refuses scoped source because it has no scope stack.
- text::Scope::number uses the TextSyntax::evaluate callback, currently an
  Option-valued pure fold. It cannot propagate a source-aware lookup failure.
- project::live_text supplies maps/depth before ordinary operand reading.
  Simply recording reads there would move effects before earlier .ref operands.
- Reader::source_value/scope_syms already implement normal source reference
  creation and ordered name queries using the live shared scope environment.
  Projection/deferred already classify source expressions and label segments.
  These are the mechanisms to reuse; another namespace would be wrong.

## Recommended design

Add an opt-in live source-expression context for linked ca65, while retaining
pure defaults for all existing non-context consumers. The context delegates
lookup, reference creation and constancy classification to the existing
Reader/Projection/shared scope environment. It carries the current span and
macro/include provenance. It does not store a second symbol environment.

Consume source functions at their numeric-expression read position, including
nested numeric string arguments, instead of folding the complete operand tail
before earlier names/queries have been read. Use a source-aware entry into the
existing expression parser, with ordinary symbol-read and numeric-function
hooks. Keep its current pure entry for other dialects. Query-only defined/ref
arguments do not become ordinary reads; constancy and numeric arguments do.
Do not represent effects as numeric sentinels or extra output bytes.

Use the existing text numeric-evaluation seam with a checked source-aware
callback/default adapter so genuine lookup/span errors survive. Constancy
must follow existing deferred source classifications, including aliases and
same-segment differences; it must not make unresolved label addresses into
source-time values or consult future definitions as current values.

The selected reader remains in charge: inactive bodies, unused templates and
includes stay lazy, apart from the already approved native lexical admission
checks. Formatting provides no live evaluation context and remains syntax-only.
Private byte-string ownership and original constructor spelling stay intact.

Expected files: dialects/ca65_flat.rs (adapter/constancy), dialects/text.rs
(checked numeric callback), dialects/ca65/mod.rs (source expression entry),
dialects/mos6502.rs (optional source hooks with the pure entry preserved), and
dialects/ca65/project.rs (ordered consumption and existing source classification).
Use scopes.rs only if an existing shared reference API needs a narrow extension;
never add a private scope stack. Confirm exact call sites before implementation.

## Bounded alternative

Use the same source-aware seam for root-scope effects first, retaining explicit
scoped-constancy refusals until a following slice. This removes the wrong-byte
answers first but deliberately leaves #795 open for its scoped cases.

A blanket whole-line read pass is not an acceptable shortcut: it changes the
first query's answer. A new dialect-private scope tree would contradict the
binding scoped-symbol-environment decision.

## Verification and agreement

Require all 56 retained outcomes, preceding 106 source-phase controls and text
constructor/consumer matrices to retain every native match. Add missing/forward
names, alias classifications, order within expressions, nested constructors,
macro/include/repeat execution, lazy malformed-source controls, formatting,
debug and output-preservation cases before implementation. Keep complete native
streams and require actual CLI/archive coverage and omission/corruption controls.
Run full Rust/Lua, Node, lint/docs and actual five-host qualification before merge.

The supplied AGENTS.md says to ask before “Introducing new architectural
patterns” and requires design agreement before behavior changes. The proposed
opt-in source-expression/context path is a significant shared parsing convention
change, so it needs that agreement. The binding scoped-symbol and selected-source
reading decisions remain unchanged; this proposal implements them through the
existing environment rather than bypassing them.
