# Opening braces are captured tokens, not nested delimiters

The 225 native programs establish valid counterexamples to a balanced-brace
reader. Native `.tcount({{1})` emits 02, and `.tcount({{{1})` emits 03. In these
captures an opening brace is an ordinary token; the first physical closing
brace terminates the list. Quoted and constructed string brace controls remain
one string token and do not terminate it.

Adding a second closing brace makes `.tcount({{1}})` a native syntax error;
Asm accepts it. Conversely, native `.match({{VALUE},{{OTHER})` emits 01 and the
xmatch variant emits 00, but Asm's balanced splitting hides the argument comma
and refuses with `takes 2 argument(s), and was given 1`.

Across all three readers and direct/included/macro source there are 36 false
acceptances and 27 false refusals under #930. Another 30 flat inactive cases
retain #895. All other 132 programs match. The capture retains 189 native
acceptances and 36 refusals; no timeout was treated as native rejection.

The approved #924 reader must retain opening-brace tokens and end this captured
list at its physical closing token, preserving quoted/string-token identity.
Do not replace the shared argument splitter globally: other dialect grammars
have independent delimiter rules. These findings add failure modes and valid
controls to the planned rewrite without changing product source in #929.
