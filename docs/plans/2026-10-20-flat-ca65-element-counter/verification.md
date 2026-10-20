# Per-element counter verification (#910)

Both flat readers now bind each Pc leaf to its data element's byte offset through
an exhaustive private Expr visitor. Numeric lists pass the directive width;
byte lists count decoded literal/constructed bytes. The shared evaluator and
other dialects keep their statement-start Pc contract. No source callback,
expression evaluation, new operation or public shape is introduced. Original
Node.source remains authoritative for formatting.

All 304 native cases agree: 271 accepted and 33 refused. This fixes 165 wrong-byte
outputs and all 33 wrong acceptances while retaining the other 106 matches.
Pc+Pc and Pc-Pc controls distinguish correct leaf binding from a final-result
adjustment. The 254-case broader data-width sweep now agrees completely.

All five focused data suites pass, covering 768 supported native cases. Success
checks compare file/JSON/native bytes, use the official debug reader, and require
symbol/section round trips and idempotence. All 33 counter-range errors are also
formatted twice and reassembled: they remain rejected at the expected location,
and all four existing output files survive. The new required archive cohort
uses 1,216 commands and cannot be omitted. All 114 Node checks, all-target Clippy,
rustfmt and generated docs/version checks pass. Full workspace and frozen
artifact/native/five-host CI evidence remain required before merge.
