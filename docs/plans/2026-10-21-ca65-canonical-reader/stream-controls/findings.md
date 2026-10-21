# Native stream controls and the first migrated consumers

The final 180 retained cases cover 36 bodies in direct, included, macro, inactive and
unused-macro source. Native tools are identified by executable hashes and version
streams in native-180.json. The immutable checked #929 executable matches 117.
The initial implementation matches all 165 constructor controls; the final 180
controls also require exact quote-valued content, including refusals with an
existing output file preserved. This is a focused control matrix, not a completion rate.

The initial 120-case captures are retained as gzip snapshots. An explicit
comparison confirmed that all original names, sources, configurations, native
exit/stdout/stderr streams and bytes survive the 165-case regeneration; the
before executable outcomes are identical too.

Native observations establish:

- Captured lists may be unbraced. The first opening brace selects `}` as the
  terminator; subsequent braces are ordinary tokens. A selected closing brace
  can itself be captured when the caller's delimiter is `)`.
- Token-producing constructors execute while capturing, but numeric calls such
  as .const remain tokens. An unbraced captured numeric call does not gain
  balanced-parenthesis parsing merely because it resembles an expression.
- A selected .ident("A") remains an identifier and differs from physical A
  under .xmatch. Rendering and reclassifying the selected list loses this fact.
- .string accepts an identifier path without reading its value, or a numeric
  constant expression when its first token is not an identifier. Thus
  .string(1+2) produces "3", .string(VALUE+1) refuses, .string('A') produces
  "65", and physical/selected register A refuses while constructed A succeeds.

The existing local cc65 reference checkout at e11fb5c39371046ebe25485f984f644c5a0d65d3
also explains these mechanisms: src/ca65/nexttok.c (NextTok, CollectTokens,
FuncString), src/ca65/toklist.c (GetTokListTerm), and src/ca65/expr.c (DoMatch,
FuncTCount). That checkout is implementation precedent, not claimed to be the
installed binary's source revision; the recorded installed-tool observations
remain the behavioral evidence.

The initial semantic migration moves linked data lists, including byte strings,
.asciiz and wider numeric lists, to one stream per directive. Selected commas
remain available to that directive. Constructor numeric arguments borrow the
existing expression grammar and operator classification, and name/query effects
run on consumption. String formatting and identifier validation still use the
existing TextSyntax grammar and StringTokens byte store. Numeric Expr values,
shared scope binding, spans and deferred requirements retain their existing roles.

A broad replay caught a missing leading global namespace marker in name-only
queries: .ref(::VALUE) was refused. The typed query path now consumes that marker
explicitly; its three native controls must pass with the full replay. No check
is waived for this failure.

This does not complete #924. Linked scalar/header/file consumers, flat readers
and canonical syntax-only consumption still require migration and qualification.
The implementation branch remains unmerged until the approved full design and
all final native archive gates are satisfied.

The first full workspace run failed the retained native character fixture on
6502_byte_39_lone_direct with `malformed character literal`. Its valid `'''`
was split at the value apostrophe. Reading a printable character and its closing
quote as one token makes all 864 native character-operand cases pass. The
175-case extension then exposed three .xmatch wrong-byte results: trimming all
quotes erased character values. The shared exact-value helper now removes only
the enclosing quote pair, and the final 180 controls cover both character and
string quote-valued content. All intermediate captures and failures are retained.

The first namespace-corrected 39-matrix run retained every match, but its final
binary hash check failed because a concurrent integration build replaced the
CLI. It is discarded as a verification receipt. A stable executable replay
subsequently passed all 39 matrices / 7,309 rows, retaining all 6,700 checked
matches and reaching 6,727. Any final code changes require that gate again.

The next full-suite failure was an obsolete known-gap expectation, not a native
regression: ca65_text_call_spacing expected left_0 to fail although the native
capture accepts 61626364. All 73 cases match on the reader, while the immutable
#929 executable fails exactly left_0 through left_3. Remove only their four
#872 annotations and require no admission gaps in this fixture. An equality
check confirms every native source, process observation, symbol value and byte
remains unchanged. The shared existing package cohort now exercises their native
success path as well. #872 is not closed by this partial reader migration.

Final checkpoint verification: 1,649 workspace Rust/Lua/doc tests passed, 38
existing tests ignored; Clippy with -D warnings, Rust/Ruff formatting, 115 Node
tests and docs/version/ecosystem/ledger checks passed. The unchanged-executable
replay compares all 39 matrices / 7,309 rows, retains every one of the prior
6,700 matches and reaches 6,727. All 180 stream controls and all 73 spacing
controls match native; their full CLI contract tests pass. stream-checks retains
all successful and failed diagnostic receipts as lossless, hashed gzip blobs.
The immutable #929 producer/package remain the baseline. This checkpoint is
not a merge qualification or a claim that the full reader is complete.
