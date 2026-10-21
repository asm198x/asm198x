# Empty calls must obey selected-source rules

The 840-program native capture preserves the original 750 source/status/byte
observations and include contents. Recognition probes call the same evaluator
with line zero and no arguments. The evaluator mistakes real empty calls for
those probes, returning empty text. The numeric selector then panics because it
expects source tokens. Other consumers accept invalid calls or reject valid
`.blank()` and `.tcount()`.

Native returns 1 for empty `.blank()` and 0 for empty `.tcount()`. The other
empty calls in the capture are errors. Selected invalid source must return a
structured error and preserve an existing output file.

The nested controls also prove numeric functions remain physical tokens inside
braced token lists: `.tcount({.blank()})` is 3, not 1. Constructors still fold:
`.tcount({.sprintf("%d",.const(VALUE))})` is 1 when VALUE=7. The linked reader's
existing producer-only brace folding policy already gives those native results;
apply that same policy to the flat paths when fixing real empty calls.

112 inactive flat-family programs encounter the separately tracked #895
conditional-reader gap. Keep them in the audit inventory with explicit issue
references. The remaining 728 programs form this fix's qualification set.
