# Numeric string-query compatibility guards

The first byte-store producer admitted named .strat indices without the scope
and reference callbacks they need. Native scoped_shadow emits 42; f920ac1a
emits 41. Native named_reference emits 4101; f920ac1a emits 4100. The preceding
main producer refuses both. The index guard restores those safe legacy
refusals under #795, while both literal-arithmetic controls match native.
The required consumer cohort executes all four controls, with the two known
query gaps explicitly annotated. The pre-guard producer archive fails exactly
that required cohort; its earlier green package used the smaller fixture and
cannot qualify this boundary.

The 26 arity controls expose a separate recognition boundary. The shared text
scanner probes known function names with no arguments at line zero. Real
empty .strlen/.strat calls, including those exposed by selectors, must refuse.
The preceding producer matches all 26 controls; the first migrated functions
wrongly admit four. Real calls enforce arity while line-zero recognition stays
unchanged. Captured numeric calls remain tokens; inactive calls remain unread.

These guards preserve previous unsupported query boundaries. They add no
scope flattening, layout state, reference callbacks or general eager lexical
reading. The complete native/predecessor/prototype records remain alongside
the focused regression tests and negative archive report.
