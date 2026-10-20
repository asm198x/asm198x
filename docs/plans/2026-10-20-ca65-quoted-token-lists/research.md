# Quoted token-list admission and literal grammar

The 623-case native audit accepts 312 inputs and refuses 311. Clean predecessor
c478532a4f4fee2fd608a594f8e0f33a125b01a4 has 262 differences: 255 native-valid
refusals and seven native-invalid acceptances. Native processes all exited 0/1;
no timeout or signal is treated as a language refusal.

Native admits quoted strings in .left/.right/.mid, .tcount/.blank and
.match/.xmatch. A string is one token even when empty or containing commas,
parentheses, colons or backslashes. Multiple adjacent string tokens remain
multiple tokens: selecting one end can emit its string, but selecting both
without a comma is refused by the data directive. Do not flatten the tokens
into a single decoded string. Shared Arg::Text currently loses the authored
quotes and resolves backslashes before the token-list reader rejects it.

Seven separate false acceptances concern quoted numeric selector bounds.
For example .left("2",{"abcd"}) emits abcd here but is refused natively.
.mid("0",1,{1,2,3}) is also refused natively but emits 1 here. Scope::number
accepts either Arg variant and evaluates the decoded string as an expression.
Those bounds must remain numeric expressions, without changing other dialects'
string grammar. Fix that isolated admission bug before enabling quoted lists.

The audit keeps bare-name/number/character and adjacent-token admission,
constructed quote/backslash values, empty selections and inactive/captured
source visible. Wider token-list/string grammar must not be guessed from the
single .left example. A raw-spelling hook on the existing TextSyntax grammar
seam is a candidate for quoted admission, but needs its own concrete design
and byte-preservation proof after the numeric-bound prerequisite.
