# Two existing macro context losses (#904)

`macros::place_nodes` puts expansion frames on successful AST node spans.
`ast::lower` carries only file/line and an optional operand span into Statement.
Data directives commonly have no operand span, so their node frames disappear.
`Statement::operand_err` falls back to `Statement::err`, which constructs a new
file/line span with no frames. That explains the missing backend range context.

The flat per-file expander also creates its frame locations with local FileId(0).
The include walk stamps the outer error/node with the actual file but leaves
frame definition/invocation files local. Parser errors therefore carry frames
that point at input.s instead of the included file. The captured parser baseline
proves this separately from backend errors.

Use existing Span fields: preserve a line-granular (column zero) operand fallback
when a lowered node has frames but no precise operand span. Let statement
line-error construction/stamping retain those frames at the same statement.
Resolve local frame file IDs only at the ca65 flat per-file boundary, retaining
already known non-root files. No new public field, expression or operation is
needed; existing precise operands and non-macro diagnostics stay precise.

The expanded primary audit contains 80 cases: 20 accepted and 60 refused. Ten
far-address acceptances expose separate #906; retain that complete evidence but
exclude its twelve far-address cases from this context-only required cohort
until its unsigned-range design is approved. The other 68 cases have 18 native
acceptances and 50 refusals. Existing definition-frame locations mean the
selected header line (Span's contract), while native notes identify body lines.
