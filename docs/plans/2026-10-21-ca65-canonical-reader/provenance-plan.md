# Native token provenance before canonical consumption

Before implementing physical/generated tokens in ca65/reader.rs, measure native
admission across numeric data, name queries, repeat counters and visibility.
provenance-probe.py crosses physical A, .ident("A"), a selector over physical A,
a selector over constructed A and a nested constructed selection with direct,
include, macro, inactive and unused contexts. Bind constructed A to 2 so a
register/name admission result cannot be confused with an undefined symbol.

Retain all 100 native source/config/status/bytes and both process streams in
provenance-native.json; compare the frozen checked #925 CLI in
provenance-before.json. Assert source counts and positive/negative native cases.
Reuse existing capture and comparison tools; materialize include files before
capture. These controls extend #918/#923/#924. Check for duplicate issues before
filing any newly confirmed admission surface. Do not infer a token's kind solely
from its expanded spelling or add whole-line constructor exceptions.

The constant prologue exposed #876: Asm198x refuses line 1 before reaching
the downstream field. Its 36 false refusals are valid evidence for #876, but
its matching refusals do not prove downstream admission or first-error parity.
Retain this capture. provenance-label-probe.py replaces only the prologue with
`.org 2` and a generated A label to isolate downstream token admission. Retain
a second complete native/before matrix and verify the valid generated-label
program reaches the consumer before interpreting matching refusals.
