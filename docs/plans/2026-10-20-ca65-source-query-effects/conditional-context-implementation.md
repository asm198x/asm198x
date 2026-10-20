# Live conditional source queries (#795)

Selected .if/.elseif/.ifconst/.ifnconst expressions now invoke an opt-in source
context at ordinary symbol and function positions in the shared numeric parser.
Lexical recognition only retains raw source-call arguments; it never performs
reads. Name-only queries expand constructor arguments without reading the
resulting identifier. Other dialects and syntax-only parsing retain pure defaults.

The context borrows Walker and Projection rather than copying their namespace.
It delegates name/reference state to the reader, .const classification to the
existing Deferred/segment machinery, and final admission to retained bound
requirements. Nested predicates retain ordinary dependencies before becoming
numbers. Numeric text callbacks use the same context; opt-in text functions
consume arguments on demand so an inner call cannot run before an earlier
numeric argument. Strings remain immutable byte tokens, and Expr remains numeric.
Braced token-list contents remain unevaluated in this live path. Native .strat
literal-only indices distinguish numeric queries from ordinary symbol reads.

.sizeof requires a size already known at the source query. The lookup-miss audit
caught a wrong acceptance of .const(.sizeof(FUTURE)) before a later declaration;
known/future/missing controls now prevent that fail-open behavior. Ordinary label
sizes remain the established #798 gap, explicitly excluded rather than guessed.

The final native audit has 200 cases: 179 accepts and 21 refusals. The frozen
#915 predecessor differs on 75. The corrected implementation matches all 197
supported cases, fixing 72 differences. Only generated_label_size_direct,
generated_label_size_included and generated_label_size_macro remain excluded
under #798. Their inactive/unused controls stay covered. The permanent CLI and
required archive cohort ca65_conditional_context retain 197 cases and 725
commands, including native bytes, JSON, official debug reading, formatter
idempotence/reassembly, symbol/section identity and refusal output preservation.
Refusals check source file/line/macro provenance. All 2,266 preceding native
cases must retain every previous match; their pre-existing gaps stay explicit.

Preflight review caught nested .strlen(.sprintf(...)) replacing a genuine
checked diagnostic with a string-type error. A red regression proved loss of
its original file/line/macro frame and stable code; the corrected test preserves
all of them. The full suite then caught a generated-label error losing its
include path in the new prefix branch. The existing generated-block regression
proved that failure, and the branch now uses the existing statement-error
stamping helper. Its two tests and the five deferred/query tests pass.

This is a conditional consumer slice of the approved design. Normal data-list
operands, repeat counts and other remaining numeric/string consumers still need
ordered live integration under #795; this slice does not close that issue.
