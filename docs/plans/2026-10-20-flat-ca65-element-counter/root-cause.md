# Flat data expressions share the statement-start counter

All 304 retained programs are accepted by the frozen #909 producer. Native
accepts 271 and refuses 33. The producer emits wrong bytes for 165 accepted
programs and wrongly accepts all 33 second-value range failures. The other
106 controls agree, including unused templates, separate statements and
counter differences that cancel within a value. There are no unrelated safe
refusals in this matrix.

Expr::Pc is documented as the statement-start address. Bytes, Words, Sized and
encoded pieces evaluate every value with that statement pc. Both flat readers
parse each value using the same value_list helper without passing its width;
byte_list also ignores the length of preceding literal/constructed strings.
The engine behaves according to its contract; per-element binding belongs in
these two ca65 frontends before engine lowering.

Use a private exhaustive Expr visitor in ca65_flat to bind each Pc leaf to
Pc plus the element offset. Pc+Pc and Pc-Pc native controls prove why adding an
offset to the final result is wrong. String literals remain Num byte vectors,
so their asterisks never become Pc; subsequent numeric elements use the decoded
byte count. Apply width before low/high/bank extraction and retain original
Node.source for formatting. No source callback or lazy macro body is evaluated.
