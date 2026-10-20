# Address extraction is distinct from word validation

The 148-case native audit accepts 107 and refuses 41. The frozen #908 producer
has 37 differences, all safe refusals of native-accepted 65816 `.addr` programs.
Every HuC6280 control and strict `.word` refusal agrees already.

The flat 65816 declaration groups names `[word, addr]` under ID words and parse_op
lowers both to Operation::Words. That operation correctly checks `.word` values,
so changing its policy globally would break existing behavior. Give only the
65816 addr declaration its own ID and use existing low/high expression
extraction, matching the measured two-byte truncation while preserving forward
resolution. The existing source-preserving AST needs no new operation or mode.

Pinned cc65 implementation precedent is in the read-only
cc65-4510-opcodes/src/ca65/pseudo.c DoAddr: it applies GenWordExpr before EmitWord
for CPU_65816. The retained installed-native V2.18 executions are the measured
acceptance/byte authority; that source snapshot is implementation precedent.

Per-element counters remain separately tracked as #910. The preceding complete
254-case audit retains both CPUs and all data widths for regression checks.
