# Native conditional source classification

The 298-case native audit includes direct and aliased expressions, both
conditional senses, current/outer scopes, includes, macros, forward names,
unknown names, cycles, segment changes, numeric coefficients, a 301-definition
chain and a shared graph. Native accepts 289 and refuses nine. The frozen
predecessor differs on 154: 153 wrong bytes and one wrong acceptance.

The classifier fixes all 153 wrong-byte results and retains every previous
match. It follows Deferred.source_value in iterative dependency order, uses
only current source constants/labels, and caches segment contributions rather
than expanding alias trees. Unknown source names remain unclassified; shared
scope bindings supply the current-scope key. Unary negation and constant
multiplication preserve native segment cancellation, including zero products.
Division by one remains nonlinear, as measured in native ca65.

The original explicit_outer_alias_scope wrong acceptance is unchanged and
remains #795's undefined source-query admission work. It is retained in the
full audit, with the exact exclusion named in the 297-case permanent fixture.
No query-time lookup, numeric string argument ordering or live .const fix is
claimed here; those remain required for #795.

The 297 supported cases pass permanent CLI/native file and JSON byte checks,
official Debug198x reads, symbol/section round trips and idempotent formatting.
All eight native failures preserve four output files and emit JSON diagnostics.
The new required ca65_source_constancy archive cohort covers 1,164 commands;
its omission fails qualification. All 115 Node checks, all-target Clippy,
rustfmt, Ruff and generated docs/version checks pass. Full workspace, frozen
native replays, archive controls and five-host CI remain required before merge.
