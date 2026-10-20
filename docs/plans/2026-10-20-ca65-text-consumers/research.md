# Next text-consumer slice (#892, #897)

The 396-case native audit has 264 accepted cases and 132 refusals. It compares
actual user-message payloads and emitted bytes across direct, labelled, selected
and macro-body source. Each include/incbin request has uniquely different file
contents, so a truncated request cannot pass by opening another fixture. Raw
stdout/stderr hex is retained beside decoded streams. The clean literal-boundary
producer differs in 56 cases: 48 quote-value cases under #892 and eight computed
comma assertion messages. Physical comma controls reproduce against preceding
main and the new producer; #897 tracks that separate physical-argument defect.

The numeric argument splitter used by `parse_assert` does not protect quoted
commas. Leading diagnostic string parsing stops at the first quote after computed
values have been reserialized into authored source. Direct UTF-8 consumption from
the approved byte-token store avoids inventing source syntax for either payload.

Preserve one native string prefix at textual consumers; do not apply the strict
whole-operand data rule to diagnostics, whose native grammar ignores trailing
text. Includes must still consume a complete request and incbin must preserve its
offset/size tail. Assertion condition/action parsing must protect native quoted
and character tokens and retain the complete optional message. Keep numeric Expr,
source-order selection, lazy source reading and canonical formatting unchanged.
No new dependency, public schema or layout evaluation is needed.

The filenames in this POSIX native audit include quotes, control characters and
backslashes that are not portable filesystem names. Test complete request payloads
through the existing in-memory loader on every host. Actual CLI filename tests
must name their platform surface; do not claim that POSIX-accepted filenames exist
on Windows. Message JSON payloads remain portable. The existing UTF-8-only text
contract remains a separate boundary from arbitrary byte strings.

The physical-assert-phases.json replay now retains both native source and link
phases for all four actions; every native message retains the comma, while the
preceding Asm198x message is empty. Before code, probe native acceptance boundaries
for absent, nonstring and trailing message operands. Then implement
through the private store and current readers, qualify exact payloads plus output
preservation and formatter/debug round trips, run all existing byte/string audits,
and retain clean producer/package/4510 evidence and five-host controls.

Native can print the same assertion warning during both assembly and linking.
The existing fused driver emits one logical diagnostic. Payload evidence is
checked against every populated native phase, rather than adding duplicate
process prints to the expected JSON warning list.
