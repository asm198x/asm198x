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
that broader policy needs the canonical reader because the flat path otherwise
loses numeric evaluation inside constructors. For this fix, keep blank/tcount
physical inside flat lists, preserving the existing policy for other calls.

112 inactive flat-family programs encounter the separately tracked #895
conditional-reader gap. Keep them in the audit inventory with explicit issue
references. The six flat `.const` token-count gaps are tracked in #928.
The remaining 722 programs form this fix's qualification set.

The new diagnostic test also exposed missing origin remapping when the flat
text pass itself returns an error: the existing parser path remaps errors,
but `expand_ca65` returned text errors before that path. Apply the existing
macros::remap_lines there too, preserving invocation lines and macro frames
for these selected-source errors. No new diagnostic representation is needed.

The syntax-only flat formatter reaches mos6502::ExprParser for these calls.
Its call grammar always reads a first argument before looking for `)`, so
valid empty blank/tcount calls fail with `expected a value`. Permit an empty
argument vector and let the existing dialect function builder enforce arity;
retain refusal of missing arguments beside commas. The native fixture checks
formatting, idempotence and reassembly rather than merely accepting syntax.
