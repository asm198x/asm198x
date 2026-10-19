# .ident syntax and lookup boundaries

The 165 native controls accept 61 inputs and refuse 104. The clean text-spacing
producer 551ec01e5c4f511f4d8758b7124dc8f8c7269b6a (CLI SHA-256
1fdcbf84dace5acad9ee6d11f5677a83906937cfbe3cc846ca2be9903646905d)
has 29 differences: 25 invalid identifier-name inputs accepted, two missing-root-
anchor inputs accepted, and two valid generated A constant definitions refused.
All three causes also occur with adjacent argument parentheses.

The text function currently emits the string verbatim. Reader trimming and
source parsing can then turn a bad generated name (leading/trailing whitespace,
empty, dot-containing or qualified-looking text) into a different valid token.
Validate its string before emitting a bare name. Do not reuse mos6502::is_ident:
that helper trims source whitespace and admits dots, unlike this native grammar.

The official V2.18 source provides the scanner rule, independently checked by
live controls. [FuncIdent](https://github.com/cc65/cc65/blob/V2.18/src/ca65/nexttok.c#L204)
permits one LocalStart prefix, then requires IsIdStart and IsIdChar throughout.
[Those default character rules](https://github.com/cc65/cc65/blob/V2.18/src/ca65/scanner.c#L644)
are ASCII letters/underscore first and ASCII alphanumeric/underscore thereafter;
feature-controlled @/$ characters are separate native settings. This repository
explicitly refuses .feature/.localchar switches, so no supported setting is lost.
The installed binaries report V2.18 - N/A; the tag source is explanatory precedent,
not a claimed build revision. File/blob/content identities are retained.

An anchored @near is a valid generated cheap-local name. Its root failure is a
missing global anchor, not invalid identifier text. Keep that reader-state bug
separate. Generated A is also a valid identifier string; its constant-definition
failure belongs to token preservation/constant binding. Do not blacklist register
or instruction spellings inside the identifier-string validator.

Native accepts invalid-name functions inside untaken conditionals, unused macro
bodies and zero-repeat bodies; selected expansion rejects them. Validation stays
at the existing selected text-function seam, without eager branch/body reading.
The next fixture should retain all syntax contexts, name/address values and
explicit native-valid A constant gaps, while the two known missing-anchor inputs
remain separately named audit controls for their own issue.
