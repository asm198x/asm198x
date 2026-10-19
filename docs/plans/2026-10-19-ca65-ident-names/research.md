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

The focused fix resolves all 25 invalid-name acceptances. It validates the
untrimmed string at Ca65Text::function(.ident), preserving one legal cheap-local
prefix and leaving register/mnemonic context to later binding. The full 165-case
CLI comparison now differs only on #875's two missing anchors and #876's two
native-valid A constant definitions.

The 163-case fixture accounts for the two separately named #875 controls against
the complete retained audit; the A constant gaps remain explicit. Rust checks
real address labels with the official debug reader, constant kind/value, bytes,
formatter idempotence/symbol-section shape and preserved failed outputs. Additional
human/JSON include and macro checks preserve five outputs, sources and frames.
Native .tcount/.match/.blank also reject an invalid nested .ident, as retained in
the supplemental token-list controls. Lazy skipped/unused/zero-repeat cases pass.
Full Rust, Node/lints, actual archives and clean producer/replay remain pending.

Final full Rust passes all 1,603 tests (38 ignored). All 97 Node tests, Clippy,
rustfmt, Ruff and generated docs pass. Native formatting preserves all 163
outcomes and 61 accepted images; only the two explicit #876 original/formatted
assembly gaps remain in this fixture. The actual archive passes all 82 cohorts,
including 340 identifier commands, and both corruption controls fail. The
preceding archive fails exactly the new cohort. Freeze this verified source and
retain a complete clean 4510 replay before opening the shipping PR.

Final clean producer `431560df6d7036be06c32351204ecd133c618f70`, tree
`df587bf9068e6ec024b6db09daa5c833779df396`, has CLI SHA-256
`9cbec83b77db0d2e0199b6391df19c3a5591f161cd50388080fda0add31a0ed5`.
Its actual archive passes all 82 cohorts and 340 identifier commands. Changed
bytes fail exactly three reference controls; corrupt archives fail SHA validation.
Clean 4510 replay matches all 564 native bytes across seven processes/fourteen
retained streams, and the changed-byte guard fails. Disposable audit work was
removed. Shipping code/harness and the native audit embedded by the Rust test
match the clean producer. The generated Python bytecode cache from the initial
probe was removed; no trial production patch or cache is shipped.
