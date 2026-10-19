# Literal .end token boundaries (#842)

The default native directive lexer ends .end before punctuation. A selected
file does not read the following tail. Whitespace splitting instead treated
.end,$100000000 and ten other forms as different words, then read an oversized
number or reported an unsupported directive. The same rule also let a following
colon make the structural cursor misread .end:.if as a labelled conditional.

The private helper now recognizes only the complete .end keyword, using native
ASCII identifier boundaries. .end2, .endmacro and .end_thing remain distinct.
The selected reader, structural classifier and sweep tail boundary share it.
Assembly only stops selected source; skipped branches and original macro/repeat
captures still validate all numeric tokens, including unused tokens after .end.

The root fixture has 25 native controls (19 accepted, six refused). The clean
formatter-includes predecessor differs on fourteen accepted cases; the current
reader matches all 25. Include and macro/repeat controls retain their source
input order. Comment/string occurrences do not become directives. A genuine
included .end2 failure preserves the binary, symbols, debug and both listing
outputs in human and JSON modes, with the included file and line retained.

The red test fails token_1 with
`asm198x: input.s:2:6: error: number $100000000 is outside the native lexical range`
(the retained stderr includes backticks). The candidate cohort requires all
25 original commands, with explicit native byte equality or refusal/output
preservation; omitting the cohort cannot qualify a package.

No general label grammar, label binding or post-.end structural rewrite is
claimed here. Extended native cases retain #830 label failures, the #844
unread-block failures, and additional label-token observations for follow-up.
The separate #833 formatter-preservation design remains unimplemented pending
approval. Existing .end formatter source loss is not asserted as a valid
round-trip invariant by this slice.

Local verification: 1582 Rust tests pass (38 ignored), 87 Node tests,
Clippy with Lua/all targets, fmt/Ruff and docs pass.
