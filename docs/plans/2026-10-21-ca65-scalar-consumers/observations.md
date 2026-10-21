# Scalar source-order baseline

The 280-case native matrix accepts 238 and refuses 42. The checked #920
producer differs on 96 cases. All native processes completed within their
10-second limit; comparison runs use the frozen executable identity in before.json.

Repeat/reserve/alignment count and fill functions currently run in a pure whole-line
text pass, before ordinary source reads. Assertion conditions and message
constructors have the same missing effects. SourceContext already provides the
approved borrowed reader/projection path; these consumers have not opted in.

Use the existing top-level field splitter when function calls retain their commas.
Counts and fills must still fold at source time; ordinary assertion values stay
deferred. Reading a zero reserve or an alignment requiring no padding still reads
the fill. Native warning tails evaluate string constructors even though the
warning text ignores their result. Keep whole text scanning for message fields,
with live numeric callbacks, rather than stopping after the leading string.

The earlier 50-case audit remains independent: this slice targets 40 scalar cases;
the remaining 10 include/incbin cases stay under #795 for the following slice.

The first implementation matches all 280 corrected cases with no lost baseline match.

The corrected matrix has 56 bodies across direct/include/macro/inactive/unused
contexts: 238 native accepts and 42 refusals. All 96 predecessor differences are
fixed. The 29 immutable predecessor matrices cover 4,791 cases; no prior match
was lost. The earlier numeric-consumer subset improves 20/50 to 44/50: only the
six active include/incbin operand cases remain. The 515 data and 1,390 external
controls retain their prior matches. Twenty-one message cases additionally pin
native payloads and severity, including error output preservation.

Only ca65/mod.rs and project.rs need behavior changes. The existing private
DataSource adapter now supplies checked source numbers and live text as well as
expressions/tokens. Repeat heads keep expressions unread until their count is
consumed; counts, allocation fields, assertion conditions and message fields use
the same borrowed SourceContext as data/conditionals. Counts/fills still require
current values, while ordinary assertion expressions stay deferred. No shared
expression, public AST, dependencies or binding decisions changed.

The main-rules API in main-rules.json confirms pull requests and 15 named required
checks. AGENTS.md now describes that protection without weakening the stronger
full-check/native-qualification/rebase convention.
