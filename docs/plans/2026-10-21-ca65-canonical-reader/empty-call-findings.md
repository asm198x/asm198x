# Empty function calls

All 85 native controls completed: 40 acceptances and 45 refusals. Checked #925
differs in 24 selected-source cases: nine panics from empty left/right/mid, nine
false acceptances of empty concat/string/sprintf, and six false refusals of
blank/tcount. Direct/include/macro contexts agree; inactive and unused programs
retain native lazy behavior.

Native `.blank()` emits 01 and `.tcount()` emits 00. Native refuses every other
empty function in this matrix. For invalid concat/string/sprintf, Asm198x
replaces the seeded output with an empty file. Empty selectors exit 101 from
the token-selector invariant instead of producing a structured refusal.

Ca65Text::function treats empty arguments as recognition probes even for real
source. Only strlen/strat distinguish line zero. The selector hook then assumes
the grammar returned source, but receives the empty Text probe result. Keep
line-zero recognition; apply native real-call rules, preserve blank/tcount
numeric results and return ordinary errors for invalid source. Test flat-family
callers and valid nonempty counterparts before choosing the final guard.
